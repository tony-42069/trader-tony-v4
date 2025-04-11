use anyhow::{Context, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature}, // Removed Signer here, will add below
    signer::Signer, // Import the Signer trait explicitly
    transaction::{Transaction, VersionedTransaction}, // Added VersionedTransaction
};
use std::sync::Arc;
use tracing::{error, info, warn}; // Removed unused debug

use crate::solana::client::SolanaClient;
use crate::error::TraderbotError; // Assuming TraderbotError exists

#[derive(Clone)] // Removed Debug
pub struct WalletManager {
    keypair: Arc<Keypair>,
    solana_client: Arc<SolanaClient>,
    demo_mode: bool,
}

impl WalletManager {
    // Changed return type to Result<Arc<Self>> to handle potential errors during creation
    pub fn new(
        private_key_bs58: &str, // Expecting base58 private key string
        solana_client: Arc<SolanaClient>,
        demo_mode: bool,
    ) -> Result<Arc<Self>> {
        // Decode base58 private key
        let bytes = bs58::decode(private_key_bs58)
            .into_vec()
            .map_err(|e| {
                error!("Failed to decode base58 private key: {}", e);
                TraderbotError::WalletError(format!("Invalid private key format: {}", e))
            })?;

        // Create Keypair from bytes
        let keypair = Keypair::from_bytes(&bytes).map_err(|e| {
            error!("Failed to create keypair from bytes: {}", e);
            TraderbotError::WalletError(format!("Invalid private key data: {}", e))
        })?;

        info!(
            "WalletManager initialized. Pubkey: {}, Demo Mode: {}",
            keypair.pubkey(),
            demo_mode
        );

        let wallet_manager = Self {
            keypair: Arc::new(keypair),
            solana_client,
            demo_mode,
        };

        Ok(Arc::new(wallet_manager))
    }

    pub fn get_public_key(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    pub async fn get_sol_balance(&self) -> Result<f64> {
        self.solana_client
            .get_sol_balance(&self.get_public_key())
            .await
            .context("Failed to get SOL balance from SolanaClient")
    }

    // Returns the UI amount (f64)
    pub async fn get_token_balance_ui(&self, token_mint: &Pubkey) -> Result<f64> {
        let ata = self
            .solana_client
            .get_associated_token_account(&self.get_public_key(), token_mint)
            .await; // Assuming this is async now in client

        match self.solana_client.get_token_balance_ui(&ata).await {
            Ok(balance) => Ok(balance),
            Err(e) => {
                // Handle case where ATA might not exist yet (balance is 0)
                if e.to_string().contains("AccountNotFound") || e.to_string().contains("unpack") { // Crude error check
                    warn!("Token account {} not found or invalid for mint {}, assuming balance 0", ata, token_mint);
                    Ok(0.0)
                } else {
                    error!("Failed to get token balance for mint {}: {}", token_mint, e);
                    Err(e).context(format!("Failed to get balance for token mint {}", token_mint))
                }
            }
        }
    }

    // Signs and sends a VersionedTransaction, returning the signature
    pub async fn sign_and_send_versioned_transaction(
        &self,
        mut transaction: VersionedTransaction, // Take ownership
        _last_valid_block_height: u64, // Prefixed as unused (blockhash fetched internally)
    ) -> Result<Signature> {
        if self.demo_mode {
            info!("[DEMO MODE] Simulating transaction send for pubkey: {}", self.get_public_key());
            // Optionally simulate the transaction here
            match self.solana_client.simulate_versioned_transaction(&transaction).await {
                 Ok(sim_res) => info!("[DEMO MODE] Simulation successful. Logs: {:?}", sim_res.logs),
                 Err(e) => warn!("[DEMO MODE] Simulation failed: {}", e),
            }
            // Return a dummy signature for demo mode
            return Ok(Signature::default());
        }

        // Fetch recent blockhash just before signing (important!)
        let recent_blockhash = self.solana_client.get_rpc().get_latest_blockhash().await?; // Use Arc<RpcClient> directly
        transaction.message.set_recent_blockhash(recent_blockhash);

        // Sign the VersionedTransaction using the keypair
        // The `sign` method takes a slice of signers.
        // It modifies the transaction in place and returns a Result.
        // Sign the transaction message bytes using the keypair
        let message_bytes = transaction.message.serialize();
        let signature = self.keypair.try_sign_message(&message_bytes)
             .map_err(|e| {
                 error!("Failed to sign versioned transaction message: {}", e);
                 TraderbotError::WalletError(format!("Signing failed: {}", e))
             })?;

        // Replace the first (payer) signature placeholder with the actual signature
        if transaction.signatures.is_empty() {
             // This shouldn't happen for transactions created by Jupiter API, but handle defensively
             error!("Transaction has no signature slots to place signature.");
             return Err(TraderbotError::WalletError("Transaction has no signature slots".to_string()).into());
        }
        transaction.signatures[0] = signature;

        // Removed warning: warn!("Transaction signing is currently commented out due to compilation issues!");
        tracing::debug!("Signed versioned transaction with blockhash: {}", transaction.message.recent_blockhash()); // Re-enabled debug log

        // Send the transaction (without confirmation here)
        let signature = self
            .solana_client
            .send_versioned_transaction(&transaction)
            .await
            .context("Failed to send signed versioned transaction")?;

        info!(
            "Transaction sent. Signature: {}, Pubkey: {}",
            signature,
            self.get_public_key()
        );

        // Confirmation should ideally happen elsewhere (e.g., in the calling function or a dedicated task)
        // Example: self.solana_client.confirm_transaction(&signature, CommitmentLevel::Confirmed, 60).await?;

        Ok(signature)
    }

    // Helper to sign a legacy transaction (less common now)
    #[allow(dead_code)]
    pub async fn sign_legacy_transaction(&self, mut transaction: Transaction) -> Result<Transaction> {
         let recent_blockhash = self.solana_client.get_rpc().get_latest_blockhash().await?;
         transaction
            .try_sign(&[&*self.keypair], recent_blockhash)
             .map_err(|e| {
                error!("Failed to sign legacy transaction: {}", e);
                TraderbotError::WalletError(format!("Legacy signing failed: {}", e))
            })?;
        Ok(transaction)
    }

    // Provide access to the underlying keypair if needed (e.g., for specific signing needs)
    pub fn keypair(&self) -> Arc<Keypair> {
        self.keypair.clone()
    }

    // Public getter for the SolanaClient
    pub fn solana_client(&self) -> Arc<SolanaClient> {
        self.solana_client.clone()
    }
}
