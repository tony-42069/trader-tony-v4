use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc}; // Added ChronoDuration
use rand::Rng; // For demo mode price updates
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, str::FromStr, sync::Arc}; // Added PathBuf, FromStr
use tokio::{
    fs, // Added tokio::fs for async file operations
    sync::{Mutex, RwLock},
    time::{interval, Duration},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::api::jupiter::JupiterClient;
use crate::config::Config;
use crate::error::TraderbotError;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;

const POSITIONS_FILE: &str = "data/positions.json"; // Define persistence file path

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)] // Added Eq
pub enum PositionStatus {
    Active,
    Closing, // Intermediate state while sell tx is pending
    TakeProfitHit,
    StopLossHit,
    TrailingStopHit,
    MaxHoldTimeReached,
    ManualClose,
    EmergencyClose, // e.g., Rug pull detected
    Failed,         // e.g., Sell transaction failed
    Closed,         // Successfully sold and recorded
    ClosedManually, // Closed manually by user command
    Liquidated,     // Liquidated (not applicable for spot)
}

impl std::fmt::Display for PositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Closing => write!(f, "Closing"),
            Self::TakeProfitHit => write!(f, "TP Hit"),
            Self::StopLossHit => write!(f, "SL Hit"),
            Self::TrailingStopHit => write!(f, "Trailing SL Hit"),
            Self::MaxHoldTimeReached => write!(f, "Max Hold Time"),
            Self::ManualClose => write!(f, "Manual Close"),
            Self::EmergencyClose => write!(f, "Emergency Close"),
            Self::Failed => write!(f, "Failed"),
            Self::Closed => write!(f, "Closed"),
            Self::ClosedManually => write!(f, "Closed Manually"),
            Self::Liquidated => write!(f, "Liquidated"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: String,                          // Unique position ID
    pub token_address: String,               // Token mint address
    pub token_name: String,                  // Token name
    pub token_symbol: String,                // Token symbol
    pub token_decimals: u8,                  // Token decimals
    pub strategy_id: String,                 // Strategy ID that opened it
    pub entry_time: DateTime<Utc>,           // Entry time
    pub exit_time: Option<DateTime<Utc>>,    // Exit time
    pub entry_value_sol: f64,                // Initial value in SOL (amount bought)
    pub entry_token_amount: f64,             // Amount of token received at entry
    pub expected_token_amount: f64,          // Expected amount of token (for partial fills)
    pub fill_percent: f64,                   // Percentage filled (entry_token_amount/expected_token_amount)
    pub exit_value_sol: Option<f64>,         // Value in SOL received at exit
    pub entry_price_sol: f64,                // Entry price (SOL per Token)
    pub current_price_sol: f64,              // Current price (SOL per Token)
    pub exit_price_sol: Option<f64>,         // Exit price (SOL per Token)
    pub pnl_sol: Option<f64>,                // Profit/loss in SOL
    pub pnl_percent: Option<f64>,            // Profit/loss percentage
    pub stop_loss_price: Option<f64>,        // Stop loss price (SOL per Token)
    pub take_profit_price: Option<f64>,      // Take profit price (SOL per Token)
    pub trailing_stop_price: Option<f64>,    // Trailing stop price (SOL per Token)
    pub trailing_stop_percent: Option<u32>,  // Trailing stop percentage (used to update price)
    pub highest_price: f64,                  // Highest price seen since entry
    pub status: PositionStatus,              // Position status
    pub entry_tx_signature: String,          // Entry transaction signature
    pub exit_tx_signature: Option<String>,   // Exit transaction signature
    pub is_demo: bool,                       // Whether position is demo
    pub max_hold_time_minutes: Option<u32>,  // Maximum hold time in minutes (optional)
    pub stop_loss_percent: Option<u32>,
    pub take_profit_percent: Option<u32>,
}

// Removed Debug derive as SolanaClient doesn't implement it
pub struct PositionManager {
    wallet_manager: Arc<WalletManager>,
    jupiter_client: Arc<JupiterClient>,
    solana_client: Arc<SolanaClient>,
    // Use HashMap for efficient lookups by position ID
    positions: Arc<RwLock<HashMap<String, Position>>>,
    monitoring: Arc<RwLock<bool>>,
    config: Arc<Config>,
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    persistence_path: PathBuf,
}

impl PositionManager {
    pub fn new(
        wallet_manager: Arc<WalletManager>,
        jupiter_client: Arc<JupiterClient>,
        solana_client: Arc<SolanaClient>,
        config: Arc<Config>,
    ) -> Self {
        let persistence_path = PathBuf::from(POSITIONS_FILE);
        Self {
            wallet_manager,
            jupiter_client,
            solana_client,
            positions: Arc::new(RwLock::new(HashMap::new())),
            monitoring: Arc::new(RwLock::new(false)),
            config,
            task_handle: Arc::new(Mutex::new(None)),
            persistence_path,
        }
    }

    // --- Persistence ---

    // Loads positions from the JSON file into the in-memory HashMap.
    async fn load_positions(&self) -> Result<()> {
        // Ensure the data directory exists, create if not.
        if let Some(dir) = self.persistence_path.parent() {
            if !dir.exists() {
                info!("Data directory not found, creating at: {:?}", dir);
                fs::create_dir_all(dir).await.context("Failed to create data directory")?;
            }
        }

        // Check if the positions file exists. If not, it's okay, start fresh.
        if !self.persistence_path.exists() {
            info!("Positions file not found at {:?}, starting with empty state.", self.persistence_path);
            return Ok(());
        }

        info!("Loading positions from {:?}...", self.persistence_path);
        let data = match fs::read_to_string(&self.persistence_path).await {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                 info!("Positions file not found (race condition?), starting fresh.");
                 return Ok(());
            }
            Err(e) => {
                return Err(e).context(format!("Failed to read positions file: {:?}", self.persistence_path));
            }
        };


        if data.trim().is_empty() {
             info!("Positions file is empty.");
             return Ok(());
        }

        // Deserialize from JSON into a Vec<Position>
        let loaded_positions: Vec<Position> = match serde_json::from_str(&data) {
             Ok(p) => p,
             Err(e) => {
                  error!("Failed to deserialize positions data from {:?}: {}. Starting with empty state.", self.persistence_path, e);
                  // Optionally back up the corrupted file here
                  return Ok(()); // Don't crash, just start fresh
             }
        };

        // Populate the in-memory HashMap
        let mut positions_map = self.positions.write().await;
        positions_map.clear(); // Clear existing in-memory positions first
        for pos in loaded_positions {
            // Filter out positions that shouldn't be loaded (e.g., already closed/failed long ago?)
            // For now, load all states. Consider filtering later if needed.
            positions_map.insert(pos.id.clone(), pos);
        }
        info!("Loaded {} positions from file.", positions_map.len());
        Ok(())
    }

    // Saves the current in-memory positions HashMap to the JSON file.
    async fn save_positions(&self) -> Result<()> {
        debug!("Saving positions state...");
        let positions_map = self.positions.read().await;
        // No need to filter here, save the complete current state
        let positions_vec: Vec<&Position> = positions_map.values().collect(); // Collect references

        // Ensure the directory exists
        if let Some(dir) = self.persistence_path.parent() {
             // No need to check existence again if load_positions already did,
             // but create_dir_all is idempotent.
            fs::create_dir_all(dir).await.context("Failed to create data directory")?;
        }

        // Serialize Vec<&Position> to JSON string
        let data = serde_json::to_string_pretty(&positions_vec)
            .context("Failed to serialize positions")?;

        // Write data to the file atomically (optional but safer)
        // Using a temporary file and rename can prevent data loss if write fails mid-way.
        let temp_path = self.persistence_path.with_extension("json.tmp");
        fs::write(&temp_path, data).await
            .context(format!("Failed to write temporary positions file: {:?}", temp_path))?;
        fs::rename(&temp_path, &self.persistence_path).await
             .context(format!("Failed to rename temporary positions file to {:?}", self.persistence_path))?;


        debug!("Saved {} positions to file: {:?}", positions_vec.len(), self.persistence_path);
        Ok(())
    }


    // --- Position Management ---

    #[allow(clippy::too_many_arguments)] // Allow many args for position creation
    pub async fn create_position(
        &self,
        token_address: &str,
        token_name: &str,
        token_symbol: &str,
        token_decimals: u8,
        strategy_id: &str,
        entry_value_sol: f64,
        entry_token_amount: f64,
        expected_token_amount: Option<f64>, // Optional expected amount for partial fills
        _price_impact_pct: f64, // Prefixed as unused
        entry_tx_sig: &str,
        stop_loss_percent: Option<u32>,
        take_profit_percent: Option<u32>,
        trailing_stop_percent: Option<u32>,
        max_hold_time_minutes: Option<u32>, // Changed to Option<u32>
    ) -> Result<Position> {
        let now = Utc::now();

        if entry_token_amount <= 0.0 || entry_value_sol <= 0.0 {
             return Err(anyhow!("Invalid entry amounts: SOL={}, Token={}", entry_value_sol, entry_token_amount));
        }
        // Calculate entry price: SOL per Token
        let entry_price_sol = entry_value_sol / entry_token_amount;

        // Calculate fill percentage
        let expected = expected_token_amount.unwrap_or(entry_token_amount);
        let fill_percent = if expected > 0.0 {
            (entry_token_amount / expected) * 100.0
        } else {
            100.0 // Default to 100% if expected is 0 or negative
        };

        // Log if this is a partial fill
        if fill_percent < 99.9 {
            info!(
                "Partial fill detected for {}: Got {} tokens ({:.2}% of expected {})",
                token_symbol, entry_token_amount, fill_percent, expected
            );
        }

        let stop_loss_price = stop_loss_percent.map(|sl| entry_price_sol * (1.0 - (sl as f64 / 100.0)));
        let take_profit_price = take_profit_percent.map(|tp| entry_price_sol * (1.0 + (tp as f64 / 100.0)));
        // Initial trailing stop is based on entry price and percentage
        let trailing_stop_price = trailing_stop_percent.map(|ts| entry_price_sol * (1.0 - (ts as f64 / 100.0)));


        let position = Position {
            id: Uuid::new_v4().to_string(),
            token_address: token_address.to_string(),
            token_name: token_name.to_string(),
            token_symbol: token_symbol.to_string(),
            token_decimals,
            strategy_id: strategy_id.to_string(),
            entry_time: now,
            exit_time: None,
            entry_value_sol,
            entry_token_amount,
            expected_token_amount: expected,
            fill_percent: fill_percent / 100.0, // Store as 0.0-1.0
            exit_value_sol: None,
            entry_price_sol,
            current_price_sol: entry_price_sol, // Start current price at entry price
            exit_price_sol: None,
            pnl_sol: Some(0.0), // Initial PnL is 0
            pnl_percent: Some(0.0),
            stop_loss_price,
            take_profit_price,
            trailing_stop_price,
            trailing_stop_percent, // Store the percentage
            highest_price: entry_price_sol, // Initial highest price is entry price
            status: PositionStatus::Active,
            entry_tx_signature: entry_tx_sig.to_string(),
            exit_tx_signature: None,
            is_demo: self.config.demo_mode,
            max_hold_time_minutes,
            stop_loss_percent,
            take_profit_percent,
        };

        info!(
            "Creating new position (ID: {}): {} ({}) | Entry SOL: {:.4} | Entry Tokens: {:.4}/{:.4} ({:.1}%) | Entry Price: {:.6} SOL/Token | SL: {:?} | TP: {:?} | Trail: {:?}",
            position.id,
            position.token_name,
            position.token_symbol,
            position.entry_value_sol,
            position.entry_token_amount,
            position.expected_token_amount,
            position.fill_percent * 100.0,
            position.entry_price_sol,
            position.stop_loss_price,
            position.take_profit_price,
            position.trailing_stop_price
        );

        let mut positions = self.positions.write().await;
        positions.insert(position.id.clone(), position.clone());
        drop(positions); // Release lock before saving

        self.save_positions().await?;

        Ok(position)
    }

    // New method to update a position with actual fill amount if it was initially created with an estimate
    pub async fn update_position_fill_amount(
        &self,
        position_id: &str,
        actual_token_amount: f64,
    ) -> Result<Position> {
        let mut positions = self.positions.write().await;
        let position = positions.get_mut(position_id)
            .ok_or_else(|| TraderbotError::PositionError(format!("Position ID {} not found for fill update", position_id)))?;
        
        // Only update if position is still active
        if position.status != PositionStatus::Active {
            return Err(anyhow!("Cannot update fill amount for non-active position: {}", position_id));
        }
        
        // No need to update if amounts are the same
        if (position.entry_token_amount - actual_token_amount).abs() < 0.000001 {
            return Ok(position.clone());
        }
        
        // Calculate new fill percentage
        let fill_percent = if position.expected_token_amount > 0.0 {
            actual_token_amount / position.expected_token_amount
        } else {
            1.0 // Default to 100% if expected is 0
        };
        
        // Calculate new entry price (SOL per token)
        let entry_price_sol = if actual_token_amount > 0.0 {
            position.entry_value_sol / actual_token_amount
        } else {
            position.entry_price_sol // Keep original if we somehow got 0 tokens
        };
        
        // Log the update
        info!(
            "Updating position fill (ID: {}): {} tokens -> {} tokens ({:.1}% fill rate) | New price: {:.6} SOL/Token",
            position_id,
            position.entry_token_amount,
            actual_token_amount,
            fill_percent * 100.0,
            entry_price_sol
        );
        
        // Update position
        position.entry_token_amount = actual_token_amount;
        position.fill_percent = fill_percent;
        position.entry_price_sol = entry_price_sol;
        position.current_price_sol = entry_price_sol; // Also update current price
        
        // Recalculate stop loss and take profit prices
        if let Some(sl_percent) = position.stop_loss_percent {
            position.stop_loss_price = Some(entry_price_sol * (1.0 - (sl_percent as f64 / 100.0)));
        }
        
        if let Some(tp_percent) = position.take_profit_percent {
            position.take_profit_price = Some(entry_price_sol * (1.0 + (tp_percent as f64 / 100.0)));
        }
        
        // Update trailing stop if set
        if let Some(ts_percent) = position.trailing_stop_percent {
            position.trailing_stop_price = Some(entry_price_sol * (1.0 - (ts_percent as f64 / 100.0)));
        }
        
        // Update highest price if needed
        if position.highest_price < entry_price_sol {
            position.highest_price = entry_price_sol;
        }
        
        let updated_position = position.clone();
        drop(positions); // Release lock before saving
        
        self.save_positions().await?;
        
        Ok(updated_position)
    }

    pub async fn create_demo_position(
        &self,
        token_address: &str,
        token_name: &str,
        token_symbol: &str,
        strategy_id: &str,
        amount_sol: f64,
    ) -> Result<Position> {
        // Simulate entry price (e.g., based on a fictional market)
        let entry_price_sol = 0.00001; // Example dummy price
        let token_amount = amount_sol / entry_price_sol;
        let decimals = 9; // Assume 9 decimals for demo

        self.create_position(
            token_address,
            token_name,
            token_symbol,
            decimals,
            strategy_id,
            amount_sol,
            token_amount,
            None, // No expected amount for demo positions
            0.1, // Dummy price impact
            &format!("DEMO_ENTRY_{}", Uuid::new_v4()),
            Some(15), // 15% SL
            Some(50), // 50% TP
            Some(5),  // 5% Trailing SL
            Some(240),      // 4 hours max hold (Wrapped in Some)
        ).await
    }

    pub async fn close_position(
        &self,
        position_id: &str,
        status: PositionStatus, // The reason for closing
        exit_price_sol: f64,
        exit_value_sol: f64,
        exit_tx_sig: &str,
    ) -> Result<Position> {
        let mut positions = self.positions.write().await;
        let position = positions.get_mut(position_id)
            .ok_or_else(|| TraderbotError::PositionError(format!("Position ID {} not found for closing", position_id)))?;

        // Allow closing only if Active or Closing
        if ![PositionStatus::Active, PositionStatus::Closing].contains(&position.status) {
            warn!("Attempted to close position {} which is already in status {}", position_id, position.status);
            return Ok(position.clone()); // Return current state without error
        }

        let now = Utc::now();
        position.exit_time = Some(now);
        position.status = status; // Use the provided final status (Closed, Failed, etc.)
        position.exit_price_sol = Some(exit_price_sol);
        position.exit_value_sol = Some(exit_value_sol);
        position.exit_tx_signature = Some(exit_tx_sig.to_string());

        // Calculate final PnL
        let pnl_sol = exit_value_sol - position.entry_value_sol;
        position.pnl_sol = Some(pnl_sol);
        if position.entry_value_sol > 0.0 {
            position.pnl_percent = Some((pnl_sol / position.entry_value_sol) * 100.0);
        } else {
            position.pnl_percent = Some(0.0);
        }

        info!(
            "Closed position {} ({}) | Status: {} | PnL: {:.4} SOL ({:.2}%) | Exit Sig: {}",
            position.token_symbol, position_id, position.status,
            pnl_sol, position.pnl_percent.unwrap_or(0.0), exit_tx_sig
        );

        let closed_position = position.clone();
        drop(positions); // Release lock before saving

        self.save_positions().await?;
        Ok(closed_position)
    }

    // Updates price and checks exit conditions, but doesn't save immediately
    // Returns true if an exit condition was met
    async fn update_and_check_position(&self, position_id: &str, current_price_sol: f64) -> Result<Option<PositionStatus>> {
        let mut positions = self.positions.write().await;
        let position = match positions.get_mut(position_id) {
            Some(p) => p,
            None => {
                warn!("Position ID {} not found during update check.", position_id);
                return Ok(None); // Not an error, just skip
            }
        };

        // Only update active positions
        if position.status != PositionStatus::Active {
            return Ok(None);
        }

        position.current_price_sol = current_price_sol;

        // Update highest price and trailing stop
        if current_price_sol > position.highest_price {
            position.highest_price = current_price_sol;
            if let Some(ts_percent) = position.trailing_stop_percent {
                let new_trailing_stop = current_price_sol * (1.0 - (ts_percent as f64 / 100.0));
                // Only update if the new trailing stop is higher than the current one (or if none exists yet)
                if position.trailing_stop_price.map_or(true, |current_ts| new_trailing_stop > current_ts) {
                     debug!("Updating trailing stop for {}: {:.6} -> {:.6}", position.token_symbol, position.trailing_stop_price.unwrap_or(0.0), new_trailing_stop);
                     position.trailing_stop_price = Some(new_trailing_stop);
                }
            }
        }

        // Check exit conditions
        let exit_reason = self.check_exit_conditions_internal(position);

        if exit_reason.is_some() {
             // Mark as Closing internally, actual close happens after successful sell
             position.status = PositionStatus::Closing;
        }

        // Don't save here, save happens after all updates in manage_positions or after close_position

        Ok(exit_reason)
    }

     // Internal check, assumes position is mutable and lock is held
     fn check_exit_conditions_internal(&self, position: &Position) -> Option<PositionStatus> {
        // Check take profit
        if let Some(tp_price) = position.take_profit_price {
            if position.current_price_sol >= tp_price {
                info!("TP hit for {}: Current {:.6} >= TP {:.6}", position.token_symbol, position.current_price_sol, tp_price);
                return Some(PositionStatus::TakeProfitHit);
            }
        }

        // Check stop loss
        if let Some(sl_price) = position.stop_loss_price {
            if position.current_price_sol <= sl_price {
                 info!("SL hit for {}: Current {:.6} <= SL {:.6}", position.token_symbol, position.current_price_sol, sl_price);
                return Some(PositionStatus::StopLossHit);
            }
        }

        // Check trailing stop
        if let Some(ts_price) = position.trailing_stop_price {
             if position.current_price_sol <= ts_price {
                 info!("Trailing SL hit for {}: Current {:.6} <= Trail {:.6}", position.token_symbol, position.current_price_sol, ts_price);
                return Some(PositionStatus::TrailingStopHit);
            }
        }

        // Check max hold time (only if it's set)
        if let Some(max_minutes) = position.max_hold_time_minutes {
            let hold_duration = Utc::now().signed_duration_since(position.entry_time);
            if hold_duration >= ChronoDuration::minutes(max_minutes as i64) {
                 info!("Max hold time reached for {}: Held for {} mins (Limit: {} mins)", position.token_symbol, hold_duration.num_minutes(), max_minutes);
                return Some(PositionStatus::MaxHoldTimeReached);
            }
        }

        None // No exit condition met
    }


    // --- Getters ---

    pub async fn get_position(&self, id: &str) -> Option<Position> {
        let positions = self.positions.read().await;
        positions.get(id).cloned()
    }
    
    /// Gets all positions for a specific token
    pub async fn get_positions_by_token(&self, token_address: &str) -> Result<Vec<Position>> {
        let positions = self.positions.read().await;
        let matching_positions: Vec<Position> = positions.values()
            .filter(|p| p.token_address == token_address)
            .cloned()
            .collect();
        
        Ok(matching_positions)
    }

    /// Gets all active positions
    pub async fn get_active_positions(&self) -> Vec<Position> {
        let positions = self.positions.read().await;
        positions.values()
            .filter(|p| p.status == PositionStatus::Active)
            .cloned()
            .collect()
    }

     /// Gets all positions (active and closed)
     pub async fn get_all_positions(&self) -> Vec<Position> {
        let positions = self.positions.read().await;
        positions.values().cloned().collect()
    }

    /// Gets all active positions for a specific strategy
    pub async fn get_active_positions_by_strategy(&self, strategy_id: &str) -> Vec<Position> {
        let positions = self.positions.read().await;
        positions
            .values()
            .filter(|p| p.strategy_id == strategy_id && (p.status == PositionStatus::Active || p.status == PositionStatus::Closing))
            .cloned()
            .collect()
    }

    pub async fn has_active_position(&self, token_address: &str) -> bool {
        let positions = self.positions.read().await;
        positions.values().any(|p|
            p.token_address == token_address &&
            (p.status == PositionStatus::Active || p.status == PositionStatus::Closing)
        )
    }

    // --- Monitoring Task ---

    pub async fn start_monitoring(self: Arc<Self>) -> Result<()> { // Take Arc<Self>
        // Load existing positions first
        self.load_positions().await?;

        let mut monitoring_guard = self.monitoring.write().await;
        if *monitoring_guard {
            warn!("Position monitoring start requested but already running.");
            return Ok(());
        }
        *monitoring_guard = true;
        drop(monitoring_guard); // Release lock

        info!("Starting position monitoring task...");

        let self_clone = self.clone(); // Clone Arc<Self>
        let handle = tokio::spawn(async move {
            let monitor_interval = Duration::from_secs(15); // Check more frequently? Configurable?
            let mut interval_timer = interval(monitor_interval);
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            info!("Position monitoring task started.");
            loop {
                if !*self_clone.monitoring.read().await {
                    info!("Monitoring flag is false, stopping position monitoring task.");
                    break;
                }
                interval_timer.tick().await;
                debug!("Position monitor tick");

                if let Err(e) = self_clone.manage_positions_cycle().await {
                    error!("Error during position management cycle: {:?}", e);
                    // Decide if error is fatal or recoverable
                }
            }
             info!("Position monitoring task finished.");
        });

         *self.task_handle.lock().await = Some(handle);
         info!("Position monitoring task successfully launched.");
         Ok(())
    }

    pub async fn stop_monitoring(&self) -> Result<()> {
        let mut monitoring_guard = self.monitoring.write().await;
        if !*monitoring_guard {
            warn!("Position monitoring stop requested but not running.");
            return Ok(());
        }
        info!("Stopping position monitoring...");
        *monitoring_guard = false;
        drop(monitoring_guard); // Release lock

        // Wait for the background task to finish
        let mut handle_guard = self.task_handle.lock().await;
         if let Some(handle) = handle_guard.take() {
             info!("Waiting for position monitoring task to complete...");
             if let Err(e) = handle.await {
                 error!("Error waiting for position monitoring task: {:?}", e);
             } else {
                  info!("Position monitoring task completed.");
             }
        } else {
             warn!("No running position monitoring task handle found to wait for.");
        }

        // Save positions on graceful shutdown
        self.save_positions().await?;
        info!("Position monitoring stopped.");
        Ok(())
    }

    // Renamed from manage_positions to avoid confusion with the public method called by AutoTrader loop (if any)
    async fn manage_positions_cycle(&self) -> Result<()> {
        let active_positions_map = self.positions.read().await;
        // Collect IDs first to avoid holding lock during async operations
        let active_ids: Vec<String> = active_positions_map
            .iter()
            .filter(|(_, p)| p.status == PositionStatus::Active)
            .map(|(id, _)| id.clone())
            .collect();
        drop(active_positions_map); // Release read lock

        if active_ids.is_empty() {
            debug!("No active positions to manage.");
            return Ok(());
        }

        debug!("Managing {} active positions...", active_ids.len());

        let mut exits_to_execute = Vec::new();

        // Process each active position individually to avoid holding lock for too long
        for position_id in active_ids {
            let mut current_price_sol_opt: Option<f64> = None;
            let mut position_snapshot: Option<Position> = None; // To hold position data outside lock

            // --- Step 1: Get Position & Fetch Price ---
            { // Scope for read lock
                let positions_map = self.positions.read().await;
                if let Some(position) = positions_map.get(&position_id) {
                    // Only process active positions
                    if position.status != PositionStatus::Active {
                        continue;
                    }
                    position_snapshot = Some(position.clone()); // Clone data needed outside lock
                } else {
                    warn!("Position {} disappeared during management cycle?", position_id);
                    continue; // Position removed between getting IDs and now
                }
            } // Read lock released here

            if let Some(ref position) = position_snapshot {
                if position.is_demo {
                    // Simulate price movement for demo positions
                    let mut rng = rand::thread_rng();
                    let price_change_factor = rng.gen_range(0.97..1.03); // -3% to +3% change
                    current_price_sol_opt = Some(position.current_price_sol * price_change_factor);
                    debug!("[DEMO] Position {}: Simulated price update to {}", position.id, current_price_sol_opt.unwrap());
                } else {
                    // Fetch real price for non-demo positions
                    match self.jupiter_client.get_price(
                        &crate::api::jupiter::SOL_MINT.to_string(), // Price relative to SOL
                        &position.token_address,
                        position.token_decimals
                    ).await {
                        Ok(price) => {
                            current_price_sol_opt = Some(price);
                            debug!("Position {}: Fetched price {:.6}", position.id, price);
                        }
                        Err(e) => {
                            warn!("Failed to get price for position {} ({}): {:?}. Skipping update.", position.id, position.token_symbol, e);
                            // Consider adding retry logic or temporary error state?
                        }
                    }
                }
            }

            // --- Step 2: Update Position & Check Exit Conditions ---
            if let (Some(current_price_sol), Some(position)) = (current_price_sol_opt, position_snapshot) {
                 // Re-acquire write lock briefly to update and check
                 let mut exit_reason_opt: Option<PositionStatus> = None;
                 { // Scope for write lock
                     let mut positions_map = self.positions.write().await;
                     if let Some(pos_mut) = positions_map.get_mut(&position_id) {
                         // Ensure it's still active before updating
                         if pos_mut.status == PositionStatus::Active {
                             pos_mut.current_price_sol = current_price_sol;
                             // Recalculate PnL (optional here, can be done just before closing)
                             pos_mut.pnl_sol = Some(pos_mut.entry_token_amount * current_price_sol - pos_mut.entry_value_sol);
                             if pos_mut.entry_value_sol > 0.0 {
                                 pos_mut.pnl_percent = Some(pos_mut.pnl_sol.unwrap_or(0.0) / pos_mut.entry_value_sol * 100.0);
                             }

                             // Update highest price and trailing stop
                             if current_price_sol > pos_mut.highest_price {
                                 pos_mut.highest_price = current_price_sol;
                                 if let Some(ts_percent) = pos_mut.trailing_stop_percent {
                                     let new_trailing_stop = current_price_sol * (1.0 - (ts_percent as f64 / 100.0));
                                     if pos_mut.trailing_stop_price.map_or(true, |current_ts| new_trailing_stop > current_ts) {
                                         debug!("Updating trailing stop for {}: {:.6} -> {:.6}", pos_mut.token_symbol, pos_mut.trailing_stop_price.unwrap_or(0.0), new_trailing_stop);
                                         pos_mut.trailing_stop_price = Some(new_trailing_stop);
                                     }
                                 }
                             }
                             // Check exit conditions based on the updated state
                             exit_reason_opt = self.check_exit_conditions_internal(pos_mut);
                             if exit_reason_opt.is_some() {
                                 pos_mut.status = PositionStatus::Closing; // Mark for exit
                                 info!("Position {} marked for closing due to: {:?}", position_id, exit_reason_opt.as_ref().unwrap());
                             }
                         } else {
                              debug!("Position {} status changed to {} before update could be applied.", position_id, pos_mut.status);
                         }
                     }
                 } // Write lock released

                 // If an exit condition was met, add to the list for execution
                 if let Some(exit_reason) = exit_reason_opt {
                     exits_to_execute.push((position_id.clone(), exit_reason));
                 }
            }
        } // End loop through active_ids


        // --- Step 3: Execute Exits ---
        for (position_id, exit_reason) in exits_to_execute { // Use the collected exits
             // Re-fetch position to ensure it's still marked for closing and get latest state
             let position_to_exit = match self.get_position(&position_id).await {
                 Some(p) if p.status == PositionStatus::Closing => p, // Ensure it's still marked for closing
                 Some(p) => {
                     warn!("Position {} status changed ({}) before exit could be executed. Skipping exit.", position_id, p.status);
                     continue; // Status changed, maybe closed by another process/manual action
                 }
                 None => {
                      warn!("Position {} not found for exit execution.", position_id);
                      continue; // Not found
                 }
             };

            // Borrow position_to_exit when calling execute_exit
            if let Err(e) = self.execute_exit(&position_to_exit, exit_reason).await {
                error!("Failed to execute exit for position {}: {:?}", position_id, e);
                // Attempt to mark as Failed status
                 if let Err(close_err) = self.close_position(
                     &position_id,
                     PositionStatus::Failed,
                     position_to_exit.current_price_sol, // Use last known price
                     0.0, // Assume 0 return on failure
                     "SELL_FAILED"
                 ).await {
                     error!("Critical: Failed to even mark position {} as Failed: {:?}", position_id, close_err);
                 }
            }
        }

        // --- Step 4: Save all changes made during the cycle ---
        // Saving happens within close_position and potentially after updates if needed,
        // but a final save ensures consistency.
        if let Err(e) = self.save_positions().await {
             error!("Failed to save positions after management cycle: {:?}", e);
        }

        Ok(())
    }

    // Changed to take &Position to avoid moving the value
    async fn execute_exit(&self, position: &Position, reason: PositionStatus) -> Result<()> {
        info!(
            "Executing exit for position {} ({}) due to: {}",
            position.token_symbol, position.id, reason
        );

        if position.is_demo {
            // Simulate exit for demo positions
            let exit_price = position.current_price_sol; // Use current price as exit price
            let exit_value_sol = position.entry_token_amount * exit_price;
            self.close_position(
                &position.id,
                PositionStatus::Closed, // Mark as Closed directly for demo
                exit_price,
                exit_value_sol,
                &format!("DEMO_EXIT_{}", Uuid::new_v4()),
            ).await?;
            info!("[DEMO] Closed position {} ({})", position.token_symbol, position.id);
            return Ok(());
        }

        // --- Real Exit ---
        let swap_result = match self.jupiter_client.swap_token_to_sol(
            &position.token_address,
            position.token_decimals,
            position.entry_token_amount, // Sell the full amount held
            self.config.default_slippage_bps, // Use default slippage for closing? Or strategy specific?
            Some(self.config.default_priority_fee_micro_lamports * 2), // Higher priority fee for closing?
            self.wallet_manager.clone(),
        ).await {
             Ok(result) => result,
             Err(e) => {
                 error!("Swap execution failed for exit of position {}: {:?}", position.id, e);
                 // Don't close yet, maybe retry or mark as failed after retries?
                 // For now, return error to indicate failure.
                 return Err(e).context(format!("Failed to execute sell swap for position {}", position.id));
             }
        };

        info!(
            "Exit swap sent for {}. Signature: {}, Estimated SOL Out: {:.6}",
            position.token_symbol, swap_result.transaction_signature, swap_result.out_amount_ui
        );

        // --- Confirm Transaction ---
        info!("Confirming exit transaction: {}", swap_result.transaction_signature);
        let signature = solana_sdk::signature::Signature::from_str(&swap_result.transaction_signature)
            .context("Failed to parse exit transaction signature")?;

        // TODO: Make confirmation timeout configurable
        match self.solana_client.confirm_transaction(&signature, solana_sdk::commitment_config::CommitmentLevel::Confirmed, 60).await {
            Ok(_) => {
                info!("Exit transaction {} confirmed successfully.", signature);

                // --- Close Position (Only after confirmation) ---
                // TODO: Get actual SOL received after confirmation if possible (requires parsing tx details)
                let actual_exit_value_sol = swap_result.actual_out_amount_ui.unwrap_or(swap_result.out_amount_ui); // Use estimate for now
                let actual_exit_price_sol = if position.entry_token_amount > 0.0 {
                    actual_exit_value_sol / position.entry_token_amount // Calculate effective exit price
                } else {
                    0.0 // Avoid division by zero if entry amount was somehow zero
                };

                self.close_position(
                    &position.id,
                    PositionStatus::Closed, // Mark as successfully closed
                    actual_exit_price_sol,
                    actual_exit_value_sol,
                    &swap_result.transaction_signature,
                ).await?;

                info!("Successfully executed exit and closed position {}", position.id);
                // TODO: Send notification
                Ok(())
            }
            Err(e) => {
                error!("Failed to confirm exit transaction {}: {:?}", signature, e);
                // Don't close the position as Closed if confirmation fails.
                // Mark as Failed instead? Or leave as Closing for retry?
                // For now, return error to indicate confirmation failure.
                // The caller (manage_positions_cycle) will mark as Failed.
                Err(e).context(format!("Exit transaction {} failed confirmation", signature))
            }
        }
    }
}
