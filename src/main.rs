use anyhow::Result;
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

use crate::bot::commands::{command_handler, Command};
use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;

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

    // Initialize Telegram bot
    let bot = Bot::new(&config.telegram_bot_token);

    // Initialize AutoTrader (already returns Arc<Self>)
    let auto_trader = AutoTrader::new(
        wallet_manager.clone(),
        solana_client.clone(),
        config.clone(), // Pass Arc<Config>
    );
    info!("AutoTrader initialized");

    // Set up shared state for Teloxide
    let bot_state = bot::BotState { // Create the state struct directly
        // auto_trader field expects Arc<Mutex<AutoTrader>>
        auto_trader: Arc::new(Mutex::new(auto_trader)), // Wrap the AutoTrader instance correctly
        wallet_manager: wallet_manager.clone(),
        solana_client: solana_client.clone(), // Pass Arc<SolanaClient>
        // config field expects Config, not Arc<Config>
        config: (*config).clone(), // Clone the inner Config value from the Arc
        authorized_users: config.authorized_users.clone(), // Clone the Vec<i64>
    };
    // Explicitly wrap bot_state in Arc<Mutex<>> for the handler
    let bot_state_dependency = Arc::new(Mutex::new(bot_state));

    // Start the bot
    info!("Starting TraderTony V4 bot...");
    let handler = Update::filter_message().branch(
        dptree::entry()
            .filter_command::<Command>()
            .endpoint(command_handler),
    );

    Dispatcher::builder(bot, handler)
        // Provide the correctly wrapped state to the dispatcher
        .dependencies(dptree::deps![bot_state_dependency])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    
    Ok(())
}
