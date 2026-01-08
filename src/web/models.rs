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
