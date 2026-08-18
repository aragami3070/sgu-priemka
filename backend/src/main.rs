mod api;
mod config;
mod entities;
mod errors;
mod services;
mod state;

use config::Config;
use state::AppState;
use tower_http::services::{ServeDir, ServeFile};

/// Загружает зависимости, привязывает HTTP-сокет и запускает сервер Axum.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::load()?;
    let listen_addr = config.listen_addr;
    let state = AppState::new(config)?;

    spawn_cleanup_task(state.clone());
    state.mail.test_connection().await?;

    let frontend_dir = std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "frontend/dist".to_owned());
    let static_service = ServeDir::new(&frontend_dir)
        .not_found_service(ServeFile::new(format!("{frontend_dir}/index.html")));
    let app = api::router(state).fallback_service(static_service);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    tracing::info!(%listen_addr, "backend запущен и слушает адрес");
    axum::serve(listener, app).await?;

    Ok(())
}

/// Периодически очищает истёкшие сессии и terminal Job.
fn spawn_cleanup_task(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            state.sessions.cleanup_expired().await;
            state.jobs.cleanup_expired().await;
        }
    });
}
