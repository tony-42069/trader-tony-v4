pub mod token;
pub mod user;
pub mod copy_trade;
pub mod simulated_position;

// Re-export commonly used types
pub use copy_trade::{
    TradeSignal, TradeAction, CopyTrader, CopyPosition, CopyPositionStatus,
    CopyTradeSettings, CopyTradeStats, BuildCopyTradeRequest, BuildCopyTradeResponse,
};
pub use simulated_position::{SimulatedPosition, SimulatedPositionStatus, SimulationStats};
