use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

// Verified base URL
const BIRDEYE_BASE_URL: &str = "https://public-api.birdeye.so";

// ============================================================================
// Combined Token Data (for Final Stretch / Migrated strategies)
// ============================================================================

/// Combined token data from multiple Birdeye endpoints
/// Used for Final Stretch and Migrated strategy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub mint: String,
    pub holders: u64,
    pub volume_24h_usd: f64,
    pub market_cap_usd: f64,
    pub price_usd: f64,
    pub liquidity_usd: f64,
}

impl Default for TokenData {
    fn default() -> Self {
        Self {
            mint: String::new(),
            holders: 0,
            volume_24h_usd: 0.0,
            market_cap_usd: 0.0,
            price_usd: 0.0,
            liquidity_usd: 0.0,
        }
    }
}

// ============================================================================
// V3 API Response Structures
// ============================================================================

/// Response from /defi/v3/token/market-data endpoint
#[derive(Debug, Deserialize)]
pub struct MarketDataResponse {
    pub data: Option<MarketData>,
    pub success: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MarketData {
    pub address: Option<String>,
    pub price: Option<f64>,              // Price in USD
    pub liquidity: Option<f64>,          // Liquidity in USD
    pub mc: Option<f64>,                 // Market cap in USD (alias: marketCap)
    #[serde(alias = "marketCap")]
    pub market_cap: Option<f64>,
    pub supply: Option<f64>,
    pub circulating_supply: Option<f64>,
}

/// Response from /defi/v3/token/trade-data/single endpoint
#[derive(Debug, Deserialize)]
pub struct TradeDataResponse {
    pub data: Option<TradeData>,
    pub success: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TradeData {
    pub address: Option<String>,
    pub holder: Option<u64>,                // Number of holders
    pub volume24h_usd: Option<f64>,         // 24h volume in USD
    #[serde(alias = "v24hUSD")]
    pub v24h_usd: Option<f64>,              // Alternate field name
    pub trade24h: Option<u64>,              // Number of trades in 24h
    pub buy24h: Option<u64>,                // Number of buys in 24h
    pub sell24h: Option<u64>,               // Number of sells in 24h
    pub unique_wallet24h: Option<u64>,      // Unique wallets in 24h
}

#[derive(Debug, Clone)]
pub struct BirdeyeClient {
    api_key: String,
    client: Client,
}

// --- Response Structs ---

// Structure for the /defi/token_overview endpoint response
#[derive(Debug, Deserialize, Serialize, Clone)] // Added Serialize and Clone
pub struct TokenOverviewResponse { // Made pub
    pub data: Option<TokenOverviewData>, // Made pub
    pub success: bool, // Made pub
}

#[derive(Debug, Deserialize, Serialize, Clone)] // Added Serialize and Clone
#[serde(rename_all = "camelCase")]
pub struct TokenOverviewData { // Made pub
    // Core Info
    pub address: String,
    pub decimals: Option<u8>,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub logo_uri: Option<String>,

    // Market Data
    pub price: Option<f64>,     // Price in USD
    pub mc: Option<f64>,        // Market Cap in USD
    pub supply: Option<f64>,    // Circulating supply (check Birdeye docs for exact definition)
    pub liquidity: Option<f64>, // Total liquidity in USD across tracked pairs

    // Volume & Trade Stats (Examples - add more if needed from the full response)
    pub v24h_usd: Option<f64>, // Volume 24h USD
    pub v24h_change_percent: Option<f64>,
    pub trade24h: Option<u64>, // Number of trades 24h

    // Add other potentially useful fields from the full response if needed for LP check later
    // e.g., fields related to pairs, LP supply, holders if they exist.
    // For now, keeping it focused on generally useful overview data.
}

// #[derive(Debug, Deserialize, Serialize)] // Removed - not currently used
// struct TokenExtensions {
//     coingeckoId: Option<String>,
//     // Add other extension fields if needed
// }

// Structure for the /defi/price endpoint response (used for SOL price)
#[derive(Debug, Deserialize)]
struct PriceResponse {
    data: Option<PriceData>,
    success: bool,
}
#[derive(Debug, Deserialize)]
struct PriceData {
    value: f64, // Price (likely USD)
    // liquidity field might exist here too, but we only need value for SOL
}


// --- Birdeye Client Implementation ---

impl BirdeyeClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("Failed to create HTTP client for Birdeye"),
        }
    }

    /// Fetches the full token overview from the /defi/token_overview endpoint.
    pub async fn get_token_overview(&self, token_address: &str) -> Result<Option<TokenOverviewData>> {
        let endpoint = "/defi/token_overview";
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);

        debug!("Fetching token overview from Birdeye for {}: {}", token_address, url);

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .query(&[("address", token_address)])
            .send()
            .await
            .context("Failed to send request to Birdeye Token Overview API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!("Birdeye Token Overview API error for token {}: {} - {}", token_address, status, error_text);
            return Ok(None);
        }

        let response_data: TokenOverviewResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse Birdeye Token Overview API response for {}: {:?}; ignoring", token_address, e);
                return Ok(None);
            }
        };


        if !response_data.success || response_data.data.is_none() {
             warn!("Birdeye Token Overview API reported failure or no data for token {}", token_address);
             return Ok(None); // Return None if API call fails logically or returns no data
        }

        // Return the data field directly
        Ok(response_data.data)
    }

    /// Helper function to get SOL price in USD using the /defi/price endpoint.
    /// Made public in case it's needed elsewhere.
    pub async fn get_sol_price_usd(&self) -> Result<f64> {
        let endpoint = "/defi/price";
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);
        let sol_address = crate::api::jupiter::SOL_MINT; // Use constant

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .query(&[("address", sol_address)])
            .send()
            .await
            .context("Failed to send SOL price request to Birdeye API")?;

         if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!("Birdeye SOL Price API error: {} - {}; returning 0", status, error_text);
            return Ok(0.0);
        }

        let response_data: PriceResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse Birdeye SOL Price API response: {:?}; returning 0", e);
                return Ok(0.0);
            }
        };

        let price = response_data.data.map(|d| d.value).unwrap_or(0.0);
        Ok(price)
    }

    // ========================================================================
    // V3 API Methods (for Final Stretch / Migrated strategies)
    // ========================================================================

    /// Fetch market data (price, market cap, liquidity) from v3 API
    pub async fn get_market_data(&self, mint: &str) -> Result<Option<MarketData>> {
        let endpoint = "/defi/v3/token/market-data";
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);

        debug!("Fetching market data from Birdeye v3 for {}", mint);

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", "solana")
            .query(&[("address", mint)])
            .send()
            .await
            .context("Failed to send request to Birdeye Market Data API")?;

        // Check for rate limiting
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            warn!("Birdeye API rate limit hit for market-data");
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!("Birdeye Market Data API error for {}: {} - {}", mint, status, error_text);
            return Ok(None);
        }

        let response_data: MarketDataResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse Birdeye Market Data response for {}: {:?}", mint, e);
                return Ok(None);
            }
        };

        if !response_data.success {
            warn!("Birdeye Market Data API reported failure for {}", mint);
            return Ok(None);
        }

        Ok(response_data.data)
    }

    /// Fetch trade data (volume, holders, trade counts) from v3 API
    pub async fn get_trade_data(&self, mint: &str) -> Result<Option<TradeData>> {
        let endpoint = "/defi/v3/token/trade-data/single";
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);

        debug!("Fetching trade data from Birdeye v3 for {}", mint);

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", "solana")
            .query(&[("address", mint)])
            .send()
            .await
            .context("Failed to send request to Birdeye Trade Data API")?;

        // Check for rate limiting
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            warn!("Birdeye API rate limit hit for trade-data");
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!("Birdeye Trade Data API error for {}: {} - {}", mint, status, error_text);
            return Ok(None);
        }

        let response_data: TradeDataResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to parse Birdeye Trade Data response for {}: {:?}", mint, e);
                return Ok(None);
            }
        };

        if !response_data.success {
            warn!("Birdeye Trade Data API reported failure for {}", mint);
            return Ok(None);
        }

        Ok(response_data.data)
    }

    /// Combined convenience method to fetch all token data needed for strategy evaluation
    /// Fetches both market-data and trade-data endpoints and combines results
    pub async fn get_token_data(&self, mint: &str) -> Result<TokenData> {
        info!("📊 Fetching combined token data for {}", mint);

        // Fetch both endpoints (could be parallelized with tokio::join!)
        let (market_data, trade_data) = tokio::join!(
            self.get_market_data(mint),
            self.get_trade_data(mint)
        );

        let market = market_data.ok().flatten();
        let trade = trade_data.ok().flatten();

        // Combine results into TokenData
        let mut token_data = TokenData {
            mint: mint.to_string(),
            ..Default::default()
        };

        if let Some(m) = market {
            token_data.price_usd = m.price.unwrap_or(0.0);
            token_data.liquidity_usd = m.liquidity.unwrap_or(0.0);
            // Market cap can be in either field
            token_data.market_cap_usd = m.mc.or(m.market_cap).unwrap_or(0.0);
        }

        if let Some(t) = trade {
            token_data.holders = t.holder.unwrap_or(0);
            // Volume can be in either field
            token_data.volume_24h_usd = t.volume24h_usd.or(t.v24h_usd).unwrap_or(0.0);
        }

        info!("   Holders: {} | Volume: ${:.0} | MCap: ${:.0} | Price: ${:.8}",
            token_data.holders, token_data.volume_24h_usd,
            token_data.market_cap_usd, token_data.price_usd);

        Ok(token_data)
    }

    /// Batch fetch token data for multiple mints (with rate limiting consideration)
    /// Fetches sequentially with small delays to avoid rate limits
    pub async fn get_token_data_batch(&self, mints: &[String]) -> Vec<(String, Result<TokenData>)> {
        let mut results = Vec::with_capacity(mints.len());

        for mint in mints {
            let data = self.get_token_data(mint).await;
            results.push((mint.clone(), data));

            // Small delay between requests to avoid rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        results
    }
}
