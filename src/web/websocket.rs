//! WebSocket handler for real-time updates

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use tracing::{debug, error, info, warn};

use super::AppState;

/// WebSocket message types broadcast to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    /// A new position was opened
    PositionOpened {
        id: String,
        token_address: String,
        token_symbol: String,
        entry_value_sol: f64,
        token_amount: f64,
        strategy_id: String,
        timestamp: DateTime<Utc>,
    },

    /// A position was closed
    PositionClosed {
        id: String,
        token_address: String,
        token_symbol: String,
        exit_value_sol: f64,
        pnl_sol: f64,
        pnl_percent: f64,
        exit_reason: String,
        timestamp: DateTime<Utc>,
    },

    /// Price update for a held token
    PriceUpdate {
        token_address: String,
        token_symbol: String,
        price_sol: f64,
        change_percent: f64,
        timestamp: DateTime<Utc>,
    },

    /// AutoTrader status changed
    StatusChange {
        running: bool,
        timestamp: DateTime<Utc>,
    },

    /// Error notification
    Error {
        message: String,
        details: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// Trade signal for copy trading
    TradeSignal {
        signal_id: String,
        token_address: String,
        token_symbol: String,
        action: String, // "buy" or "sell"
        amount_sol: f64,
        price_sol: f64,
        bot_position_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Heartbeat/ping message
    Ping {
        timestamp: DateTime<Utc>,
    },
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.subscribe_ws();

    info!("New WebSocket client connected");

    // Send initial ping
    let ping = WsMessage::Ping {
        timestamp: Utc::now(),
    };
    if let Ok(json) = serde_json::to_string(&ping) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    // Spawn task to forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize WebSocket message: {}", e);
                }
            }
        }
    });

    // Handle incoming messages from client
    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    // Handle client messages if needed (e.g., subscription preferences)
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping, will auto-respond with pong");
                    // Axum handles pong automatically
                    let _ = data; // Acknowledge we received it
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket client disconnected");
                    break;
                }
                Ok(Message::Binary(_)) => {
                    warn!("Received unexpected binary message");
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    info!("WebSocket connection closed");
}
