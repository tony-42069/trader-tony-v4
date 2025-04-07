use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::error::TraderbotError;

// Verified base URL
const BIRDEYE_BASE_URL: &str = "https://public-api.birdeye.so";

#[derive(Debug, Clone)]
pub struct BirdeyeClient {
    api_key: String,
    client: Client,
}

// --- Response Structs ---

// Matches the structure for /defi/v3/pair/overview/single
#[derive(Debug, Deserialize, Serialize)]
pub struct PairOverviewResponse {
    pub data: Option<PairData>,
    pub success: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")] // Match Birdeye's camelCase fields
pub struct PairData {
    pub address: String, // Pair address
    pub base: TokenInfo,
    pub quote: TokenInfo,
    pub liquidity: Option<f64>, // Total liquidity in USD
    pub price: Option<f64>,     // Price of base token in quote token (e.g., SOL price in USDC)
    // Add other potentially useful fields if needed later
    // pub volume_24h: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenInfo {
    pub address: String,
    pub decimals: u8,
    pub symbol: String,
    // pub name: String, // Name might not always be present
    // pub icon: Option<String>,
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

    /// Fetches liquidity for a given token, attempting to find its primary SOL pair.
    /// Returns liquidity denominated in SOL.
    pub async fn get_liquidity_sol(&self, token_address: &str) -> Result<f64> {
        // Birdeye's pair overview endpoint expects the PAIR address, not the token address.
        // Finding the primary SOL pair address for a token programmatically is complex.
        // Common methods involve:
        // 1. Using another Birdeye endpoint (like token overview or tokenlist) if it provides the pair address.
        // 2. Querying DEX program accounts (e.g., Raydium, Orca) - complex SDK/RPC interaction.
        // 3. Using Helius DAS API if it returns market/pair info.

        // For now, we'll stick to the placeholder approach of returning 0.0
        // and log a warning, as implementing pair finding is out of scope for this step.
        // TODO: Implement robust primary pair finding logic.
        warn!(
            "get_liquidity_sol for {} needs primary pair finding logic. Returning placeholder 0.0.",
            token_address
        );
        Ok(0.0) // Return 0.0 until pair finding is implemented

        /* // --- Ideal Implementation (Requires Pair Finding) ---
        // 1. Find the primary SOL pair address for `token_address` (e.g., using another API call)
        let pair_address = find_primary_sol_pair(token_address).await?; // Hypothetical function

        // 2. Call the pair overview endpoint
        let endpoint = format!("/defi/v3/pair/overview/single");
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);

        debug!("Fetching pair overview from Birdeye for pair {}: {}", pair_address, url);

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .query(&[("address", pair_address)]) // Pass pair address as query param
            .send()
            .await
            .context("Failed to send request to Birdeye Pair Overview API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Birdeye Pair Overview API error: {} - {}", status, error_text);
            return Err(TraderbotError::ApiError(format!(
                "Birdeye Pair Overview API failed with status {}: {}", status, error_text
            )).into());
        }

        let response_data: PairOverviewResponse = response
            .json()
            .await
            .context("Failed to parse Birdeye Pair Overview API response")?;

        if !response_data.success || response_data.data.is_none() {
             warn!("Birdeye API reported failure or no data for pair {}", pair_address);
             return Ok(0.0);
        }

        let data = response_data.data.unwrap();

        // 3. Extract USD liquidity and SOL price
        let usd_liquidity = data.liquidity.unwrap_or(0.0);
        let sol_price_usd = data.price; // Assuming base is SOL, quote is USD(C)

        if usd_liquidity <= 0.0 {
            return Ok(0.0);
        }

        // Ensure the base token is SOL for correct calculation
        if data.base.address != crate::api::jupiter::SOL_MINT {
             warn!("Primary pair found ({}) for {} is not SOL-based. Cannot calculate SOL liquidity accurately.", data.address, token_address);
             // Could potentially use quote token liquidity if it's SOL, but less reliable.
             return Ok(0.0); // Return 0 if not a direct SOL pair for simplicity
        }

        let sol_price = match sol_price_usd {
            Some(price) if price > 0.0 => price,
            _ => {
                warn!("Invalid or missing SOL price in Birdeye response for pair {}", data.address);
                return Ok(0.0); // Cannot calculate without SOL price
            }
        };

        // 4. Calculate SOL liquidity
        let calculated_liquidity_sol = usd_liquidity / sol_price;
        info!(
            "Calculated SOL liquidity for {}: {:.2} (USD Liq: {:.2}, SOL Price: {:.2})",
            token_address, calculated_liquidity_sol, usd_liquidity, sol_price
        );

        Ok(calculated_liquidity_sol)
        */
    }
}
