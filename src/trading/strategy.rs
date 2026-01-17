use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Strategy type determines which discovery/evaluation method is used
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    /// Sniper - catches tokens at creation (0% progress, milliseconds old)
    /// Uses WebSocket CreateEvent monitoring
    #[default]
    NewPairs,
    /// Bonding curve with traction - tokens still on pump.fun but with activity
    /// Uses periodic scanner with Birdeye data
    FinalStretch,
    /// Graduated to PumpSwap/Raydium - tokens that completed bonding curve
    /// Uses periodic scanner with Birdeye data
    Migrated,
}

impl StrategyType {
    pub fn display_name(&self) -> &'static str {
        match self {
            StrategyType::NewPairs => "New Pairs",
            StrategyType::FinalStretch => "Final Stretch",
            StrategyType::Migrated => "Migrated",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            StrategyType::NewPairs => "Sniper - catches tokens within milliseconds of creation",
            StrategyType::FinalStretch => "Tokens on bonding curve with proven traction (20-80% progress)",
            StrategyType::Migrated => "Tokens graduated to PumpSwap/Raydium with established liquidity",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: String,                          // Unique strategy ID (UUID)
    pub name: String,                        // User-defined strategy name
    pub enabled: bool,                       // Whether strategy is active for trading

    /// Strategy type determines discovery method (NewPairs, FinalStretch, Migrated)
    #[serde(default)]
    pub strategy_type: StrategyType,

    // Position Sizing & Budget
    pub max_concurrent_positions: u32,       // Max number of open positions for this strategy
    pub max_position_size_sol: f64,          // Max SOL value for a single position entry
    pub total_budget_sol: f64,               // Total SOL allocated to this strategy
    
    // Exit Conditions
    pub stop_loss_percent: Option<u32>,      // Stop loss percentage (optional)
    pub take_profit_percent: Option<u32>,    // Take profit percentage (optional)
    pub trailing_stop_percent: Option<u32>,  // Trailing stop percentage (optional)
    pub max_hold_time_minutes: u32,          // Max time to hold a position before forced exit
    
    // Entry Filters (Token Selection Criteria)
    pub min_liquidity_sol: u32,              // Minimum liquidity required in SOL
    pub max_risk_level: u32,                 // Maximum acceptable risk score (0-100) from RiskAnalyzer
    pub min_holders: u32,                    // Minimum number of token holders
    pub max_token_age_minutes: u32,          // Maximum age of token since creation
    // Add more specific risk filters based on RiskAnalysis fields
    pub require_lp_burned: bool,             // Require LP tokens to be burned/locked
    pub reject_if_mint_authority: bool,      // Reject if mint authority exists
    pub reject_if_freeze_authority: bool,    // Reject if freeze authority exists
    pub require_can_sell: bool,              // Require passing the sellability (honeypot) check
    pub max_transfer_tax_percent: Option<f64>, // Maximum acceptable transfer tax (None means no check)
    pub max_concentration_percent: Option<f64>, // Maximum acceptable top holder concentration (None means no check)

    // Final Stretch / Migrated Strategy Criteria (from Birdeye API)
    pub min_volume_usd: Option<f64>,         // Minimum 24h volume in USD (e.g., 20000.0 for $20k)
    pub min_market_cap_usd: Option<f64>,     // Minimum market cap in USD (e.g., 20000.0 for $20k)
    pub min_bonding_progress: Option<f64>,   // Minimum bonding curve progress % (0-100, e.g., 20.0)
    pub require_migrated: Option<bool>,      // TRUE = must be migrated, FALSE = must NOT be migrated, None = don't check

    // Transaction Parameters (Optional overrides for config defaults)
    pub slippage_bps: Option<u32>,           // Slippage basis points for swaps (overrides config)
    pub priority_fee_micro_lamports: Option<u64>, // Priority fee for swaps (overrides config)

    // Metadata
    pub created_at: DateTime<Utc>,           // Strategy creation time
    pub updated_at: DateTime<Utc>,           // Strategy last update time
}

impl Strategy {
    // Provides sensible defaults for a new strategy
    pub fn default(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            enabled: true,
            strategy_type: StrategyType::NewPairs, // Default to sniper
            max_concurrent_positions: 3,
            max_position_size_sol: 0.05, // Default smaller size
            total_budget_sol: 0.2,      // Default smaller budget
            stop_loss_percent: Some(15), // Default 15% SL
            take_profit_percent: Some(50), // Default 50% TP
            trailing_stop_percent: Some(5), // Default 5% Trailing SL
            max_hold_time_minutes: 240, // 4 hours
            min_liquidity_sol: 10,      // Min 10 SOL liquidity
            max_risk_level: 60,         // Max risk score 60
            min_holders: 50,            // Min 50 holders
            max_token_age_minutes: 120, // Max 2 hours old
            require_lp_burned: true,
            reject_if_mint_authority: true,
            reject_if_freeze_authority: true,
            require_can_sell: true,
            max_transfer_tax_percent: Some(5.0), // Reject if tax > 5%
            max_concentration_percent: Some(60.0), // Reject if concentration > 60%
            // Final Stretch / Migrated criteria (None = not applicable for NewPairs)
            min_volume_usd: None,
            min_market_cap_usd: None,
            min_bonding_progress: None,
            require_migrated: None,
            slippage_bps: None, // Use global default
            priority_fee_micro_lamports: None, // Use global default
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a Final Stretch strategy with recommended defaults
    pub fn final_stretch(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            enabled: true,
            strategy_type: StrategyType::FinalStretch,
            max_concurrent_positions: 5,
            max_position_size_sol: 0.1,
            total_budget_sol: 1.0,
            stop_loss_percent: Some(20),
            take_profit_percent: Some(50),
            trailing_stop_percent: Some(10),
            max_hold_time_minutes: 60,
            min_liquidity_sol: 1,       // Virtual liquidity for bonding curve
            max_risk_level: 70,
            min_holders: 50,            // Minimum 50 holders
            max_token_age_minutes: 60,  // 0-60 minutes old
            require_lp_burned: false,   // N/A for bonding curve
            reject_if_mint_authority: false,
            reject_if_freeze_authority: false,
            require_can_sell: false,
            max_transfer_tax_percent: None,
            max_concentration_percent: None,
            // Final Stretch specific criteria
            min_volume_usd: Some(20_000.0),      // $20k minimum volume
            min_market_cap_usd: Some(20_000.0),  // $20k minimum market cap
            min_bonding_progress: Some(20.0),    // 20% minimum progress
            require_migrated: Some(false),       // Must NOT be migrated
            slippage_bps: None,
            priority_fee_micro_lamports: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a Migrated strategy with recommended defaults
    pub fn migrated(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            enabled: true,
            strategy_type: StrategyType::Migrated,
            max_concurrent_positions: 5,
            max_position_size_sol: 0.1,
            total_budget_sol: 1.0,
            stop_loss_percent: Some(15),
            take_profit_percent: Some(40),
            trailing_stop_percent: Some(8),
            max_hold_time_minutes: 1440, // 24 hours
            min_liquidity_sol: 10,       // Real DEX liquidity
            max_risk_level: 50,          // Lower risk tolerance for established tokens
            min_holders: 75,             // Minimum 75 holders
            max_token_age_minutes: 1440, // 0-24 hours old
            require_lp_burned: false,
            reject_if_mint_authority: true,
            reject_if_freeze_authority: true,
            require_can_sell: true,
            max_transfer_tax_percent: Some(5.0),
            max_concentration_percent: Some(50.0),
            // Migrated specific criteria
            min_volume_usd: Some(40_000.0),      // $40k minimum volume
            min_market_cap_usd: Some(40_000.0),  // $40k minimum market cap
            min_bonding_progress: None,          // N/A - already graduated
            require_migrated: Some(true),        // Must BE migrated
            slippage_bps: None,
            priority_fee_micro_lamports: None,
            created_at: now,
            updated_at: now,
        }
    }

    // Call this when updating strategy parameters
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
    
    // Create a basic strategy with more conservative parameters
    pub fn conservative(name: &str) -> Self {
        let mut strategy = Self::default(name);
        strategy.strategy_type = StrategyType::NewPairs;
        strategy.max_position_size_sol = 0.01;
        strategy.total_budget_sol = 0.1;
        strategy.max_risk_level = 30;
        strategy.min_liquidity_sol = 20;
        strategy.min_holders = 100;
        strategy.stop_loss_percent = Some(10);
        strategy.take_profit_percent = Some(30);
        strategy.trailing_stop_percent = Some(3);
        strategy
    }

    // Create a basic strategy with more aggressive parameters
    pub fn aggressive(name: &str) -> Self {
        let mut strategy = Self::default(name);
        strategy.strategy_type = StrategyType::NewPairs;
        strategy.max_position_size_sol = 0.1;
        strategy.total_budget_sol = 0.5;
        strategy.max_risk_level = 75;
        strategy.min_liquidity_sol = 5;
        strategy.min_holders = 30;
        strategy.stop_loss_percent = Some(20);
        strategy.take_profit_percent = Some(100);
        strategy.trailing_stop_percent = Some(10);
        strategy
    }
    
    // Validates the strategy parameters to ensure they're coherent
    pub fn validate(&self) -> Result<(), String> {
        // Check for logical parameter relationships
        if self.max_position_size_sol <= 0.0 {
            return Err("Maximum position size must be greater than 0".to_string());
        }
        
        if self.total_budget_sol <= 0.0 {
            return Err("Total budget must be greater than 0".to_string());
        }
        
        if self.max_position_size_sol > self.total_budget_sol {
            return Err("Maximum position size cannot be greater than total budget".to_string());
        }
        
        if self.max_concurrent_positions == 0 {
            return Err("Maximum concurrent positions must be at least 1".to_string());
        }
        
        // All conditions met
        Ok(())
    }
}

// Utility functions for strategy persistence (independent of AutoTrader)
pub mod persistence {
    use super::*;
    use anyhow::{Context, Result};
    use serde_json;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use tokio::fs;
    use tracing::{debug, error, info, warn};

    const DEFAULT_STRATEGIES_FILENAME: &str = "strategies.json";
    
    // Get the default path to the strategies file
    pub fn get_default_strategies_path() -> PathBuf {
        Path::new("data").join(DEFAULT_STRATEGIES_FILENAME)
    }
    
    // Load strategies from a JSON file
    pub async fn load_strategies(file_path: &Path) -> Result<HashMap<String, Strategy>> {
        // Ensure the data directory exists
        if let Some(dir) = file_path.parent() {
            if !dir.exists() {
                info!("Data directory not found, creating at: {:?}", dir);
                fs::create_dir_all(dir).await.context("Failed to create data directory")?;
            }
        }
        
        // Check if the strategies file exists
        if !file_path.exists() {
            info!("Strategies file not found at {:?}, starting with an empty strategy set.", file_path);
            return Ok(HashMap::new());
        }
        
        info!("Loading strategies from {:?}...", file_path);
        let data = match fs::read_to_string(file_path).await {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("Strategies file not found (race condition?), starting with an empty strategy set.");
                return Ok(HashMap::new());
            }
            Err(e) => {
                return Err(e).context(format!("Failed to read strategies file: {:?}", file_path));
            }
        };
        
        if data.trim().is_empty() {
            info!("Strategies file is empty, using an empty strategy set.");
            return Ok(HashMap::new());
        }
        
        // Deserialize from JSON into a Vec<Strategy>
        let loaded_strategies: Vec<Strategy> = match serde_json::from_str(&data) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to deserialize strategies from {:?}: {}. Using an empty strategy set.", file_path, e);
                // Optionally back up the corrupted file
                let backup_path = file_path.with_extension("json.bak");
                if let Err(backup_err) = fs::copy(file_path, &backup_path).await {
                    warn!("Failed to create backup of corrupted strategies file: {}", backup_err);
                } else {
                    info!("Created backup of corrupted strategies file at {:?}", backup_path);
                }
                return Ok(HashMap::new());
            }
        };
        
        // Convert to HashMap for easy lookup
        let mut strategies_map = HashMap::new();
        for strategy in loaded_strategies {
            strategies_map.insert(strategy.id.clone(), strategy);
        }
        
        info!("Loaded {} strategies from file", strategies_map.len());
        Ok(strategies_map)
    }
    
    // Save strategies to a JSON file
    pub async fn save_strategies(strategies: &HashMap<String, Strategy>, file_path: &Path) -> Result<()> {
        debug!("Saving strategies to {:?}...", file_path);
        
        // Collect all strategies into a Vec for serialization
        let strategies_vec: Vec<&Strategy> = strategies.values().collect();
        
        // Ensure the directory exists
        if let Some(dir) = file_path.parent() {
            fs::create_dir_all(dir).await.context("Failed to create data directory")?;
        }
        
        // Serialize strategies to JSON string
        let data = serde_json::to_string_pretty(&strategies_vec)
            .context("Failed to serialize strategies")?;
        
        // Write data to the file atomically
        let temp_path = file_path.with_extension("json.tmp");
        fs::write(&temp_path, data).await
            .context(format!("Failed to write temporary strategies file: {:?}", temp_path))?;
        fs::rename(&temp_path, file_path).await
            .context(format!("Failed to rename temporary strategies file to {:?}", file_path))?;
        
        debug!("Saved {} strategies to file: {:?}", strategies_vec.len(), file_path);
        Ok(())
    }
    
}
