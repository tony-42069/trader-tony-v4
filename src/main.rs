use anyhow::{Context, Result};
use dotenv::dotenv;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod config;
mod error;
mod models;
mod solana;
mod trading;
mod web;

use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;
use crate::web::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load environment variables
    dotenv().ok();

    // Load configuration
    let config = Arc::new(Config::load()?);
    info!("Configuration loaded successfully");
    info!("Demo mode: {}", config.demo_mode);

    // Initialize Solana client
    let solana_client = Arc::new(SolanaClient::new(&config.solana_rpc_url)?);
    solana_client.check_connection().await?;
    info!("Solana client initialized successfully");

    // Initialize wallet manager
    let wallet_manager = WalletManager::new(
        &config.solana_private_key,
        solana_client.clone(),
        config.demo_mode,
    )?;
    info!("Wallet initialized with address: {}", wallet_manager.get_public_key());

    // Initialize AutoTrader
    let auto_trader = AutoTrader::new(
        wallet_manager.clone(),
        solana_client.clone(),
        config.clone(),
    ).await?;
    info!("AutoTrader initialized");

    // Wrap AutoTrader in Arc<Mutex> for shared access
    let auto_trader = Arc::new(Mutex::new(auto_trader));

    // Auto-start trading if configured
    if config.auto_start_trading {
        info!("Auto-starting trading as configured...");
        let trader = auto_trader.lock().await;
        if let Err(e) = trader.start().await {
            tracing::error!("Failed to auto-start trading: {}", e);
        }
    }

    // Create application state for web server
    let app_state = AppState::new(
        auto_trader,
        wallet_manager,
        solana_client,
        config.clone(),
    );

    // Initialize async components (copy trade manager, etc.)
    app_state.init().await.context("Failed to initialize app state")?;
    info!("Copy trade manager initialized");

    // Start the web server
    info!("Starting TraderTony V4 API server...");
    web::server::start_server(app_state, config).await?;

    Ok(())
}
