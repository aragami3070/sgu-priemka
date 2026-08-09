use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use futures_util::StreamExt;

use crate::{
    api::extractors::AuthenticatedUser,
    entities::job::{JobStage, JobStatus, ResultReference},
    services::{
        import::parser::{MailCredentialRow, parse_credentials_csv},
        mail::{CredentialTemplateData, MailBatchStatus, MailDeliveryStatus, PreparedMail},
    },
    state::AppState,
};

/// Объявляет ручку запуска рассылки по сохранённому результату.
pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/mail/{owner}/{filename}/send-credentials",
        post(send_credentials),
    )
}

/// Идентификатор фоновой задачи рассылки.
#[derive(Debug, serde::Serialize)]
struct MailOperationResponse {
    /// Идентификатор задачи для подписки на существующий import WebSocket.
    job_id: String,
}

/// Запускает отправку логинов и временных паролей по выбранному CSV.
async fn send_credentials(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path((owner, filename)): Path<(String, String)>,
) -> Result<(StatusCode, Json<MailOperationResponse>), crate::errors::AppError> {
    // Проверяем существование файла до создания задачи, чтобы не оставлять
    // задачу, которая заведомо завершится из-за отсутствующего результата.
    state.results.read(&owner, &filename).await?;

    // Атомарно проверяем и переводим рассылку в Running.
    state
        .mail
        .try_start_delivery(&format!("{owner}/{filename}"))
        .await
        .map_err(|_| crate::errors::AppError::MailDeliveryBusy)?;

    let job_id = state
        .jobs
        .create(
            user.username.clone(),
            JobStatus::MailProgress {
                current: 0,
                total: 0,
                accepted: 0,
                failed: 0,
            },
        )
        .await?;

    let task_state = state.clone();
    let task_job_id = job_id.clone();
    let task_owner = owner.clone();
    let task_filename = filename.clone();
    tokio::spawn(async move {
        run_mail_delivery(task_state, task_job_id, task_owner, task_filename).await;
    });

    tracing::info!(%job_id, %owner, %filename, "рассылка учётных данных принята");
    Ok((StatusCode::ACCEPTED, Json(MailOperationResponse { job_id })))
}

/// Выполняет чтение результата, подготовку писем и batch-отправку через SMTP.
async fn run_mail_delivery(state: AppState, job_id: String, owner: String, filename: String) {
    let key = format!("{owner}/{filename}");
    let result = ResultReference {
        owner: owner.clone(),
        filename: filename.clone(),
    };
    let students = match load_students(&state, &job_id, &owner, &filename).await {
        Ok(students) => students,
        Err(message) => {
            finish_failure(&state, &job_id, message).await;
            state
                .mail
                .finish_delivery(&key, MailBatchStatus::Failed)
                .await;
            return;
        }
    };

    let total = students.len();
    let mails = match prepare_mails(&state, students) {
        Ok(mails) => mails,
        Err(message) => {
            finish_failure(&state, &job_id, message).await;
            state
                .mail
                .finish_delivery(&key, MailBatchStatus::Failed)
                .await;
            return;
        }
    };
    if state
        .jobs
        .publish(
            &job_id,
            JobStatus::MailProgress {
                current: 0,
                total,
                accepted: 0,
                failed: 0,
            },
        )
        .await
        .is_err()
    {
        state
            .mail
            .finish_delivery(&key, MailBatchStatus::Failed)
            .await;
        return;
    }

    // Потребляем stream по мере завершения каждой SMTP-операции,
    // публикуя WebSocket progress в реальном времени.
    let mut deliveries = state.mail.send_batch_stream(mails);
    let mut accepted = 0;
    let mut failed = 0;
    let mut current = 0;

    while let Some(delivery) = deliveries.next().await {
        current += 1;
        match &delivery.status {
            MailDeliveryStatus::AcceptedBySmtp => accepted += 1,
            MailDeliveryStatus::Failed { retryable, reason } => {
                failed += 1;
                tracing::warn!(
                    %job_id,
                    row_id = %delivery.row_id,
                    recipient = %delivery.email,
                    retryable,
                    reason = %reason,
                    "письмо не принято SMTP"
                );
            }
        }
        if let Err(error) = state
            .jobs
            .publish(
                &job_id,
                JobStatus::MailProgress {
                    current,
                    total,
                    accepted,
                    failed,
                },
            )
            .await
        {
            tracing::warn!(%job_id, %error, "не удалось опубликовать прогресс рассылки");
            state
                .mail
                .finish_delivery(&key, MailBatchStatus::Failed)
                .await;
            return;
        }
    }

    let batch_status = if failed == 0 {
        MailBatchStatus::Completed
    } else if accepted == 0 {
        MailBatchStatus::Failed
    } else {
        MailBatchStatus::PartiallyFailed
    };

    let status = JobStatus::MailCompleted {
        accepted,
        failed,
        total,
        result,
    };
    if let Err(error) = state.jobs.publish(&job_id, status).await {
        tracing::warn!(%job_id, %error, "не удалось опубликовать итог рассылки");
    } else {
        tracing::info!(%job_id, accepted, failed, total, "рассылка учётных данных завершена");
    }
    state.mail.finish_delivery(&key, batch_status).await;
}

/// Читает выбранный CSV и преобразует его в записи с credentials.
async fn load_students(
    state: &AppState,
    job_id: &str,
    owner: &str,
    filename: &str,
) -> Result<Vec<MailCredentialRow>, String> {
    let bytes = state.results.read(owner, filename).await.map_err(|error| {
        tracing::error!(%job_id, %error, "не удалось прочитать CSV перед рассылкой");
        "не удалось прочитать выбранный CSV".to_owned()
    })?;
    tokio::task::spawn_blocking(move || parse_credentials_csv(&bytes))
        .await
        .map_err(|error| {
            tracing::error!(%job_id, %error, "задача разбора CSV рассылки завершилась с ошибкой");
            "не удалось разобрать выбранный CSV".to_owned()
        })?
        .map_err(|error| {
            tracing::error!(%job_id, %error, "CSV не прошёл проверку перед рассылкой");
            "выбранный CSV имеет неверный формат".to_owned()
        })
}

/// Рендерит письмо для каждой строки результата без раскрытия паролей в логах.
fn prepare_mails(
    state: &AppState,
    students: Vec<MailCredentialRow>,
) -> Result<Vec<PreparedMail>, String> {
    students
        .into_iter()
        .map(|student| {
            let rendered = state
                .mail
                .render_credentials(CredentialTemplateData {
                    login: &student.login,
                    temporary_password: &student.password,
                })
                .map_err(|error| {
                    tracing::error!(%error, "не удалось отрендерить шаблон письма");
                    "не удалось подготовить шаблон письма".to_owned()
                })?;
            Ok(PreparedMail {
                row_id: student.source_row.to_string(),
                recipient: student.email,
                html_body: rendered.html,
                plain_text_body: rendered.plain_text,
            })
        })
        .collect()
}

/// Публикует стабильный терминальный статус при ошибке подготовки рассылки.
async fn finish_failure(state: &AppState, job_id: &str, message: String) {
    if let Err(error) = state
        .jobs
        .publish(
            job_id,
            JobStatus::Failed {
                stage: JobStage::SendingMail,
                code: "mail_preparation".to_owned(),
                message,
                row: None,
            },
        )
        .await
    {
        tracing::warn!(%job_id, %error, "не удалось опубликовать ошибку рассылки");
    }
}
