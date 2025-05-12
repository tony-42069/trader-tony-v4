use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::str::FromStr;
use std::time::Duration;
use std::future::Future;
use tracing::{info, warn, debug, error};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::{Signature},
    transaction::{VersionedTransaction, TransactionError},
    program_pack::Pack,
};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    client_error::ClientError,
    rpc_config::{RpcTransactionConfig, RpcSimulateTransactionConfig, RpcSendTransactionConfig},
    rpc_response::{RpcSimulateTransactionResult, RpcTokenAccountBalance},
};
use solana_transaction_status::{UiTransactionEncoding, EncodedConfirmedTransactionWithStatusMeta};
use spl_token::state::{Account as TokenAccount, Mint};
use spl_associated_token_account::get_associated_token_address;
use tokio::time::sleep;

use crate::error::TraderbotError;

/// Helper function to retry an async operation with exponential backoff
async fn with_retries<T, F, Fut>(operation: F, max_retries: u32, initial_delay_ms: u64) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>, // Closure returns Result<T, anyhow::Error>
{
    let mut attempt = 0;
    let mut delay_ms = initial_delay_ms;

    loop {
        attempt += 1;
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt >= max_retries {
                    return Err(e.context(format!("Failed after {} retries", max_retries)));
                }
                
                debug!("Retry {}/{}: {}. Delaying {}ms before next attempt.", 
                    attempt, max_retries, e, delay_ms);
                
                sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms * 2).min(10000); // Double delay, cap at 10s
            }
        }
    }
}

/// Simpler retry function (used occasionally)
async fn retry<T, F, Fut>(f: F, max_attempts: u32, description: &str) -> Result<T>
where
    F: Fn() -> Fut + Send,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt >= max_attempts {
                    return Err(e.context(format!("{} failed after {} attempts", description, max_attempts)));
                }
                warn!("{} attempt {}/{} failed: {}. Retrying...", description, attempt, max_attempts, e);
                sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
        }
    }
}

/// Wrapper around Solana's RpcClient that adds retry logic and error handling.
pub struct SolanaClient {
    rpc_client: Arc<RpcClient>,
}

impl SolanaClient {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let commitment_config = CommitmentConfig::confirmed();
        let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), commitment_config);
        Ok(Self {
            rpc_client: Arc::new(rpc_client),
        })
    }

    pub async fn check_connection(&self) -> Result<()> {
        self.rpc_client.get_latest_blockhash().await
            .map(|_| info!("Successfully connected to Solana RPC"))
            .map_err(|e| {
                error!("Failed to connect to Solana RPC: {}", e);
                TraderbotError::SolanaError(format!("Failed to connect to RPC: {}", e)).into()
            })
    }

    // Modified run_blocking to use the custom with_retries function
    async fn run_blocking<F, T>(&self, f: F) -> Result<T>
    where
        // Closure now takes Arc<RpcClient> and returns the specific solana_client::Result
        F: Fn(Arc<RpcClient>) -> solana_client::client_error::Result<T> + Send + Sync + Clone + 'static,
        T: Send + 'static,
    {
        with_retries(
            || { // The operation closure passed to with_retries
                let client = self.rpc_client.clone();
                let f_clone = f.clone();
                async move { // The future returned by the operation closure
                    tokio::task::spawn_blocking(move || f_clone(client))
                        .await // Await the JoinHandle
                        .map_err(|e| anyhow!("Tokio task JoinError: {}", e))? // Handle JoinError, convert to anyhow::Error
                        .map_err(|e| anyhow!("RPC client error: {}", e)) // Map solana_client::Error to anyhow::Error
                }
            },
            3, // Max attempts (total 4 tries)
            100 // Initial delay in ms
        ).await
    }


    pub async fn get_sol_balance(&self, pubkey: &Pubkey) -> Result<f64> {
        let lamports = self.rpc_client.get_balance(pubkey).await?;
        let sol_balance = lamports as f64 / 1_000_000_000.0;
        Ok(sol_balance)
    }

    pub async fn get_token_balance(&self, token_account_pubkey: &Pubkey) -> Result<(u64, u8)> {
        let account_data = self.get_account_data(token_account_pubkey).await?;
        let token_account = TokenAccount::unpack(&account_data)
            .map_err(|e| TraderbotError::SolanaError(format!("Failed to unpack token account: {}", e)))?;
        let mint_info = self.get_mint_info(&token_account.mint).await?;
        let decimals = mint_info.decimals;
        Ok((token_account.amount, decimals))
    }

    pub async fn get_token_balance_ui(&self, token_account_pubkey: &Pubkey) -> Result<f64> {
        let (amount, decimals) = self.get_token_balance(token_account_pubkey).await?;
        Ok(spl_token::amount_to_ui_amount(amount, decimals))
    }

    pub async fn get_token_supply(&self, mint_pubkey: &Pubkey) -> Result<u64> {
        let ui_amount = self.rpc_client.get_token_supply(mint_pubkey).await.context("Failed to get token supply RPC response")?;
        ui_amount.amount.parse::<u64>().context(format!(
            "Failed to parse token supply amount '{}' into u64",
            ui_amount.amount
        ))
    }

    pub async fn get_token_largest_accounts(&self, mint_pubkey: &Pubkey) -> Result<Vec<RpcTokenAccountBalance>> {
        let result = self.rpc_client.get_token_largest_accounts(mint_pubkey).await.context("Failed to get token largest accounts")?;
        Ok(result)
    }

    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self.rpc_client.get_account(pubkey).await.context(format!("Failed to get account data for {}", pubkey))?;
        Ok(account.data)
    }

     pub async fn get_mint_info(&self, mint_pubkey: &Pubkey) -> Result<Mint> {
        let account_data = self.get_account_data(mint_pubkey).await?;
        let mint_info = Mint::unpack(&account_data)
            .map_err(|e| TraderbotError::SolanaError(format!("Failed to unpack mint account: {}", e)))?;
        Ok(mint_info)
    }

    pub async fn get_associated_token_account(
        &self,
        wallet_address: &Pubkey,
        token_mint_address: &Pubkey,
    ) -> Pubkey {
        get_associated_token_address(wallet_address, token_mint_address)
    }

    // Updated to use with_retries directly
    pub async fn send_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<Signature> {
        let config = RpcSendTransactionConfig {
            skip_preflight: false,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
            max_retries: Some(0), // Set RPC client retries to 0, handle retries ourselves
            min_context_slot: None,
        };
        
        // Clone to pass into the retry function
        let transaction_clone = transaction.clone();
        let rpc_client = self.rpc_client.clone();
        
        with_retries(
            move || {
                let tx = transaction_clone.clone();
                let client = rpc_client.clone();
                let config = config.clone();
                
                async move {
                    match client.send_transaction_with_config(&tx, config).await {
                        Ok(sig) => {
                            debug!("Transaction sent with signature: {}", sig);
                            Ok(sig)
                        },
                        Err(e) => {
                            // Filter which errors should be retried
                            if Self::should_retry_transaction_error(&e) {
                                warn!("Retriable transaction error: {:?}", e);
                                Err(anyhow!("Retriable transaction error: {}", e))
                            } else {
                                error!("Non-retriable transaction error: {:?}", e);
                                Err(TraderbotError::TransactionError(format!("Send failed (non-retriable): {}", e)).into())
                            }
                        }
                    }
                }
            },
            4, // Max attempts
            500 // Initial delay in ms (longer for transactions)
        ).await
    }

    // Updated to use with_retries directly
    pub async fn simulate_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<RpcSimulateTransactionResult> {
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: true,
            commitment: Some(CommitmentConfig::confirmed()),
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
            accounts: None,
            min_context_slot: None,
            inner_instructions: false,
        };
        
        // Clone to pass into the retry function
        let transaction_clone = transaction.clone();
        let rpc_client = self.rpc_client.clone();
        
        let simulation_response = with_retries(
            move || {
                let tx = transaction_clone.clone();
                let client = rpc_client.clone();
                let config = config.clone();
                
                async move {
                    match client.simulate_transaction_with_config(&tx, config).await {
                        Ok(resp) => Ok(resp),
                        Err(e) => {
                            if Self::should_retry_rpc_error(&e) {
                                warn!("Retriable simulation error: {:?}", e);
                                Err(anyhow!("Retriable simulation error: {}", e))
                            } else {
                                error!("Non-retriable simulation error: {:?}", e);
                                Err(TraderbotError::TransactionError(format!("Simulation request failed (non-retriable): {}", e)).into())
                            }
                        }
                    }
                }
            },
            3, // Max attempts
            200 // Initial delay in ms
        ).await?;
        
        let simulation_result = simulation_response.value;
        if let Some(err) = &simulation_result.err {
            error!("Transaction simulation failed: {:?}", err);
            return Err(TraderbotError::TransactionError(format!("Simulation failed: {:?}", err)).into());
        } else {
            debug!("Transaction simulation successful. Logs: {:?}", simulation_result.logs);
        }
        Ok(simulation_result)
    }

    pub async fn resolve_token_address(&self, address_or_symbol: &str) -> Result<Pubkey> {
        match Pubkey::from_str(address_or_symbol) {
            Ok(pubkey) => Ok(pubkey),
            Err(_) => {
                warn!(
                    "Token symbol lookup not implemented. Failed to resolve: {}",
                    address_or_symbol
                );
                Err(TraderbotError::TokenNotFound(address_or_symbol.to_string()).into())
            }
        }
    }

     pub fn get_rpc(&self) -> Arc<RpcClient> {
        self.rpc_client.clone()
    }

    // Enhanced with better retry and error handling
    pub async fn confirm_transaction(
        &self,
        signature: &Signature,
        commitment: CommitmentLevel,
        timeout_secs: u64,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        let signature_copy = *signature;
        
        // The max time we'll wait
        let deadline = start_time + Duration::from_secs(timeout_secs);
        
        // Initial backoff values
        let mut retry_delay_ms = 1000; // Start with 1 second
        let max_delay_ms = 5000; // Cap at 5 seconds
        
        loop {
            // Use with_retries for the get_signature_statuses call
            let statuses_result = with_retries(
                move || {
                    let sig = signature_copy;
                    let client = self.rpc_client.clone();
                    
                    async move {
                        match client.get_signature_statuses(&[sig]).await {
                            Ok(response) => Ok(response),
                            Err(e) => {
                                if Self::should_retry_rpc_error(&e) {
                                    Err(anyhow!("Retriable status check error: {}", e))
                                } else {
                                    Err(TraderbotError::SolanaError(format!("Status check failed (non-retriable): {}", e)).into())
                                }
                            }
                        }
                    }
                },
                2, // Max attempts for status check
                100 // Initial delay in ms
            ).await;
            
            match statuses_result {
                Ok(statuses_response) => {
                    let status = statuses_response.value.get(0).cloned().flatten();
                    match status.map(|s| s.err) {
                        Some(None) => {
                            info!("Transaction {} confirmed.", signature_copy);
                            return Ok(());
                        }
                        Some(Some(e)) => {
                            error!("Transaction {} failed: {:?}", signature_copy, e);
                            return Err(TraderbotError::TransactionError(format!(
                                "Transaction failed: {:?}", e
                            )).into());
                        }
                        None => {
                            // Not yet confirmed, continue waiting
                            debug!("Transaction {} status not yet available...", signature_copy);
                        }
                    }
                },
                Err(e) => {
                    // If we hit a serious error in status checking, log it but continue
                    // if we haven't reached timeout
                    warn!("Error checking transaction status: {:?}", e);
                }
            }
            
            // Check timeout
            if std::time::Instant::now() > deadline {
                warn!("Timeout waiting for transaction {} confirmation after {}s", 
                    signature_copy, timeout_secs);
                return Err(TraderbotError::TransactionError(
                    format!("Confirmation timeout after {}s", timeout_secs)
                ).into());
            }
            
            // Exponential backoff with cap
            let jitter = rand::random::<u64>() % (retry_delay_ms / 10);
            let sleep_time = retry_delay_ms.saturating_add(jitter);
            tokio::time::sleep(Duration::from_millis(sleep_time)).await;
            
            // Increase delay for next iteration (capped)
            retry_delay_ms = (retry_delay_ms * 3 / 2).min(max_delay_ms);
        }
    }

    pub async fn get_transaction(
        &self,
        signature: &Signature,
        _commitment: CommitmentLevel,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(_commitment),
            max_supported_transaction_version: Some(0),
        };
        
        // Clone for async move
        let rpc_client = self.rpc_client.clone();
        let signature_copy = *signature;
        
        with_retries(
            move || {
                let client = rpc_client.clone();
                let sig = signature_copy;
                let cfg = config.clone();
                
                async move {
                    match client.get_transaction_with_config(&sig, cfg).await {
                        Ok(tx) => Ok(tx),
                        Err(e) => {
                            if Self::should_retry_rpc_error(&e) {
                                Err(anyhow!("Retriable error getting transaction: {}", e))
                            } else {
                                Err(TraderbotError::SolanaError(format!("Failed to get transaction: {}", e)).into())
                            }
                        }
                    }
                }
            },
            3, // Max retries
            200 // Initial delay in ms
        ).await
    }

    // Helper function to determine if an RPC error should be retried
    fn should_retry_rpc_error(error: &ClientError) -> bool {
        // Fallback: match error string for common retryable errors
        let err_str = error.to_string().to_lowercase();
        err_str.contains("rate limit")
            || err_str.contains("429")
            || err_str.contains("timeout")
            || err_str.contains("503")
            || err_str.contains("504")
            || err_str.contains("server error")
            || err_str.contains("too many requests")
            || err_str.contains("network")
            || err_str.contains("connection")
    }

    // Helper to determine if a transaction error should be retried
    fn should_retry_transaction_error(error: &ClientError) -> bool {
        // Extract TransactionError if present
        if let Some(transaction_error) = error.get_transaction_error() {
            match transaction_error {
                TransactionError::BlockhashNotFound |
                TransactionError::AccountInUse |
                TransactionError::AccountLoadedTwice |
                TransactionError::InstructionError(_, _) |
                TransactionError::AlreadyProcessed => true,
                _ => false
            }
        } else {
            // Check the error string for hints
            let error_str = error.to_string().to_lowercase();
            error_str.contains("blockhash not found") ||
            error_str.contains("already processed") ||
            error_str.contains("timeout") ||
            // Check if this could be a network error
            Self::should_retry_rpc_error(error)
        }
    }
}
