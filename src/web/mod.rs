//! Web API module for TraderTony V4
//!
//! This module provides the REST API and WebSocket server for the trading bot,
//! replacing the previous Telegram bot interface.

pub mod server;
pub mod routes;
pub mod handlers;
pub mod websocket;
pub mod models;

use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;

use self::websocket::WsMessage;

/// Shared application state for all API handlers
#[derive(Clone)]
pub struct AppState {
    /// The AutoTrader instance for managing trading operations
    pub auto_trader: Arc<Mutex<AutoTrader>>,
    /// Wallet manager for transaction signing
    pub wallet_manager: Arc<WalletManager>,
    /// Solana RPC client
    pub solana_client: Arc<SolanaClient>,
    /// Application configuration
    pub config: Arc<Config>,
    /// Broadcast channel for WebSocket messages
    pub ws_tx: broadcast::Sender<WsMessage>,
}

impl AppState {
    /// Create a new AppState instance
    pub fn new(
        auto_trader: Arc<Mutex<AutoTrader>>,
        wallet_manager: Arc<WalletManager>,
        solana_client: Arc<SolanaClient>,
        config: Arc<Config>,
    ) -> Self {
        // Create broadcast channel for WebSocket messages (capacity of 100 messages)
        let (ws_tx, _) = broadcast::channel(100);

        Self {
            auto_trader,
            wallet_manager,
            solana_client,
            config,
            ws_tx,
        }
    }

    /// Get a new receiver for WebSocket messages
    pub fn subscribe_ws(&self) -> broadcast::Receiver<WsMessage> {
        self.ws_tx.subscribe()
    }

    /// Broadcast a message to all WebSocket clients
    pub fn broadcast(&self, msg: WsMessage) {
        // Ignore errors (no subscribers)
        let _ = self.ws_tx.send(msg);
    }
}
