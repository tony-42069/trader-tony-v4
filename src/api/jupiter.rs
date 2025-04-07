use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::Duration,
};
use tracing::{debug, error, info, warn}; // Added warn
use base64::{engine::general_purpose::STANDARD, Engine as _}; // Import Engine trait globally for the file

use crate::solana::wallet::WalletManager;
use crate::error::TraderbotError; // Assuming error enum exists

const JUPITER_BASE_URL: &str = "https://quote-api.jup.ag/v6";
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112"; // Made public

#[derive(Debug, Clone)]
pub struct JupiterClient {
    client: Client,
    api_key: Option<String>,
}

// --- Request/Response Structs ---

#[derive(Debug, Deserialize, Serialize, Clone)] // Added Clone
pub struct QuoteResponse {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "inAmount")] // Field name might be inAmount
    pub in_amount: String, // Renamed from amount for clarity
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "outAmount")] // Field name might be outAmount
    pub out_amount: String, // Renamed from amount for clarity
    #[serde(rename = "otherAmountThreshold")]
    pub other_amount_threshold: String, // Min amount out with slippage
    #[serde(rename = "swapMode")]
    pub swap_mode: String,
    #[serde(rename = "slippageBps")]
    pub slippage_bps: u32,
    #[serde(rename = "platformFee")]
    pub platform_fee: Option<PlatformFee>, // Added platform fee
    pub price_impact_pct: String, // Jupiter returns as string
    #[serde(rename = "routePlan")]
    pub route_plan: Vec<RoutePlan>,
    #[serde(rename = "contextSlot")]
    pub context_slot: Option<u64>, // Optional field
    #[serde(rename = "timeTaken")]
    pub time_taken: Option<f64>, // Optional field
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlatformFee {
    pub amount: String,
    #[serde(rename = "feeBps")]
    pub fee_bps: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)] // Added Clone
pub struct RoutePlan {
    pub swap_info: SwapInfo, // Field name might be swapInfo
    pub percent: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone)] // Added Clone
pub struct SwapInfo {
    #[serde(rename = "ammKey")]
    pub amm_key: String,
    pub label: String,
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "feeAmount")]
    pub fee_amount: String,
    #[serde(rename = "feeMint")]
    pub fee_mint: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SwapRequest {
    #[serde(rename = "quoteResponse")]
    pub quote_response: QuoteResponse,
    #[serde(rename = "userPublicKey")]
    pub user_public_key: String,
    #[serde(rename = "wrapAndUnwrapSol", default)] // Correct field name, default if missing
    pub wrap_unwrap_sol: bool,
    // Optional: Add fields like computeUnitPriceMicroLamports, prioritizationFeeLamports
    #[serde(rename = "computeUnitPriceMicroLamports", skip_serializing_if = "Option::is_none")]
    pub compute_unit_price_micro_lamports: Option<u64>,
    #[serde(rename = "prioritizationFeeLamports", skip_serializing_if = "Option::is_none")]
    pub prioritization_fee_lamports: Option<u64>,
    #[serde(rename = "dynamicComputeUnitLimit", default)] // Use dynamic CU limit
    pub dynamic_compute_unit_limit: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SwapResponse {
    #[serde(rename = "swapTransaction")]
    pub swap_transaction: String, // Base64 encoded transaction
    #[serde(rename = "lastValidBlockHeight")]
    pub last_valid_block_height: u64,
    #[serde(rename = "prioritizationFeeLamports")]
    pub prioritization_fee_lamports: Option<u64>, // Added optional field
}

// --- Swap Result Struct ---

#[derive(Debug, Clone)]
pub struct SwapResult {
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount_ui: f64, // Amount input by user (UI representation)
    pub out_amount_ui: f64, // Estimated amount out (UI representation)
    pub actual_out_amount_ui: Option<f64>, // Actual amount received after tx confirmation
    pub price_impact_pct: f64,
    pub transaction_signature: String,
}

// --- Jupiter Client Implementation ---

impl JupiterClient {
    pub fn new(api_key: Option<String>) -> Self { // Take ownership of api_key
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
        }
    }

    pub async fn get_quote(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount_lamports: u64, // Use lamports for precision
        slippage_bps: u32,
    ) -> Result<QuoteResponse> {
        let url = format!("{}/quote", JUPITER_BASE_URL);

        // Use Vec<(&str, String)> for query parameters
        let params = vec![
            ("inputMint", input_mint.to_string()),
            ("outputMint", output_mint.to_string()),
            ("amount", amount_lamports.to_string()),
            ("slippageBps", slippage_bps.to_string()),
            // Optional: Add other params like 'onlyDirectRoutes', 'asLegacyTransaction'
            ("onlyDirectRoutes", "false".to_string()), // Example
            ("asLegacyTransaction", "false".to_string()), // Use VersionedTransaction
        ];

        debug!("Getting quote from Jupiter: {:?}", params);

        let mut request_builder = self.client.get(&url).query(&params);

        if let Some(key) = &self.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", key)); // Use Bearer token if applicable
            // Or use Jupiter-API-Key if that's the correct header
            // request_builder = request_builder.header("Jupiter-API-Key", key);
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to send quote request to Jupiter API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!("Jupiter Quote API error: Status {}, Body: {}", status, error_text);
            return Err(TraderbotError::ApiError(format!(
                "Jupiter Quote API failed with status {}: {}", status, error_text
            )).into());
        }

        let quote: QuoteResponse = response
            .json()
            .await
            .context("Failed to parse Jupiter Quote API response")?;

        debug!("Received Jupiter quote: {:?}", quote);

        // Basic validation
        if quote.in_amount.parse::<u64>().unwrap_or(0) == 0 || quote.out_amount.parse::<u64>().unwrap_or(0) == 0 {
             warn!("Received quote with zero in/out amount: {:?}", quote);
             return Err(TraderbotError::ApiError("Received invalid quote from Jupiter (zero amount)".to_string()).into());
        }


        Ok(quote)
    }

    pub async fn get_swap_transaction(
        &self,
        quote: &QuoteResponse,
        user_public_key: &str,
        priority_fee_micro_lamports: Option<u64>, // Allow specifying priority fee
    ) -> Result<SwapResponse> {
        let url = format!("{}/swap", JUPITER_BASE_URL);

        let request_body = SwapRequest {
            quote_response: quote.clone(),
            user_public_key: user_public_key.to_string(),
            wrap_unwrap_sol: true, // Automatically wrap/unwrap SOL
            compute_unit_price_micro_lamports: priority_fee_micro_lamports,
            prioritization_fee_lamports: None, // Let Jupiter calculate if needed, or set explicitly
            dynamic_compute_unit_limit: true, // Recommended by Jupiter
        };

        debug!("Getting swap transaction from Jupiter: {:?}", request_body);

        let mut request_builder = self.client.post(&url).json(&request_body);

        if let Some(key) = &self.api_key {
             request_builder = request_builder.header("Authorization", format!("Bearer {}", key));
            // Or use Jupiter-API-Key
            // request_builder = request_builder.header("Jupiter-API-Key", key);
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to send swap request to Jupiter API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!("Jupiter Swap API error: Status {}, Body: {}", status, error_text);
             return Err(TraderbotError::ApiError(format!(
                "Jupiter Swap API failed with status {}: {}", status, error_text
            )).into());
        }

        let swap_response: SwapResponse = response
            .json()
            .await
            .context("Failed to parse Jupiter Swap API response")?;

        debug!("Received Jupiter swap response: {:?}", swap_response);

        Ok(swap_response)
    }

    // --- High-Level Swap Functions ---

    // Note: These functions need the token's decimals for accurate amount conversion.
    // Assuming 9 decimals for SOL and requiring decimals for the other token.
    pub async fn swap_sol_to_token(
        &self,
        token_mint: &str,
        token_decimals: u8,
        amount_sol: f64,
        slippage_bps: u32,
        priority_fee_micro_lamports: Option<u64>,
        wallet_manager: Arc<WalletManager>,
    ) -> Result<SwapResult> {
        info!(
            "Initiating swap: {:.6} SOL to Token {}",
            amount_sol, token_mint
        );

        let lamports_in = (amount_sol * 1_000_000_000.0) as u64;
        if lamports_in == 0 {
            return Err(anyhow!("Input SOL amount is too small or zero"));
        }

        // 1. Get Quote
        let quote = self
            .get_quote(SOL_MINT, token_mint, lamports_in, slippage_bps)
            .await
            .context("Failed to get quote for SOL to token swap")?;

        let estimated_out_lamports = quote.out_amount.parse::<u64>()
            .context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 10f64.powi(token_decimals as i32);
        let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);

        info!(
            "Quote received: {:.6} SOL -> {:.6} {} (Price Impact: {:.4}%)",
            amount_sol, estimated_out_ui, token_mint, price_impact
        );

        // 2. Get Swap Transaction
        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self
            .get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports)
            .await
            .context("Failed to get swap transaction")?;

        // 3. Decode, Sign, Send Transaction
        // use base64::{engine::general_purpose::STANDARD, Engine as _}; // No longer needed here
        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction) // Use STANDARD engine
            .context("Failed to decode swap transaction")?;

        // Deserialize as VersionedTransaction (recommended)
        let versioned_tx: solana_sdk::transaction::VersionedTransaction =
            bincode::deserialize(&transaction_bytes)
                .context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager
            .sign_and_send_versioned_transaction(versioned_tx, swap_response.last_valid_block_height)
            .await
            .context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        // 4. TODO: Confirm transaction and get actual output amount from logs/balance change.
        // This requires parsing transaction details after confirmation.
        let actual_out_amount_ui = None; // Placeholder

        Ok(SwapResult {
            input_mint: SOL_MINT.to_string(),
            output_mint: token_mint.to_string(),
            in_amount_ui: amount_sol,
            out_amount_ui: estimated_out_ui,
            actual_out_amount_ui,
            price_impact_pct: price_impact,
            transaction_signature: signature.to_string(),
        })
    }

    pub async fn swap_token_to_sol(
        &self,
        token_mint: &str,
        token_decimals: u8,
        token_amount_ui: f64, // Amount of token to sell (UI representation)
        slippage_bps: u32,
        priority_fee_micro_lamports: Option<u64>,
        wallet_manager: Arc<WalletManager>,
    ) -> Result<SwapResult> {
        info!(
            "Initiating swap: {:.6} Token {} to SOL",
            token_amount_ui, token_mint
        );

        let token_amount_lamports = (token_amount_ui * 10f64.powi(token_decimals as i32)) as u64;
         if token_amount_lamports == 0 {
            return Err(anyhow!("Input token amount is too small or zero"));
        }

        // 1. Get Quote
        let quote = self
            .get_quote(token_mint, SOL_MINT, token_amount_lamports, slippage_bps)
            .await
            .context("Failed to get quote for token to SOL swap")?;

        let estimated_out_lamports = quote.out_amount.parse::<u64>()
            .context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 1_000_000_000.0; // SOL has 9 decimals
        let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);

         info!(
            "Quote received: {:.6} {} -> {:.6} SOL (Price Impact: {:.4}%)",
            token_amount_ui, token_mint, estimated_out_ui, price_impact
        );

        // 2. Get Swap Transaction
        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self
            .get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports)
            .await
            .context("Failed to get swap transaction")?;

        // 3. Decode, Sign, Send Transaction
        // use base64::{engine::general_purpose::STANDARD, Engine as _}; // No longer needed here
        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction) // Use STANDARD engine
            .context("Failed to decode swap transaction")?;
        let versioned_tx: solana_sdk::transaction::VersionedTransaction =
            bincode::deserialize(&transaction_bytes)
                .context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager
            .sign_and_send_versioned_transaction(versioned_tx, swap_response.last_valid_block_height)
            .await
            .context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        // 4. TODO: Confirm transaction and get actual output amount.
        let actual_out_amount_ui = None; // Placeholder

        Ok(SwapResult {
            input_mint: token_mint.to_string(),
            output_mint: SOL_MINT.to_string(),
            in_amount_ui: token_amount_ui,
            out_amount_ui: estimated_out_ui,
            actual_out_amount_ui,
            price_impact_pct: price_impact,
            transaction_signature: signature.to_string(),
        })
    }

    // --- Price Function (Example) ---

    // Gets the price of 1 unit of the output token in terms of the input token.
    // e.g., get_price("SOL_MINT", "USDC_MINT", 6) -> price of 1 USDC in SOL
    pub async fn get_price(
        &self,
        input_mint: &str,
        output_mint: &str,
        output_token_decimals: u8,
    ) -> Result<f64> {
        // Quote for buying 1 unit of the output token
        let _amount_out_lamports = 10u64.pow(output_token_decimals as u32); // Prefixed with _

        // Need to use /quote with swapMode=ExactOut, but that requires amount specified for output
        // Let's try quoting a small amount of input token instead for an approximate price.
        // Quote 0.01 SOL for the token price (adjust input amount as needed)
        let input_lamports = 10_000_000; // 0.01 SOL

        let quote = self.get_quote(input_mint, output_mint, input_lamports, 50).await?; // Low slippage for price check

        let out_lamports = quote.out_amount.parse::<f64>()?;
        let in_lamports = quote.in_amount.parse::<f64>()?;

        if out_lamports == 0.0 || in_lamports == 0.0 {
            return Err(anyhow!("Failed to get valid price quote (zero amount)"));
        }

        // Price = Input Amount / Output Amount (adjusting for decimals)
        // Price of 1 Output Token in terms of Input Token
        let price = (in_lamports / 1_000_000_000.0) / (out_lamports / 10f64.powi(output_token_decimals as i32));

        debug!("Price calculated: 1 {} = {:.9} {}", output_mint, price, input_mint);

        Ok(price)
    }
}
