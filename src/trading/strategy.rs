use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: String,                          // Unique strategy ID (UUID)
    pub name: String,                        // User-defined strategy name
    pub enabled: bool,                       // Whether strategy is active for trading
    
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
            slippage_bps: None, // Use global default
            priority_fee_micro_lamports: None, // Use global default
            created_at: now,
            updated_at: now,
        }
    }

    // Call this when updating strategy parameters
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}
