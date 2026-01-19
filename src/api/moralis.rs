//! Moralis API Client for Pump.fun Token Discovery
//!
//! Provides access to Moralis endpoints for discovering Pump.fun tokens:
//! - /token/mainnet/exchange/pumpfun/bonding - Tokens in bonding phase (for Final Stretch)
//! - /token/mainnet/exchange/pumpfun/graduated - Graduated tokens (for Migrated)
//! - /token/mainnet/holders/{address} - Holder count for a token

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const MORALIS_SOLANA_BASE_URL: &str = "https://solana-gateway.moralis.io";

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

    /// Get bonding progress (0-100)
    pub fn bonding_progress(&self) -> f64 {
        self.bonding_curve_progress.unwrap_or(0.0)
    }
}

/// Response for holder stats endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderStatsResponse {
    pub total_holders: Option<u64>,
}

/// Combined token data with holder count
#[derive(Debug, Clone)]
pub struct MoralisTokenWithHolders {
    pub token: MoralisPumpToken,
    pub holders: u64,
}

// ============================================================================
// Moralis Client
// ============================================================================

#[derive(Debug, Clone)]
pub struct MoralisClient {
    api_key: String,
    client: Client,
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

        let response_data: HolderStatsResponse = response
            .json()
            .await
            .context("Failed to parse Moralis holders response")?;

        Ok(response_data.total_holders.unwrap_or(0))
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
        let candidates: Vec<_> = bonding_tokens
            .into_iter()
            .filter(|t| {
                let progress = t.bonding_progress();
                let mcap = t.market_cap_usd();

                // Check basic criteria
                if progress < min_progress || mcap < min_market_cap {
                    return false;
                }

                // Check token age - CRITICAL: reject old tokens
                if let Some(ref created_at) = t.created_at {
                    if let Ok(created_time) = chrono::DateTime::parse_from_rfc3339(created_at) {
                        let age_minutes = (now - created_time.with_timezone(&chrono::Utc)).num_minutes();
                        if age_minutes < 0 || age_minutes as u64 > max_age_minutes {
                            debug!("   {} rejected: age {} min > {} max", t.symbol, age_minutes, max_age_minutes);
                            return false;
                        }
                    }
                }

                true
            })
            .collect();

        if candidates.is_empty() {
            debug!("No tokens passed progress/mcap filters");
            return Ok(vec![]);
        }

        info!("   {} tokens passed initial filters, fetching holder counts...", candidates.len());

        // 3. Fetch holder counts and filter
        let mut results = Vec::new();
        for token in candidates {
            // Small delay to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;

            let holders = self.get_holder_count(&token.token_address).await?;

            if holders >= min_holders {
                info!("🔥 [FINAL STRETCH] {} ({}) - Progress: {:.1}%, MCap: ${:.0}, Holders: {}",
                    token.name, token.symbol, token.bonding_progress(), token.market_cap_usd(), holders);
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

                // Check graduation age if timestamp available
                if let Some(ref grad_at) = t.graduated_at {
                    if let Ok(grad_time) = chrono::DateTime::parse_from_rfc3339(grad_at) {
                        let age_hours = (now - grad_time.with_timezone(&chrono::Utc)).num_hours();
                        if age_hours < 0 || age_hours as u64 > max_age_hours {
                            return false;
                        }
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
        let mut results = Vec::new();
        for token in candidates {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let holders = self.get_holder_count(&token.token_address).await?;

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
        assert!((token.bonding_progress() - 45.5).abs() < 0.01);
    }
}
