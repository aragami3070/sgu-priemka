use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use time::format_description::well_known::Rfc3339;

use crate::{
    api::extractors::AuthenticatedUser,
    entities::{
        import::ImportContext,
        job::{JobStage, JobStatus},
    },
    errors::AppError,
    state::AppState,
};

/// Объявляет маршруты просмотра и скачивания результатов без подключения состояния.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/results", get(list_results))
        .route(
            "/results/{owner}/{filename}",
            get(download_result).delete(delete_result),
        )
        .route(
            "/results/{owner}/{filename}/create-accounts",
            post(create_accounts),
        )
        .route(
            "/results/{owner}/{filename}/delete-accounts",
            post(delete_accounts),
        )
}

/// Метаданные одного сформированного CSV для истории результатов.
#[derive(Debug, Serialize)]
struct ResultItem {
    /// `sAMAccountName` владельца каталога, содержащего файл.
    owner: String,
    /// Имя файла, сформированное из даты и времени создания.
    filename: String,
    /// Дата и время создания в UTC.
    created_at: String,
    /// Размер файла в байтах.
    size: u64,
}

/// Тело ответа метода получения истории результатов.
#[derive(Debug, Serialize)]
struct ResultListResponse {
    /// Все сформированные результаты, доступные пользователю.
    items: Vec<ResultItem>,
}

/// Идентификатор задачи изменения LDAP-аккаунтов из готового результата.
#[derive(Debug, Serialize)]
struct AccountOperationResponse {
    /// Идентификатор задачи для подписки на существующий import WebSocket.
    job_id: String,
}

/// Возвращает список сформированных CSV, доступных аутентифицированному пользователю.
async fn list_results(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<ResultListResponse>, AppError> {
    let items = state
        .results
        .list()
        .await?
        .into_iter()
        .map(|result| {
            Ok(ResultItem {
                owner: result.owner,
                filename: result.filename,
                created_at: result
                    .created_at
                    .format(&Rfc3339)
                    .map_err(|_| AppError::Internal)?,
                size: result.size,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(Json(ResultListResponse { items }))
}

/// Возвращает один доступный пользователю CSV как скачиваемый файл.
async fn download_result(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((owner, filename)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let bytes = state.results.read(&owner, &filename).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| AppError::Internal)?,
    );
    Ok((headers, bytes).into_response())
}

/// Удаляет выбранный итоговый CSV по явному действию администратора.
async fn delete_result(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((owner, filename)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    state.results.delete(&owner, &filename).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Запускает создание LDAP-аккаунтов из уже сохранённого CSV-результата.
async fn create_accounts(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((owner, filename)): Path<(String, String)>,
) -> Result<(StatusCode, Json<AccountOperationResponse>), AppError> {
    // Проверяем существование результата до создания job, чтобы не запускать
    // задачу, которая сразу завершится с NotFound.
    state.results.read(&owner, &filename).await?;
    let job_id = state
        .jobs
        .create(
            user.username.clone(),
            JobStatus::Progress {
                stage: JobStage::Parsing,
                current: 0,
                total: 0,
            },
        )
        .await?;
    let context = ImportContext {
        job_id: job_id.clone(),
        username: user.username,
        kerberos_credentials: user.kerberos_credentials,
        original_filename: filename.clone(),
    };
    let imports = state.imports.clone();
    let job_id_for_task = job_id.clone();
    let filename_for_task = filename.clone();
    let result_owner = owner.clone();
    tokio::spawn(async move {
        if let Err(error) = imports
            .run_result(context, result_owner, filename_for_task)
            .await
        {
            tracing::error!(job_id = %job_id_for_task, %error, "stored-result LDAP creation stopped unexpectedly");
        }
    });

    tracing::info!(%job_id, %owner, %filename, "LDAP creation from stored result accepted");
    Ok((
        StatusCode::ACCEPTED,
        Json(AccountOperationResponse { job_id }),
    ))
}

/// Запускает удаление LDAP-аккаунтов из уже сохранённого CSV-результата.
async fn delete_accounts(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((owner, filename)): Path<(String, String)>,
) -> Result<(StatusCode, Json<AccountOperationResponse>), AppError> {
    state.results.read(&owner, &filename).await?;
    let job_id = state
        .jobs
        .create(
            user.username.clone(),
            JobStatus::Progress {
                stage: JobStage::Parsing,
                current: 0,
                total: 0,
            },
        )
        .await?;
    let context = ImportContext {
        job_id: job_id.clone(),
        username: user.username,
        kerberos_credentials: user.kerberos_credentials,
        original_filename: filename.clone(),
    };
    let imports = state.imports.clone();
    let job_id_for_task = job_id.clone();
    let filename_for_task = filename.clone();
    let result_owner = owner.clone();
    tokio::spawn(async move {
        if let Err(error) = imports
            .run_result_deletion(context, result_owner, filename_for_task)
            .await
        {
            tracing::error!(
                job_id = %job_id_for_task,
                %error,
                "stored-result LDAP deletion stopped unexpectedly"
            );
        }
    });

    tracing::info!(%job_id, %owner, %filename, "LDAP deletion from stored result accepted");
    Ok((
        StatusCode::ACCEPTED,
        Json(AccountOperationResponse { job_id }),
    ))
}
