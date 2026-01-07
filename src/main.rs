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

use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;
use crate::api::birdeye::BirdeyeClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load environment variables
    dotenv().ok();

    // Load configuration and wrap in Arc
    let config = Arc::new(Config::load()?);
    info!("Configuration loaded successfully");

    // Initialize Solana client
    let solana_client = SolanaClient::new(&config.solana_rpc_url)?;
    solana_client.check_connection().await?;
    info!("Solana client initialized successfully");

    // Initialize wallet manager
    let wallet_manager = WalletManager::new(&config.solana_private_key, Arc::new(solana_client), config.demo_mode)?;
    info!("Wallet initialized with address: {}", wallet_manager.get_public_key());

    // Initialize Birdeye client
    let _birdeye_client = Arc::new(BirdeyeClient::new(
        config.birdeye_api_key.as_ref().context("BIRDEYE_API_KEY missing")?
    ));
    info!("Birdeye client initialized");

    // Initialize AutoTrader
    let auto_trader = AutoTrader::new(
        wallet_manager.clone(),
        wallet_manager.solana_client().clone(),
        config.clone(),
    ).await?;
    info!("AutoTrader initialized");

    // Wrap AutoTrader in Arc<Mutex> for shared access
    let _auto_trader = Arc::new(Mutex::new(auto_trader));

    // TODO: Web API server will be added in Phase 1, Tasks 1.3-1.7
    // For now, just keep the process running
    info!("TraderTony V4 initialized. Web API server coming soon...");
    info!("Press Ctrl+C to exit.");

    // Keep the process running
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received, exiting...");

    Ok(())
}
