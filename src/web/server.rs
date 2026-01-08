//! Axum web server setup and configuration

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use super::routes::create_routes;
use super::AppState;
use crate::config::Config;

/// Start the Axum web server
pub async fn start_server(state: AppState, config: Arc<Config>) -> Result<()> {
    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any) // TODO: Restrict to specific origins in production
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the router with all routes
    let app = create_routes(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Determine bind address
    let host = config.api_host.as_deref().unwrap_or("0.0.0.0");
    let port = config.api_port.unwrap_or(3000);
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .context("Invalid API_HOST or API_PORT")?;

    info!("Starting API server on http://{}", addr);

    // Start the server
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind to address")?;

    axum::serve(listener, app)
        .await
        .context("Server error")?;

    Ok(())
}

/// Create the Axum router without starting the server (useful for testing)
pub fn create_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    create_routes(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
