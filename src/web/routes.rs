//! API route definitions

use axum::{
    routing::{get, post, put, delete},
    Router,
};

use super::handlers;
use super::websocket::ws_handler;
use super::AppState;

/// Create all API routes
pub fn create_routes(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/api/health", get(handlers::health_check))

        // Wallet
        .route("/api/wallet", get(handlers::get_wallet))

        // Positions
        .route("/api/positions", get(handlers::get_positions))
        .route("/api/positions/active", get(handlers::get_active_positions))

        // Trades
        .route("/api/trades", get(handlers::get_trades))

        // Statistics
        .route("/api/stats", get(handlers::get_stats))

        // Strategies
        .route("/api/strategies", get(handlers::list_strategies))
        .route("/api/strategies", post(handlers::create_strategy))
        .route("/api/strategies/:id", get(handlers::get_strategy))
        .route("/api/strategies/:id", put(handlers::update_strategy))
        .route("/api/strategies/:id", delete(handlers::delete_strategy))
        .route("/api/strategies/:id/toggle", post(handlers::toggle_strategy))

        // AutoTrader control
        .route("/api/autotrader/status", get(handlers::get_autotrader_status))
        .route("/api/autotrader/start", post(handlers::start_autotrader))
        .route("/api/autotrader/stop", post(handlers::stop_autotrader))

        // Token analysis
        .route("/api/analyze", post(handlers::analyze_token))

        // WebSocket
        .route("/ws", get(ws_handler))

        // Add state to all routes
        .with_state(state)
}
