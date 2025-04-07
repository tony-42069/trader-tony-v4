use anyhow::{anyhow, Context, Result};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcSendTransactionConfig, RpcSimulateTransactionConfig}, // Added RpcSimulateTransactionConfig
    rpc_response::{RpcResult, RpcSimulateTransactionResult}, // Added RpcSimulateTransactionResult
};
use solana_sdk::{
    account::Account as SolanaAccount, // Alias to avoid conflict with spl_token::state::Account
    commitment_config::{CommitmentConfig, CommitmentLevel},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signature,
    transaction::{Transaction, VersionedTransaction}, // Added VersionedTransaction
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::state::{Account as TokenAccount, Mint}; // Renamed Account to TokenAccount
use std::{str::FromStr, sync::Arc, time::Duration}; // Added Arc, Duration
use tracing::{debug, error, info, warn};

use crate::error::TraderbotError; // Assuming TraderbotError exists

// Use Arc for shared ownership if the client needs to be shared across threads
#[derive(Clone)] // Removed Debug derive
pub struct SolanaClient {
    rpc_client: Arc<RpcClient>, // Use Arc for shared ownership
}

impl SolanaClient {
    pub fn new(rpc_url: &str) -> Result<Self> {
        // Use confirmed commitment for reads, but allow specifying for transactions
        let commitment_config = CommitmentConfig::confirmed();
        let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), commitment_config);
        // Optional: Add connection check
        match rpc_client.get_latest_blockhash() {
            Ok(_) => info!("Successfully connected to Solana RPC: {}", rpc_url),
            Err(e) => {
                error!("Failed to connect to Solana RPC {}: {}", rpc_url, e);
                return Err(TraderbotError::SolanaError(format!(
                    "Failed to connect to RPC {}: {}",
                    rpc_url, e
                ))
                .into());
            }
        }

        Ok(Self {
            rpc_client: Arc::new(rpc_client),
        })
    }

    // Helper to run blocking RPC calls in a tokio task, extracting the value from RpcResponse
    async fn run_blocking<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Arc<RpcClient>) -> solana_client::client_error::Result<T> + Send + 'static, // Use the client_error::Result type directly
        T: Send + 'static,
    {
        let client = self.rpc_client.clone();
        let client_result = tokio::task::spawn_blocking(move || f(client))
            .await? // Handle JoinError
            .map_err(|e| {
                error!("Solana RPC client error: {:?}", e);
                // Convert ClientError to anyhow::Error using TraderbotError as intermediate
                TraderbotError::SolanaError(format!("RPC Client Error: {}", e))
            })?;

        // If the RPC call itself returned an error, it's already converted.
        // If it succeeded, client_result is T.
        Ok(client_result)
    }


    pub async fn get_sol_balance(&self, pubkey: &Pubkey) -> Result<f64> {
        let pubkey_copy = *pubkey; // Copy the Pubkey value
        let lamports = self
            .run_blocking(move |client| client.get_balance(&pubkey_copy)) // Move the copy into the closure
            .await?;
        let sol_balance = lamports as f64 / 1_000_000_000.0;
        Ok(sol_balance)
    }

    pub async fn get_token_balance(&self, token_account_pubkey: &Pubkey) -> Result<(u64, u8)> {
        let account_data = self.get_account_data(token_account_pubkey).await?;
        let token_account = TokenAccount::unpack(&account_data)
            .map_err(|e| TraderbotError::SolanaError(format!("Failed to unpack token account: {}", e)))?;

        // To get decimals, we need the mint info
        let mint_info = self.get_mint_info(&token_account.mint).await?;
        let decimals = mint_info.decimals;

        Ok((token_account.amount, decimals))
    }

     pub async fn get_token_balance_ui(&self, token_account_pubkey: &Pubkey) -> Result<f64> {
        let (amount, decimals) = self.get_token_balance(token_account_pubkey).await?;
        // Use amount_to_ui_amount to convert lamports (u64) to UI representation (f64)
        Ok(spl_token::amount_to_ui_amount(amount, decimals)) // Correct function
     }


    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        // get_account returns Result<Account>, not RpcResult<Account>
        let account = self.rpc_client.get_account(pubkey).map_err(|e| {
             error!("Failed to get account data for {}: {:?}", pubkey, e);
             TraderbotError::SolanaError(format!("Failed to get account {}: {}", pubkey, e))
        })?;
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

    // Sends a VersionedTransaction without confirmation
    pub async fn send_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<Signature> {
         let config = RpcSendTransactionConfig {
            skip_preflight: false, // Perform preflight checks
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64), // Specify encoding
            max_retries: Some(5), // Retry sending a few times
            min_context_slot: None,
        };
        // send_transaction_with_config returns Result<Signature>, not RpcResult
        let signature = self.rpc_client.send_transaction_with_config(transaction, config).map_err(|e| {
             error!("Failed to send transaction: {:?}", e);
             TraderbotError::TransactionError(format!("Send failed: {}", e))
        })?;

        debug!("Transaction sent with signature: {}", signature);
        Ok(signature)
    }


    // Confirms a transaction with a timeout
    pub async fn confirm_transaction(
        &self,
        signature: &Signature,
        commitment: CommitmentLevel,
        timeout_secs: u64,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        loop {
            // get_signature_statuses returns Result<Response<Vec<Option<TransactionStatus>>>>
            let statuses_response = self.rpc_client.get_signature_statuses(&[*signature]).map_err(|e| {
                 error!("Failed to get signature status for {}: {:?}", signature, e);
                 TraderbotError::SolanaError(format!("Status check failed: {}", e))
            })?;

            // Extract the status from the Response and Vec
            let status = statuses_response.value.get(0).cloned().flatten(); // Get Option<TransactionStatus>

            match status.map(|s| s.err) { // Check the err field within TransactionStatus
                Some(None) => { // Status received, no error field
                    info!("Transaction {} confirmed.", signature);
                    return Ok(());
                }
                Some(Some(e)) => { // Status received, contains a TransactionError
                    error!("Transaction {} failed: {:?}", signature, e);
                    return Err(TraderbotError::TransactionError(format!(
                        "Transaction failed: {:?}", e
                    )).into());
                }
                None => { // Status not yet available or Vec was empty
                    debug!("Transaction {} status not yet available...", signature);
                }
            }


            if start_time.elapsed() > Duration::from_secs(timeout_secs) {
                warn!("Timeout waiting for transaction {} confirmation", signature);
                return Err(TraderbotError::TransactionError(
                    "Confirmation timeout".to_string(),
                )
                .into());
            }

            tokio::time::sleep(Duration::from_secs(2)).await; // Poll interval
        }
    }

    // Simulate a versioned transaction
    pub async fn simulate_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<RpcSimulateTransactionResult> {
         let config = RpcSimulateTransactionConfig {
            sig_verify: false, // Signature verification is handled before simulation usually
            replace_recent_blockhash: true, // Use a recent blockhash for simulation
            commitment: Some(CommitmentConfig::confirmed()), // Use CommitmentConfig
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64), // Specify encoding
            accounts: None, // Not needed for basic simulation
            min_context_slot: None,
            inner_instructions: false, // Added missing field
        };

        // simulate_transaction_with_config returns Result<Response<RpcSimulateTransactionResult>>
        let simulation_response = self.rpc_client.simulate_transaction_with_config(transaction, config).map_err(|e| {
             error!("Failed to send simulation request: {:?}", e);
             TraderbotError::TransactionError(format!("Simulation request failed: {}", e))
        })?;

        let simulation_result = simulation_response.value; // Extract value from Response

        if let Some(err) = &simulation_result.err {
            error!("Transaction simulation failed: {:?}", err);
             return Err(TraderbotError::TransactionError(format!("Simulation failed: {:?}", err)).into());
        } else {
            debug!("Transaction simulation successful. Logs: {:?}", simulation_result.logs);
        }

        Ok(simulation_result)
    }


    // Placeholder for resolving token address (requires external data source)
    pub async fn resolve_token_address(&self, address_or_symbol: &str) -> Result<Pubkey> {
        match Pubkey::from_str(address_or_symbol) {
            Ok(pubkey) => Ok(pubkey),
            Err(_) => {
                // TODO: Implement lookup using a token list (e.g., Jupiter's list) or API
                warn!(
                    "Token symbol lookup not implemented. Failed to resolve: {}",
                    address_or_symbol
                );
                Err(TraderbotError::TokenNotFound(address_or_symbol.to_string()).into())
            }
        }
    }

     // Get the underlying RpcClient if direct access is needed (use with caution)
    pub fn get_rpc(&self) -> Arc<RpcClient> {
        self.rpc_client.clone()
    }
}
