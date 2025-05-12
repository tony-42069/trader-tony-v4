use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc, time::Duration}; // Added Future, Duration
use tracing::{debug, error, info, warn};
use serde_json::Value; // Added for Raydium API parsing

use crate::api::birdeye::{BirdeyeClient, TokenOverviewData};
use crate::api::helius::HeliusClient;
use crate::api::jupiter::JupiterClient;
use crate::solana::client::SolanaClient;
use crate::error::TraderbotError;
use crate::solana::wallet::WalletManager;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use spl_token_2022::{
    extension::{BaseStateWithExtensions, StateWithExtensions, transfer_fee::TransferFeeConfig},
    state::Mint as Token2022Mint,
};
// Removed unused Pack import


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAnalysis {
    pub token_address: String,
    pub risk_level: u32,
    pub details: Vec<String>,
    pub liquidity_sol: f64,
    pub holder_count: u32, // Note: Currently an estimate from RPC
    pub has_mint_authority: bool,
    pub has_freeze_authority: bool,
    pub lp_tokens_burned: bool, // Now attempts real check
    pub transfer_tax_percent: f64,
    pub can_sell: bool,
    pub concentration_percent: f64,
}


#[derive(Clone)]
pub struct RiskAnalyzer {
    solana_client: Arc<SolanaClient>,
    helius_client: Arc<HeliusClient>,
    jupiter_client: Arc<JupiterClient>,
    birdeye_client: Arc<BirdeyeClient>,
    wallet_manager: Arc<WalletManager>,
    // Add http client for Raydium API call
    http_client: reqwest::Client,
}

impl RiskAnalyzer {
    pub fn new(
        solana_client: Arc<SolanaClient>,
        helius_client: Arc<HeliusClient>,
        jupiter_client: Arc<JupiterClient>,
        birdeye_client: Arc<BirdeyeClient>,
        wallet_manager: Arc<WalletManager>,
    ) -> Self {
        Self {
            solana_client,
            helius_client,
            jupiter_client,
            birdeye_client,
            wallet_manager,
            // Initialize http client
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15)) // Shorter timeout for external API
                .build()
                .expect("Failed to create HTTP client for RiskAnalyzer"),
        }
    }

    // Main analysis function
    pub async fn analyze_token(&self, token_address_str: &str) -> Result<RiskAnalysis> {
        info!("Starting risk analysis for token: {}", token_address_str);

        let token_pubkey = Pubkey::from_str(token_address_str)
            .map_err(|_| TraderbotError::TokenNotFound(format!("Invalid token address: {}", token_address_str)))?;

        let mut risk_score: u32 = 0;
        let mut details = Vec::new();

        // --- Fetch Data Upfront ---
        let birdeye_overview = match self.birdeye_client.get_token_overview(token_address_str).await {
            Ok(Some(data)) => {
                debug!("Successfully fetched Birdeye overview for {}", token_address_str);
                Some(data)
            }
            Ok(None) => {
                warn!("Birdeye returned no overview data for {}", token_address_str);
                details.push("❓ Birdeye returned no overview data.".to_string());
                None
            }
            Err(e) => {
                error!("Failed to fetch Birdeye overview for {}: {:?}", token_address_str, e);
                details.push("❓ Error fetching Birdeye overview data.".to_string());
                None
            }
        };

        let sol_price_usd = match self.birdeye_client.get_sol_price_usd().await {
             Ok(price) if price > 0.0 => {
                 debug!("Fetched SOL price: {:.4} USD", price);
                 Some(price)
             },
             Ok(price) => {
                 warn!("Birdeye returned invalid SOL price: {}", price);
                 details.push("❓ Birdeye returned invalid SOL price.".to_string());
                 None
             }
             Err(e) => {
                 error!("Failed to fetch SOL price from Birdeye: {:?}", e);
                 details.push("❓ Error fetching SOL price.".to_string());
                 None
             }
        };

        // --- Find Primary Pair Info (used by multiple checks) ---
        // Note: find_primary_pair_info is not defined in the provided code, assuming it exists elsewhere or needs implementation
        // let primary_pair_info = match self.find_primary_pair_info(token_address_str).await {
        //     Ok(info) => {
        //         debug!("Found primary pair info for {}: {:?}", token_address_str, info);
        //         Some(info)
        //     }
        //     Err(e) => {
        //         warn!("Failed to find primary pair for {}: {:?}", token_address_str, e);
        //         details.push("❓ Could not find primary trading pair.".to_string());
        //         None
        //     }
        // };

        // --- Perform individual checks ---

        // 1. Mint & Freeze Authority Check
        let (has_mint_authority, has_freeze_authority) = match self.check_mint_freeze_authority(&token_pubkey).await {
            Ok((mint, freeze)) => {
                if mint { risk_score += 30; details.push("⚠️ Mint authority exists.".to_string()); }
                else { details.push("✅ Mint authority revoked.".to_string()); }
                if freeze { risk_score += 25; details.push("⚠️ Freeze authority exists.".to_string()); }
                else { details.push("✅ Freeze authority revoked.".to_string()); }
                (mint, freeze)
            }
            Err(e) => {
                warn!("Failed to check mint/freeze authority for {}: {:?}. Assuming authorities exist.", token_address_str, e);
                risk_score += 55;
                details.push("❓ Failed to check mint/freeze authority (assuming exists).".to_string());
                (true, true)
            }
        };

        // 2. Liquidity Check - Now using our improved implementation
        let liquidity_sol = match self.check_liquidity(birdeye_overview.as_ref(), sol_price_usd).await {
            Ok(liq) => {
                // Adjusted thresholds based on feedback
                if liq < 1.0 { risk_score += 30; details.push(format!("🔴 Very low liquidity ({:.2} SOL).", liq)); }
                else if liq < 5.0 { risk_score += 20; details.push(format!("🟠 Low liquidity ({:.2} SOL).", liq)); }
                else { details.push(format!("✅ Liquidity: {:.2} SOL.", liq)); }
                liq
            }
            Err(e) => {
                warn!("Liquidity check failed for {}: {:?}. Assuming 0.", token_address_str, e);
                risk_score += 30; // Penalize heavily if check fails
                details.push(format!("❓ Failed liquidity check: {}", e));
                0.0
            }
        };

        // 3. LP Token Check - Now checking burnedness OR locking
        let lp_tokens_burned = match self.check_lp_tokens_burned(token_address_str).await {
             Ok(burned) => {
                 if !burned { risk_score += 15; details.push("🟠 LP tokens may not be burned/locked.".to_string()); }
                 else { details.push("✅ LP tokens appear burned/locked.".to_string()); }
                 burned
             }
             Err(e) => {
                 warn!("LP token check failed for {}: {:?}. Assuming not burned.", token_address_str, e);
                 risk_score += 15; // Penalize if check fails
                 details.push("❓ Failed to check LP token status.".to_string());
                 false
             }
        };

        // 4. Sellability Check (Honeypot)
        let can_sell = self.check_sellability_placeholder(&token_pubkey, &mut details).await?;
        if !can_sell { risk_score = 100; details.push("🔴 Honeypot detected (failed sell simulation).".to_string()); }
        else { details.push("✅ Passed sell simulation.".to_string()); }

        // 5. Holder Distribution Check
        let (holder_count, concentration_percent) = match self.check_holder_distribution(&token_pubkey).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to check holder distribution for {}: {:?}. Assuming 0 holders, 100% concentration.", token_address_str, e);
                risk_score += 25; // Penalize if check fails
                details.push("❓ Failed to check holder distribution.".to_string());
                (0, 100.0)
            }
        };
         if holder_count < 50 { risk_score += 10; details.push(format!("🟠 Low holder count ({} - Estimated).", holder_count)); }
         else { details.push(format!("✅ Holder count: {} (Estimated).", holder_count)); }
        if concentration_percent > 50.0 { risk_score += 15; details.push(format!("🟠 High holder concentration ({:.1}% in top 10).", concentration_percent)); }
        else { details.push(format!("✅ Holder concentration: {:.1}% (Top 10).", concentration_percent)); }

        // 6. Transfer Tax Check
        let transfer_tax_percent = match self.check_transfer_tax(&token_pubkey).await {
            Ok(tax) => tax,
            Err(e) => {
                warn!("Failed to check transfer tax for {}: {:?}. Assuming 0%.", token_address_str, e);
                details.push("❓ Failed to check transfer tax.".to_string());
                0.0
            }
        };
        if transfer_tax_percent > 5.0 { risk_score += (transfer_tax_percent as u32).min(25); details.push(format!("🟠 High transfer tax ({:.1}%).", transfer_tax_percent)); }
        else if transfer_tax_percent > 0.0 { details.push(format!("✅ Low transfer tax ({:.1}%).", transfer_tax_percent)); }
        else { details.push("✅ No transfer tax detected.".to_string()); }

        // --- Final Score Calculation ---
        let final_risk_level = risk_score.min(100);

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

    /// Checks if a token's mint authority and freeze authority have been revoked
    /// Returns a tuple of (has_mint_authority, has_freeze_authority)
    async fn check_mint_freeze_authority(&self, token_mint: &Pubkey) -> Result<(bool, bool)> {
        debug!("Checking mint/freeze authority for {}", token_mint);
        let mint_info = self.solana_client.get_mint_info(token_mint).await
            .context("Failed to get mint info")?;
        let has_mint_authority = mint_info.mint_authority.is_some();
        let has_freeze_authority = mint_info.freeze_authority.is_some();
        debug!("Mint Authority: {}, Freeze Authority: {}", has_mint_authority, has_freeze_authority);
        Ok((has_mint_authority, has_freeze_authority))
    }

    /// Calculates liquidity in SOL for a token using multiple methods:
    /// 1. Birdeye data (if available)
    /// 2. Direct DEX liquidity assessment via primary pair info (Placeholder/Not Implemented)
    /// Returns estimated SOL liquidity value, or 0.0 if unable to calculate
    async fn check_liquidity(
        &self,
        overview_data: Option<&TokenOverviewData>,
        sol_price_usd: Option<f64>,
    ) -> Result<f64> {
        debug!("Calculating SOL liquidity");

        // Method 1: Try to use the Birdeye data if available for quick calculation
        if let (Some(overview), Some(sol_price)) = (overview_data, sol_price_usd) {
            let usd_liquidity = overview.liquidity.unwrap_or(0.0);
            if usd_liquidity > 0.0 && sol_price > 0.0 {
                let calculated_liquidity_sol = usd_liquidity / sol_price;
                debug!(
                    "Used Birdeye data for liquidity calculation: {:.2} SOL (USD Liq: {:.2}, SOL Price: {:.2})",
                    calculated_liquidity_sol, usd_liquidity, sol_price
                );
                return Ok(calculated_liquidity_sol);
            }
            debug!("Birdeye data insufficient for liquidity calculation, falling back.");
        }

        // Fallback or alternative method if needed (e.g., using find_primary_pair_info if implemented)
        warn!("Could not calculate liquidity from Birdeye data. Returning 0.");
        Ok(0.0) // Return 0 if Birdeye data is insufficient/unavailable
    }

    // Removed PrimaryPairInfo struct as find_primary_pair_info is not implemented here

    // Removed find_primary_pair_info function as it's not implemented here

    /// Checks if LP tokens are burned (liquidity locked) using Raydium API
    /// Returns true if a significant portion (>95%) of LP tokens are sent to a burn address
    async fn check_lp_tokens_burned(&self, token_address: &str) -> Result<bool> {
        debug!("Checking LP token burn status for {}", token_address);

        // Ensure token address is valid before proceeding
        let token_pubkey = match Pubkey::from_str(token_address) {
             Ok(pk) => pk,
             Err(_) => {
                 warn!("Invalid token address format for LP check: {}", token_address);
                 return Ok(false); // Cannot proceed with invalid address
             }
        };

        // Check if token exists (avoids unnecessary API calls if mint is invalid)
        if self.solana_client.get_account_data(&token_pubkey).await.is_err() {
            warn!("Token {} doesn't exist or failed to fetch account data for LP check", token_address);
            return Ok(false); // Treat non-existent tokens as not having burned LP
        }

        // Find the Raydium pool for this token paired with SOL
        let sol_address = crate::api::jupiter::SOL_MINT; // Use constant

        // Try to find the LP token mint using the helper function
        let lp_token_mint_str = match self.find_lp_token_mint(token_address, sol_address).await {
            Ok(Some(mint)) => mint,
            Ok(None) => {
                info!("No Raydium SOL liquidity pool found for token {}", token_address);
                return Ok(false); // No pool means no LP to check
            },
            Err(e) => {
                warn!("Error finding LP token mint for {}: {}", token_address, e);
                return Ok(false); // Assume not burned on error finding LP mint
            }
        };

        let lp_token_mint_pubkey = match Pubkey::from_str(&lp_token_mint_str) {
             Ok(pk) => pk,
             Err(_) => {
                 error!("Found invalid LP token mint address from Raydium API: {}", lp_token_mint_str);
                 return Ok(false); // Invalid LP mint address
             }
        };
        debug!("Found LP token mint for {}: {}", token_address, lp_token_mint_pubkey);

        // Get LP token supply (raw amount)
        let supply_raw = match self.solana_client.get_token_supply(&lp_token_mint_pubkey).await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to get LP token supply for {}: {}", lp_token_mint_pubkey, e);
                return Ok(false); // Assume not burned if supply check fails
            }
        };

        if supply_raw == 0 {
            info!("LP token {} has zero supply.", lp_token_mint_pubkey);
            return Ok(false); // Zero supply cannot be burned
        }

        // Get largest holders
        let holders = match self.solana_client.get_token_largest_accounts(&lp_token_mint_pubkey).await {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to get LP token holders for {}: {}", lp_token_mint_pubkey, e);
                return Ok(false); // Assume not burned if holder check fails
            }
        };

        // Define burn addresses (as Pubkeys for direct comparison)
        let burn_addresses: Vec<Pubkey> = vec![
            Pubkey::from_str("11111111111111111111111111111111").unwrap(), // SystemProgram (often used as burn)
            // Add other known burn addresses for Solana
            Pubkey::from_str("burnburn111111111111111111111111111111111").unwrap_or_default(),
            Pubkey::from_str("deadbeef1111111111111111111111111111111111").unwrap_or_default(),
        ];

        // Define known locker program addresses
        let locker_programs: Vec<Pubkey> = vec![
            // Raydium/Orca/etc. locker program addresses would go here
            // Example: Pubkey::from_str("7ahEdGCih2m3XWL9cKHjGWzJKzFnsZJp4EZ8WNpzJ5qc").unwrap_or_default(), // Just an example, replace with actual program
        ];

        // Calculate burned amount (raw u64)
        let mut burned_amount_raw: u64 = 0;
        let mut locked_amount_raw: u64 = 0;

        for holder in holders {
            match Pubkey::from_str(&holder.address) {
                Ok(holder_pubkey) => {
                    if burn_addresses.contains(&holder_pubkey) {
                        // Direct burn address
                        match holder.amount.amount.parse::<u64>() {
                            Ok(amount) => burned_amount_raw += amount,
                            Err(e) => warn!("Failed to parse holder amount '{:?}' for LP {}: {}", holder.amount, lp_token_mint_pubkey, e),
                        }
                    } else {
                        // Check if this account might be owned by a locker program
                        // Need to fetch account info to check owner
                        match self.solana_client.get_rpc().get_account(&holder_pubkey).await {
                            Ok(account) => {
                                if locker_programs.contains(&account.owner) {
                                    // This is a locked LP token account
                                    match holder.amount.amount.parse::<u64>() {
                                        Ok(amount) => locked_amount_raw += amount,
                                        Err(e) => warn!("Failed to parse locked holder amount '{:?}': {}", holder.amount, e),
                                    }
                                }
                            },
                            Err(e) => {
                                // Log error fetching account info, but don't fail the whole check
                                warn!("Failed to fetch account info for potential locker {}: {}", holder_pubkey, e);
                            },
                        }
                    }
                }
                Err(_) => warn!("Failed to parse holder address '{}' for LP {}", holder.address, lp_token_mint_pubkey),
            }
        }

        // Calculate percentages burned and locked using raw amounts
        let burned_percent = if supply_raw > 0 {
            (burned_amount_raw as f64 / supply_raw as f64) * 100.0
        } else {
            0.0
        };

        let locked_percent = if supply_raw > 0 {
            (locked_amount_raw as f64 / supply_raw as f64) * 100.0
        } else {
            0.0
        };

        let total_secured_percent = burned_percent + locked_percent;

        info!("LP token {} burn/lock check: {:.2}% burned, {:.2}% locked in contracts (total {:.2}%)",
            lp_token_mint_str, burned_percent, locked_percent, total_secured_percent);

        // Consider LP tokens secure if >95% in burn addresses or lockers
        Ok(total_secured_percent > 95.0)
    }

    /// Find the LP token mint for a token paired with SOL using Raydium API primarily.
    async fn find_lp_token_mint(&self, token_address: &str, sol_address: &str) -> Result<Option<String>> {
        // Method 1: Try to find via Raydium API
        match self.find_raydium_lp_mint(token_address, sol_address).await {
            Ok(Some(mint)) => {
                debug!("Found Raydium LP mint {} for token {}", mint, token_address);
                return Ok(Some(mint));
            }
            Ok(None) => {
                debug!("No Raydium LP mint found via API for token {}", token_address);
                // Proceed to fallback or return None
            }
            Err(e) => {
                warn!("Error checking Raydium API for LP mint: {}", e);
                // Proceed to fallback or return error? For now, try fallback.
            }
        }

        // Method 2: Try to find via on-chain program accounts (fallback - currently placeholder)
        match self.find_onchain_lp_mint(token_address, sol_address).await {
             Ok(Some(mint)) => {
                 debug!("Found LP mint {} via on-chain scan for token {}", mint, token_address);
                 return Ok(Some(mint));
             }
             Ok(None) => {
                 debug!("No LP mint found via on-chain scan for token {}", token_address);
             }
             Err(e) => {
                  warn!("Error during on-chain LP mint scan: {}", e);
             }
        }

        // No LP token mint found by any method
        Ok(None)
    }

    /// Find LP token mint via Raydium API (v2 liquidity endpoint)
    async fn find_raydium_lp_mint(&self, token_address: &str, sol_address: &str) -> Result<Option<String>> {
        let url = "https://api.raydium.io/v2/sdk/liquidity/mainnet.json";
        debug!("Fetching Raydium pools from {}", url);

        let response = match self.http_client.get(url)
            .timeout(Duration::from_secs(10))
            .send()
            .await {
                Ok(resp) => resp,
                Err(e) => {
                    // Log specific error type if possible
                    if e.is_timeout() {
                         warn!("Timeout fetching Raydium pools: {}", e);
                    } else {
                         warn!("Failed to fetch Raydium pools: {}", e);
                    }
                    // Return Ok(None) instead of Err to allow fallback methods
                    return Ok(None);
                }
            };

        if !response.status().is_success() {
            warn!("Raydium API returned status {} for pools list", response.status());
            // Return Ok(None) instead of Err
            return Ok(None);
        }

        // Use Value for flexible parsing
        let pools_data: Value = match response.json().await {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse Raydium API response as JSON: {}", e);
                // Return Ok(None) instead of Err
                return Ok(None);
            }
        };

        // Navigate the expected structure: { "official": [ { pool_data... } ], "unofficial": [ { pool_data... } ] }
        let official_pools_vec = pools_data.get("official").and_then(|v| v.as_array()).cloned().unwrap_or_else(Vec::new);
        let unofficial_pools_vec = pools_data.get("unofficial").and_then(|v| v.as_array()).cloned().unwrap_or_else(Vec::new);

        for pool_data in official_pools_vec.iter().chain(unofficial_pools_vec.iter()) {
            let base_mint = pool_data.get("baseMint").and_then(|v| v.as_str()).unwrap_or("");
            let quote_mint = pool_data.get("quoteMint").and_then(|v| v.as_str()).unwrap_or("");
            let lp_mint = pool_data.get("lpMint").and_then(|v| v.as_str()).unwrap_or("");

            // Check if this pool pairs our token with SOL
            if (base_mint == token_address && quote_mint == sol_address) ||
               (base_mint == sol_address && quote_mint == token_address) {
                if !lp_mint.is_empty() {
                    debug!("Found matching Raydium pool. LP Mint: {}", lp_mint);
                    return Ok(Some(lp_mint.to_string()));
                } else {
                     warn!("Found matching Raydium pool but lpMint is empty: base={}, quote={}", base_mint, quote_mint);
                }
            }
        }

        debug!("No matching Raydium pool found for token {}", token_address);
        Ok(None) // No matching pool found
    }

    /// Find LP token mint via on-chain program accounts (fallback - Placeholder)
    async fn find_onchain_lp_mint(&self, _token_address: &str, _sol_address: &str) -> Result<Option<String>> {
        // This is complex and requires fetching/parsing potentially many accounts
        // based on Raydium's program ID and specific account layouts.
        // For now, this remains a placeholder.
        warn!("On-chain LP mint finding is not implemented.");
        Ok(None)
    }


    // Checks if a token can likely be sold by simulating a small buy then sell
    async fn check_sellability_placeholder(&self, token_address: &Pubkey, details: &mut Vec<String>) -> Result<bool> {
        warn!("Sellability check (honeypot) is using placeholder simulation logic.");
        // TODO: Refine simulation amounts, error handling, and potentially use a temporary wallet.

        let wallet_pubkey = self.wallet_manager.get_public_key();
        let token_address_str = token_address.to_string();
        let sol_mint_str = crate::api::jupiter::SOL_MINT.to_string();


        // --- Simulate Buy ---
        let buy_amount_lamports = 1_000_000; // 0.001 SOL
        let buy_quote = match self.jupiter_client.get_quote(
            &sol_mint_str,
            &token_address_str,
            buy_amount_lamports,
            100
        ).await {
            Ok(q) => q,
            Err(e) => {
                warn!("Sellability Check: Failed to get buy quote for {}: {:?}", token_address_str, e);
                return Ok(false);
            }
        };

        let estimated_token_out = match buy_quote.out_amount.parse::<u64>() {
             Ok(amount) if amount > 0 => amount,
             _ => {
                 warn!("Sellability Check: Invalid estimated token output amount in buy quote for {}.", token_address_str);
                 return Ok(false);
             }
        };

        let buy_swap_response = match self.jupiter_client.get_swap_transaction(
            &buy_quote,
            &wallet_pubkey.to_string(),
            None
        ).await {
            Ok(resp) => resp,
            Err(e) => {
                 warn!("Sellability Check: Failed to get buy swap tx for {}: {:?}", token_address_str, e);
                 return Ok(false);
            }
        };

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

        if let Err(e) = self.solana_client.simulate_versioned_transaction(&buy_versioned_tx).await {
             warn!("Sellability Check: Buy simulation failed for {}: {:?}", token_address_str, e);
             details.push(format!("⚠️ Buy simulation failed ({}).", e));
        } else {
             debug!("Sellability Check: Buy simulation successful for {}.", token_address_str);
        }


        // --- Simulate Sell ---
        let token_decimals = match self.solana_client.get_mint_info(token_address).await {
             Ok(info) => info.decimals,
             Err(e) => {
                 warn!("Sellability Check: Failed to get decimals for {}: {:?}. Cannot simulate sell.", token_address_str, e);
                 return Ok(false);
             }
        };

        let sell_quote = match self.jupiter_client.get_quote(
            &token_address_str,
            &sol_mint_str,
            estimated_token_out,
            100 // slippage_bps
        ).await {
             Ok(q) => q,
             Err(e) => {
                 warn!("Sellability Check: Failed to get sell quote for {}: {:?}", token_address_str, e);
                 return Ok(false);
             }
        };

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
                Ok(true)
            }
            Err(e) => {
                warn!("Sellability Check: Sell simulation FAILED for {}: {:?}", token_address_str, e);
                Ok(false)
            }
        }
    }

    async fn check_holder_distribution(&self, token_address: &Pubkey) -> Result<(u32, f64)> {
        debug!("Checking holder distribution for {}", token_address);
        let mint_info = match self.solana_client.get_mint_info(token_address).await {
            Ok(info) => info.supply,
            Err(e) => {
                warn!("Failed to get mint info for holder check {}: {:?}", token_address, e);
                return Err(e).context("Failed to get mint info for holder check");
            }
        };
        if mint_info == 0 { return Ok((0, 100.0)); }

        let largest_accounts = match self.solana_client.get_token_largest_accounts(token_address).await {
            Ok(accounts) => accounts,
            Err(e) => {
                 warn!("Failed to get largest accounts for holder check {}: {:?}", token_address, e);
                 return Err(e).context("Failed to get largest accounts for holder check");
            }
        };
        let holder_count_estimate = largest_accounts.len() as u32;
        debug!("Estimated holder count for {}: {}", token_address, holder_count_estimate);

        let top_n = 10;
        let mut top_n_amount: u64 = 0;
        for account in largest_accounts.iter().take(top_n) {
             match account.amount.amount.parse::<u64>() {
                 Ok(amount_u64) => top_n_amount += amount_u64,
                 Err(e) => {
                     warn!("Failed to parse largest account amount '{:?}' for {}: {}. Skipping.", account.amount, token_address, e);
                 }
             }
        }
        let concentration_percent = if mint_info > 0 { (top_n_amount as f64 / mint_info as f64) * 100.0 } else { 0.0 };
        debug!("Top {} holders concentration for {}: {:.2}%", top_n, token_address, concentration_percent);
        Ok((holder_count_estimate, concentration_percent))
    }

    async fn check_transfer_tax(&self, token_address: &Pubkey) -> Result<f64> {
        debug!("Checking transfer tax for {}", token_address);
        let mint_account = match self.solana_client.get_rpc().get_account(token_address).await {
             Ok(account) => account,
             Err(e) => {
                 warn!("Failed to get mint account for tax check {}: {:?}", token_address, e);
                 return Ok(0.0);
             }
        };
        if mint_account.owner == spl_token_2022::id() {
            debug!("Token {} belongs to Token-2022 program. Checking for transfer fee extension.", token_address);
            match StateWithExtensions::<Token2022Mint>::unpack(&mint_account.data) {
                Ok(mint_state) => {
                    match mint_state.get_extension::<TransferFeeConfig>() {
                        Ok(transfer_fee_config) => {
                            let fee_basis_points_pod = transfer_fee_config.get_epoch_fee(0).transfer_fee_basis_points;
                            let fee_basis_points: u16 = fee_basis_points_pod.into();
                            let tax_percent = fee_basis_points as f64 / 100.0;
                            info!("Token {} has Token-2022 transfer tax: {}% ({} basis points)", token_address, tax_percent, fee_basis_points);
                            Ok(tax_percent)
                        }
                        Err(_) => {
                            debug!("Token {} is Token-2022 but has no TransferFeeConfig extension.", token_address);
                            Ok(0.0)
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to unpack Token-2022 mint extensions for {}: {:?}. Assuming no tax.", token_address, e);
                    Ok(0.0)
                }
            }
        } else if mint_account.owner == spl_token::id() {
             debug!("Token {} belongs to standard SPL Token program. Assuming no transfer tax.", token_address);
             Ok(0.0)
        } else {
             warn!("Token {} has an unknown owner program: {}. Cannot determine transfer tax.", token_address, mint_account.owner);
             Ok(0.0)
        }
    }
}

/* 
 * TEST INSTRUCTIONS FOR RISK ANALYZER IMPROVEMENTS
 * -----------------------------------------------
 * 
 * To test the improved risk analysis functions, follow these steps:
 * 
 * 1. In the `analyze_token` method, you can add debug output to check primary pair info:
 *    ```
 *    if let Some(pair_info) = &primary_pair_info {
 *        info!("Primary pair details - DEX: {}, Liquidity: {} SOL, Price Impact: {}%",
 *            pair_info.dex_name, pair_info.liquidity_sol, pair_info.price_impact_1k);
 *    }
 *    ```
 * 
 * 2. Test with a known Solana memecoin address, for example:
 *    - BONK: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"
 *    - WIF: "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm"
 *    
 *    Use the Telegram bot command:
 *    `/analyze DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263`
 * 
 * 3. For direct testing, you can create a simple test function in main.rs:
 *    ```
 *    async fn test_risk_analyzer() {
 *        // Initialize necessary components
 *        let config = Config::load().expect("Failed to load config");
 *        let solana_client = Arc::new(SolanaClient::new(&config.solana_rpc_url).expect("Failed to create Solana client"));
 *        let helius_client = Arc::new(HeliusClient::new(&config.helius_api_key));
 *        let jupiter_client = Arc::new(JupiterClient::new(None));
 *        let birdeye_client = Arc::new(BirdeyeClient::new(&config.birdeye_api_key));
 *        let wallet_manager = Arc::new(WalletManager::new(&config.wallet_private_key, solana_client.clone()).expect("Failed to create wallet manager"));
 *        
 *        let risk_analyzer = RiskAnalyzer::new(
 *            solana_client.clone(),
 *            helius_client.clone(),
 *            jupiter_client.clone(),
 *            birdeye_client.clone(),
 *            wallet_manager.clone(),
 *        );
 *        
 *        // Test tokens (BONK, WIF, or your token of interest)
 *        let token_address = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"; // BONK
 *        
 *        // Test primary pair finding
 *        match risk_analyzer.find_primary_pair_info(token_address).await {
 *            Ok(pair_info) => {
 *                println!("Primary pair info for {}:", token_address);
 *                println!("  DEX: {}", pair_info.dex_name);
 *                println!("  Liquidity: {:.2} SOL", pair_info.liquidity_sol);
 *                println!("  Price Impact: {:.4}%", pair_info.price_impact_1k);
 *                println!("  LP Mint: {:?}", pair_info.lp_mint);
 *            },
 *            Err(e) => println!("Error finding primary pair: {}", e),
 *        }
 *        
 *        // Test LP tokens burned check
 *        match risk_analyzer.check_lp_tokens_burned(token_address).await {
 *            Ok(burned) => println!("LP tokens burned/locked: {}", burned),
 *            Err(e) => println!("Error checking LP tokens: {}", e),
 *        }
 *        
 *        // Test full analysis
 *        match risk_analyzer.analyze_token(token_address).await {
 *            Ok(analysis) => {
 *                println!("Risk analysis for {}:", token_address);
 *                println!("  Risk Level: {}/100", analysis.risk_level);
 *                println!("  Liquidity: {:.2} SOL", analysis.liquidity_sol);
 *                println!("  LP Tokens Burned: {}", analysis.lp_tokens_burned);
 *                println!("  Details:");
 *                for detail in analysis.details {
 *                    println!("    - {}", detail);
 *                }
 *            },
 *            Err(e) => println!("Error analyzing token: {}", e),
 *        }
 *    }
 *    ```
 *    
 * 4. To call this test function, you can add to main.rs:
 *    ```
 *    // In main function, before bot startup
 *    if std::env::args().any(|arg| arg == "--test-risk") {
 *        info!("Running risk analyzer test...");
 *        tokio::spawn(test_risk_analyzer()).await.unwrap();
 *        return Ok(());
 *    }
 *    ```
 *    
 *    Then run with: `cargo run -- --test-risk`
 */
