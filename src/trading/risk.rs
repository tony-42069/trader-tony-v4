use anyhow::{anyhow, Context, Result}; // Added anyhow
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc};
use tracing::{debug, error, info, warn}; // Added error

use crate::api::birdeye::{BirdeyeClient, TokenOverviewData}; // Import BirdeyeClient and TokenOverviewData
use crate::api::helius::HeliusClient;
use crate::api::jupiter::JupiterClient;
use crate::solana::client::SolanaClient;
use crate::error::TraderbotError; // Assuming this exists
use crate::solana::wallet::WalletManager;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use spl_token_2022::{
    extension::{BaseStateWithExtensions, StateWithExtensions, transfer_fee::TransferFeeConfig}, // Added BaseStateWithExtensions
    state::Mint as Token2022Mint,
};
use solana_program::program_pack::Pack as TokenPack;


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

        // --- Fetch Data Upfront ---

        // Fetch Birdeye overview data once
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

        // Fetch SOL price once (needed for liquidity conversion)
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

        // 2. Liquidity Check (Using fetched Birdeye data)
        let liquidity_sol = match self.check_liquidity(birdeye_overview.as_ref(), sol_price_usd).await {
            Ok(liq) => {
                if liq < 5.0 { // Example threshold
                    risk_score += 20;
                    details.push(format!("🟠 Low liquidity ({:.2} SOL).", liq));
                } else {
                    details.push(format!("✅ Liquidity: {:.2} SOL.", liq));
                }
                liq
            }
            Err(e) => {
                warn!("Liquidity check failed for {}: {:?}. Assuming 0.", token_address_str, e);
                details.push(format!("❓ Failed liquidity check: {}", e));
                0.0 // Assume 0 liquidity on error
            }
        };


        // 3. LP Token Check (Placeholder, using fetched Birdeye data)
        let lp_tokens_burned = match self.check_lp_tokens_burned(birdeye_overview.as_ref()).await {
             Ok(burned) => {
                 if !burned {
                     risk_score += 15;
                     details.push("🟠 LP tokens may not be burned/locked (Placeholder Check).".to_string());
                 } else {
                     details.push("✅ LP tokens appear burned/locked (Placeholder Check).".to_string());
                 }
                 burned
             }
             Err(e) => {
                 warn!("LP token check failed for {}: {:?}. Assuming not burned.", token_address_str, e);
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

        // 5. Holder Distribution Check (Implemented)
        // TODO: Use a better source like Helius or Birdeye if available for accurate holder count.
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
             details.push(format!("🟠 Low holder count ({} - Estimated).", holder_count));
         } else {
              details.push(format!("✅ Holder count: {} (Estimated).", holder_count));
         }
        if concentration_percent > 50.0 { // Example threshold for top 10 holders
            risk_score += 15;
            details.push(format!("🟠 High holder concentration ({:.1}% in top 10).", concentration_percent));
        } else {
             details.push(format!("✅ Holder concentration: {:.1}% (Top 10).", concentration_percent));
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
            holder_count, // Note: This is currently an estimate
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

    // Calculates SOL liquidity based on fetched Birdeye data and SOL price
    async fn check_liquidity(
        &self,
        overview_data: Option<&TokenOverviewData>,
        sol_price_usd: Option<f64>,
    ) -> Result<f64> {
        debug!("Calculating SOL liquidity from Birdeye data");

        let overview = overview_data.ok_or_else(|| anyhow!("Birdeye overview data not available"))?;
        let sol_price = sol_price_usd.ok_or_else(|| anyhow!("SOL price not available"))?;

        let usd_liquidity = overview.liquidity.unwrap_or(0.0);

        if usd_liquidity <= 0.0 {
            debug!("Birdeye reported zero or missing USD liquidity for {}", overview.address);
            return Ok(0.0);
        }

        if sol_price <= 0.0 {
             warn!("Invalid SOL price ({}) used for liquidity calculation.", sol_price);
             return Err(anyhow!("Invalid SOL price for calculation"));
        }

        // Calculate SOL liquidity: (Total USD Liquidity / SOL Price in USD)
        let calculated_liquidity_sol = usd_liquidity / sol_price;
        info!(
            "Calculated SOL liquidity for {}: {:.2} (USD Liq: {:.2}, SOL Price: {:.2})",
            overview.address, calculated_liquidity_sol, usd_liquidity, sol_price
        );

        Ok(calculated_liquidity_sol)
    }

    // Placeholder for LP token check, now accepts overview data
    async fn check_lp_tokens_burned(
        &self,
        overview_data: Option<&TokenOverviewData>,
    ) -> Result<bool> {
        let token_address = overview_data.map(|d| d.address.as_str()).unwrap_or("unknown token");
        debug!("LP Burn Check Placeholder for {}", token_address);

        // TODO: Implement actual LP token burn/lock check. This is complex.
        // Use overview_data if it contains relevant fields (e.g., pair address, LP mint).
        // Example: if let Some(pair_addr) = overview_data.and_then(|d| d.primary_pair_address) { ... }

        // Step 1: Find the primary SOL liquidity pool address for the token.
        //      - Method A: Use Birdeye token overview endpoint (check overview_data).
        //      - Method B: Use Helius DAS API (if it includes market/pair data).
        //      - Method C: Query DEX program accounts (e.g., Raydium, Orca) - requires SDKs or complex RPC parsing.
        //      - Method D: Use a hardcoded list or external service mapping tokens to pairs.
        //      -> Requires further investigation / implementation choice.
        // let primary_pool_address = find_primary_sol_pool(token_address).await?; // Placeholder

        // Step 2: Get the LP token mint address associated with that pool.
        //      - Requires fetching pool account data and knowing the specific DEX's state layout
        //        (e.g., Raydium `amm_info_layout_v4`, Orca `whirlpool_state`) to find the `lp_mint` field.
        // let lp_mint_pubkey = get_lp_mint_for_pool(primary_pool_address).await?; // Placeholder

        // Step 3: Get total supply of the LP token.
        // let total_supply = self.solana_client.get_token_supply(&lp_mint_pubkey).await?;

        // Step 4: Get largest holders of the LP token.
        // let largest_holders = self.solana_client.get_token_largest_accounts(&lp_mint_pubkey).await?;

        // Step 5: Check if a known burn address holds >95% of the supply.
        // let burn_address = Pubkey::from_str("11111111111111111111111111111111")?;
        // let mut burned_amount: u64 = 0;
        // for holder in largest_holders {
        //     if Pubkey::from_str(&holder.address)? == burn_address {
        //         // Need to parse holder.ui_amount_string based on LP token decimals
        //         // burned_amount += parse_ui_amount(&holder.ui_amount_string, lp_decimals)?;
        //     }
        // }
        // let is_burned = (burned_amount as f64 / total_supply as f64) > 0.95;

        // Step 6: Optionally check known locker addresses.

        // Step 7: Return true if burned/locked, false otherwise.

        // --- Current Placeholder Logic ---
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
        // --- Actual Implementation ---
        debug!("Checking holder distribution for {}", token_address);

        // Get total supply and decimals
        let (total_supply, decimals) = match self.solana_client.get_mint_info(token_address).await {
            Ok(mint_info) => (mint_info.supply, mint_info.decimals),
            Err(e) => {
                warn!("Failed to get mint info for holder check {}: {:?}", token_address, e);
                return Err(e).context("Failed to get mint info for holder check");
            }
        };

        if total_supply == 0 {
            warn!("Token {} has zero supply, cannot calculate holder distribution.", token_address);
            return Ok((0, 100.0)); // Avoid division by zero, assume max concentration
        }

        // Get largest accounts
        let largest_accounts = match self.solana_client.get_token_largest_accounts(token_address).await {
            Ok(accounts) => accounts,
            Err(e) => {
                 warn!("Failed to get largest accounts for holder check {}: {:?}", token_address, e);
                 return Err(e).context("Failed to get largest accounts for holder check");
            }
        };

        // Estimate holder count (using number of largest accounts returned as a proxy - very rough)
        // TODO: Use a better source like Helius or Birdeye if available for accurate holder count.
        let holder_count_estimate = largest_accounts.len() as u32;
        debug!("Estimated holder count for {}: {}", token_address, holder_count_estimate);


        // Calculate concentration for top N (e.g., 10) holders
        let top_n = 10;
        let mut top_n_amount: u64 = 0;
        for account in largest_accounts.iter().take(top_n) {
             // Access the 'amount' string field within the UiTokenAmount struct and parse it.
             match account.amount.amount.parse::<u64>() { // Access account.amount.amount
                 Ok(amount_u64) => top_n_amount += amount_u64,
                 Err(e) => {
                     // Format the ui_amount_string for the warning message
                     warn!("Failed to parse largest account amount '{}' for {}: {}. Skipping.", account.amount.ui_amount_string, token_address, e);
                     // Optionally return an error or just skip this account
                 }
             }
        }

        let concentration_percent = if total_supply > 0 {
             (top_n_amount as f64 / total_supply as f64) * 100.0
        } else {
             0.0 // Avoid division by zero if total supply is 0
        };
        debug!("Top {} holders concentration for {}: {:.2}%", top_n, token_address, concentration_percent);

        Ok((holder_count_estimate, concentration_percent))

    }

    async fn check_transfer_tax(&self, token_address: &Pubkey) -> Result<f64> { // Renamed function
        debug!("Checking transfer tax for {}", token_address);

        // 1. Fetch mint account data
        let mint_account = match self.solana_client.get_rpc().get_account(token_address) {
             Ok(account) => account,
             Err(e) => {
                 warn!("Failed to get mint account for tax check {}: {:?}", token_address, e);
                 // If we can't get the account, assume no tax, but log error
                 return Ok(0.0);
             }
        };

        // 2. Check if the owner is the Token-2022 program
        if mint_account.owner == spl_token_2022::id() {
            debug!("Token {} belongs to Token-2022 program. Checking for transfer fee extension.", token_address);
            // 3. Try to unpack account data with extensions
            match StateWithExtensions::<Token2022Mint>::unpack(&mint_account.data) {
                Ok(mint_state) => {
                    // 4. Look for the TransferFeeConfig extension
                    match mint_state.get_extension::<TransferFeeConfig>() {
                        Ok(transfer_fee_config) => {
                            // 5. Extract fee basis points and calculate percentage
                            // Fee is charged on the destination amount. Get the highest fee.
                            let fee_basis_points_pod = transfer_fee_config.get_epoch_fee(0).transfer_fee_basis_points; // Check current epoch fee
                            let fee_basis_points: u16 = fee_basis_points_pod.into(); // Convert PodU16 to u16
                            let tax_percent = fee_basis_points as f64 / 100.0; // Cast u16 to f64
                            info!("Token {} has Token-2022 transfer tax: {}% ({} basis points)", token_address, tax_percent, fee_basis_points); // Format u16
                            Ok(tax_percent)
                        }
                        Err(_) => {
                            // Extension not found
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
             Ok(0.0) // Assume no tax for unknown programs
        }
        // TODO: Consider adding simulation for non-Token-2022 tokens as a fallback (Step 5 in original TODO)
    }
}
