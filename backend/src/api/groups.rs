use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    routing::post,
};
use serde::Serialize;

use crate::{api::extractors::AuthenticatedUser, errors::AppError, state::AppState};

const MAX_GROUPS_FILE_SIZE: usize = 1024 * 1024;

/// Объявляет маршрут загрузки TOML с соответствиями учебных групп.
pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/groups",
        post(replace_groups).layer(DefaultBodyLimit::max(MAX_GROUPS_FILE_SIZE + 64 * 1024)),
    )
}

#[derive(Debug, Serialize)]
struct ReplaceGroupsResponse {
    /// Количество групп в принятом TOML-файле.
    groups: usize,
}

/// Проверяет и атомарно заменяет серверный TOML-файл групп.
async fn replace_groups(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    multipart: Multipart,
) -> Result<(StatusCode, Json<ReplaceGroupsResponse>), AppError> {
    let bytes = read_groups_upload(multipart).await?;
    let groups = state.config.groups.replace(&bytes).map_err(|error| {
        tracing::error!(%error, "не удалось заменить файл групп");
        AppError::Internal
    })?;
    tracing::info!(group_count = groups.len(), "файл групп успешно заменён");
    Ok((
        StatusCode::OK,
        Json(ReplaceGroupsResponse {
            groups: groups.len(),
        }),
    ))
}

/// Извлекает единственный TOML-файл из multipart-запроса.
async fn read_groups_upload(mut multipart: Multipart) -> Result<Vec<u8>, AppError> {
    let mut upload = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::InvalidUpload(error.to_string()))?
    {
        if upload.is_some() || field.name() != Some("file") {
            return Err(AppError::InvalidUpload(
                "multipart должен содержать один файл в поле `file`".to_owned(),
            ));
        }
        let filename = field
            .file_name()
            .map(str::to_owned)
            .filter(|name| name.to_lowercase().ends_with(".toml"))
            .ok_or_else(|| AppError::InvalidUpload("нужен файл с расширением .toml".to_owned()))?;
        let bytes = field
            .bytes()
            .await
            .map_err(|error| AppError::InvalidUpload(error.to_string()))?;
        if bytes.is_empty() {
            return Err(AppError::InvalidUpload("TOML-файл пустой".to_owned()));
        }
        if bytes.len() > MAX_GROUPS_FILE_SIZE {
            return Err(AppError::InvalidUpload(
                "размер TOML-файла превышает 1 МБ".to_owned(),
            ));
        }
        tracing::info!(%filename, size = bytes.len(), "TOML-файл групп загружен");
        upload = Some(bytes.to_vec());
    }
    upload.ok_or_else(|| AppError::InvalidUpload("TOML-файл отсутствует".to_owned()))
}
