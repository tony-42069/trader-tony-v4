//! Copy Trade Manager and Transaction Builder
//!
//! Handles copy trading functionality including:
//! - Managing copy traders (registered users)
//! - Storing and retrieving trade signals
//! - Building copy trade transactions
//! - Fee calculation and collection

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::models::copy_trade::{
    CopyPosition, CopyPositionStatus, CopyTradeSettings, CopyTradeStats, CopyTrader,
    TradeAction, TradeSignal,
};
use crate::trading::position::Position;

const COPY_TRADERS_FILE: &str = "data/copy_traders.json";
const SIGNALS_FILE: &str = "data/signals.json";
const COPY_POSITIONS_FILE: &str = "data/copy_positions.json";

/// Manages all copy trading functionality
pub struct CopyTradeManager {
    /// Registered copy traders by wallet address
    traders: Arc<RwLock<HashMap<String, CopyTrader>>>,
    /// Trade signals history
    signals: Arc<RwLock<Vec<TradeSignal>>>,
    /// Copy positions by copier wallet
    copy_positions: Arc<RwLock<HashMap<String, Vec<CopyPosition>>>>,
    /// Configuration
    config: Arc<Config>,
    /// Treasury wallet for fee collection
    treasury_wallet: String,
    /// Fee percentage (e.g., 10.0 for 10%)
    fee_percent: f64,
}

impl CopyTradeManager {
    pub fn new(config: Arc<Config>) -> Self {
        let treasury_wallet = config
            .treasury_wallet
            .clone()
            .unwrap_or_else(|| "NOT_SET".to_string());
        let fee_percent = config.copy_trade_fee_percent;

        Self {
            traders: Arc::new(RwLock::new(HashMap::new())),
            signals: Arc::new(RwLock::new(Vec::new())),
            copy_positions: Arc::new(RwLock::new(HashMap::new())),
            config,
            treasury_wallet,
            fee_percent,
        }
    }

    /// Initialize and load data from disk
    pub async fn init(&self) -> Result<()> {
        info!("Initializing CopyTradeManager...");
        self.load_traders().await?;
        self.load_signals().await?;
        self.load_copy_positions().await?;
        info!(
            "CopyTradeManager initialized: {} traders, {} signals",
            self.traders.read().await.len(),
            self.signals.read().await.len()
        );
        Ok(())
    }

    // ==========================================================================
    // Trader Management
    // ==========================================================================

    /// Register a new copy trader
    pub async fn register_trader(
        &self,
        wallet_address: &str,
        _signature: &str,
        _message: &str,
    ) -> Result<CopyTrader> {
        // TODO: Verify the signature matches the message and wallet
        // For now, we trust the frontend verification

        let mut traders = self.traders.write().await;

        if traders.contains_key(wallet_address) {
            return Err(anyhow!("Wallet already registered"));
        }

        let trader = CopyTrader::new(wallet_address, 0.1); // Default 0.1 SOL per trade
        traders.insert(wallet_address.to_string(), trader.clone());
        drop(traders);

        self.save_traders().await?;
        info!("Registered new copy trader: {}", wallet_address);

        Ok(trader)
    }

    /// Unregister a copy trader
    pub async fn unregister_trader(&self, wallet_address: &str) -> Result<()> {
        let mut traders = self.traders.write().await;

        if traders.remove(wallet_address).is_none() {
            return Err(anyhow!("Wallet not registered"));
        }
        drop(traders);

        self.save_traders().await?;
        info!("Unregistered copy trader: {}", wallet_address);

        Ok(())
    }

    /// Get a copy trader by wallet address
    pub async fn get_trader(&self, wallet_address: &str) -> Option<CopyTrader> {
        let traders = self.traders.read().await;
        traders.get(wallet_address).cloned()
    }

    /// Update copy trade settings for a trader
    pub async fn update_settings(
        &self,
        wallet_address: &str,
        settings: CopyTradeSettings,
    ) -> Result<CopyTrader> {
        let mut traders = self.traders.write().await;

        let trader = traders
            .get_mut(wallet_address)
            .ok_or_else(|| anyhow!("Wallet not registered"))?;

        trader.auto_copy_enabled = settings.auto_copy_enabled;
        trader.copy_amount_sol = settings.copy_amount_sol;
        trader.max_positions = settings.max_positions;
        trader.slippage_bps = settings.slippage_bps;
        trader.last_active = Utc::now();

        let updated_trader = trader.clone();
        drop(traders);

        self.save_traders().await?;
        info!(
            "Updated settings for trader {}: auto_copy={}, amount={}",
            wallet_address, settings.auto_copy_enabled, settings.copy_amount_sol
        );

        Ok(updated_trader)
    }

    /// Get all traders with auto-copy enabled
    pub async fn get_auto_copy_traders(&self) -> Vec<CopyTrader> {
        let traders = self.traders.read().await;
        traders
            .values()
            .filter(|t| t.auto_copy_enabled && t.is_verified)
            .cloned()
            .collect()
    }

    // ==========================================================================
    // Signal Management
    // ==========================================================================

    /// Create a buy signal from a bot position
    pub async fn create_buy_signal(&self, position: &Position) -> TradeSignal {
        let signal = TradeSignal::new_buy(
            &position.token_address,
            &position.token_symbol,
            &position.token_name,
            position.entry_value_sol,
            position.entry_price_sol,
            &position.id,
        );

        let mut signals = self.signals.write().await;
        signals.push(signal.clone());
        drop(signals);

        if let Err(e) = self.save_signals().await {
            error!("Failed to save signals: {}", e);
        }

        info!(
            "Created BUY signal for {} ({})",
            position.token_symbol, signal.id
        );
        signal
    }

    /// Create a sell signal from a bot position
    pub async fn create_sell_signal(&self, position: &Position) -> TradeSignal {
        let pnl_percent = position.pnl_percent.unwrap_or(0.0);
        let exit_price = position.exit_price_sol.unwrap_or(position.current_price_sol);
        let exit_value = position.exit_value_sol.unwrap_or(
            position.entry_token_amount * exit_price
        );

        let signal = TradeSignal::new_sell(
            &position.token_address,
            &position.token_symbol,
            &position.token_name,
            exit_value,
            exit_price,
            pnl_percent,
            &position.id,
        );

        // Deactivate the corresponding buy signal
        {
            let mut signals = self.signals.write().await;
            for s in signals.iter_mut() {
                if s.bot_position_id == position.id && s.action == TradeAction::Buy {
                    s.is_active = false;
                }
            }
            signals.push(signal.clone());
        }

        if let Err(e) = self.save_signals().await {
            error!("Failed to save signals: {}", e);
        }

        info!(
            "Created SELL signal for {} ({}) - PnL: {:.2}%",
            position.token_symbol, signal.id, pnl_percent
        );
        signal
    }

    /// Update signal with current position data
    pub async fn update_signal_prices(&self, position: &Position) {
        let mut signals = self.signals.write().await;

        for signal in signals.iter_mut() {
            if signal.bot_position_id == position.id && signal.is_active {
                signal.current_price_sol = Some(position.current_price_sol);
                signal.current_pnl_percent = position.pnl_percent;
            }
        }
    }

    /// Get all signals
    pub async fn get_all_signals(&self) -> Vec<TradeSignal> {
        let signals = self.signals.read().await;
        signals.clone()
    }

    /// Get active signals (bot's current open positions)
    pub async fn get_active_signals(&self) -> Vec<TradeSignal> {
        let signals = self.signals.read().await;
        signals.iter().filter(|s| s.is_active).cloned().collect()
    }

    /// Get recent signals (last N)
    pub async fn get_recent_signals(&self, limit: usize) -> Vec<TradeSignal> {
        let signals = self.signals.read().await;
        let mut sorted: Vec<_> = signals.clone();
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        sorted.into_iter().take(limit).collect()
    }

    /// Get a signal by ID
    pub async fn get_signal(&self, signal_id: &str) -> Option<TradeSignal> {
        let signals = self.signals.read().await;
        signals.iter().find(|s| s.id == signal_id).cloned()
    }

    // ==========================================================================
    // Copy Position Management
    // ==========================================================================

    /// Record a new copy position
    pub async fn create_copy_position(
        &self,
        copier_wallet: &str,
        token_address: &str,
        token_symbol: &str,
        entry_price_sol: f64,
        entry_amount_sol: f64,
        token_amount: f64,
        bot_position_id: &str,
        buy_signal_id: &str,
        entry_tx_signature: &str,
    ) -> Result<CopyPosition> {
        let copy_position = CopyPosition::new(
            copier_wallet,
            token_address,
            token_symbol,
            entry_price_sol,
            entry_amount_sol,
            token_amount,
            bot_position_id,
            buy_signal_id,
            entry_tx_signature,
        );

        let mut positions = self.copy_positions.write().await;
        positions
            .entry(copier_wallet.to_string())
            .or_insert_with(Vec::new)
            .push(copy_position.clone());
        drop(positions);

        // Update trader stats
        {
            let mut traders = self.traders.write().await;
            if let Some(trader) = traders.get_mut(copier_wallet) {
                trader.total_copy_trades += 1;
                trader.last_active = Utc::now();
            }
        }

        self.save_copy_positions().await?;
        self.save_traders().await?;

        info!(
            "Created copy position {} for {} copying {}",
            copy_position.id, copier_wallet, token_symbol
        );

        Ok(copy_position)
    }

    /// Close a copy position
    pub async fn close_copy_position(
        &self,
        position_id: &str,
        exit_price_sol: f64,
        exit_amount_sol: f64,
        exit_tx_signature: &str,
    ) -> Result<CopyPosition> {
        let mut positions = self.copy_positions.write().await;

        // Find the position
        let mut found_position: Option<CopyPosition> = None;
        let mut found_wallet: Option<String> = None;

        for (wallet, wallet_positions) in positions.iter_mut() {
            for pos in wallet_positions.iter_mut() {
                if pos.id == position_id {
                    // Calculate fee if profitable
                    let gross_pnl = exit_amount_sol - pos.entry_amount_sol;
                    let fee = if gross_pnl > 0.0 {
                        gross_pnl * (self.fee_percent / 100.0)
                    } else {
                        0.0
                    };

                    pos.close(exit_price_sol, exit_amount_sol, fee, exit_tx_signature);
                    found_position = Some(pos.clone());
                    found_wallet = Some(wallet.clone());
                    break;
                }
            }
            if found_position.is_some() {
                break;
            }
        }
        drop(positions);

        match (found_position, found_wallet) {
            (Some(position), Some(wallet)) => {
                // Update trader stats
                {
                    let mut traders = self.traders.write().await;
                    if let Some(trader) = traders.get_mut(&wallet) {
                        if let Some(fee) = position.fee_paid_sol {
                            trader.total_fees_paid_sol += fee;
                        }
                    }
                }

                self.save_copy_positions().await?;
                self.save_traders().await?;

                info!(
                    "Closed copy position {} - PnL: {:?} SOL, Fee: {:?} SOL",
                    position_id, position.pnl_sol, position.fee_paid_sol
                );

                Ok(position)
            }
            _ => Err(anyhow!("Copy position not found: {}", position_id)),
        }
    }

    /// Get copy positions for a wallet
    pub async fn get_copy_positions(&self, wallet: &str) -> Vec<CopyPosition> {
        let positions = self.copy_positions.read().await;
        positions.get(wallet).cloned().unwrap_or_default()
    }

    /// Get active copy positions for a wallet
    pub async fn get_active_copy_positions(&self, wallet: &str) -> Vec<CopyPosition> {
        let positions = self.copy_positions.read().await;
        positions
            .get(wallet)
            .map(|p| {
                p.iter()
                    .filter(|pos| pos.status == CopyPositionStatus::Open)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get copy positions by bot position ID
    pub async fn get_copy_positions_by_bot_position(
        &self,
        bot_position_id: &str,
    ) -> Vec<CopyPosition> {
        let positions = self.copy_positions.read().await;
        let mut result = Vec::new();

        for wallet_positions in positions.values() {
            for pos in wallet_positions {
                if pos.bot_position_id == bot_position_id {
                    result.push(pos.clone());
                }
            }
        }

        result
    }

    /// Calculate stats for a trader
    pub async fn get_trader_stats(&self, wallet: &str) -> CopyTradeStats {
        let positions = self.get_copy_positions(wallet).await;

        let closed_positions: Vec<_> = positions
            .iter()
            .filter(|p| p.status == CopyPositionStatus::Closed)
            .collect();

        if closed_positions.is_empty() {
            return CopyTradeStats::default();
        }

        let total_trades = closed_positions.len() as u32;
        let winning_trades = closed_positions
            .iter()
            .filter(|p| p.pnl_sol.unwrap_or(0.0) > 0.0)
            .count() as u32;
        let losing_trades = total_trades - winning_trades;

        let win_rate = if total_trades > 0 {
            (winning_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let total_pnl_sol: f64 = closed_positions
            .iter()
            .map(|p| p.pnl_sol.unwrap_or(0.0))
            .sum();

        let total_fees_paid_sol: f64 = closed_positions
            .iter()
            .map(|p| p.fee_paid_sol.unwrap_or(0.0))
            .sum();

        let avg_pnl_percent = if total_trades > 0 {
            closed_positions
                .iter()
                .map(|p| p.pnl_percent.unwrap_or(0.0))
                .sum::<f64>()
                / total_trades as f64
        } else {
            0.0
        };

        let best_trade_pnl_sol = closed_positions
            .iter()
            .map(|p| p.pnl_sol.unwrap_or(0.0))
            .fold(f64::MIN, f64::max);

        let worst_trade_pnl_sol = closed_positions
            .iter()
            .map(|p| p.pnl_sol.unwrap_or(0.0))
            .fold(f64::MAX, f64::min);

        CopyTradeStats {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            total_pnl_sol,
            total_fees_paid_sol,
            avg_pnl_percent,
            best_trade_pnl_sol,
            worst_trade_pnl_sol,
        }
    }

    // ==========================================================================
    // Fee Calculation
    // ==========================================================================

    /// Calculate the fee for a profitable sell
    pub fn calculate_fee(&self, entry_amount_sol: f64, exit_amount_sol: f64) -> f64 {
        let profit = exit_amount_sol - entry_amount_sol;
        if profit > 0.0 {
            profit * (self.fee_percent / 100.0)
        } else {
            0.0
        }
    }

    /// Get the treasury wallet address
    pub fn get_treasury_wallet(&self) -> &str {
        &self.treasury_wallet
    }

    /// Get the fee percentage
    pub fn get_fee_percent(&self) -> f64 {
        self.fee_percent
    }

    // ==========================================================================
    // Persistence
    // ==========================================================================

    async fn ensure_data_dir(&self) -> Result<()> {
        let path = PathBuf::from("data");
        if !path.exists() {
            fs::create_dir_all(&path).await?;
        }
        Ok(())
    }

    async fn load_traders(&self) -> Result<()> {
        let path = PathBuf::from(COPY_TRADERS_FILE);
        if !path.exists() {
            debug!("No traders file found, starting fresh");
            return Ok(());
        }

        let data = fs::read_to_string(&path).await?;
        if data.trim().is_empty() {
            return Ok(());
        }

        let traders: Vec<CopyTrader> = serde_json::from_str(&data)
            .context("Failed to parse traders file")?;

        let mut traders_map = self.traders.write().await;
        for trader in traders {
            traders_map.insert(trader.wallet_address.clone(), trader);
        }

        info!("Loaded {} copy traders", traders_map.len());
        Ok(())
    }

    async fn save_traders(&self) -> Result<()> {
        self.ensure_data_dir().await?;

        let traders = self.traders.read().await;
        let traders_vec: Vec<&CopyTrader> = traders.values().collect();
        let data = serde_json::to_string_pretty(&traders_vec)?;

        let temp_path = PathBuf::from(COPY_TRADERS_FILE).with_extension("json.tmp");
        fs::write(&temp_path, data).await?;
        fs::rename(&temp_path, COPY_TRADERS_FILE).await?;

        debug!("Saved {} copy traders", traders.len());
        Ok(())
    }

    async fn load_signals(&self) -> Result<()> {
        let path = PathBuf::from(SIGNALS_FILE);
        if !path.exists() {
            debug!("No signals file found, starting fresh");
            return Ok(());
        }

        let data = fs::read_to_string(&path).await?;
        if data.trim().is_empty() {
            return Ok(());
        }

        let signals: Vec<TradeSignal> = serde_json::from_str(&data)
            .context("Failed to parse signals file")?;

        let mut signals_vec = self.signals.write().await;
        *signals_vec = signals;

        info!("Loaded {} trade signals", signals_vec.len());
        Ok(())
    }

    async fn save_signals(&self) -> Result<()> {
        self.ensure_data_dir().await?;

        let signals = self.signals.read().await;
        // Only keep last 1000 signals to prevent unbounded growth
        let signals_to_save: Vec<&TradeSignal> = if signals.len() > 1000 {
            signals.iter().rev().take(1000).collect()
        } else {
            signals.iter().collect()
        };

        let data = serde_json::to_string_pretty(&signals_to_save)?;

        let temp_path = PathBuf::from(SIGNALS_FILE).with_extension("json.tmp");
        fs::write(&temp_path, data).await?;
        fs::rename(&temp_path, SIGNALS_FILE).await?;

        debug!("Saved {} trade signals", signals_to_save.len());
        Ok(())
    }

    async fn load_copy_positions(&self) -> Result<()> {
        let path = PathBuf::from(COPY_POSITIONS_FILE);
        if !path.exists() {
            debug!("No copy positions file found, starting fresh");
            return Ok(());
        }

        let data = fs::read_to_string(&path).await?;
        if data.trim().is_empty() {
            return Ok(());
        }

        let positions: HashMap<String, Vec<CopyPosition>> = serde_json::from_str(&data)
            .context("Failed to parse copy positions file")?;

        let mut positions_map = self.copy_positions.write().await;
        *positions_map = positions;

        let total: usize = positions_map.values().map(|v| v.len()).sum();
        info!("Loaded {} copy positions", total);
        Ok(())
    }

    async fn save_copy_positions(&self) -> Result<()> {
        self.ensure_data_dir().await?;

        let positions = self.copy_positions.read().await;
        let data = serde_json::to_string_pretty(&*positions)?;

        let temp_path = PathBuf::from(COPY_POSITIONS_FILE).with_extension("json.tmp");
        fs::write(&temp_path, data).await?;
        fs::rename(&temp_path, COPY_POSITIONS_FILE).await?;

        let total: usize = positions.values().map(|v| v.len()).sum();
        debug!("Saved {} copy positions", total);
        Ok(())
    }
}
