use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey; // Added Pubkey
use std::{str::FromStr, sync::Arc}; // Added FromStr
use tracing::{debug, info, warn}; // Added warn, debug

use crate::api::birdeye::BirdeyeClient; // Import BirdeyeClient
use crate::api::helius::HeliusClient;
use crate::api::jupiter::JupiterClient;
use crate::solana::client::SolanaClient;
use crate::error::TraderbotError; // Assuming this exists
use crate::solana::wallet::WalletManager; // Added WalletManager
use base64::{engine::general_purpose::STANDARD, Engine as _}; // Import Engine trait globally for the file


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAnalysis {
    pub token_address: String,            // Added token address for context
    pub risk_level: u32,                  // 0-100 risk score
    pub details: Vec<String>,             // List of risk factors found
    pub liquidity_sol: f64,               // Liquidity in SOL (from primary pair)
    pub holder_count: u32,                // Number of holders
    pub has_mint_authority: bool,         // Whether mint authority exists
    pub has_freeze_authority: bool,       // Whether freeze authority exists
    pub lp_tokens_burned: bool,           // Whether LP tokens are burned/locked
    pub transfer_tax_percent: f64,        // Transfer tax percentage (buy/sell)
    pub can_sell: bool,                   // Whether token can be sold (honeypot check)
    pub concentration_percent: f64,       // Percentage held by top N holders
}


#[derive(Clone)] // Removed Debug
pub struct RiskAnalyzer {
    solana_client: Arc<SolanaClient>,
    helius_client: Arc<HeliusClient>, // Use Arc if shared
    jupiter_client: Arc<JupiterClient>, // Use Arc if shared
    birdeye_client: Arc<BirdeyeClient>, // Add BirdeyeClient
    wallet_manager: Arc<WalletManager>, // Added WalletManager
}

impl RiskAnalyzer {
    pub fn new(
        solana_client: Arc<SolanaClient>,
        helius_client: Arc<HeliusClient>,
        jupiter_client: Arc<JupiterClient>,
        birdeye_client: Arc<BirdeyeClient>, // Add BirdeyeClient parameter
        wallet_manager: Arc<WalletManager>, // Added WalletManager
    ) -> Self {
        Self {
            solana_client,
            helius_client,
            jupiter_client,
            birdeye_client, // Initialize BirdeyeClient field
            wallet_manager, // Added WalletManager
        }
    }

    // Main analysis function
    pub async fn analyze_token(&self, token_address_str: &str) -> Result<RiskAnalysis> {
        info!("Starting risk analysis for token: {}", token_address_str);

        let token_pubkey = Pubkey::from_str(token_address_str)
            .map_err(|_| TraderbotError::TokenNotFound(format!("Invalid token address: {}", token_address_str)))?;

        let mut risk_score: u32 = 0;
        let mut details = Vec::new();

        // --- Perform individual checks ---

        // 1. Mint & Freeze Authority Check
        let (has_mint_authority, has_freeze_authority) = match self.check_mint_freeze_authority(&token_pubkey).await {
            Ok((mint, freeze)) => {
                if mint {
                    risk_score += 30; // High risk
                    details.push("⚠️ Mint authority exists.".to_string());
                } else {
                     details.push("✅ Mint authority revoked.".to_string());
                }
                if freeze {
                    risk_score += 25; // High risk
                    details.push("⚠️ Freeze authority exists.".to_string());
                } else {
                     details.push("✅ Freeze authority revoked.".to_string());
                }
                (mint, freeze)
            }
            Err(e) => {
                warn!("Failed to check mint/freeze authority for {}: {:?}. Assuming authorities exist.", token_address_str, e);
                risk_score += 55; // Penalize heavily if check fails
                details.push("❓ Failed to check mint/freeze authority (assuming exists).".to_string());
                (true, true) // Assume worst case on error
            }
        };

        // 2. Liquidity Check (Using Birdeye)
        // TODO: Refine BirdeyeClient implementation for accurate SOL liquidity.
        let liquidity_sol = match self.check_liquidity(&token_pubkey).await { // Call the new function
            Ok(liq) => liq,
            Err(e) => {
                warn!("Failed to check liquidity for {}: {:?}. Assuming 0.", token_address_str, e);
                details.push("❓ Failed to check liquidity.".to_string());
                0.0 // Assume 0 liquidity on error
            }
        };
        if liquidity_sol < 5.0 { // Example threshold (adjust based on Birdeye data)
             risk_score += 20;
             details.push(format!("🟠 Low liquidity ({:.2} SOL).", liquidity_sol)); // Keep SOL unit for now
        } else {
             details.push(format!("✅ Liquidity: {:.2} SOL.", liquidity_sol));
        }


        // 3. LP Token Check (Placeholder)
        // TODO: Implement actual check using primary pair LP mint address (requires pair finding).
        let lp_tokens_burned = match self.check_lp_tokens_burned(&token_pubkey).await { // Call renamed function
             Ok(burned) => burned,
             Err(e) => {
                 warn!("Failed to check LP token status for {}: {:?}. Assuming not burned.", token_address_str, e);
                 details.push("❓ Failed to check LP token status.".to_string());
                 false // Assume not burned on error
             }
        };
        if !lp_tokens_burned {
            risk_score += 15;
            details.push("🟠 LP tokens may not be burned/locked (Placeholder Check).".to_string());
        } else {
             details.push("✅ LP tokens appear burned/locked.".to_string());
        }

        // 4. Sellability Check (Honeypot - Placeholder)
        // TODO: Implement simulation of a small buy followed by a sell.
        //       This requires careful handling of temporary accounts and potential costs.
        let can_sell = self.check_sellability_placeholder(&token_pubkey, &mut details).await?; // Pass details mutably
        if !can_sell {
            risk_score = 100; // CRITICAL RISK - Honeypot
            details.push("🔴 Honeypot detected (failed sell simulation).".to_string());
        } else {
             details.push("✅ Passed sell simulation.".to_string());
        }

        // 5. Holder Distribution Check (Placeholder)
        // TODO: Implement using RPC calls (getTokenLargestAccounts) or Helius/Birdeye APIs.
        let (holder_count, concentration_percent) = match self.check_holder_distribution(&token_pubkey).await { // Call renamed function
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to check holder distribution for {}: {:?}. Assuming 0 holders, 100% concentration.", token_address_str, e);
                details.push("❓ Failed to check holder distribution.".to_string());
                (0, 100.0) // Assume worst case on error
            }
        };
         if holder_count < 50 { // Example threshold
             risk_score += 10;
             details.push(format!("🟠 Low holder count ({}).", holder_count));
         } else {
              details.push(format!("✅ Holder count: {}.", holder_count));
         }
        if concentration_percent > 50.0 { // Example threshold for top 10 holders
            risk_score += 15;
            details.push(format!("🟠 High holder concentration ({:.1}% in top holders).", concentration_percent));
        } else {
             details.push(format!("✅ Holder concentration: {:.1}%.", concentration_percent));
        }

        // 6. Transfer Tax Check (Placeholder)
        // TODO: Implement actual check using Token-2022 extensions or simulation.
        let transfer_tax_percent = match self.check_transfer_tax(&token_pubkey).await { // Call renamed function
            Ok(tax) => tax,
            Err(e) => {
                warn!("Failed to check transfer tax for {}: {:?}. Assuming 0%.", token_address_str, e);
                details.push("❓ Failed to check transfer tax.".to_string());
                0.0 // Assume 0 tax on error
            }
        };
        if transfer_tax_percent > 5.0 { // Example threshold
            risk_score += (transfer_tax_percent as u32).min(25); // Cap penalty
            details.push(format!("🟠 High transfer tax ({:.1}% - Placeholder Check).", transfer_tax_percent));
        } else if transfer_tax_percent > 0.0 {
             details.push(format!("✅ Low transfer tax ({:.1}%).", transfer_tax_percent));
        } else {
             details.push("✅ No transfer tax detected.".to_string());
        }

        // --- Final Score Calculation ---
        let final_risk_level = risk_score.min(100); // Cap score at 100

        info!(
            "Risk analysis complete for {}: Score = {}/100",
            token_address_str, final_risk_level
        );
        debug!("Risk details for {}: {:?}", token_address_str, details);

        Ok(RiskAnalysis {
            token_address: token_address_str.to_string(),
            risk_level: final_risk_level,
            details,
            liquidity_sol,
            holder_count,
            has_mint_authority,
            has_freeze_authority,
            lp_tokens_burned,
            transfer_tax_percent,
            can_sell,
            concentration_percent,
        })
    }

    // --- Risk Check Implementations ---

    async fn check_mint_freeze_authority(&self, token_mint: &Pubkey) -> Result<(bool, bool)> {
        debug!("Checking mint/freeze authority for {}", token_mint);
        let mint_info = self.solana_client.get_mint_info(token_mint).await
            .context("Failed to get mint info")?;

        let has_mint_authority = mint_info.mint_authority.is_some();
        let has_freeze_authority = mint_info.freeze_authority.is_some();
        debug!("Mint Authority: {}, Freeze Authority: {}", has_mint_authority, has_freeze_authority);
        Ok((has_mint_authority, has_freeze_authority))
    }

    // Checks liquidity using the BirdeyeClient
    async fn check_liquidity(&self, token_address: &Pubkey) -> Result<f64> {
        debug!("Checking liquidity via Birdeye for {}", token_address);
        // Use the Birdeye client to fetch liquidity data
        // The BirdeyeClient::get_liquidity_sol function currently returns a placeholder.
        // It needs to be implemented correctly based on Birdeye's API response structure
        // to extract actual SOL liquidity from the primary pair.
        self.birdeye_client.get_liquidity_sol(&token_address.to_string()).await
            .context(format!("Failed to get liquidity from Birdeye for {}", token_address))
    }

    async fn check_lp_tokens_burned(&self, token_address: &Pubkey) -> Result<bool> { // Renamed function
        warn!("LP token burn check is using placeholder data and needs full implementation.");
        // TODO: Implement actual LP token burn/lock check.
        // Steps:
        // 1. Find the primary liquidity pool address for the token (e.g., TOKEN/SOL).
        //    - This might require querying an external API (Helius get_token_metadata, Birdeye, Jupiter strict list)
        //    - Or finding the pool associated with the token mint via Raydium/Orca SDKs/APIs.
        // 2. Get the LP token mint address associated with that pool.
        // 3. Use `solana_client.get_token_supply(&lp_mint_pubkey)` to get total supply.
        // 4. Use `solana_client.get_token_largest_accounts(&lp_mint_pubkey)` to get top holders.
        // 5. Check if a known burn address (e.g., 11111111111111111111111111111111) holds >95% of the supply.
        // 6. Alternatively/Additionally, check if significant amounts are held by known locker contract addresses.
        // 7. Return true if burned/locked, false otherwise.

        // Placeholder logic:
        debug!("LP Burn Check Placeholder for {}", token_address);
        Ok(rand::random::<f64>() > 0.2) // 80% chance LP tokens are "burned" in placeholder
    }

    // Checks if a token can likely be sold by simulating a small buy then sell
    async fn check_sellability_placeholder(&self, token_address: &Pubkey, details: &mut Vec<String>) -> Result<bool> {
        warn!("Sellability check (honeypot) is using placeholder simulation logic.");
        // TODO: Refine simulation amounts, error handling, and potentially use a temporary wallet.

        let wallet_pubkey = self.wallet_manager.get_public_key();
        let token_address_str = token_address.to_string();
        let sol_mint_str = crate::api::jupiter::SOL_MINT.to_string();


        // --- Simulate Buy ---
        // 1. Get Buy Quote (Small amount, e.g., 0.001 SOL)
        let buy_amount_lamports = 1_000_000; // 0.001 SOL
        let buy_quote = match self.jupiter_client.get_quote(
            &sol_mint_str,
            &token_address_str,
            buy_amount_lamports,
            100 // 1% slippage for simulation
        ).await {
            Ok(q) => q,
            Err(e) => {
                warn!("Sellability Check: Failed to get buy quote for {}: {:?}", token_address_str, e);
                return Ok(false); // Cannot buy, assume not sellable for safety
            }
        };

        // Estimate amount of token we would receive
        let estimated_token_out = match buy_quote.out_amount.parse::<u64>() {
             Ok(amount) if amount > 0 => amount,
             _ => {
                 warn!("Sellability Check: Invalid estimated token output amount in buy quote for {}.", token_address_str);
                 return Ok(false); // Invalid quote
             }
        };


        // 2. Get Buy Swap Transaction
        let buy_swap_response = match self.jupiter_client.get_swap_transaction(
            &buy_quote,
            &wallet_pubkey.to_string(),
            None // No priority fee for simulation
        ).await {
            Ok(resp) => resp,
            Err(e) => {
                 warn!("Sellability Check: Failed to get buy swap tx for {}: {:?}", token_address_str, e);
                 return Ok(false);
            }
        };

        // 3. Decode and Simulate Buy Transaction
        let buy_tx_bytes = match STANDARD.decode(&buy_swap_response.swap_transaction) {
             Ok(bytes) => bytes,
             Err(e) => {
                 warn!("Sellability Check: Failed to decode buy tx for {}: {:?}", token_address_str, e);
                 return Ok(false);
             }
        };
         let buy_versioned_tx: solana_sdk::transaction::VersionedTransaction = match bincode::deserialize(&buy_tx_bytes) {
             Ok(tx) => tx,
             Err(e) => {
                  warn!("Sellability Check: Failed to deserialize buy tx for {}: {:?}", token_address_str, e);
                  return Ok(false);
             }
         };

        // We don't strictly need the buy simulation to succeed, only the sell.
        // But logging the failure is useful.
        if let Err(e) = self.solana_client.simulate_versioned_transaction(&buy_versioned_tx).await {
             warn!("Sellability Check: Buy simulation failed for {}: {:?}", token_address_str, e);
             // Don't return false here, proceed to sell check.
             details.push(format!("⚠️ Buy simulation failed ({}).", e)); // Add detail
        } else {
             debug!("Sellability Check: Buy simulation successful for {}.", token_address_str);
        }


        // --- Simulate Sell ---
        // 4. Get Sell Quote (Selling the estimated amount received from buy)
        // Need token decimals for this quote
        let _token_decimals = match self.solana_client.get_mint_info(token_address).await { // Prefixed as unused
             Ok(info) => info.decimals,
             Err(e) => {
                 warn!("Sellability Check: Failed to get decimals for {}: {:?}. Cannot simulate sell.", token_address_str, e);
                 return Ok(false); // Cannot proceed without decimals
             }
        };

        let sell_quote = match self.jupiter_client.get_quote(
            &token_address_str,
            &sol_mint_str,
            estimated_token_out, // Sell the amount we simulated buying
            100 // 1% slippage
        ).await {
             Ok(q) => q,
             Err(e) => {
                 warn!("Sellability Check: Failed to get sell quote for {}: {:?}", token_address_str, e);
                 return Ok(false); // If we can't get a sell quote, assume not sellable
             }
        };

        // 5. Get Sell Swap Transaction
         let sell_swap_response = match self.jupiter_client.get_swap_transaction(
            &sell_quote,
            &wallet_pubkey.to_string(),
            None
        ).await {
            Ok(resp) => resp,
            Err(e) => {
                 warn!("Sellability Check: Failed to get sell swap tx for {}: {:?}", token_address_str, e);
                 return Ok(false);
            }
        };

        // 6. Decode and Simulate Sell Transaction
         let sell_tx_bytes = match STANDARD.decode(&sell_swap_response.swap_transaction) {
             Ok(bytes) => bytes,
             Err(e) => {
                 warn!("Sellability Check: Failed to decode sell tx for {}: {:?}", token_address_str, e);
                 return Ok(false);
             }
        };
         let sell_versioned_tx: solana_sdk::transaction::VersionedTransaction = match bincode::deserialize(&sell_tx_bytes) {
             Ok(tx) => tx,
             Err(e) => {
                  warn!("Sellability Check: Failed to deserialize sell tx for {}: {:?}", token_address_str, e);
                  return Ok(false);
             }
         };

        match self.solana_client.simulate_versioned_transaction(&sell_versioned_tx).await {
            Ok(_) => {
                debug!("Sellability Check: Sell simulation successful for {}.", token_address_str);
                Ok(true) // Both buy (optional) and sell simulations succeeded
            }
            Err(e) => {
                warn!("Sellability Check: Sell simulation FAILED for {}: {:?}", token_address_str, e);
                Ok(false) // Sell simulation failed - potential honeypot
            }
        }
    }

    async fn check_holder_distribution(&self, token_address: &Pubkey) -> Result<(u32, f64)> { // Renamed function
        warn!("Holder distribution check is using placeholder data and needs full implementation.");
        // TODO: Implement actual holder distribution check.
        // Steps:
        // 1. Get total supply using `solana_client.get_token_supply(token_address)`.
        // 2. Get largest token accounts using `solana_client.get_token_largest_accounts(token_address)`.
        //    - This returns a Vec<RpcTokenAccountBalance> which includes `ui_amount_string`.
        // 3. Calculate the total amount held by the top N (e.g., top 10) holders.
        //    - Need to parse `ui_amount_string` to f64/u64, considering decimals.
        // 4. Calculate the concentration percentage: (top_N_amount / total_supply) * 100.
        // 5. Estimate total holder count (this is difficult via RPC, might need Helius/Birdeye).
        //    - Helius DAS API might provide holder count directly in asset metadata.
        //    - Alternatively, estimate based on the number of accounts returned by `getTokenLargestAccounts` if it returns more than N.

        // Placeholder logic:
        debug!("Holder Distribution Check Placeholder for {}", token_address);
        let holder_count = 50 + rand::random::<u32>() % 1000; // Random: 50-1049 holders
        let concentration_percent = rand::random::<f64>() * 60.0; // Random: 0-60% concentration
        Ok((holder_count, concentration_percent))
    }

    async fn check_transfer_tax(&self, token_address: &Pubkey) -> Result<f64> { // Renamed function
        warn!("Transfer tax check is using placeholder data and needs full implementation.");
        // TODO: Implement actual transfer tax check.
        // Steps:
        // 1. Check if the token mint is associated with the Token-2022 program.
        // 2. If it is, fetch mint account data and parse extensions using `spl_token_2022::extension::StateWithExtensions::unpack`.
        // 3. Look for the `TransferFeeConfig` extension.
        // 4. If found, extract the transfer fee basis points and calculate the percentage.
        // 5. If not Token-2022 or no extension found, potentially simulate a small transfer between two temporary wallets
        //    owned by the bot's keypair to observe any fee deduction (more complex).
        // 6. Return the calculated tax percentage (or 0.0 if none detected).

        // Placeholder logic:
        debug!("Transfer Tax Check Placeholder for {}", token_address);
        if rand::random::<f64>() < 0.1 { // 10% chance of having tax in placeholder
            Ok(rand::random::<f64>() * 15.0) // Random: 0-15% tax
        } else {
            Ok(0.0) // Assume 0 tax otherwise
        }
    }
}
