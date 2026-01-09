use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a simulated position in DRY_RUN_MODE
/// Tracks what the bot WOULD have bought and how it's performing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedPosition {
    pub id: String,
    pub token_address: String,
    pub token_symbol: String,
    pub token_name: String,

    // Entry details
    pub entry_price_sol: f64,
    pub entry_amount_sol: f64,  // How much SOL we "would have spent"
    pub token_amount: f64,      // How many tokens we "would have received"
    pub entry_time: DateTime<Utc>,

    // Current tracking
    pub current_price_sol: f64,
    pub current_value_sol: f64,
    pub unrealized_pnl_sol: f64,
    pub unrealized_pnl_percent: f64,

    // Why we picked it
    pub risk_score: u32,
    pub risk_details: Vec<String>,
    pub selection_reason: String,
    pub strategy_id: String,

    // Status
    pub status: SimulatedPositionStatus,
    pub exit_price_sol: Option<f64>,
    pub exit_time: Option<DateTime<Utc>>,
    pub realized_pnl_sol: Option<f64>,
    pub realized_pnl_percent: Option<f64>,
    pub exit_reason: Option<String>,

    // Tracking highest price for trailing stop simulation
    pub highest_price_sol: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimulatedPositionStatus {
    Open,
    ClosedTakeProfit,
    ClosedStopLoss,
    ClosedTrailingStop,
    ClosedMaxHoldTime,
    ClosedManual,
}

impl std::fmt::Display for SimulatedPositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::ClosedTakeProfit => write!(f, "Closed (Take Profit)"),
            Self::ClosedStopLoss => write!(f, "Closed (Stop Loss)"),
            Self::ClosedTrailingStop => write!(f, "Closed (Trailing Stop)"),
            Self::ClosedMaxHoldTime => write!(f, "Closed (Max Hold Time)"),
            Self::ClosedManual => write!(f, "Closed (Manual)"),
        }
    }
}

impl SimulatedPosition {
    /// Create a new simulated position
    pub fn new(
        token_address: String,
        token_symbol: String,
        token_name: String,
        entry_price_sol: f64,
        entry_amount_sol: f64,
        risk_score: u32,
        risk_details: Vec<String>,
        selection_reason: String,
        strategy_id: String,
    ) -> Self {
        let token_amount = if entry_price_sol > 0.0 {
            entry_amount_sol / entry_price_sol
        } else {
            0.0
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            token_address,
            token_symbol,
            token_name,
            entry_price_sol,
            entry_amount_sol,
            token_amount,
            entry_time: Utc::now(),
            current_price_sol: entry_price_sol,
            current_value_sol: entry_amount_sol,
            unrealized_pnl_sol: 0.0,
            unrealized_pnl_percent: 0.0,
            risk_score,
            risk_details,
            selection_reason,
            strategy_id,
            status: SimulatedPositionStatus::Open,
            exit_price_sol: None,
            exit_time: None,
            realized_pnl_sol: None,
            realized_pnl_percent: None,
            exit_reason: None,
            highest_price_sol: entry_price_sol,
        }
    }

    /// Update the current price and recalculate P&L
    pub fn update_price(&mut self, new_price: f64) {
        self.current_price_sol = new_price;

        // Update highest price for trailing stop
        if new_price > self.highest_price_sol {
            self.highest_price_sol = new_price;
        }

        // Calculate current value
        self.current_value_sol = self.token_amount * new_price;

        // Calculate unrealized P&L
        self.unrealized_pnl_sol = self.current_value_sol - self.entry_amount_sol;
        self.unrealized_pnl_percent = if self.entry_amount_sol > 0.0 {
            (self.unrealized_pnl_sol / self.entry_amount_sol) * 100.0
        } else {
            0.0
        };
    }

    /// Close the position with a given reason
    pub fn close(&mut self, exit_price: f64, status: SimulatedPositionStatus, reason: String) {
        self.exit_price_sol = Some(exit_price);
        self.exit_time = Some(Utc::now());
        self.status = status;
        self.exit_reason = Some(reason);

        // Calculate realized P&L
        let exit_value = self.token_amount * exit_price;
        self.realized_pnl_sol = Some(exit_value - self.entry_amount_sol);
        self.realized_pnl_percent = if self.entry_amount_sol > 0.0 {
            Some((self.realized_pnl_sol.unwrap() / self.entry_amount_sol) * 100.0)
        } else {
            Some(0.0)
        };
    }

    /// Check if position is still open
    pub fn is_open(&self) -> bool {
        self.status == SimulatedPositionStatus::Open
    }
}

/// Statistics for simulation performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationStats {
    pub total_simulated_trades: u32,
    pub open_positions: u32,
    pub closed_positions: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub total_realized_pnl_sol: f64,
    pub total_unrealized_pnl_sol: f64,
    pub win_rate: f64,
    pub would_have_spent_sol: f64,
    pub would_have_returned_sol: f64,
    pub average_pnl_percent: f64,
    pub best_trade_pnl_percent: f64,
    pub worst_trade_pnl_percent: f64,
}

impl Default for SimulationStats {
    fn default() -> Self {
        Self {
            total_simulated_trades: 0,
            open_positions: 0,
            closed_positions: 0,
            winning_trades: 0,
            losing_trades: 0,
            total_realized_pnl_sol: 0.0,
            total_unrealized_pnl_sol: 0.0,
            win_rate: 0.0,
            would_have_spent_sol: 0.0,
            would_have_returned_sol: 0.0,
            average_pnl_percent: 0.0,
            best_trade_pnl_percent: 0.0,
            worst_trade_pnl_percent: 0.0,
        }
    }
}
