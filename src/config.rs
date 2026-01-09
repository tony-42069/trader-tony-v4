use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    // Solana Configuration
    pub solana_rpc_url: String,
    pub solana_ws_url: String,
    pub solana_private_key: String,
    pub network: String,

    // API Keys
    pub helius_api_key: String,
    pub jupiter_api_key: Option<String>,
    pub birdeye_api_key: Option<String>,

    // Web API Configuration
    pub api_host: Option<String>,
    pub api_port: Option<u16>,
    pub cors_origins: Vec<String>,
    pub auto_start_trading: bool,

    // Copy Trade Configuration
    pub treasury_wallet: Option<String>,
    pub copy_trade_fee_percent: f64,

    // Trading Configuration
    pub demo_mode: bool,
    pub dry_run_mode: bool,  // Scans real tokens, simulates trades without execution
    pub max_position_size_sol: f64,
    pub total_budget_sol: f64,
    pub default_stop_loss_percent: u32,
    pub default_take_profit_percent: u32,
    pub default_trailing_stop_percent: u32,
    pub max_hold_time_minutes: u32,

    // Risk Parameters
    pub min_liquidity_sol: u32,
    pub max_risk_level: u32,
    pub min_holders: u32,

    // Transaction Parameters
    pub default_slippage_bps: u32,
    pub default_priority_fee_micro_lamports: u64,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Parse CORS origins from comma-separated string
        let cors_origins: Vec<String> = env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "*".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            // Solana Configuration
            solana_rpc_url: env::var("SOLANA_RPC_URL")
                .context("SOLANA_RPC_URL not set in environment")?,
            solana_ws_url: env::var("SOLANA_WS_URL")
                .unwrap_or_else(|_| {
                    // Derive WebSocket URL from RPC URL if not provided
                    let rpc = env::var("SOLANA_RPC_URL").unwrap_or_default();
                    rpc.replace("https://", "wss://").replace("http://", "ws://")
                }),
            solana_private_key: env::var("WALLET_PRIVATE_KEY")
                .or_else(|_| env::var("SOLANA_PRIVATE_KEY"))
                .context("WALLET_PRIVATE_KEY or SOLANA_PRIVATE_KEY not set in environment")?,
            network: env::var("NETWORK").unwrap_or_else(|_| "mainnet".to_string()),

            // API Keys
            helius_api_key: env::var("HELIUS_API_KEY")
                .context("HELIUS_API_KEY not set in environment")?,
            jupiter_api_key: env::var("JUPITER_API_KEY").ok(),
            birdeye_api_key: env::var("BIRDEYE_API_KEY").ok(),

            // Web API Configuration
            api_host: env::var("API_HOST").ok(),
            api_port: env::var("API_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .or_else(|| env::var("PORT").ok().and_then(|v| v.parse().ok())), // Railway uses PORT
            cors_origins,
            auto_start_trading: env::var("AUTO_START_TRADING")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),

            // Copy Trade Configuration
            treasury_wallet: env::var("TREASURY_WALLET").ok(),
            copy_trade_fee_percent: env::var("COPY_TRADE_FEE_PERCENT")
                .unwrap_or_else(|_| "10.0".to_string())
                .parse()
                .unwrap_or(10.0),

            // Trading Configuration
            demo_mode: env::var("DEMO_MODE")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(true), // Default to demo mode
            dry_run_mode: env::var("DRY_RUN_MODE")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false), // Default to false
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

            // Risk Parameters
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

            // Transaction Parameters
            default_slippage_bps: env::var("DEFAULT_SLIPPAGE_BPS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .context("Failed to parse DEFAULT_SLIPPAGE_BPS")?,
            default_priority_fee_micro_lamports: env::var("DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS")
                .unwrap_or_else(|_| "50000".to_string())
                .parse()
                .context("Failed to parse DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS")?,
        })
    }
}
