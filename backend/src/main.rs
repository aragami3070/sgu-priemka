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
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    tracing::info!(%listen_addr, "backend listening");
    axum::serve(listener, app).await?;

    Ok(())
}
