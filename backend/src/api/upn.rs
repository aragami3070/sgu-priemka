use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
};
use serde::Serialize;

use crate::{
    api::extractors::AuthenticatedUser,
    entities::ldap::{UpnRepairEntry, UpnRepairResult},
    errors::AppError,
    state::AppState,
};

const MAX_UPN_REPORT_SIZE: usize = 10 * 1024 * 1024;

/// Объявляет защищённую cookie-сессией ручку исправления UPN.
pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/ldap/fix-upn",
        post(fix_upn).layer(DefaultBodyLimit::max(MAX_UPN_REPORT_SIZE)),
    )
}

#[derive(Debug, Serialize)]
struct FixUpnResponse {
    processed: usize,
    items: Vec<UpnRepairResult>,
}

/// Обрабатывает JSON-отчёт от имени LDAP/Kerberos-сессии из cookie.
async fn fix_upn(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(entries): Json<Vec<UpnRepairEntry>>,
) -> Result<(StatusCode, Json<FixUpnResponse>), AppError> {
    if entries.is_empty() {
        return Err(AppError::Validation("JSON report is empty".to_owned()));
    }
    tracing::info!(
        username = %user.username,
        entries = entries.len(),
        "принят JSON-отчёт исправления UPN"
    );
    let repaired = state
        .ldap
        .repair_user_principal_names(&user.kerberos_credentials, &entries)
        .await?;
    let processed = repaired.len();
    Ok((
        StatusCode::OK,
        Json(FixUpnResponse {
            processed,
            items: repaired,
        }),
    ))
}
