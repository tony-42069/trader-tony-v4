use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, warn}; // Removed info

use crate::error::TraderbotError;

// Verified base URL
const BIRDEYE_BASE_URL: &str = "https://public-api.birdeye.so";

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
}
