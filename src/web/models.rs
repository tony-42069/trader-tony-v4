//! Request and Response DTOs for the Web API

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ============================================================================
// Health & Status
// ============================================================================

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AutoTraderStatus {
    pub running: bool,
    pub demo_mode: bool,
    pub active_strategies: usize,
    pub active_positions: usize,
}

// ============================================================================
// Wallet
// ============================================================================

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub address: String,
    pub balance_sol: f64,
}

// ============================================================================
// Positions
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PositionResponse {
    pub id: String,
    pub token_address: String,
    pub token_name: String,
    pub token_symbol: String,
    pub strategy_id: String,
    pub entry_value_sol: f64,
    pub current_value_sol: Option<f64>,
    pub token_amount: f64,
    pub entry_price: f64,
    pub current_price: Option<f64>,
    pub pnl_percent: Option<f64>,
    pub pnl_sol: Option<f64>,
    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub exit_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PositionsListResponse {
    pub positions: Vec<PositionResponse>,
    pub total: usize,
}

// ============================================================================
// Trades
// ============================================================================

#[derive(Debug, Serialize)]
pub struct TradeResponse {
    pub id: String,
    pub token_address: String,
    pub token_symbol: String,
    pub action: String, // "buy" or "sell"
    pub amount_sol: f64,
    pub token_amount: f64,
    pub price: f64,
    pub pnl_sol: Option<f64>,
    pub pnl_percent: Option<f64>,
    pub transaction_signature: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct TradesQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TradesListResponse {
    pub trades: Vec<TradeResponse>,
    pub total: usize,
    pub page: u32,
    pub limit: u32,
}

// ============================================================================
// Statistics
// ============================================================================

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: f64,
    pub total_pnl_sol: f64,
    pub avg_roi_percent: f64,
    pub total_volume_sol: f64,
    pub best_trade_pnl: f64,
    pub worst_trade_pnl: f64,
}

// ============================================================================
// Strategies
// ============================================================================

#[derive(Debug, Serialize)]
pub struct StrategyResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub max_concurrent_positions: u32,
    pub max_position_size_sol: f64,
    pub total_budget_sol: f64,
    pub stop_loss_percent: Option<u32>,
    pub take_profit_percent: Option<u32>,
    pub trailing_stop_percent: Option<u32>,
    pub max_hold_time_minutes: u32,
    pub min_liquidity_sol: u32,
    pub max_risk_level: u32,
    pub min_holders: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStrategyRequest {
    pub name: String,
    pub max_concurrent_positions: Option<u32>,
    pub max_position_size_sol: Option<f64>,
    pub total_budget_sol: Option<f64>,
    pub stop_loss_percent: Option<u32>,
    pub take_profit_percent: Option<u32>,
    pub trailing_stop_percent: Option<u32>,
    pub max_hold_time_minutes: Option<u32>,
    pub min_liquidity_sol: Option<u32>,
    pub max_risk_level: Option<u32>,
    pub min_holders: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStrategyRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub max_concurrent_positions: Option<u32>,
    pub max_position_size_sol: Option<f64>,
    pub total_budget_sol: Option<f64>,
    pub stop_loss_percent: Option<u32>,
    pub take_profit_percent: Option<u32>,
    pub trailing_stop_percent: Option<u32>,
    pub max_hold_time_minutes: Option<u32>,
    pub min_liquidity_sol: Option<u32>,
    pub max_risk_level: Option<u32>,
    pub min_holders: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct StrategiesListResponse {
    pub strategies: Vec<StrategyResponse>,
    pub total: usize,
}

// ============================================================================
// Token Analysis
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub token_address: String,
    pub risk_level: u32,
    pub risk_rating: String, // "Low", "Medium", "High", "Very High"
    pub liquidity_sol: f64,
    pub holder_count: u32,
    pub has_mint_authority: bool,
    pub has_freeze_authority: bool,
    pub lp_tokens_burned: bool,
    pub transfer_tax_percent: f64,
    pub can_sell: bool,
    pub concentration_percent: f64,
    pub details: Vec<String>,
    pub recommendation: String,
}

// ============================================================================
// Generic Responses
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

// ============================================================================
// Copy Trade
// ============================================================================

/// Response for trade signals endpoint
#[derive(Debug, Serialize)]
pub struct SignalResponse {
    pub id: String,
    pub token_address: String,
    pub token_symbol: String,
    pub token_name: String,
    pub action: String,
    pub amount_sol: f64,
    pub price_sol: f64,
    pub timestamp: DateTime<Utc>,
    pub bot_position_id: String,
    pub is_active: bool,
    pub current_price_sol: Option<f64>,
    pub current_pnl_percent: Option<f64>,
}

/// Response for signals list
#[derive(Debug, Serialize)]
pub struct SignalsListResponse {
    pub signals: Vec<SignalResponse>,
    pub total: usize,
}

/// Request to register for copy trading
#[derive(Debug, Deserialize)]
pub struct CopyTradeRegisterRequest {
    pub wallet_address: String,
    pub signature: String,
    pub message: String,
}

/// Request to update copy trade settings
#[derive(Debug, Deserialize)]
pub struct CopyTradeSettingsRequest {
    pub auto_copy_enabled: Option<bool>,
    pub copy_amount_sol: Option<f64>,
    pub max_positions: Option<u32>,
    pub slippage_bps: Option<u32>,
}

/// Response for copy trade status
#[derive(Debug, Serialize)]
pub struct CopyTradeStatusResponse {
    pub is_registered: bool,
    pub wallet_address: Option<String>,
    pub auto_copy_enabled: bool,
    pub copy_amount_sol: f64,
    pub max_positions: u32,
    pub slippage_bps: u32,
    pub total_copy_trades: u32,
    pub active_copy_positions: usize,
    pub total_fees_paid_sol: f64,
}

/// Request to build a copy trade transaction
#[derive(Debug, Deserialize)]
pub struct BuildCopyTxRequest {
    pub user_wallet: String,
    pub signal_id: String,
    pub amount_sol: Option<f64>,
    pub copy_position_id: Option<String>,
    pub slippage_bps: Option<u32>,
}

/// Response with built transaction
#[derive(Debug, Serialize)]
pub struct BuildCopyTxResponse {
    pub success: bool,
    pub transaction: Option<String>,
    pub error: Option<String>,
    pub estimated_output: Option<f64>,
    pub estimated_fee: Option<f64>,
    pub estimated_pnl: Option<f64>,
}

/// Response for copy position
#[derive(Debug, Serialize)]
pub struct CopyPositionResponse {
    pub id: String,
    pub copier_wallet: String,
    pub token_address: String,
    pub token_symbol: String,
    pub entry_price_sol: f64,
    pub entry_amount_sol: f64,
    pub token_amount: f64,
    pub bot_position_id: String,
    pub status: String,
    pub current_price_sol: Option<f64>,
    pub current_pnl_percent: Option<f64>,
    pub pnl_sol: Option<f64>,
    pub fee_paid_sol: Option<f64>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Response for copy positions list
#[derive(Debug, Serialize)]
pub struct CopyPositionsListResponse {
    pub positions: Vec<CopyPositionResponse>,
    pub total: usize,
}

/// Query params for copy positions
#[derive(Debug, Deserialize)]
pub struct CopyPositionsQuery {
    pub wallet: String,
    pub status: Option<String>,
}

/// Response for copy trade stats
#[derive(Debug, Serialize)]
pub struct CopyTradeStatsResponse {
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: f64,
    pub total_pnl_sol: f64,
    pub total_fees_paid_sol: f64,
    pub avg_pnl_percent: f64,
    pub best_trade_pnl_sol: f64,
    pub worst_trade_pnl_sol: f64,
}
