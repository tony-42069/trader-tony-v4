use anyhow::{Context, Result};
use dotenv::dotenv;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn, Level};
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
    eprintln!("=== TraderTony V4 main() entered ===");

    // Catch panics so we see them in logs instead of a silent exit
    std::panic::set_hook(Box::new(|info| {
        eprintln!("=== PANIC === {}", info);
    }));

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load environment variables
    dotenv().ok();

    // Load configuration
    let config = Arc::new(Config::load()?);
    info!("Configuration loaded successfully (v4.1.0 - multi-strategy)");
    info!("Demo mode: {}", config.demo_mode);
    info!("Dry run mode: {}", config.dry_run_mode);

    // Initialize Solana client
    let solana_client = Arc::new(SolanaClient::new(&config.solana_rpc_url)?);
    // Don't block startup on RPC connection check - just log warning if it fails
    match solana_client.check_connection().await {
        Ok(_) => info!("Solana RPC connection verified"),
        Err(e) => tracing::warn!("Solana RPC connection check failed (will retry later): {}", e),
    }
    info!("Solana client initialized");

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

    // If TG_SESSION_B64 is set and the session file doesn't exist, decode and write it.
    // Lets us ship the session via env var on Railway instead of needing a volume mount.
    if let Ok(b64) = std::env::var("TG_SESSION_B64") {
        let session_path = std::path::PathBuf::from(&config.tg_session_path);
        let trimmed = b64.trim();
        if !session_path.exists() && !trimmed.is_empty() {
            if let Some(parent) = session_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            use base64::Engine as _;
            match base64::engine::general_purpose::STANDARD.decode(trimmed) {
                Ok(bytes) => match std::fs::write(&session_path, &bytes) {
                    Ok(_) => info!("✅ Restored TG session from TG_SESSION_B64 env var ({} bytes)", bytes.len()),
                    Err(e) => tracing::error!("Failed to write TG session from env var: {:?}", e),
                },
                Err(e) => tracing::error!("Failed to decode TG_SESSION_B64: {:?}", e),
            }
        }
    }

    // Start Telegram listener if creds are configured
    if let (Some(api_id), Some(api_hash), Some(channel)) =
        (config.tg_api_id, config.tg_api_hash.as_ref(), config.tg_channel.as_ref())
    {
        let session_path = std::path::PathBuf::from(&config.tg_session_path);
        match crate::api::telegram::TelegramClient::connect(
            api_id,
            api_hash,
            &session_path,
            channel,
        )
        .await
        {
            Ok(tg) => {
                // spawn_listener consumes `tg` by value and returns a text receiver.
                let text_rx = tg.spawn_listener();

                // Bridge text -> CallSignal by running the parser
                let (sig_tx, sig_rx) = tokio::sync::mpsc::channel::<crate::trading::sniper::CallSignal>(32);
                tokio::spawn(async move {
                    let mut text_rx = text_rx;
                    while let Some(text) = text_rx.recv().await {
                        let preview: String = text.chars().take(60).collect();
                        tracing::debug!("TG msg: {}...", preview);
                        if let Some(signal) = crate::trading::sniper::parser::parse_call_message(&text) {
                            info!("🎯 PARSED CALL: trigger={} mint={}", signal.trigger, signal.mint);
                            if let Err(e) = sig_tx.send(signal).await {
                                warn!("Failed to forward call signal: {:?}", e);
                                break;
                            }
                        }
                    }
                });

                let trader = auto_trader.lock().await;
                trader.attach_telegram_signal_rx(sig_rx).await;
                drop(trader);
                info!("✅ Telegram listener active on @{}", channel.trim_start_matches('@'));
            }
            Err(e) => {
                warn!("Failed to start Telegram client: {:?}", e);
                warn!("Run `cargo run --bin tg_login` to authorise, then restart.");
            }
        }
    } else {
        info!("Telegram creds not set — sniper disabled");
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
