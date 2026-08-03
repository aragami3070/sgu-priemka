use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, State, WebSocketUpgrade, ws::Message},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use std::sync::Arc;

use tokio::sync::watch;

use crate::{
    api::extractors::AuthenticatedUser,
    entities::{
        import::ImportContext,
        job::{JobClientMessage, JobStage, JobStatus, LoginResolutionBatch},
    },
    errors::AppError,
    state::AppState,
};

const MAX_CSV_SIZE: usize = 10 * 1024 * 1024;
const MULTIPART_OVERHEAD_ALLOWANCE: usize = 64 * 1024;

/// Объявляет маршруты загрузки и прогресса без подключения состояния приложения.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/imports",
            post(create_import).layer(DefaultBodyLimit::max(
                MAX_CSV_SIZE + MULTIPART_OVERHEAD_ALLOWANCE,
            )),
        )
        .route("/imports/active", get(active_import))
        .route("/imports/{job_id}/events", get(import_events))
}

/// Идентификатор, возвращаемый сразу после принятия задачи импорта.
#[derive(Debug, Serialize)]
struct CreateImportResponse {
    /// Идентификатор задачи для подписки на события прогресса.
    job_id: String,
}

/// Текущая задача пользователя для восстановления страницы после reload.
#[derive(Debug, Serialize)]
struct ActiveImportResponse {
    job_id: String,
}

/// Возвращает нетерминальную задачу текущего пользователя, если она существует.
async fn active_import(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    match state.jobs.active_for_owner(&user.username).await {
        Some(job_id) => Ok(Json(ActiveImportResponse { job_id }).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

/// Принимает один multipart CSV, создаёт задачу и запускает фоновую обработку.
async fn create_import(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    multipart: Multipart,
) -> Result<(StatusCode, Json<CreateImportResponse>), AppError> {
    let (original_filename, file_bytes) = read_csv_upload(multipart).await?;
    let initial_status = JobStatus::Progress {
        stage: JobStage::Uploading,
        current: 1,
        total: 1,
    };
    let job_id = state
        .jobs
        .create(user.username.clone(), initial_status)
        .await?;
    let context = ImportContext {
        job_id: job_id.clone(),
        username: user.username,
        ldap_credentials: user.ldap_credentials,
        original_filename,
    };
    let imports = state.imports.clone();
    tokio::spawn(async move {
        if let Err(error) = imports.run(context, file_bytes).await {
            tracing::error!(%error, "background import pipeline stopped unexpectedly");
        }
    });

    tracing::info!(%job_id, "CSV upload accepted and import pipeline spawned");
    Ok((StatusCode::ACCEPTED, Json(CreateImportResponse { job_id })))
}

/// Открывает WebSocket с изменениями статуса принадлежащей пользователю задачи.
async fn import_events(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(job_id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let receiver = state.jobs.subscribe(&job_id, &user.username).await?;
    tracing::info!(%job_id, username = %user.username, "import event WebSocket accepted");
    Ok(upgrade.on_upgrade(move |socket| {
        stream_job_events(socket, job_id, user.username, receiver, state.jobs)
    }))
}

async fn read_csv_upload(mut multipart: Multipart) -> Result<(String, Vec<u8>), AppError> {
    let mut upload = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::InvalidUpload(error.to_string()))?
    {
        if upload.is_some() {
            return Err(AppError::InvalidUpload(
                "multipart must contain exactly one CSV file".to_owned(),
            ));
        }
        if field.name() != Some("file") {
            return Err(AppError::InvalidUpload(
                "multipart file field must be named `file`".to_owned(),
            ));
        }

        let filename = field
            .file_name()
            .map(str::to_owned)
            .filter(|filename| filename.to_lowercase().ends_with(".csv"))
            .ok_or_else(|| AppError::InvalidUpload("only .csv files are accepted".to_owned()))?;
        let bytes = field
            .bytes()
            .await
            .map_err(|error| AppError::InvalidUpload(error.to_string()))?;
        if bytes.is_empty() {
            return Err(AppError::InvalidUpload("CSV file is empty".to_owned()));
        }
        if bytes.len() > MAX_CSV_SIZE {
            return Err(AppError::InvalidUpload(format!(
                "CSV file exceeds the {} MiB limit",
                MAX_CSV_SIZE / 1024 / 1024
            )));
        }
        upload = Some((filename, bytes.to_vec()));
    }

    upload.ok_or_else(|| AppError::InvalidUpload("CSV file is missing".to_owned()))
}

async fn stream_job_events(
    mut socket: axum::extract::ws::WebSocket,
    job_id: String,
    owner: String,
    mut receiver: watch::Receiver<JobStatus>,
    jobs: Arc<crate::services::jobs::JobService>,
) {
    let mut send_current_status = true;
    loop {
        if send_current_status {
            let status = receiver.borrow_and_update().clone();
            let terminal = status.is_terminal();
            let message = match serde_json::to_string(&status) {
                Ok(message) => message,
                Err(error) => {
                    tracing::error!(%job_id, %error, "failed to serialize import job status");
                    return;
                }
            };

            if let Err(error) = socket.send(Message::Text(message.into())).await {
                tracing::info!(%job_id, %error, "import event WebSocket disconnected");
                return;
            }
            if terminal {
                tracing::info!(%job_id, "terminal import job status sent over WebSocket");
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            send_current_status = false;
        }

        tokio::select! {
            changed = receiver.changed() => {
                if changed.is_err() {
                    tracing::info!(%job_id, "import job event channel closed");
                    return;
                }
                send_current_status = true;
            }
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else {
                    tracing::info!(%job_id, "import event WebSocket disconnected");
                    return;
                };
                match message {
                    Message::Text(text) => match serde_json::from_str::<JobClientMessage>(&text) {
                        Ok(JobClientMessage::ResolveLogins { resolutions }) => {
                            if let Err(error) = jobs
                                .submit_login_resolutions(
                                    &job_id,
                                    &owner,
                                    LoginResolutionBatch { resolutions },
                                )
                                .await
                            {
                                tracing::warn!(%job_id, %error, "login resolution batch was rejected");
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%job_id, %error, "invalid import WebSocket message ignored");
                        }
                    },
                    Message::Close(_) => return,
                    _ => {}
                }
            }
        }
    }
}
