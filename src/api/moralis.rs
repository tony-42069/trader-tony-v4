//! Moralis API Client for Pump.fun Token Discovery
//!
//! Provides access to Moralis endpoints for discovering Pump.fun tokens:
//! - /token/mainnet/exchange/pumpfun/bonding - Tokens in bonding phase (for Final Stretch)
//! - /token/mainnet/exchange/pumpfun/graduated - Graduated tokens (for Migrated)
//! - /token/mainnet/holders/{address} - Holder count for a token

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const MORALIS_SOLANA_BASE_URL: &str = "https://solana-gateway.moralis.io";
const MORALIS_DEEP_INDEX_BASE_URL: &str = "https://deep-index.moralis.io/api/v2.2";

// ============================================================================
// Response Structures
// ============================================================================

/// Response wrapper for bonding/graduated endpoints
#[derive(Debug, Deserialize)]
pub struct MoralisTokenListResponse {
    pub result: Vec<MoralisPumpToken>,
    pub cursor: Option<String>,
}

/// A Pump.fun token from Moralis API
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoralisPumpToken {
    pub token_address: String,
    pub name: String,
    pub symbol: String,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub price_usd: Option<String>,
    #[serde(default)]
    pub liquidity: Option<String>,
    #[serde(default)]
    pub fully_diluted_valuation: Option<String>,  // This is market cap for 1B supply tokens
    #[serde(default)]
    pub bonding_curve_progress: Option<f64>,       // 0-100 percentage
    #[serde(default)]
    pub graduated_at: Option<String>,              // ISO timestamp for graduated tokens
    #[serde(default)]
    pub created_at: Option<String>,                // Token creation timestamp
}

impl MoralisPumpToken {
    /// Get market cap in USD (from fullyDilutedValuation)
    pub fn market_cap_usd(&self) -> f64 {
        self.fully_diluted_valuation
            .as_ref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    /// Get liquidity in USD
    pub fn liquidity_usd(&self) -> f64 {
        self.liquidity
            .as_ref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    /// Get price in USD
    pub fn price_usd_f64(&self) -> f64 {
        self.price_usd
            .as_ref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    /// Get bonding progress (0-100), returns None if data is missing
    pub fn bonding_progress(&self) -> Option<f64> {
        self.bonding_curve_progress
    }
}

/// Response for holder stats endpoint
/// NOTE: totalHolders is signed because Moralis occasionally returns negative
/// values (data glitch). A failed parse here used to abort entire scan cycles.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderStatsResponse {
    pub total_holders: Option<i64>,
}

impl HolderStatsResponse {
    /// Holder count, clamped to a sane value
    pub fn holder_count(&self) -> u64 {
        self.total_holders.map(|h| h.max(0) as u64).unwrap_or(0)
    }
}

/// Combined token data with holder count
#[derive(Debug, Clone)]
pub struct MoralisTokenWithHolders {
    pub token: MoralisPumpToken,
    pub holders: u64,
}

/// Native (SOL) price component of the token price response
#[derive(Debug, Clone, Deserialize)]
pub struct NativePrice {
    pub value: String,
    pub decimals: u8,
}

/// Response from /token/mainnet/{address}/price
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPriceData {
    #[serde(default)]
    pub usd_price: Option<f64>,
    #[serde(default)]
    pub native_price: Option<NativePrice>,
    /// Main trading pair for the token - used to look up pair stats
    #[serde(default)]
    pub pair_address: Option<String>,
}

impl TokenPriceData {
    pub fn usd_price_f64(&self) -> f64 {
        self.usd_price.unwrap_or(0.0).max(0.0)
    }

    /// Price per token in SOL, derived from nativePrice (value / 10^decimals)
    pub fn price_sol(&self) -> Option<f64> {
        self.native_price.as_ref().and_then(|np| {
            np.value
                .parse::<f64>()
                .ok()
                .map(|v| (v / 10f64.powi(np.decimals as i32)).max(0.0))
        })
    }
}

/// One metric across Moralis analytics timeframes; only the 24h window is used.
/// Values are f64 (not u64) so glitched negative numbers parse instead of
/// aborting - same lesson as the holders endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TimeframeMetric {
    #[serde(rename = "24h", default)]
    pub h24: Option<f64>,
}

/// Response from deep-index /tokens/{address}/analytics?chain=solana
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAnalytics {
    #[serde(default)]
    pub total_buy_volume: TimeframeMetric,
    #[serde(default)]
    pub total_sell_volume: TimeframeMetric,
    #[serde(default)]
    pub total_buys: TimeframeMetric,
    #[serde(default)]
    pub total_sells: TimeframeMetric,
    #[serde(default)]
    pub unique_wallets: TimeframeMetric,
}

impl TokenAnalytics {
    pub fn volume_24h_usd(&self) -> f64 {
        self.total_buy_volume.h24.unwrap_or(0.0).max(0.0)
            + self.total_sell_volume.h24.unwrap_or(0.0).max(0.0)
    }

    pub fn buys_24h(&self) -> u64 {
        self.total_buys.h24.unwrap_or(0.0).max(0.0) as u64
    }

    pub fn sells_24h(&self) -> u64 {
        self.total_sells.h24.unwrap_or(0.0).max(0.0) as u64
    }

    pub fn unique_wallets_24h(&self) -> u64 {
        self.unique_wallets.h24.unwrap_or(0.0).max(0.0) as u64
    }
}

/// Response from /token/mainnet/pairs/{pairAddress}/stats
/// NOTE: timeframe keys here are "5min"/"1h"/"4h"/"24h" - only 24h is read,
/// which TimeframeMetric handles for both this and the analytics endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStats {
    #[serde(default)]
    pub buys: TimeframeMetric,
    #[serde(default)]
    pub sells: TimeframeMetric,
    #[serde(default)]
    pub buyers: TimeframeMetric,
    #[serde(default)]
    pub sellers: TimeframeMetric,
    #[serde(default)]
    pub total_volume: TimeframeMetric,
}

/// Unified 24h trade metrics, sourced from whichever Moralis endpoint responded
#[derive(Debug, Clone, PartialEq)]
pub struct TradeMetrics {
    pub volume_24h_usd: f64,
    pub buys_24h: u64,
    pub sells_24h: u64,
    pub unique_wallets_24h: u64,
}

impl TradeMetrics {
    pub fn from_analytics(a: &TokenAnalytics) -> Self {
        Self {
            volume_24h_usd: a.volume_24h_usd(),
            buys_24h: a.buys_24h(),
            sells_24h: a.sells_24h(),
            unique_wallets_24h: a.unique_wallets_24h(),
        }
    }

    pub fn from_pair_stats(p: &PairStats) -> Self {
        let buyers = p.buyers.h24.unwrap_or(0.0).max(0.0) as u64;
        let sellers = p.sellers.h24.unwrap_or(0.0).max(0.0) as u64;
        Self {
            volume_24h_usd: p.total_volume.h24.unwrap_or(0.0).max(0.0),
            buys_24h: p.buys.h24.unwrap_or(0.0).max(0.0) as u64,
            sells_24h: p.sells.h24.unwrap_or(0.0).max(0.0) as u64,
            // Pair stats has no uniqueWallets; max(buyers, sellers) is a
            // guaranteed lower bound on distinct wallets in the window.
            unique_wallets_24h: buyers.max(sellers),
        }
    }
}

// ============================================================================
// Moralis Client
// ============================================================================

#[derive(Debug)]
pub struct MoralisClient {
    api_key: String,
    client: Client,
    /// Cached SOL/USD price (60s TTL; stale value served if refresh fails)
    sol_price_cache: std::sync::Mutex<Option<(f64, Instant)>>,
}

impl MoralisClient {
    /// Create a new Moralis client
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client for Moralis"),
            sol_price_cache: std::sync::Mutex::new(None),
        }
    }

    /// Fetch current token price (USD + native SOL) for a mint
    pub async fn get_token_price(&self, mint: &str) -> Result<Option<TokenPriceData>> {
        let url = format!("{}/token/mainnet/{}/price", MORALIS_SOLANA_BASE_URL, mint);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Moralis price endpoint")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis price API error for {}: {} - {}", mint, status, error_text);
            return Ok(None);
        }

        match response.json::<TokenPriceData>().await {
            Ok(p) => Ok(Some(p)),
            Err(e) => {
                warn!("Failed to parse Moralis price response for {}: {:?}", mint, e);
                Ok(None)
            }
        }
    }

    /// Fetch 24h trading analytics (volume, buys/sells, unique wallets) for a mint
    pub async fn get_token_analytics(&self, mint: &str) -> Result<Option<TokenAnalytics>> {
        let url = format!("{}/tokens/{}/analytics", MORALIS_DEEP_INDEX_BASE_URL, mint);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("chain", "solana")])
            .send()
            .await
            .context("Failed to send request to Moralis analytics endpoint")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis analytics API error for {}: {} - {}", mint, status, error_text);
            return Ok(None);
        }

        match response.json::<TokenAnalytics>().await {
            Ok(a) => Ok(Some(a)),
            Err(e) => {
                warn!("Failed to parse Moralis analytics response for {}: {:?}", mint, e);
                Ok(None)
            }
        }
    }

    /// Fetch 24h stats for a specific trading pair
    pub async fn get_pair_stats(&self, pair_address: &str) -> Result<Option<PairStats>> {
        let url = format!("{}/token/mainnet/pairs/{}/stats", MORALIS_SOLANA_BASE_URL, pair_address);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Moralis pair stats endpoint")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis pair stats API error for {}: {} - {}", pair_address, status, error_text);
            return Ok(None);
        }

        match response.json::<PairStats>().await {
            Ok(s) => Ok(Some(s)),
            Err(e) => {
                warn!("Failed to parse Moralis pair stats response for {}: {:?}", pair_address, e);
                Ok(None)
            }
        }
    }

    /// 24h trade metrics for a mint, trying sources in order:
    /// 1. token analytics (one call, exact unique wallets)
    /// 2. price endpoint -> pair stats (two calls, buyer/seller lower bound)
    /// Returns None when no source responded - caller decides the fallback.
    pub async fn get_trade_metrics(&self, mint: &str) -> Option<TradeMetrics> {
        if let Ok(Some(a)) = self.get_token_analytics(mint).await {
            return Some(TradeMetrics::from_analytics(&a));
        }

        debug!("Analytics unavailable for {} - falling back to pair stats", mint);
        if let Ok(Some(price)) = self.get_token_price(mint).await {
            if let Some(pair) = price.pair_address.as_deref() {
                if let Ok(Some(stats)) = self.get_pair_stats(pair).await {
                    return Some(TradeMetrics::from_pair_stats(&stats));
                }
            }
        }

        None
    }

    /// SOL/USD price via the wSOL price endpoint, cached for 60 seconds.
    /// Serves the stale cached value if a refresh fails; last resort $150.
    pub async fn get_sol_price_usd(&self) -> f64 {
        const CACHE_TTL: Duration = Duration::from_secs(60);

        {
            let cache = self.sol_price_cache.lock().unwrap();
            if let Some((price, at)) = *cache {
                if at.elapsed() < CACHE_TTL {
                    return price;
                }
            }
        }

        let fresh = match self.get_token_price(crate::api::jupiter::SOL_MINT).await {
            Ok(Some(p)) if p.usd_price_f64() > 0.0 => Some(p.usd_price_f64()),
            _ => None,
        };

        let mut cache = self.sol_price_cache.lock().unwrap();
        match fresh {
            Some(price) => {
                *cache = Some((price, Instant::now()));
                info!("SOL price updated via Moralis: ${:.2}", price);
                price
            }
            None => {
                if let Some((stale, _)) = *cache {
                    warn!("Moralis SOL price refresh failed - serving stale value ${:.2}", stale);
                    stale
                } else {
                    warn!("Moralis SOL price unavailable and no cache - using fallback $150");
                    150.0
                }
            }
        }
    }

    /// Fetch tokens in bonding phase (for Final Stretch strategy)
    /// Returns tokens with bondingCurveProgress < 100%
    pub async fn get_bonding_tokens(&self, limit: u32) -> Result<Vec<MoralisPumpToken>> {
        let url = format!(
            "{}/token/mainnet/exchange/pumpfun/bonding",
            MORALIS_SOLANA_BASE_URL
        );

        debug!("Fetching bonding tokens from Moralis: {}", url);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("limit", limit.to_string())])
            .send()
            .await
            .context("Failed to send request to Moralis bonding endpoint")?;

        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            warn!("Moralis API returned 403 - may be rate limited or endpoint requires paid tier");
            return Ok(vec![]);
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis bonding API error: {} - {}", status, error_text);
            return Ok(vec![]);
        }

        let response_data: MoralisTokenListResponse = response
            .json()
            .await
            .context("Failed to parse Moralis bonding response")?;

        info!("📡 Moralis: Got {} bonding tokens", response_data.result.len());
        Ok(response_data.result)
    }

    /// Fetch graduated tokens (for Migrated strategy)
    /// Returns tokens that have completed bonding and migrated to Raydium
    pub async fn get_graduated_tokens(&self, limit: u32) -> Result<Vec<MoralisPumpToken>> {
        let url = format!(
            "{}/token/mainnet/exchange/pumpfun/graduated",
            MORALIS_SOLANA_BASE_URL
        );

        debug!("Fetching graduated tokens from Moralis: {}", url);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("limit", limit.to_string())])
            .send()
            .await
            .context("Failed to send request to Moralis graduated endpoint")?;

        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            warn!("Moralis graduated endpoint returned 403 - may require paid tier");
            return Ok(vec![]);
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis graduated API error: {} - {}", status, error_text);
            return Ok(vec![]);
        }

        let response_data: MoralisTokenListResponse = response
            .json()
            .await
            .context("Failed to parse Moralis graduated response")?;

        info!("📡 Moralis: Got {} graduated tokens", response_data.result.len());
        Ok(response_data.result)
    }

    /// Fetch holder count for a specific token
    pub async fn get_holder_count(&self, token_address: &str) -> Result<u64> {
        let url = format!(
            "{}/token/mainnet/holders/{}",
            MORALIS_SOLANA_BASE_URL, token_address
        );

        debug!("Fetching holder count from Moralis for {}", token_address);

        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Moralis holders endpoint")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("Moralis holders API error for {}: {} - {}", token_address, status, error_text);
            return Ok(0);
        }

        let response_data: HolderStatsResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse Moralis holders response for {}: {:?}; treating as 0 holders", token_address, e);
                return Ok(0);
            }
        };

        Ok(response_data.holder_count())
    }

    /// Scan for Final Stretch candidates
    /// Filters bonding tokens by: progress >= min_progress, market_cap >= min_mcap, age <= max_age
    /// Then fetches holder counts for each candidate
    pub async fn scan_final_stretch(
        &self,
        min_progress: f64,
        min_market_cap: f64,
        min_holders: u64,
        max_age_minutes: u64,
        limit: u32,
    ) -> Result<Vec<MoralisTokenWithHolders>> {
        info!("🔍 Scanning for Final Stretch candidates (progress >= {:.0}%, mcap >= ${:.0}, holders >= {}, age <= {} min)",
            min_progress, min_market_cap, min_holders, max_age_minutes);

        // 1. Get bonding tokens from Moralis
        let bonding_tokens = self.get_bonding_tokens(limit).await?;

        if bonding_tokens.is_empty() {
            debug!("No bonding tokens returned from Moralis");
            return Ok(vec![]);
        }

        let now = chrono::Utc::now();

        // 2. Filter by progress, market cap, and AGE
        let mut rejected_no_progress = 0u32;
        let mut rejected_low_progress = 0u32;
        let mut rejected_low_mcap = 0u32;
        let mut allowed_no_timestamp = 0u32;
        let mut rejected_bad_timestamp = 0u32;
        let mut rejected_too_old = 0u32;
        let total_input = bonding_tokens.len();

        let candidates: Vec<_> = bonding_tokens
            .into_iter()
            .filter(|t| {
                let mcap = t.market_cap_usd();

                // Reject tokens with missing bonding progress
                let progress = match t.bonding_progress() {
                    Some(p) => p,
                    None => {
                        rejected_no_progress += 1;
                        return false;
                    }
                };

                // Check basic criteria
                if progress < min_progress {
                    rejected_low_progress += 1;
                    return false;
                }
                if mcap < min_market_cap {
                    rejected_low_mcap += 1;
                    return false;
                }

                // Check token age - CRITICAL: reject tokens with missing or invalid timestamps
                match t.created_at.as_ref() {
                    Some(created_at) => {
                        match chrono::DateTime::parse_from_rfc3339(created_at) {
                            Ok(created_time) => {
                                let age_minutes = (now - created_time.with_timezone(&chrono::Utc)).num_minutes();
                                if age_minutes < 0 || age_minutes as u64 > max_age_minutes {
                                    rejected_too_old += 1;
                                    return false;
                                }
                            }
                            Err(_) => {
                                rejected_bad_timestamp += 1;
                                return false;
                            }
                        }
                    }
                    None => {
                        // Allow tokens without timestamp - Moralis often doesn't include it
                        // The token still passed progress and mcap checks from Moralis data
                        allowed_no_timestamp += 1;
                    }
                }

                true
            })
            .collect();

        info!("   Filter results: {}/{} passed ({} had no timestamp but allowed) | Rejected: {} no progress, {} low progress, {} low mcap, {} too old, {} bad timestamp",
            candidates.len(), total_input, allowed_no_timestamp,
            rejected_no_progress, rejected_low_progress, rejected_low_mcap,
            rejected_too_old, rejected_bad_timestamp);

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        info!("   {} tokens passed initial filters, fetching holder counts...", candidates.len());

        // 3. Fetch holder counts and filter
        // A failure for ONE token must not abort the whole batch - skip it instead.
        let mut results = Vec::new();
        for token in candidates {
            // Small delay to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;

            let holders = match self.get_holder_count(&token.token_address).await {
                Ok(h) => h,
                Err(e) => {
                    warn!("   {} skipped: holder lookup failed ({:?})", token.symbol, e);
                    continue;
                }
            };

            if holders >= min_holders {
                info!("🔥 [FINAL STRETCH] {} ({}) - Progress: {:.1}%, MCap: ${:.0}, Holders: {}",
                    token.name, token.symbol, token.bonding_progress().unwrap_or(0.0), token.market_cap_usd(), holders);
                results.push(MoralisTokenWithHolders { token, holders });
            } else {
                debug!("   {} rejected: {} holders < {} min", token.symbol, holders, min_holders);
            }
        }

        info!("✅ Found {} Final Stretch candidates", results.len());
        Ok(results)
    }

    /// Scan for Migrated candidates
    /// Filters graduated tokens by: market_cap >= min_mcap, graduated within max_age_hours
    /// Then fetches holder counts for each candidate
    pub async fn scan_migrated(
        &self,
        min_market_cap: f64,
        min_holders: u64,
        max_age_hours: u64,
        limit: u32,
    ) -> Result<Vec<MoralisTokenWithHolders>> {
        info!("🔍 Scanning for Migrated candidates (mcap >= ${:.0}, holders >= {}, age <= {}h)",
            min_market_cap, min_holders, max_age_hours);

        // 1. Get graduated tokens from Moralis
        let graduated_tokens = self.get_graduated_tokens(limit).await?;

        if graduated_tokens.is_empty() {
            debug!("No graduated tokens returned from Moralis");
            return Ok(vec![]);
        }

        // 2. Filter by market cap and graduation age
        let now = chrono::Utc::now();
        let candidates: Vec<_> = graduated_tokens
            .into_iter()
            .filter(|t| {
                let mcap = t.market_cap_usd();
                if mcap < min_market_cap {
                    return false;
                }

                // Check graduation age - MUST have graduated_at timestamp
                match t.graduated_at.as_ref() {
                    Some(grad_at) => {
                        match chrono::DateTime::parse_from_rfc3339(grad_at) {
                            Ok(grad_time) => {
                                let age_hours = (now - grad_time.with_timezone(&chrono::Utc)).num_hours();
                                if age_hours < 0 || age_hours as u64 > max_age_hours {
                                    return false;
                                }
                            }
                            Err(_) => {
                                debug!("   {} rejected: unparseable graduated_at timestamp", t.symbol);
                                return false;
                            }
                        }
                    }
                    None => {
                        debug!("   {} rejected: missing graduated_at timestamp - cannot verify age", t.symbol);
                        return false;
                    }
                }

                true
            })
            .collect();

        if candidates.is_empty() {
            debug!("No tokens passed mcap/age filters");
            return Ok(vec![]);
        }

        info!("   {} tokens passed initial filters, fetching holder counts...", candidates.len());

        // 3. Fetch holder counts and filter
        // A failure for ONE token must not abort the whole batch - skip it instead.
        let mut results = Vec::new();
        for token in candidates {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let holders = match self.get_holder_count(&token.token_address).await {
                Ok(h) => h,
                Err(e) => {
                    warn!("   {} skipped: holder lookup failed ({:?})", token.symbol, e);
                    continue;
                }
            };

            if holders >= min_holders {
                info!("🚀 [MIGRATED] {} ({}) - MCap: ${:.0}, Holders: {}, Graduated: {:?}",
                    token.name, token.symbol, token.market_cap_usd(), holders, token.graduated_at);
                results.push(MoralisTokenWithHolders { token, holders });
            } else {
                debug!("   {} rejected: {} holders < {} min", token.symbol, holders, min_holders);
            }
        }

        info!("✅ Found {} Migrated candidates", results.len());
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_stats_parses_24h_fields() {
        // Real response shape from /token/mainnet/pairs/{pair}/stats (probed 2026-06-10)
        // NOTE: timeframe keys here are "5min"/"1h"/"4h"/"24h" (unlike analytics' "5m"/"6h")
        let json = r#"{
            "tokenAddress": "7J5pp54YwSNhqungZUWvojgGB7ws3WihkXMUiq1ipump",
            "pairLabel": "CYBER/SOL",
            "exchange": "PumpSwap",
            "currentUsdPrice": "0.00004544",
            "totalLiquidityUsd": "13647.16",
            "buys": {"5min": 24, "1h": 88, "4h": 88, "24h": 88},
            "sells": {"5min": 65, "1h": 177, "4h": 177, "24h": 177},
            "buyers": {"5min": 21, "1h": 59, "4h": 59, "24h": 59},
            "sellers": {"5min": 54, "1h": 124, "4h": 124, "24h": 124},
            "totalVolume": {"5min": 3990.95, "1h": 14617.25, "4h": 14617.25, "24h": 14617.258490689836}
        }"#;
        let stats: PairStats = serde_json::from_str(json).unwrap();
        let m = TradeMetrics::from_pair_stats(&stats);
        assert!((m.volume_24h_usd - 14617.258490689836).abs() < 0.01);
        assert_eq!(m.buys_24h, 88);
        assert_eq!(m.sells_24h, 177);
        // unique wallets estimated as max(buyers, sellers) - a lower bound
        assert_eq!(m.unique_wallets_24h, 124);
    }

    #[test]
    fn trade_metrics_from_analytics_maps_fields() {
        let json = r#"{
            "totalBuyVolume": {"24h": 100.0},
            "totalSellVolume": {"24h": 50.0},
            "totalBuys": {"24h": 10},
            "totalSells": {"24h": 4},
            "uniqueWallets": {"24h": 12}
        }"#;
        let a: TokenAnalytics = serde_json::from_str(json).unwrap();
        let m = TradeMetrics::from_analytics(&a);
        assert!((m.volume_24h_usd - 150.0).abs() < 1e-9);
        assert_eq!(m.buys_24h, 10);
        assert_eq!(m.sells_24h, 4);
        assert_eq!(m.unique_wallets_24h, 12);
    }

    #[test]
    fn token_price_exposes_pair_address() {
        let json = r#"{"tokenAddress":"X","pairAddress":"PAIR123","usdPrice":1.0}"#;
        let price: TokenPriceData = serde_json::from_str(json).unwrap();
        assert_eq!(price.pair_address.as_deref(), Some("PAIR123"));
    }

    #[test]
    fn token_price_parses_native_sol_price() {
        // Real response shape from /token/mainnet/{mint}/price (probed 2026-06-09)
        let json = r#"{
            "tokenAddress": "5E6qqE9seGbgxVau86Em5GbxeLy3W4LaMPtchkYppump",
            "pairAddress": "2JjrEFMJAGWiLk2Kt7kraCvukJurMW3QBAdbxNjgL5mE",
            "exchangeName": "PumpSwap",
            "nativePrice": {"value": "23.61320705237", "symbol": "WSOL", "name": "Wrapped Solana", "decimals": 9},
            "usdPrice": 0.00000153866139971182
        }"#;
        let price: TokenPriceData = serde_json::from_str(json).unwrap();
        assert!((price.usd_price_f64() - 0.00000153866139971182).abs() < 1e-18);
        let sol = price.price_sol().expect("native price present");
        assert!((sol - 2.361320705237e-8).abs() < 1e-15);
    }

    #[test]
    fn token_price_handles_missing_native_price() {
        let json = r#"{"tokenAddress":"X","usdPrice":1.5}"#;
        let price: TokenPriceData = serde_json::from_str(json).unwrap();
        assert_eq!(price.price_sol(), None);
        assert!((price.usd_price_f64() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn token_analytics_parses_24h_metrics() {
        // Real response shape from /api/v2.2/tokens/{mint}/analytics?chain=solana (probed 2026-06-09)
        let json = r#"{
            "chainId": "solana",
            "tokenAddress": "5E6qqE9seGbgxVau86Em5GbxeLy3W4LaMPtchkYppump",
            "totalBuyVolume": {"5m": 96.19, "1h": 9857.71, "6h": 9857.71, "24h": 9857.719029},
            "totalSellVolume": {"5m": 3245.19, "1h": 24935.85, "6h": 24935.85, "24h": 24935.855429},
            "totalBuyers": {"5m": 7, "1h": 93, "6h": 93, "24h": 93},
            "totalSellers": {"5m": 126, "1h": 450, "6h": 450, "24h": 450},
            "totalBuys": {"5m": 22, "1h": 201, "6h": 201, "24h": 201},
            "totalSells": {"5m": 165, "1h": 1021, "6h": 1021, "24h": 1021},
            "uniqueWallets": {"5m": 131, "1h": 471, "6h": 471, "24h": 471},
            "pricePercentChange": {"5m": -90.8, "1h": 0, "6h": 0, "24h": 0},
            "usdPrice": "0.00000153866139971182",
            "totalLiquidityUsd": "0",
            "totalFullyDilutedValuation": "0"
        }"#;
        let a: TokenAnalytics = serde_json::from_str(json).unwrap();
        assert!((a.volume_24h_usd() - (9857.719029 + 24935.855429)).abs() < 0.01);
        assert_eq!(a.buys_24h(), 201);
        assert_eq!(a.sells_24h(), 1021);
        assert_eq!(a.unique_wallets_24h(), 471);
    }

    #[test]
    fn token_analytics_clamps_negative_and_missing_values() {
        // Same defensive posture as holders: glitched negatives must not break anything
        let json = r#"{"totalBuys":{"24h":-5},"totalSells":{"24h":10},"uniqueWallets":{"24h":-1}}"#;
        let a: TokenAnalytics = serde_json::from_str(json).unwrap();
        assert_eq!(a.buys_24h(), 0);
        assert_eq!(a.sells_24h(), 10);
        assert_eq!(a.unique_wallets_24h(), 0);
        assert_eq!(a.volume_24h_usd(), 0.0);
    }

    #[test]
    fn holder_stats_tolerates_negative_count() {
        // Moralis occasionally returns a negative totalHolders (data glitch).
        // This must parse successfully and clamp to 0, not abort the scan.
        let resp: HolderStatsResponse =
            serde_json::from_str(r#"{"totalHolders":-72}"#).expect("negative count must parse");
        assert_eq!(resp.holder_count(), 0);
    }

    #[test]
    fn holder_stats_parses_normal_count() {
        let resp: HolderStatsResponse =
            serde_json::from_str(r#"{"totalHolders":902}"#).unwrap();
        assert_eq!(resp.holder_count(), 902);
    }

    #[test]
    fn holder_stats_parses_missing_count() {
        let resp: HolderStatsResponse = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(resp.holder_count(), 0);
    }

    #[test]
    fn test_moralis_pump_token_parsing() {
        let json = r#"{
            "tokenAddress": "ABC123",
            "name": "Test Token",
            "symbol": "TEST",
            "priceUsd": "0.001",
            "liquidity": "5000",
            "fullyDilutedValuation": "25000",
            "bondingCurveProgress": 45.5
        }"#;

        let token: MoralisPumpToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.token_address, "ABC123");
        assert_eq!(token.symbol, "TEST");
        assert!((token.market_cap_usd() - 25000.0).abs() < 0.01);
        assert!((token.bonding_progress().unwrap() - 45.5).abs() < 0.01);
    }
}
