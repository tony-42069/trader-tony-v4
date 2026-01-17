//! Token Scanner Module
//!
//! Periodically scans watchlist tokens and evaluates them against
//! Final Stretch and Migrated strategy criteria using Birdeye data.

use anyhow::{Context, Result};
use chrono::Utc;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::api::birdeye::{BirdeyeClient, TokenData};
use crate::trading::pumpfun::BondingCurveState;
use crate::trading::strategy::{Strategy, StrategyType};
use crate::trading::watchlist::{Watchlist, WatchlistToken};

/// Default scan interval in seconds
const DEFAULT_SCAN_INTERVAL_SECS: u64 = 15;

/// Result of scanning a single token
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub token: WatchlistToken,
    pub birdeye_data: TokenData,
    pub bonding_state: Option<BondingCurveState>,
    pub meets_criteria: bool,
    pub rejection_reasons: Vec<String>,
}

/// Scanner configuration
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// How often to scan (in seconds)
    pub scan_interval_secs: u64,
    /// Maximum tokens to scan per cycle (for rate limiting)
    pub max_tokens_per_cycle: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: DEFAULT_SCAN_INTERVAL_SECS,
            max_tokens_per_cycle: 20, // Conservative to avoid rate limits
        }
    }
}

/// Token scanner for Final Stretch and Migrated strategies
pub struct Scanner {
    watchlist: Arc<Watchlist>,
    birdeye_client: Arc<BirdeyeClient>,
    rpc_client: Arc<RpcClient>,
    config: ScannerConfig,
    running: Arc<RwLock<bool>>,
}

impl Scanner {
    /// Create a new scanner
    pub fn new(
        watchlist: Arc<Watchlist>,
        birdeye_client: Arc<BirdeyeClient>,
        rpc_client: Arc<RpcClient>,
    ) -> Self {
        Self {
            watchlist,
            birdeye_client,
            rpc_client,
            config: ScannerConfig::default(),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Create a new scanner with custom config
    pub fn with_config(
        watchlist: Arc<Watchlist>,
        birdeye_client: Arc<BirdeyeClient>,
        rpc_client: Arc<RpcClient>,
        config: ScannerConfig,
    ) -> Self {
        Self {
            watchlist,
            birdeye_client,
            rpc_client,
            config,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if the scanner is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Run a single scan cycle
    /// Returns tokens that meet the criteria for the given strategy
    pub async fn scan_cycle(&self, strategy: &Strategy) -> Result<Vec<ScanResult>> {
        let strategy_type = &strategy.strategy_type;
        info!("🔍 Starting scan cycle for {:?} strategy", strategy_type);

        // Get appropriate tokens based on strategy type
        let tokens = match strategy_type {
            StrategyType::NewPairs => {
                debug!("NewPairs strategy uses WebSocket discovery, not scanner");
                return Ok(vec![]);
            }
            StrategyType::FinalStretch => {
                self.watchlist.get_tokens_for_final_stretch().await
            }
            StrategyType::Migrated => {
                self.watchlist.get_tokens_for_migrated().await
            }
        };

        if tokens.is_empty() {
            debug!("No tokens to scan for {:?} strategy", strategy_type);
            return Ok(vec![]);
        }

        info!("📋 Scanning {} tokens for {:?} strategy", tokens.len(), strategy_type);

        // Limit tokens per cycle for rate limiting
        let tokens_to_scan: Vec<_> = tokens
            .into_iter()
            .take(self.config.max_tokens_per_cycle)
            .collect();

        let mut results = Vec::new();

        for token in tokens_to_scan {
            match self.evaluate_token(&token, strategy).await {
                Ok(result) => {
                    // Update watchlist with new status
                    let _ = self.watchlist.update_token_status(
                        &token.mint,
                        result.bonding_state.as_ref().map(|b| b.get_progress_percent()),
                        result.bonding_state.as_ref().map(|b| b.complete).unwrap_or(token.is_migrated),
                    ).await;

                    if result.meets_criteria {
                        results.push(result);
                    }
                }
                Err(e) => {
                    warn!("Error evaluating token {}: {:?}", token.symbol, e);
                }
            }

            // Small delay between tokens to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !results.is_empty() {
            info!("✅ Found {} tokens meeting {:?} criteria", results.len(), strategy_type);
        }

        Ok(results)
    }

    /// Evaluate a single token against strategy criteria
    async fn evaluate_token(&self, token: &WatchlistToken, strategy: &Strategy) -> Result<ScanResult> {
        debug!("Evaluating {} ({}) for {:?}", token.name, token.symbol, strategy.strategy_type);

        // Fetch Birdeye data
        let birdeye_data = self.birdeye_client
            .get_token_data(&token.mint)
            .await
            .context(format!("Failed to fetch Birdeye data for {}", token.symbol))?;

        // Fetch bonding curve state (if not migrated)
        let bonding_state = if !token.is_migrated {
            match self.fetch_bonding_curve_state(&token.bonding_curve).await {
                Ok(state) => Some(state),
                Err(e) => {
                    debug!("Could not fetch bonding curve for {}: {:?}", token.symbol, e);
                    None
                }
            }
        } else {
            None
        };

        // Evaluate against strategy criteria
        let (meets_criteria, rejection_reasons) = match strategy.strategy_type {
            StrategyType::FinalStretch => {
                self.evaluate_final_stretch(token, &birdeye_data, bonding_state.as_ref(), strategy)
            }
            StrategyType::Migrated => {
                self.evaluate_migrated(token, &birdeye_data, bonding_state.as_ref(), strategy)
            }
            StrategyType::NewPairs => (false, vec!["NewPairs uses WebSocket, not scanner".to_string()]),
        };

        if meets_criteria {
            self.log_candidate(&token, &birdeye_data, bonding_state.as_ref(), &strategy.strategy_type);
        } else if !rejection_reasons.is_empty() {
            debug!("❌ {} rejected: {}", token.symbol, rejection_reasons.join(", "));
        }

        Ok(ScanResult {
            token: token.clone(),
            birdeye_data,
            bonding_state,
            meets_criteria,
            rejection_reasons,
        })
    }

    /// Evaluate token for Final Stretch strategy
    fn evaluate_final_stretch(
        &self,
        token: &WatchlistToken,
        birdeye: &TokenData,
        bonding: Option<&BondingCurveState>,
        strategy: &Strategy,
    ) -> (bool, Vec<String>) {
        let mut reasons = Vec::new();

        // Age: 0-60 minutes
        let age_minutes = token.age_minutes();
        if age_minutes > strategy.max_token_age_minutes as i64 {
            reasons.push(format!("Age {} min > {} max", age_minutes, strategy.max_token_age_minutes));
        }

        // Holders: minimum from strategy (default 50)
        if birdeye.holders < strategy.min_holders as u64 {
            reasons.push(format!("Holders {} < {} min", birdeye.holders, strategy.min_holders));
        }

        // Volume: minimum from strategy (default $20,000)
        if let Some(min_vol) = strategy.min_volume_usd {
            if birdeye.volume_24h_usd < min_vol {
                reasons.push(format!("Volume ${:.0} < ${:.0} min", birdeye.volume_24h_usd, min_vol));
            }
        }

        // Market Cap: minimum from strategy (default $20,000)
        if let Some(min_mc) = strategy.min_market_cap_usd {
            if birdeye.market_cap_usd < min_mc {
                reasons.push(format!("MCap ${:.0} < ${:.0} min", birdeye.market_cap_usd, min_mc));
            }
        }

        // Bonding Progress: minimum from strategy (default 20%)
        if let Some(min_progress) = strategy.min_bonding_progress {
            let progress = bonding.map(|b| b.get_progress_percent()).unwrap_or(0.0);
            if progress < min_progress {
                reasons.push(format!("Progress {:.1}% < {:.1}% min", progress, min_progress));
            }
        }

        // Must NOT be migrated
        if let Some(ref b) = bonding {
            if b.complete {
                reasons.push("Already migrated".to_string());
            }
        }

        (reasons.is_empty(), reasons)
    }

    /// Evaluate token for Migrated strategy
    fn evaluate_migrated(
        &self,
        token: &WatchlistToken,
        birdeye: &TokenData,
        bonding: Option<&BondingCurveState>,
        strategy: &Strategy,
    ) -> (bool, Vec<String>) {
        let mut reasons = Vec::new();

        // Age: 0-1440 minutes (24 hours)
        let age_minutes = token.age_minutes();
        if age_minutes > strategy.max_token_age_minutes as i64 {
            reasons.push(format!("Age {} min > {} max", age_minutes, strategy.max_token_age_minutes));
        }

        // Holders: minimum from strategy (default 75)
        if birdeye.holders < strategy.min_holders as u64 {
            reasons.push(format!("Holders {} < {} min", birdeye.holders, strategy.min_holders));
        }

        // Volume: minimum from strategy (default $40,000)
        if let Some(min_vol) = strategy.min_volume_usd {
            if birdeye.volume_24h_usd < min_vol {
                reasons.push(format!("Volume ${:.0} < ${:.0} min", birdeye.volume_24h_usd, min_vol));
            }
        }

        // Market Cap: minimum from strategy (default $40,000)
        if let Some(min_mc) = strategy.min_market_cap_usd {
            if birdeye.market_cap_usd < min_mc {
                reasons.push(format!("MCap ${:.0} < ${:.0} min", birdeye.market_cap_usd, min_mc));
            }
        }

        // Must BE migrated
        let is_migrated = token.is_migrated || bonding.map(|b| b.complete).unwrap_or(false);
        if !is_migrated {
            reasons.push("Not yet migrated".to_string());
        }

        (reasons.is_empty(), reasons)
    }

    /// Fetch bonding curve state from on-chain
    async fn fetch_bonding_curve_state(&self, bonding_curve_address: &str) -> Result<BondingCurveState> {
        use solana_sdk::pubkey::Pubkey;
        use std::str::FromStr;
        use borsh::BorshDeserialize;

        let pubkey = Pubkey::from_str(bonding_curve_address)
            .context("Invalid bonding curve address")?;

        let account = self.rpc_client
            .get_account(&pubkey)
            .await
            .context("Failed to fetch bonding curve account")?;

        // Skip 8-byte discriminator
        if account.data.len() < 8 {
            anyhow::bail!("Account data too small");
        }

        let state = BondingCurveState::try_from_slice(&account.data[8..])
            .context("Failed to deserialize bonding curve state")?;

        Ok(state)
    }

    /// Log a candidate token that meets criteria
    fn log_candidate(
        &self,
        token: &WatchlistToken,
        birdeye: &TokenData,
        bonding: Option<&BondingCurveState>,
        strategy_type: &StrategyType,
    ) {
        let age = token.age_minutes();
        let progress = bonding.map(|b| b.get_progress_percent());

        match strategy_type {
            StrategyType::FinalStretch => {
                info!("🔥 [FINAL STRETCH] {} ({}) meeting criteria!", token.name, token.symbol);
                info!("   Age: {} min | Holders: {} | Volume: ${:.0}",
                    age, birdeye.holders, birdeye.volume_24h_usd);
                info!("   Market Cap: ${:.0} | Progress: {:.1}%",
                    birdeye.market_cap_usd, progress.unwrap_or(0.0));
            }
            StrategyType::Migrated => {
                info!("🚀 [MIGRATED] {} ({}) meeting criteria!", token.name, token.symbol);
                info!("   Age: {} min | Holders: {} | Volume: ${:.0}",
                    age, birdeye.holders, birdeye.volume_24h_usd);
                info!("   Market Cap: ${:.0} | Status: Graduated",
                    birdeye.market_cap_usd);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.scan_interval_secs, 15);
        assert_eq!(config.max_tokens_per_cycle, 20);
    }
}
