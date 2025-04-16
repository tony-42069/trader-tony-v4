use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    signature::Signature,
    transaction::VersionedTransaction,
};
use solana_transaction_status::{
    EncodedTransaction,
    option_serializer::OptionSerializer,
};
use std::{
    sync::Arc,
    time::Duration,
    str::FromStr,
};
use tracing::{debug, error, info, warn};

use crate::solana::wallet::WalletManager;
use crate::error::TraderbotError;
use crate::solana::client::SolanaClient;

const JUPITER_BASE_URL: &str = "https://quote-api.jup.ag/v6";
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

#[derive(Debug, Clone)]
pub struct JupiterClient {
    client: Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QuoteResponse {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "otherAmountThreshold")]
    pub other_amount_threshold: String,
    #[serde(rename = "swapMode")]
    pub swap_mode: String,
    #[serde(rename = "slippageBps")]
    pub slippage_bps: u32,
    #[serde(rename = "platformFee")]
    pub platform_fee: Option<PlatformFee>,
    #[serde(rename = "priceImpactPct", default)]
    pub price_impact_pct: Option<String>,
    #[serde(rename = "routePlan")]
    pub route_plan: Vec<RoutePlan>,
    #[serde(rename = "contextSlot")]
    pub context_slot: Option<u64>,
    #[serde(rename = "timeTaken")]
    pub time_taken: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlatformFee {
    pub amount: String,
    #[serde(rename = "feeBps")]
    pub fee_bps: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RoutePlan {
    #[serde(rename = "swapInfo")]
    pub swap_info: SwapInfo,
    pub percent: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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
    #[serde(rename = "wrapAndUnwrapSol", default)]
    pub wrap_unwrap_sol: bool,
    #[serde(rename = "computeUnitPriceMicroLamports", skip_serializing_if = "Option::is_none")]
    pub compute_unit_price_micro_lamports: Option<u64>,
    #[serde(rename = "prioritizationFeeLamports", skip_serializing_if = "Option::is_none")]
    pub prioritization_fee_lamports: Option<u64>,
    #[serde(rename = "dynamicComputeUnitLimit", default)]
    pub dynamic_compute_unit_limit: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SwapResponse {
    #[serde(rename = "swapTransaction")]
    pub swap_transaction: String,
    #[serde(rename = "lastValidBlockHeight")]
    pub last_valid_block_height: u64,
    #[serde(rename = "prioritizationFeeLamports")]
    pub prioritization_fee_lamports: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SwapResult {
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount_ui: f64,
    pub out_amount_ui: f64,
    pub actual_out_amount_ui: Option<f64>,
    pub price_impact_pct: f64,
    pub transaction_signature: String,
}

impl JupiterClient {
    pub fn new(api_key: Option<String>) -> Self {
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
        amount_lamports: u64,
        slippage_bps: u32,
    ) -> Result<QuoteResponse> {
        let url = format!("{}/quote", JUPITER_BASE_URL);
        let params = vec![
            ("inputMint", input_mint.to_string()),
            ("outputMint", output_mint.to_string()),
            ("amount", amount_lamports.to_string()),
            ("slippageBps", slippage_bps.to_string()),
            ("onlyDirectRoutes", "false".to_string()),
            ("asLegacyTransaction", "false".to_string()),
        ];
        debug!("Getting quote from Jupiter: {:?}", params);
        let mut request_builder = self.client.get(&url).query(&params);
        if let Some(key) = &self.api_key {
            request_builder = request_builder.header("Jupiter-API-Key", key);
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
        // Fallback for both v6 (wrapper) and v5 (direct)
        let body = response
            .text()
            .await
            .context("Failed to read Jupiter Quote API response body")?;
        let quote = match serde_json::from_str::<QuoteResponseWrapper>(&body) {
            Ok(wrapper) => wrapper.data.into_iter().next()
                .ok_or_else(|| TraderbotError::ApiError("Jupiter Quote API returned empty data".to_string()))?,
            Err(_) => serde_json::from_str::<QuoteResponse>(&body)
                .context("Failed to parse Jupiter Quote API response")?,
        };
        debug!("Received Jupiter quote: {:?}", quote);
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
        priority_fee_micro_lamports: Option<u64>,
    ) -> Result<SwapResponse> {
        let url = format!("{}/swap", JUPITER_BASE_URL);
        let request_body = SwapRequest {
            quote_response: quote.clone(),
            user_public_key: user_public_key.to_string(),
            wrap_unwrap_sol: true,
            compute_unit_price_micro_lamports: priority_fee_micro_lamports,
            prioritization_fee_lamports: None,
            dynamic_compute_unit_limit: true,
        };
        debug!("Getting swap transaction from Jupiter: {:?}", request_body);
        let mut request_builder = self.client.post(&url).json(&request_body);
        if let Some(key) = &self.api_key {
            request_builder = request_builder.header("Jupiter-API-Key", key);
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

    pub async fn swap_sol_to_token(
        &self,
        token_mint: &str,
        token_decimals: u8,
        amount_sol: f64,
        slippage_bps: u32,
        priority_fee_micro_lamports: Option<u64>,
        wallet_manager: Arc<WalletManager>,
    ) -> Result<SwapResult> {
        info!("Initiating swap: {:.6} SOL to Token {}", amount_sol, token_mint);
        let lamports_in = (amount_sol * 1_000_000_000.0) as u64;
        if lamports_in == 0 { return Err(anyhow!("Input SOL amount is too small or zero")); }

        let quote = self.get_quote(SOL_MINT, token_mint, lamports_in, slippage_bps).await
            .context("Failed to get quote for SOL to token swap")?;
        let estimated_out_lamports = quote.out_amount.parse::<u64>()
            .context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 10f64.powi(token_decimals as i32);
        let price_impact = quote.price_impact_pct.as_deref().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
        info!("Quote received: {:.6} SOL -> {:.6} {} (Price Impact: {:.4}%)", 
              amount_sol, estimated_out_ui, token_mint, price_impact);

        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self.get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports).await
            .context("Failed to get swap transaction")?;

        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction)
            .context("Failed to decode swap transaction")?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&transaction_bytes)
            .context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager.sign_and_send_versioned_transaction(
            versioned_tx, 
            swap_response.last_valid_block_height
        ).await.context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        let actual_out_amount_ui = self.get_actual_amount_from_transaction(
            &signature.to_string(), 
            quote.input_mint.as_str(), 
            quote.output_mint.as_str(), 
            token_decimals, 
            &wallet_manager.solana_client()
        ).await?;

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
        token_amount_ui: f64,
        slippage_bps: u32,
        priority_fee_micro_lamports: Option<u64>,
        wallet_manager: Arc<WalletManager>,
    ) -> Result<SwapResult> {
        info!("Initiating swap: {:.6} Token {} to SOL", token_amount_ui, token_mint);
        let token_amount_lamports = (token_amount_ui * 10f64.powi(token_decimals as i32)) as u64;
        if token_amount_lamports == 0 { return Err(anyhow!("Input token amount is too small or zero")); }

        let quote = self.get_quote(token_mint, SOL_MINT, token_amount_lamports, slippage_bps).await
            .context("Failed to get quote for token to SOL swap")?;
        let estimated_out_lamports = quote.out_amount.parse::<u64>()
            .context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 1_000_000_000.0;
        let price_impact = quote.price_impact_pct.as_deref().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
        info!("Quote received: {:.6} {} -> {:.6} SOL (Price Impact: {:.4}%)", 
              token_amount_ui, token_mint, estimated_out_ui, price_impact);

        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self.get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports).await
            .context("Failed to get swap transaction")?;

        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction)
            .context("Failed to decode swap transaction")?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&transaction_bytes)
            .context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager.sign_and_send_versioned_transaction(
            versioned_tx, 
            swap_response.last_valid_block_height
        ).await.context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        let actual_out_amount_ui = self.get_actual_amount_from_transaction(
            &signature.to_string(), 
            quote.input_mint.as_str(), 
            quote.output_mint.as_str(), 
            9, 
            &wallet_manager.solana_client()
        ).await?;

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

    async fn get_actual_amount_from_transaction(
        &self,
        signature: &str,
        _input_mint: &str,
        output_mint: &str,
        _output_decimals: u8,
        solana_client: &SolanaClient,
    ) -> Result<Option<f64>> {
        // Get transaction details
        let tx_details = match solana_client.get_transaction(
            &Signature::from_str(signature)?,
            solana_sdk::commitment_config::CommitmentConfig::confirmed(),
        ).await {
            Ok(details) => details,
            Err(e) => {
                warn!("Could not get transaction details for tx {}: {}", signature, e);
                return Ok(None);
            }
        };

        // Check if transaction succeeded
        if let Some(meta) = tx_details.transaction.meta.as_ref() {
            if let Some(err) = &meta.err {
                warn!("Transaction {} failed with error: {:?}", signature, err);
                return Ok(None);
            }

            // Try to extract token balance from post_token_balances
            match &meta.post_token_balances {
                OptionSerializer::Some(balances) => {
                    for balance in balances {
                        if balance.mint == output_mint {
                            // Found our token
                            if let Some(ui_amount) = balance.ui_token_amount.ui_amount {
                                info!("Found token amount in tx {}: {}", signature, ui_amount);
                                return Ok(Some(ui_amount));
                            }
                        }
                    }
                },
                _ => {
                    warn!("No post token balances found in transaction {}", signature);
                }
            }

            // Fallback: try to find amount in logs
            match &meta.log_messages {
                OptionSerializer::Some(logs) => {
                    for log in logs {
                        if log.contains("Transfer:") && log.contains(output_mint) {
                            debug!("Found transfer log: {}", log);
                            // Try to extract amount - this is a simplistic approach
                            if let Some(amount_str) = log.split("Transfer:").nth(1) {
                                if let Ok(amount) = amount_str.trim().parse::<f64>() {
                                    info!("Parsed amount from logs: {}", amount);
                                    return Ok(Some(amount));
                                }
                            }
                        }
                    }
                },
                _ => {
                    warn!("No logs found in transaction {}", signature);
                }
            }
        }

        warn!("Could not determine exact amount for transaction {}", signature);
        Ok(None)
    }

    pub async fn get_price(
        &self,
        input_mint: &str,
        output_mint: &str,
        output_token_decimals: u8,
    ) -> Result<f64> {
        let input_lamports = 10_000_000; // 0.01 SOL (or other small amount)
        let quote = self.get_quote(input_mint, output_mint, input_lamports, 50).await?;
        let out_lamports = quote.out_amount.parse::<f64>()?;
        let in_lamports = quote.in_amount.parse::<f64>()?;
        if out_lamports == 0.0 || in_lamports == 0.0 {
            return Err(anyhow!("Failed to get valid price quote (zero amount)"));
        }
        let price = (in_lamports / 1e9) / (out_lamports / 10f64.powi(output_token_decimals as i32));
        debug!("Price calculated: 1 {} = {:.9} {}", output_mint, price, input_mint);
        Ok(price)
    }
}

#[derive(Debug, Deserialize)]
struct QuoteResponseWrapper {
    #[serde(rename = "data")]
    pub data: Vec<QuoteResponse>,
}