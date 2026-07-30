use axum::{
    Json, Router,
    extract::{Path, State},
    response::Response,
    routing::get,
};
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{api::extractors::AuthenticatedUser, errors::AppError, state::AppState};

/// Объявляет маршруты просмотра и скачивания результатов без подключения состояния.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/results", get(list_results))
        .route("/results/{storage_id}/{filename}", get(download_result))
}

/// Метаданные одного сформированного CSV для истории результатов.
#[derive(Debug, Serialize)]
struct ResultItem {
    /// Непрозрачный каталог владельца, содержащий файл.
    storage_id: Uuid,
    /// Имя файла, сформированное из даты и времени создания.
    filename: String,
    /// Дата и время создания в UTC.
    created_at: OffsetDateTime,
    /// Размер файла в байтах.
    size: u64,
}

/// Тело ответа метода получения истории результатов.
#[derive(Debug, Serialize)]
struct ResultListResponse {
    /// Неистёкшие сформированные результаты, доступные пользователю.
    items: Vec<ResultItem>,
}

/// Возвращает список сформированных CSV, доступных аутентифицированному пользователю.
async fn list_results(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<ResultListResponse>, AppError> {
    todo!("authorize the caller and list non-expired CSV results")
}

/// Возвращает один доступный пользователю CSV как скачиваемый файл.
async fn download_result(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path((_storage_id, _filename)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    todo!("authorize the caller and stream a CSV result")
}
