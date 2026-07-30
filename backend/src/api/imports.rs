use axum::{
    Json, Router,
    extract::{Multipart, Path, State, WebSocketUpgrade},
    response::Response,
    routing::{get, post},
};
use serde::Serialize;

use crate::{api::extractors::AuthenticatedUser, errors::AppError, state::AppState};

/// Объявляет маршруты загрузки и прогресса без подключения состояния приложения.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/imports", post(create_import))
        .route("/imports/{job_id}/events", get(import_events))
}

/// Идентификатор, возвращаемый сразу после принятия задачи импорта.
#[derive(Debug, Serialize)]
struct CreateImportResponse {
    /// Идентификатор задачи для подписки на события прогресса.
    job_id: String,
}

/// Принимает один multipart CSV, создаёт задачу и запускает фоновую обработку.
async fn create_import(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    _multipart: Multipart,
) -> Result<Json<CreateImportResponse>, AppError> {
    todo!("validate the upload, create a job, and spawn the import pipeline")
}

/// Открывает WebSocket с изменениями статуса принадлежащей пользователю задачи.
async fn import_events(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_job_id): Path<String>,
    _upgrade: WebSocketUpgrade,
) -> Result<Response, AppError> {
    todo!("authorize the job and stream watch channel updates")
}
