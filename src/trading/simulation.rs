use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::api::birdeye::BirdeyeClient;
use crate::models::simulated_position::{SimulatedPosition, SimulatedPositionStatus, SimulationStats};

const SIMULATED_POSITIONS_FILE: &str = "data/simulated_positions.json";

/// Manages simulated positions for DRY_RUN_MODE
pub struct SimulationManager {
    positions: Arc<RwLock<HashMap<String, SimulatedPosition>>>,
    data_path: PathBuf,
    birdeye_client: Arc<BirdeyeClient>,
}

impl SimulationManager {
    pub fn new(birdeye_client: Arc<BirdeyeClient>) -> Self {
        Self {
            positions: Arc::new(RwLock::new(HashMap::new())),
            data_path: PathBuf::from(SIMULATED_POSITIONS_FILE),
            birdeye_client,
        }
    }

    /// Load simulated positions from disk
    pub async fn load(&self) -> Result<()> {
        info!("Loading simulated positions from {:?}", self.data_path);

        if !self.data_path.exists() {
            debug!("No simulated positions file found, starting fresh");
            return Ok(());
        }

        match tokio::fs::read_to_string(&self.data_path).await {
            Ok(data) => {
                if data.is_empty() {
                    return Ok(());
                }
                match serde_json::from_str::<HashMap<String, SimulatedPosition>>(&data) {
                    Ok(loaded_positions) => {
                        let mut positions = self.positions.write().await;
                        *positions = loaded_positions;
                        info!("Loaded {} simulated positions", positions.len());
                    }
                    Err(e) => {
                        error!("Failed to parse simulated positions file: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to read simulated positions file: {}", e);
            }
        }

        Ok(())
    }

    /// Save simulated positions to disk
    pub async fn save(&self) -> Result<()> {
        debug!("Saving simulated positions to {:?}", self.data_path);

        // Ensure directory exists
        if let Some(parent) = self.data_path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create directory for simulated positions file")?;
            }
        }

        let positions = self.positions.read().await;
        let json = serde_json::to_string_pretty(&*positions)
            .context("Failed to serialize simulated positions")?;

        tokio::fs::write(&self.data_path, json)
            .await
            .context("Failed to write simulated positions file")?;

        debug!("Saved {} simulated positions to disk", positions.len());
        Ok(())
    }

    /// Create a simulated buy position
    pub async fn simulate_buy(
        &self,
        token_address: &str,
        token_symbol: &str,
        token_name: &str,
        current_price_sol: f64,
        amount_sol: f64,
        risk_score: u32,
        risk_details: Vec<String>,
        selection_reason: String,
        strategy_id: String,
    ) -> Result<SimulatedPosition> {
        // Check if we already have an open position for this token
        {
            let positions = self.positions.read().await;
            for pos in positions.values() {
                if pos.token_address == token_address && pos.is_open() {
                    return Err(anyhow::anyhow!(
                        "Already have an open simulated position for token {}",
                        token_symbol
                    ));
                }
            }
        }

        let position = SimulatedPosition::new(
            token_address.to_string(),
            token_symbol.to_string(),
            token_name.to_string(),
            current_price_sol,
            amount_sol,
            risk_score,
            risk_details.clone(),
            selection_reason.clone(),
            strategy_id,
        );

        info!(
            "🔍 [DRY RUN] Simulated BUY: {} ({}) @ {} SOL - Amount: {} SOL - Risk: {}/100",
            token_symbol, token_address, current_price_sol, amount_sol, risk_score
        );
        info!(
            "🔍 [DRY RUN] Selection reason: {} - Risk details: {:?}",
            selection_reason, risk_details
        );

        // Store the position
        let mut positions = self.positions.write().await;
        positions.insert(position.id.clone(), position.clone());
        drop(positions);

        // Save to disk
        self.save().await?;

        Ok(position)
    }

    /// Update prices for all open positions using Birdeye
    pub async fn update_prices(&self) -> Result<()> {
        let mut positions = self.positions.write().await;
        let open_positions: Vec<_> = positions
            .values()
            .filter(|p| p.is_open())
            .map(|p| p.token_address.clone())
            .collect();

        if open_positions.is_empty() {
            return Ok(());
        }

        debug!(
            "Updating prices for {} open simulated positions",
            open_positions.len()
        );

        // Get SOL price for USD to SOL conversion
        let sol_price_usd = self.birdeye_client.get_sol_price_usd().await.unwrap_or(150.0);

        for token_address in open_positions {
            match self.birdeye_client.get_token_overview(&token_address).await {
                Ok(Some(token_data)) => {
                    // Convert USD price to SOL price
                    let price_sol = if let Some(price_usd) = token_data.price {
                        if sol_price_usd > 0.0 {
                            price_usd / sol_price_usd
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    if price_sol > 0.0 {
                        if let Some(pos) = positions
                            .values_mut()
                            .find(|p| p.token_address == token_address && p.is_open())
                        {
                            let old_price = pos.current_price_sol;
                            pos.update_price(price_sol);
                            debug!(
                                "Updated {} price: {} -> {} SOL (P&L: {:.2}%)",
                                pos.token_symbol,
                                old_price,
                                price_sol,
                                pos.unrealized_pnl_percent
                            );
                        }
                    }
                }
                Ok(None) => {
                    warn!(
                        "No price data found for simulated position {}",
                        token_address
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to get price for simulated position {}: {}",
                        token_address, e
                    );
                }
            }
        }

        drop(positions);
        self.save().await?;
        Ok(())
    }

    /// Check exit conditions for all open positions
    pub async fn check_exit_conditions(
        &self,
        stop_loss_pct: f64,
        take_profit_pct: f64,
        trailing_stop_pct: Option<f64>,
        max_hold_minutes: Option<u32>,
    ) -> Result<Vec<SimulatedPosition>> {
        let mut closed_positions = Vec::new();
        let mut positions = self.positions.write().await;

        for pos in positions.values_mut() {
            if !pos.is_open() {
                continue;
            }

            let pnl_percent = pos.unrealized_pnl_percent;
            let hold_duration = Utc::now()
                .signed_duration_since(pos.entry_time)
                .num_minutes();

            // Check stop loss
            if pnl_percent <= -stop_loss_pct {
                info!(
                    "🔍 [DRY RUN] STOP LOSS triggered for {} - P&L: {:.2}%",
                    pos.token_symbol, pnl_percent
                );
                pos.close(
                    pos.current_price_sol,
                    SimulatedPositionStatus::ClosedStopLoss,
                    format!("Stop loss triggered at {:.2}%", pnl_percent),
                );
                closed_positions.push(pos.clone());
                continue;
            }

            // Check take profit
            if pnl_percent >= take_profit_pct {
                info!(
                    "🔍 [DRY RUN] TAKE PROFIT triggered for {} - P&L: {:.2}%",
                    pos.token_symbol, pnl_percent
                );
                pos.close(
                    pos.current_price_sol,
                    SimulatedPositionStatus::ClosedTakeProfit,
                    format!("Take profit triggered at {:.2}%", pnl_percent),
                );
                closed_positions.push(pos.clone());
                continue;
            }

            // Check trailing stop
            if let Some(trail_pct) = trailing_stop_pct {
                let drop_from_high = if pos.highest_price_sol > 0.0 {
                    ((pos.highest_price_sol - pos.current_price_sol) / pos.highest_price_sol) * 100.0
                } else {
                    0.0
                };

                if drop_from_high >= trail_pct && pos.current_price_sol > pos.entry_price_sol {
                    info!(
                        "🔍 [DRY RUN] TRAILING STOP triggered for {} - Dropped {:.2}% from high",
                        pos.token_symbol, drop_from_high
                    );
                    pos.close(
                        pos.current_price_sol,
                        SimulatedPositionStatus::ClosedTrailingStop,
                        format!(
                            "Trailing stop triggered - dropped {:.2}% from high of {} SOL",
                            drop_from_high, pos.highest_price_sol
                        ),
                    );
                    closed_positions.push(pos.clone());
                    continue;
                }
            }

            // Check max hold time
            if let Some(max_minutes) = max_hold_minutes {
                if hold_duration >= max_minutes as i64 {
                    info!(
                        "🔍 [DRY RUN] MAX HOLD TIME reached for {} - Held for {} minutes",
                        pos.token_symbol, hold_duration
                    );
                    pos.close(
                        pos.current_price_sol,
                        SimulatedPositionStatus::ClosedMaxHoldTime,
                        format!("Max hold time of {} minutes reached", max_minutes),
                    );
                    closed_positions.push(pos.clone());
                }
            }
        }

        drop(positions);

        if !closed_positions.is_empty() {
            self.save().await?;
        }

        Ok(closed_positions)
    }

    /// Get all simulated positions
    pub async fn get_positions(&self) -> Vec<SimulatedPosition> {
        let positions = self.positions.read().await;
        positions.values().cloned().collect()
    }

    /// Get only open positions
    pub async fn get_open_positions(&self) -> Vec<SimulatedPosition> {
        let positions = self.positions.read().await;
        positions.values().filter(|p| p.is_open()).cloned().collect()
    }

    /// Get closed positions
    pub async fn get_closed_positions(&self) -> Vec<SimulatedPosition> {
        let positions = self.positions.read().await;
        positions.values().filter(|p| !p.is_open()).cloned().collect()
    }

    /// Check if we have an open position for a token
    pub async fn has_open_position(&self, token_address: &str) -> bool {
        let positions = self.positions.read().await;
        positions
            .values()
            .any(|p| p.token_address == token_address && p.is_open())
    }

    /// Get simulation statistics
    pub async fn get_stats(&self) -> SimulationStats {
        let positions = self.positions.read().await;

        let mut stats = SimulationStats::default();
        let mut total_pnl_percent = 0.0;
        let mut pnl_count = 0;

        for pos in positions.values() {
            stats.total_simulated_trades += 1;
            stats.would_have_spent_sol += pos.entry_amount_sol;

            if pos.is_open() {
                stats.open_positions += 1;
                stats.total_unrealized_pnl_sol += pos.unrealized_pnl_sol;
                stats.would_have_returned_sol += pos.current_value_sol;
            } else {
                stats.closed_positions += 1;
                if let Some(realized_pnl) = pos.realized_pnl_sol {
                    stats.total_realized_pnl_sol += realized_pnl;
                    stats.would_have_returned_sol += pos.entry_amount_sol + realized_pnl;

                    if realized_pnl > 0.0 {
                        stats.winning_trades += 1;
                    } else {
                        stats.losing_trades += 1;
                    }
                }

                if let Some(pnl_pct) = pos.realized_pnl_percent {
                    total_pnl_percent += pnl_pct;
                    pnl_count += 1;

                    if pnl_pct > stats.best_trade_pnl_percent {
                        stats.best_trade_pnl_percent = pnl_pct;
                    }
                    if stats.worst_trade_pnl_percent == 0.0 || pnl_pct < stats.worst_trade_pnl_percent
                    {
                        stats.worst_trade_pnl_percent = pnl_pct;
                    }
                }
            }
        }

        // Calculate win rate
        if stats.closed_positions > 0 {
            stats.win_rate =
                (stats.winning_trades as f64 / stats.closed_positions as f64) * 100.0;
        }

        // Calculate average P&L percent
        if pnl_count > 0 {
            stats.average_pnl_percent = total_pnl_percent / pnl_count as f64;
        }

        stats
    }

    /// Clear all simulated positions
    pub async fn clear(&self) -> Result<()> {
        let mut positions = self.positions.write().await;
        positions.clear();
        drop(positions);
        self.save().await?;
        info!("🔍 [DRY RUN] Cleared all simulated positions");
        Ok(())
    }

    /// Manually close a simulated position
    pub async fn close_position(&self, position_id: &str) -> Result<SimulatedPosition> {
        let mut positions = self.positions.write().await;

        let pos = positions
            .get_mut(position_id)
            .ok_or_else(|| anyhow::anyhow!("Position not found: {}", position_id))?;

        if !pos.is_open() {
            return Err(anyhow::anyhow!("Position is already closed"));
        }

        pos.close(
            pos.current_price_sol,
            SimulatedPositionStatus::ClosedManual,
            "Manually closed".to_string(),
        );

        let closed_pos = pos.clone();
        drop(positions);

        self.save().await?;

        info!(
            "🔍 [DRY RUN] Manually closed position for {} - P&L: {:.2}%",
            closed_pos.token_symbol,
            closed_pos.realized_pnl_percent.unwrap_or(0.0)
        );

        Ok(closed_pos)
    }
}
