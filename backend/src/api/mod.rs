//! HTTP-интерфейс Axum для фронтенда.

/// Методы входа и выхода.
mod auth;
/// Создание, чтение и удаление локальной session cookie.
mod cookies;
/// Экстрактор аутентификации для защищённых методов.
mod extractors;
/// Загрузка и замена конфигурации учебных групп.
mod groups;
/// Методы загрузки CSV и отслеживания прогресса импорта.
mod imports;
/// Отправка учётных данных по электронной почте.
mod mail;
/// Методы просмотра и скачивания сформированных результатов.
mod results;

use axum::{Router, http::StatusCode, routing::get};

use crate::state::AppState;

/// Объединяет HTTP-маршруты и подключает общее состояние приложения.
pub(crate) fn router(state: AppState) -> Router {
    let api = Router::new()
        .merge(auth::routes())
        .merge(groups::routes())
        .merge(imports::routes())
        .merge(results::routes())
        .merge(mail::routes());

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(state)
}

/// Простой метод проверки работоспособности без аутентификации.
async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}
