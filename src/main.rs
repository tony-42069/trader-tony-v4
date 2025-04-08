use anyhow::{Context, Result}; // Import Context trait
use dotenv::dotenv;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod bot;
mod config;
mod error;
mod models;
mod solana;
mod trading;

// Import command and callback handlers
use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;
use crate::api::birdeye::BirdeyeClient; // Import BirdeyeClient
use crate::api::jupiter::JupiterClient;

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

    // Initialize Solana client and wrap in Arc
    let solana_client = Arc::new(SolanaClient::new(&config.solana_rpc_url)?);
    info!("Solana client initialized successfully");

    // Initialize wallet manager (already returns Arc<Self>)
    let wallet_manager = WalletManager::new(&config.solana_private_key, solana_client.clone(), config.demo_mode)?;
    info!("Wallet initialized with address: {}", wallet_manager.get_public_key());

    // Initialize Birdeye client
    let birdeye_client = Arc::new(BirdeyeClient::new(
        config.birdeye_api_key.as_ref().context("BIRDEYE_API_KEY missing")?
    ));
    info!("Birdeye client initialized");

    // Initialize AutoTrader
    let auto_trader = AutoTrader::new(
        Arc::new(wallet_manager),
        Arc::new(solana_client),
        config.clone(), // Already Arc<Config>
    )?; // Handle potential error from AutoTrader::new
    info!("AutoTrader initialized");

    // Set up shared state for Teloxide
    let bot_state = bot::BotState {
        auto_trader: Arc::new(Mutex::new(auto_trader)),
        wallet_manager: None, // Will be inside AutoTrader now
        solana_client: None, // Will be inside AutoTrader now
        config: config.clone(),
        authorized_users: config.authorized_users.clone(), // Use correct field name
        notification_manager: None, // Will initialize after bot creation
    };

    // Arc-wrap the state for thread safety
    let state = Arc::new(Mutex::new(bot_state));
    
    // Initialize bot and notification manager
    let bot = Bot::new(&config.telegram_bot_token); // Use correct field name
    
    // Create NotificationManager and update state
    {
        let notification_manager = Arc::new(bot::notification::NotificationManager::new(bot.clone(), state.clone()));
        let mut locked_state = state.lock().await;
        locked_state.notification_manager = Some(notification_manager.clone());
        
        // Set notification manager in AutoTrader
        let mut auto_trader = locked_state.auto_trader.lock().await;
        auto_trader.set_notification_manager(notification_manager);
        drop(auto_trader);
        drop(locked_state);
    }
    
    // Start the Telegram bot
    info!("Starting TraderTony V4 bot...");
    bot::commands::start_bot(bot, state).await?;
    
    Ok(())
}
