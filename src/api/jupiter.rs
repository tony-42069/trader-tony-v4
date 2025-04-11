use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use solana_transaction_status::{
    EncodedTransaction,
    option_serializer::OptionSerializer,
    UiTransactionTokenBalance,
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
    pub price_impact_pct: String,
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
        let quote: QuoteResponse = response
            .json()
            .await
            .context("Failed to parse Jupiter Quote API response")?;
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

        let quote = self.get_quote(SOL_MINT, token_mint, lamports_in, slippage_bps).await.context("Failed to get quote for SOL to token swap")?;
        let estimated_out_lamports = quote.out_amount.parse::<u64>().context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 10f64.powi(token_decimals as i32);
        let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);
        info!("Quote received: {:.6} SOL -> {:.6} {} (Price Impact: {:.4}%)", amount_sol, estimated_out_ui, token_mint, price_impact);

        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self.get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports).await.context("Failed to get swap transaction")?;

        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction).context("Failed to decode swap transaction")?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&transaction_bytes).context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager.sign_and_send_versioned_transaction(versioned_tx, swap_response.last_valid_block_height).await.context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        let actual_out_amount_ui = self.get_actual_amount_from_transaction(&signature.to_string(), quote.input_mint.as_str(), quote.output_mint.as_str(), token_decimals, &wallet_manager.solana_client()).await?;

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

        let quote = self.get_quote(token_mint, SOL_MINT, token_amount_lamports, slippage_bps).await.context("Failed to get quote for token to SOL swap")?;
        let estimated_out_lamports = quote.out_amount.parse::<u64>().context("Failed to parse quote out_amount")?;
        let estimated_out_ui = estimated_out_lamports as f64 / 1_000_000_000.0;
        let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);
         info!("Quote received: {:.6} {} -> {:.6} SOL (Price Impact: {:.4}%)", token_amount_ui, token_mint, estimated_out_ui, price_impact);

        let user_public_key = wallet_manager.get_public_key().to_string();
        let swap_response = self.get_swap_transaction(&quote, &user_public_key, priority_fee_micro_lamports).await.context("Failed to get swap transaction")?;

        let transaction_bytes = STANDARD.decode(&swap_response.swap_transaction).context("Failed to decode swap transaction")?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&transaction_bytes).context("Failed to deserialize VersionedTransaction")?;

        info!("Sending swap transaction...");
        let signature = wallet_manager.sign_and_send_versioned_transaction(versioned_tx, swap_response.last_valid_block_height).await.context("Failed to sign and send swap transaction")?;
        info!("Swap transaction sent: {}", signature);

        let actual_out_amount_ui = self.get_actual_amount_from_transaction(&signature.to_string(), quote.input_mint.as_str(), quote.output_mint.as_str(), 9, &wallet_manager.solana_client()).await?;

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

        if let Some(meta) = tx_details.transaction.meta.as_ref() {
            if let Some(err) = &meta.err {
                warn!("Transaction {} failed with error: {:?}", signature, err);
                return Ok(None);
            }

            // --- FIX for OptionSerializer post_token_balances ---
            if let OptionSerializer::Some(balances) = &meta.post_token_balances {
                // Get the wallet public key
                // --- FIX for EncodedTransaction ---
                let encoded_tx_str = match &tx_details.transaction.transaction {
                    EncodedTransaction::Legacy(s) |
                    EncodedTransaction::Versioned(s) => s,
                    _ => {
                        warn!("Transaction data is not in expected Legacy/Versioned string format for tx {}", signature);
                        return Ok(None);
                    }
                };
                let decoded_tx_bytes = STANDARD.decode(encoded_tx_str).context(format!("Failed to decode base64 tx {}", signature))?;
                let versioned_tx: VersionedTransaction = bincode::deserialize(&decoded_tx_bytes).context(format!("Failed to deserialize VersionedTransaction for tx {}", signature))?;
                let wallet_key = match versioned_tx.message.static_account_keys().get(0) {
                    Some(key) => key.to_string(),
                    None => { warn!("Could not get wallet key (static_account_keys[0]) from transaction {}", signature); return Ok(None); }
                };
                // --- End Fixes ---

                // Find the token account for the output token owned by the wallet
                let token_account = balances.iter().find(|balance|
                    balance.mint == output_mint &&
                    balance.owner.as_ref().map_or(false, |owner| *owner == wallet_key)
                );

                if let Some(balance) = token_account {
                    if let Some(amount) = &balance.ui_token_amount {
                        let amount_raw = amount.ui_amount.unwrap_or(0.0);
                        info!("Actual token amount received in transaction {}: {} tokens", signature, amount_raw);
                        return Ok(Some(amount_raw));
                    }
                }
                warn!("Could not find specific output token balance for wallet in tx {}", signature);
            } else {
                 warn!("Post token balances OptionSerializer was None for tx {}", signature);
            }

            // --- FIX for OptionSerializer log_messages ---
            if let OptionSerializer::Some(logs) = &meta.log_messages {
                debug!("Attempting to parse logs for actual token amount in transaction {}", signature);
                for log in logs.iter() {
                    if log.contains("Transfer:") && log.contains(output_mint) {
                        if let Some(amount_str) = log.split("Transfer:").nth(1).map(|s| s.trim()) {
                            if let Ok(amount) = amount_str.parse::<f64>() {
                                info!("Parsed from logs: Actual token amount in tx {}: {}", signature, amount);
                                // Optionally return here if primary method failed
                            }
                        }
                    }
                }
                warn!("Could not parse actual token amount from transaction logs");
            } else {
                 warn!("Log messages OptionSerializer was None for tx {}", signature);
            }
        } else {
             warn!("Transaction meta was None for tx {}", signature);
        }

        warn!("Could not determine actual token amount for transaction {}", signature);
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

</final_file_content>

IMPORTANT: For any future changes to this file, use the final_file_content shown above as your reference. This content reflects the current state of the file, including any auto-formatting (e.g., if you used single quotes but the formatter converted them to double quotes). Always base your SEARCH/REPLACE operations on this final version to ensure accuracy.

<environment_details>
# VSCode Visible Files
src/api/jupiter.rs

# VSCode Open Tabs
src/config.rs
src/api/mod.rs
TODO.md
src/trading/strategy.rs
src/trading/autotrader.rs
src/bot/commands.rs
src/api/jupiter.rs
src/main.rs
src/trading/risk.rs
src/solana/client.rs
src/solana/wallet.rs
.env
src/trading/position.rs
Cargo.toml
src/api/birdeye.rs
src/api/helius.rs
.gitignore
src/error.rs
src/bot/mod.rs
src/models/mod.rs
src/models/token.rs
src/models/user.rs
src/solana/mod.rs
src/trading/mod.rs
README.md
deployment.md
strategy.md
api.md
.env.example
src/bot/keyboards.rs

# Current Time
4/10/2025, 9:10:13 PM (America/New_York, UTC-4:00)

# Current Mode
ACT MODE
</environment_details>
