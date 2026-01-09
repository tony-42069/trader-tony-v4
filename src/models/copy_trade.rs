use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a trade signal that can be copied by users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    /// Unique signal ID
    pub id: String,
    /// Token mint address
    pub token_address: String,
    /// Token symbol (e.g., "BONK")
    pub token_symbol: String,
    /// Token name
    pub token_name: String,
    /// Trade action (Buy or Sell)
    pub action: TradeAction,
    /// Amount in SOL for the trade
    pub amount_sol: f64,
    /// Price in SOL per token at signal time
    pub price_sol: f64,
    /// When the signal was created
    pub timestamp: DateTime<Utc>,
    /// Reference to the bot's position ID
    pub bot_position_id: String,
    /// Whether this signal is still active (position is open)
    pub is_active: bool,
    /// Current price (updated for active positions)
    pub current_price_sol: Option<f64>,
    /// Current PnL percentage (for active positions)
    pub current_pnl_percent: Option<f64>,
}

impl TradeSignal {
    /// Create a new buy signal from a position
    pub fn new_buy(
        token_address: &str,
        token_symbol: &str,
        token_name: &str,
        amount_sol: f64,
        price_sol: f64,
        bot_position_id: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            token_name: token_name.to_string(),
            action: TradeAction::Buy,
            amount_sol,
            price_sol,
            timestamp: Utc::now(),
            bot_position_id: bot_position_id.to_string(),
            is_active: true,
            current_price_sol: Some(price_sol),
            current_pnl_percent: Some(0.0),
        }
    }

    /// Create a new sell signal from a position
    pub fn new_sell(
        token_address: &str,
        token_symbol: &str,
        token_name: &str,
        amount_sol: f64,
        price_sol: f64,
        pnl_percent: f64,
        bot_position_id: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            token_name: token_name.to_string(),
            action: TradeAction::Sell,
            amount_sol,
            price_sol,
            timestamp: Utc::now(),
            bot_position_id: bot_position_id.to_string(),
            is_active: false, // Sell signals are immediately inactive
            current_price_sol: Some(price_sol),
            current_pnl_percent: Some(pnl_percent),
        }
    }
}

/// Trade action type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TradeAction {
    Buy,
    Sell,
}

impl std::fmt::Display for TradeAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => write!(f, "buy"),
            Self::Sell => write!(f, "sell"),
        }
    }
}

/// A registered copy trader (user who wants to copy trades)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTrader {
    /// User's wallet address (Solana public key)
    pub wallet_address: String,
    /// When the user registered
    pub registered_at: DateTime<Utc>,
    /// Whether auto-copy is enabled
    pub auto_copy_enabled: bool,
    /// How much SOL to use per copy trade
    pub copy_amount_sol: f64,
    /// Maximum number of concurrent positions
    pub max_positions: u32,
    /// Slippage tolerance in basis points
    pub slippage_bps: u32,
    /// Whether the user's registration is verified (signed message)
    pub is_verified: bool,
    /// Last activity timestamp
    pub last_active: DateTime<Utc>,
    /// Total number of copy trades executed
    pub total_copy_trades: u32,
    /// Total fees paid in SOL
    pub total_fees_paid_sol: f64,
}

impl CopyTrader {
    pub fn new(wallet_address: &str, copy_amount_sol: f64) -> Self {
        let now = Utc::now();
        Self {
            wallet_address: wallet_address.to_string(),
            registered_at: now,
            auto_copy_enabled: false, // Disabled by default, user must explicitly enable
            copy_amount_sol,
            max_positions: 5,      // Default max positions
            slippage_bps: 300,     // Default 3% slippage
            is_verified: false,
            last_active: now,
            total_copy_trades: 0,
            total_fees_paid_sol: 0.0,
        }
    }
}

/// Status of a copy position
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CopyPositionStatus {
    /// Position is open
    Open,
    /// Position is being closed
    Closing,
    /// Position was successfully closed
    Closed,
    /// Position close failed
    Failed,
}

impl std::fmt::Display for CopyPositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Closing => write!(f, "closing"),
            Self::Closed => write!(f, "closed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// A copy position (user's position that mirrors the bot's position)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyPosition {
    /// Unique ID for this copy position
    pub id: String,
    /// The copier's wallet address
    pub copier_wallet: String,
    /// Token mint address
    pub token_address: String,
    /// Token symbol
    pub token_symbol: String,
    /// Entry price in SOL per token
    pub entry_price_sol: f64,
    /// Entry amount in SOL
    pub entry_amount_sol: f64,
    /// Amount of tokens held
    pub token_amount: f64,
    /// Reference to the bot's position ID being copied
    pub bot_position_id: String,
    /// Reference to the buy signal ID
    pub buy_signal_id: String,
    /// Current position status
    pub status: CopyPositionStatus,
    /// Entry transaction signature
    pub entry_tx_signature: String,
    /// Exit transaction signature (if closed)
    pub exit_tx_signature: Option<String>,
    /// Exit price in SOL per token (if closed)
    pub exit_price_sol: Option<f64>,
    /// Exit amount received in SOL (if closed)
    pub exit_amount_sol: Option<f64>,
    /// Profit/Loss in SOL (if closed)
    pub pnl_sol: Option<f64>,
    /// Profit/Loss percentage (if closed)
    pub pnl_percent: Option<f64>,
    /// Fee paid to treasury (if profitable sell)
    pub fee_paid_sol: Option<f64>,
    /// When the position was opened
    pub opened_at: DateTime<Utc>,
    /// When the position was closed (if closed)
    pub closed_at: Option<DateTime<Utc>>,
}

impl CopyPosition {
    pub fn new(
        copier_wallet: &str,
        token_address: &str,
        token_symbol: &str,
        entry_price_sol: f64,
        entry_amount_sol: f64,
        token_amount: f64,
        bot_position_id: &str,
        buy_signal_id: &str,
        entry_tx_signature: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            copier_wallet: copier_wallet.to_string(),
            token_address: token_address.to_string(),
            token_symbol: token_symbol.to_string(),
            entry_price_sol,
            entry_amount_sol,
            token_amount,
            bot_position_id: bot_position_id.to_string(),
            buy_signal_id: buy_signal_id.to_string(),
            status: CopyPositionStatus::Open,
            entry_tx_signature: entry_tx_signature.to_string(),
            exit_tx_signature: None,
            exit_price_sol: None,
            exit_amount_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            fee_paid_sol: None,
            opened_at: Utc::now(),
            closed_at: None,
        }
    }

    /// Close the position with the given details
    pub fn close(
        &mut self,
        exit_price_sol: f64,
        exit_amount_sol: f64,
        fee_paid_sol: f64,
        exit_tx_signature: &str,
    ) {
        self.exit_price_sol = Some(exit_price_sol);
        self.exit_amount_sol = Some(exit_amount_sol);
        self.exit_tx_signature = Some(exit_tx_signature.to_string());
        self.closed_at = Some(Utc::now());
        self.status = CopyPositionStatus::Closed;

        // Calculate PnL
        let gross_pnl = exit_amount_sol - self.entry_amount_sol;
        let net_pnl = gross_pnl - fee_paid_sol;
        self.pnl_sol = Some(net_pnl);
        self.fee_paid_sol = Some(fee_paid_sol);

        if self.entry_amount_sol > 0.0 {
            self.pnl_percent = Some((net_pnl / self.entry_amount_sol) * 100.0);
        }
    }
}

/// Request to build a copy trade transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCopyTradeRequest {
    /// User's wallet address
    pub user_wallet: String,
    /// The signal to copy
    pub signal_id: String,
    /// Amount in SOL to trade (for buys)
    pub amount_sol: Option<f64>,
    /// For sells: the copy position ID to close
    pub copy_position_id: Option<String>,
    /// Slippage tolerance in basis points
    pub slippage_bps: Option<u32>,
}

/// Response with a built transaction ready to sign
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCopyTradeResponse {
    /// Success status
    pub success: bool,
    /// Base64-encoded serialized transaction (if success)
    pub transaction: Option<String>,
    /// Error message (if not success)
    pub error: Option<String>,
    /// Estimated output amount
    pub estimated_output: Option<f64>,
    /// Estimated fee (for sells)
    pub estimated_fee: Option<f64>,
    /// Estimated PnL (for sells)
    pub estimated_pnl: Option<f64>,
}

/// Copy trade settings for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTradeSettings {
    /// Whether auto-copy is enabled
    pub auto_copy_enabled: bool,
    /// Amount in SOL to use per copy trade
    pub copy_amount_sol: f64,
    /// Maximum number of concurrent copy positions
    pub max_positions: u32,
    /// Slippage tolerance in basis points (e.g., 300 = 3%)
    pub slippage_bps: u32,
}

impl Default for CopyTradeSettings {
    fn default() -> Self {
        Self {
            auto_copy_enabled: false,
            copy_amount_sol: 0.1,
            max_positions: 5,
            slippage_bps: 300,
        }
    }
}

/// Copy trade statistics for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTradeStats {
    /// Total number of copy trades
    pub total_trades: u32,
    /// Number of winning trades
    pub winning_trades: u32,
    /// Number of losing trades
    pub losing_trades: u32,
    /// Win rate percentage
    pub win_rate: f64,
    /// Total PnL in SOL (after fees)
    pub total_pnl_sol: f64,
    /// Total fees paid in SOL
    pub total_fees_paid_sol: f64,
    /// Average PnL percentage
    pub avg_pnl_percent: f64,
    /// Best trade PnL in SOL
    pub best_trade_pnl_sol: f64,
    /// Worst trade PnL in SOL
    pub worst_trade_pnl_sol: f64,
}

impl Default for CopyTradeStats {
    fn default() -> Self {
        Self {
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            win_rate: 0.0,
            total_pnl_sol: 0.0,
            total_fees_paid_sol: 0.0,
            avg_pnl_percent: 0.0,
            best_trade_pnl_sol: 0.0,
            worst_trade_pnl_sol: 0.0,
        }
    }
}
