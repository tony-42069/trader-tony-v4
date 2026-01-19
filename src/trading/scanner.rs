//! Token Scanner Module
//!
//! Scans for trading opportunities using Moralis API:
//! - Final Stretch: Discovers tokens in bonding phase meeting criteria
//! - Migrated: Discovers recently graduated tokens meeting criteria
//!
//! This scanner DISCOVERS tokens directly from the API - it does not watch
//! a pre-populated watchlist. This is the correct architecture for
//! strategies that need to find tokens already meeting certain criteria.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::api::birdeye::BirdeyeClient;
use crate::api::moralis::{MoralisClient, MoralisTokenWithHolders};
use crate::trading::strategy::{Strategy, StrategyType};

/// Default scan interval in seconds
const DEFAULT_SCAN_INTERVAL_SECS: u64 = 15;

/// Scanner configuration
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// How often to scan (in seconds)
    pub scan_interval_secs: u64,
    /// Maximum tokens to fetch per scan
    pub max_tokens_per_scan: u32,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: DEFAULT_SCAN_INTERVAL_SECS,
            max_tokens_per_scan: 100,
        }
    }
}

/// Result of scanning - tokens that meet strategy criteria
#[derive(Debug, Clone)]
pub struct ScanCandidate {
    pub token_address: String,
    pub name: String,
    pub symbol: String,
    pub price_usd: f64,
    pub market_cap_usd: f64,
    pub liquidity_usd: f64,
    pub holders: u64,
    pub bonding_progress: Option<f64>,  // For Final Stretch
    pub graduated_at: Option<String>,   // For Migrated
    pub strategy_type: StrategyType,
}

/// Token scanner for Final Stretch and Migrated strategies
/// Uses Moralis API to discover tokens directly
pub struct Scanner {
    moralis_client: Arc<MoralisClient>,
    birdeye_client: Arc<BirdeyeClient>,
    config: ScannerConfig,
    /// Track tokens we've already seen to avoid duplicate signals
    seen_tokens: Arc<RwLock<HashSet<String>>>,
}

impl Scanner {
    /// Create a new scanner with Moralis and Birdeye clients
    pub fn new(
        moralis_client: Arc<MoralisClient>,
        birdeye_client: Arc<BirdeyeClient>,
    ) -> Self {
        Self {
            moralis_client,
            birdeye_client,
            config: ScannerConfig::default(),
            seen_tokens: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create a new scanner with custom config
    pub fn with_config(
        moralis_client: Arc<MoralisClient>,
        birdeye_client: Arc<BirdeyeClient>,
        config: ScannerConfig,
    ) -> Self {
        Self {
            moralis_client,
            birdeye_client,
            config,
            seen_tokens: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Run a single scan cycle for the given strategy
    /// Returns NEW candidates (tokens not seen before in this session)
    pub async fn scan_cycle(&self, strategy: &Strategy) -> Result<Vec<ScanCandidate>> {
        match strategy.strategy_type {
            StrategyType::NewPairs => {
                debug!("NewPairs strategy uses WebSocket discovery, not scanner");
                Ok(vec![])
            }
            StrategyType::FinalStretch => {
                self.scan_final_stretch(strategy).await
            }
            StrategyType::Migrated => {
                self.scan_migrated(strategy).await
            }
        }
    }

    /// Scan for Final Stretch candidates using Moralis bonding endpoint
    async fn scan_final_stretch(&self, strategy: &Strategy) -> Result<Vec<ScanCandidate>> {
        info!("🔍 [FINAL STRETCH] Scanning Moralis for bonding tokens...");

        // Get filter criteria from strategy
        let min_progress = strategy.min_bonding_progress.unwrap_or(20.0);
        let min_market_cap = strategy.min_market_cap_usd.unwrap_or(20_000.0);
        let min_holders = strategy.min_holders as u64;
        let min_volume = strategy.min_volume_usd.unwrap_or(20_000.0);
        let max_age_minutes = strategy.max_token_age_minutes as u64; // IMPORTANT: Filter by age!

        // Use Moralis client to scan and filter (now includes age filter)
        let candidates = self.moralis_client
            .scan_final_stretch(min_progress, min_market_cap, min_holders, max_age_minutes, self.config.max_tokens_per_scan)
            .await
            .context("Failed to scan Final Stretch candidates from Moralis")?;

        if candidates.is_empty() {
            debug!("No Final Stretch candidates found");
            return Ok(vec![]);
        }

        // Filter for new tokens (not seen before) and apply volume filter via Birdeye
        let mut results = Vec::new();
        let mut seen = self.seen_tokens.write().await;

        for candidate in candidates {
            let addr = &candidate.token.token_address;

            // Skip if we've already seen this token
            if seen.contains(addr) {
                debug!("Skipping {} - already seen", candidate.token.symbol);
                continue;
            }

            // STRICT volume filter via Birdeye - FAIL-CLOSE (reject if can't verify volume)
            let (volume_ok, volume) = if min_volume > 0.0 {
                match self.birdeye_client.get_trade_data(addr).await {
                    Ok(Some(trade_data)) => {
                        let vol = trade_data.volume24h_usd
                            .or(trade_data.v24h_usd)
                            .unwrap_or(0.0);
                        if vol < min_volume {
                            info!("   ❌ {} rejected: volume ${:.0} < ${:.0} min",
                                candidate.token.symbol, vol, min_volume);
                            (false, vol)
                        } else {
                            (true, vol)
                        }
                    }
                    Ok(None) => {
                        // No trade data = likely zero volume = REJECT
                        info!("   ❌ {} rejected: no trade data (likely zero volume)", candidate.token.symbol);
                        (false, 0.0)
                    }
                    Err(e) => {
                        // API error = can't verify = REJECT (fail-close)
                        warn!("   ❌ {} rejected: could not fetch volume ({})", candidate.token.symbol, e);
                        (false, 0.0)
                    }
                }
            } else {
                (true, 0.0)
            };

            if volume_ok {
                // Mark as seen
                seen.insert(addr.clone());

                // Create scan candidate
                results.push(ScanCandidate {
                    token_address: addr.clone(),
                    name: candidate.token.name.clone(),
                    symbol: candidate.token.symbol.clone(),
                    price_usd: candidate.token.price_usd_f64(),
                    market_cap_usd: candidate.token.market_cap_usd(),
                    liquidity_usd: candidate.token.liquidity_usd(),
                    holders: candidate.holders,
                    bonding_progress: Some(candidate.token.bonding_progress()),
                    graduated_at: None,
                    strategy_type: StrategyType::FinalStretch,
                });

                info!("✅ [CANDIDATE] {} ({}) - Progress: {:.1}%, MCap: ${:.0}, Vol: ${:.0}, Holders: {}",
                    candidate.token.name, candidate.token.symbol,
                    candidate.token.bonding_progress(), candidate.token.market_cap_usd(),
                    volume, candidate.holders);
            }

            // Small delay between Birdeye calls
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !results.is_empty() {
            info!("✅ [FINAL STRETCH] Found {} NEW candidates to trade", results.len());
        }

        Ok(results)
    }

    /// Scan for Migrated candidates using Moralis graduated endpoint
    async fn scan_migrated(&self, strategy: &Strategy) -> Result<Vec<ScanCandidate>> {
        info!("🔍 [MIGRATED] Scanning Moralis for graduated tokens...");

        // Get filter criteria from strategy
        let min_market_cap = strategy.min_market_cap_usd.unwrap_or(40_000.0);
        let min_holders = strategy.min_holders as u64;
        let max_age_hours = (strategy.max_token_age_minutes / 60) as u64; // Convert minutes to hours
        let min_volume = strategy.min_volume_usd.unwrap_or(40_000.0);

        // Use Moralis client to scan and filter
        let candidates = self.moralis_client
            .scan_migrated(min_market_cap, min_holders, max_age_hours, self.config.max_tokens_per_scan)
            .await
            .context("Failed to scan Migrated candidates from Moralis")?;

        if candidates.is_empty() {
            debug!("No Migrated candidates found");
            return Ok(vec![]);
        }

        // Filter for new tokens and apply volume filter
        let mut results = Vec::new();
        let mut seen = self.seen_tokens.write().await;

        for candidate in candidates {
            let addr = &candidate.token.token_address;

            if seen.contains(addr) {
                debug!("Skipping {} - already seen", candidate.token.symbol);
                continue;
            }

            // Fetch volume from Birdeye
            let volume_ok = if min_volume > 0.0 {
                match self.birdeye_client.get_trade_data(addr).await {
                    Ok(Some(trade_data)) => {
                        let volume = trade_data.volume24h_usd
                            .or(trade_data.v24h_usd)
                            .unwrap_or(0.0);
                        if volume < min_volume {
                            debug!("   {} rejected: volume ${:.0} < ${:.0} min",
                                candidate.token.symbol, volume, min_volume);
                            false
                        } else {
                            true
                        }
                    }
                    _ => {
                        warn!("Could not fetch volume for {} - allowing anyway", candidate.token.symbol);
                        true
                    }
                }
            } else {
                true
            };

            if volume_ok {
                seen.insert(addr.clone());

                results.push(ScanCandidate {
                    token_address: addr.clone(),
                    name: candidate.token.name.clone(),
                    symbol: candidate.token.symbol.clone(),
                    price_usd: candidate.token.price_usd_f64(),
                    market_cap_usd: candidate.token.market_cap_usd(),
                    liquidity_usd: candidate.token.liquidity_usd(),
                    holders: candidate.holders,
                    bonding_progress: None,
                    graduated_at: candidate.token.graduated_at.clone(),
                    strategy_type: StrategyType::Migrated,
                });

                info!("🚀 [NEW CANDIDATE] {} ({}) - MCap: ${:.0}, Holders: {}, Graduated: {:?}",
                    candidate.token.name, candidate.token.symbol,
                    candidate.token.market_cap_usd(), candidate.holders,
                    candidate.token.graduated_at);
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if !results.is_empty() {
            info!("✅ [MIGRATED] Found {} NEW candidates to trade", results.len());
        }

        Ok(results)
    }

    /// Clear seen tokens (e.g., when restarting or changing strategies)
    pub async fn clear_seen_tokens(&self) {
        let mut seen = self.seen_tokens.write().await;
        seen.clear();
        info!("Cleared seen tokens cache");
    }

    /// Get count of seen tokens
    pub async fn seen_count(&self) -> usize {
        self.seen_tokens.read().await.len()
    }

    /// Get scan interval
    pub fn scan_interval(&self) -> Duration {
        Duration::from_secs(self.config.scan_interval_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.scan_interval_secs, 15);
        assert_eq!(config.max_tokens_per_scan, 100);
    }
}
