use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub solana_rpc_url: String,
    pub solana_ws_url: String,
    pub solana_private_key: String,
    pub network: String,
    
    pub helius_api_key: String,
    pub jupiter_api_key: Option<String>, // Optional, not currently in .env
    pub birdeye_api_key: Option<String>, // Optional, not currently in .env
    
    pub telegram_bot_token: String,
    pub authorized_users: Vec<i64>, // Will be populated from TELEGRAM_ADMIN_USER_ID
    
    pub demo_mode: bool,
    pub max_position_size_sol: f64,
    pub total_budget_sol: f64,
    pub default_stop_loss_percent: u32,
    pub default_take_profit_percent: u32,
    pub default_trailing_stop_percent: u32,
    pub max_hold_time_minutes: u32,
    
    pub min_liquidity_sol: u32,
    pub max_risk_level: u32,
    pub min_holders: u32,

    // Added default transaction parameters
    pub default_slippage_bps: u32,
    pub default_priority_fee_micro_lamports: u64,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Use TELEGRAM_ADMIN_USER_ID for authorized_users
        let authorized_user_id_str = env::var("TELEGRAM_ADMIN_USER_ID")
            .context("TELEGRAM_ADMIN_USER_ID not set in environment")?;
        
        // Parse the single admin ID into a Vec<i64>
        let authorized_users = authorized_user_id_str
            .trim()
            .parse::<i64>()
            .map(|id| vec![id]) // Put the single ID into a vector
            .context("Failed to parse TELEGRAM_ADMIN_USER_ID as integer")?;
        
        Ok(Self {
            solana_rpc_url: env::var("SOLANA_RPC_URL")
                .context("SOLANA_RPC_URL not set in environment")?,
            solana_ws_url: env::var("SOLANA_WS_URL")
                .context("SOLANA_WS_URL not set in environment")?,
            solana_private_key: env::var("WALLET_PRIVATE_KEY") // Corrected env var name
                .context("WALLET_PRIVATE_KEY not set in environment")?,
            network: env::var("NETWORK").unwrap_or_else(|_| "testnet".to_string()), // Default to testnet based on RPC URL
            
            helius_api_key: env::var("HELIUS_API_KEY")
                .context("HELIUS_API_KEY not set in environment")?,
            jupiter_api_key: env::var("JUPITER_API_KEY").ok(), // Optional
            birdeye_api_key: env::var("BIRDEYE_API_KEY").ok(), // Optional
            
            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN")
                .context("TELEGRAM_BOT_TOKEN not set in environment")?,
            authorized_users, // Use the parsed admin ID vector
            
            // Default values from environment or hardcoded fallbacks
            demo_mode: env::var("DEMO_MODE")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(true), // Default to demo mode true
            max_position_size_sol: env::var("MAX_POSITION_SIZE_SOL")
                .unwrap_or_else(|_| "0.01".to_string())
                .parse()
                .unwrap_or(0.01),
            total_budget_sol: env::var("TOTAL_BUDGET_SOL")
                .unwrap_or_else(|_| "0.1".to_string())
                .parse()
                .unwrap_or(0.1),
            default_stop_loss_percent: env::var("DEFAULT_STOP_LOSS_PERCENT")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            default_take_profit_percent: env::var("DEFAULT_TAKE_PROFIT_PERCENT")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            default_trailing_stop_percent: env::var("DEFAULT_TRAILING_STOP_PERCENT")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            max_hold_time_minutes: env::var("MAX_HOLD_TIME_MINUTES")
                .unwrap_or_else(|_| "240".to_string())
                .parse()
                .unwrap_or(240),
            
            min_liquidity_sol: env::var("MIN_LIQUIDITY_SOL")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            max_risk_level: env::var("MAX_RISK_LEVEL")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            min_holders: env::var("MIN_HOLDERS")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),

            // Load new default transaction parameters
            default_slippage_bps: env::var("DEFAULT_SLIPPAGE_BPS")
                .unwrap_or_else(|_| "100".to_string()) // Default 1%
                .parse()
                .context("Failed to parse DEFAULT_SLIPPAGE_BPS")?,
            default_priority_fee_micro_lamports: env::var("DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS")
                .unwrap_or_else(|_| "50000".to_string()) // Default 50k
                .parse()
                .context("Failed to parse DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS")?,
        })
    }
}
