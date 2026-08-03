#![allow(
    dead_code,
    reason = "типы и методы каркаса будут подключаться по мере реализации todo"
)]

mod api;
mod config;
mod entities;
mod errors;
mod services;
mod state;

use config::Config;
use state::AppState;

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
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    tracing::info!(%listen_addr, "backend listening");
    axum::serve(listener, app).await?;

    Ok(())
}

/// Периодически очищает истёкшие сессии, terminal Job и итоговые CSV.
fn spawn_cleanup_task(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            state.sessions.cleanup_expired().await;
            state.jobs.cleanup_expired().await;
            if let Err(error) = state.results.cleanup_expired().await {
                tracing::error!(%error, "periodic result cleanup failed");
            }
        }
    });
}
