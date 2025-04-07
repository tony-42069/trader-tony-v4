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

// Structure for the /defi/price endpoint response
#[derive(Debug, Deserialize)]
struct PriceResponse {
    data: Option<PriceData>,
    success: bool,
}
#[derive(Debug, Deserialize)]
struct PriceData {
    value: f64, // Price (likely USD)
    // Attempt to find liquidity directly in this response if available
    // The exact field name might vary, check Birdeye docs if this fails.
    liquidity: Option<f64>, // Liquidity in USD?
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

    /// Fetches liquidity for a given token using the /defi/price endpoint.
    /// Returns liquidity denominated in SOL.
    pub async fn get_liquidity_sol(&self, token_address: &str) -> Result<f64> {
        let endpoint = "/defi/price";
        let url = format!("{}{}", BIRDEYE_BASE_URL, endpoint);

        debug!("Fetching price/liquidity from Birdeye for {}: {}", token_address, url);

        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .query(&[("address", token_address)])
            .send()
            .await
            .context("Failed to send request to Birdeye Price API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Birdeye Price API error for token {}: {} - {}", token_address, status, error_text);
            // Treat API errors as potentially zero liquidity for risk assessment
            return Ok(0.0);
        }

        let response_data: PriceResponse = response
            .json()
            .await
            .context("Failed to parse Birdeye Price API response")?;

        if !response_data.success || response_data.data.is_none() {
             warn!("Birdeye Price API reported failure or no data for token {}", token_address);
             return Ok(0.0); // Return 0 liquidity if API call fails or returns no data
        }

        let data = response_data.data.unwrap();

        // Extract USD liquidity and current token price (in USD)
        let usd_liquidity = data.liquidity.unwrap_or(0.0);
        // let token_price_usd = data.value; // Token price isn't directly needed for SOL liquidity calc

        if usd_liquidity <= 0.0 {
            debug!("Birdeye reported zero or missing USD liquidity for {}", token_address);
            return Ok(0.0);
        }

        // To get SOL liquidity, we need the current SOL price in USD.
        // Fetch SOL price using the same endpoint.
        let sol_price_usd = match self.get_sol_price_usd().await {
            Ok(price) => price,
            Err(e) => {
                warn!("Failed to get SOL price from Birdeye for liquidity calculation: {:?}. Assuming 0 SOL liquidity.", e);
                return Ok(0.0);
            }
        };

        if sol_price_usd <= 0.0 {
             warn!("Birdeye returned invalid SOL price: {}", sol_price_usd);
             return Ok(0.0); // Cannot calculate without SOL price
        }

        // Calculate SOL liquidity: (Total USD Liquidity / SOL Price in USD)
        let calculated_liquidity_sol = usd_liquidity / sol_price_usd;
        info!(
            "Calculated SOL liquidity for {}: {:.2} (USD Liq: {:.2}, SOL Price: {:.2})",
            token_address, calculated_liquidity_sol, usd_liquidity, sol_price_usd
        );

        Ok(calculated_liquidity_sol)
    }

    // Helper function to get SOL price in USD using the /defi/price endpoint
    async fn get_sol_price_usd(&self) -> Result<f64> {
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
            error!("Birdeye SOL Price API error: {} - {}", status, error_text);
            return Err(TraderbotError::ApiError(format!(
                "Birdeye SOL Price API failed with status {}: {}", status, error_text
            )).into());
        }

        let response_data: PriceResponse = response
            .json()
            .await
            .context("Failed to parse Birdeye SOL Price API response")?;

        if let Some(data) = response_data.data {
            if data.value > 0.0 {
                Ok(data.value)
            } else {
                Err(anyhow!("Birdeye returned invalid SOL price: {}", data.value))
            }
        } else {
            Err(anyhow!("Birdeye returned no data for SOL price"))
        }
    }
}
