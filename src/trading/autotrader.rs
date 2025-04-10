use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, interval};
use chrono::Utc;
use tracing::{debug, error, info, warn}; // Added debug

use crate::api::birdeye::BirdeyeClient; // Add BirdeyeClient import
use crate::api::helius::HeliusClient;
use crate::api::jupiter::{JupiterClient, SwapResult};
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::config::Config;
use crate::trading::position::{PositionManager, PositionStatus}; // Assuming these exist
use crate::trading::risk::{RiskAnalysis, RiskAnalyzer}; // Assuming these exist
use crate::trading::strategy::Strategy;
use crate::models::token::TokenMetadata;
use solana_sdk::signature::Signature;
use solana_sdk::pubkey::Pubkey;


// --- Standalone Task Functions ---

/// The main cycle executed by the background task.
async fn run_scan_cycle(
    strategies_arc: Arc<RwLock<HashMap<String, Strategy>>>,
    helius_client: Arc<HeliusClient>,
    risk_analyzer: Arc<RiskAnalyzer>,
    position_manager: Arc<PositionManager>,
    config: Arc<Config>,
    wallet_manager: Arc<WalletManager>,
    jupiter_client: Arc<JupiterClient>,
    // solana_client is implicitly used by risk_analyzer/position_manager/wallet_manager
) -> Result<()> {
    debug!("Scanning for trading opportunities...");

    let strategies_guard = strategies_arc.read().await;
    let enabled_strategies: Vec<_> = strategies_guard
        .values()
        .filter(|s| s.enabled)
        .cloned()
        .collect();
    drop(strategies_guard); // Release read lock

    if enabled_strategies.is_empty() {
        debug!("No enabled strategies found. Skipping scan.");
        return Ok(());
    }

    if config.demo_mode {
        run_simulated_scan_cycle(&enabled_strategies, &position_manager, &config).await?;
        return Ok(());
    }

    // --- Real Mode Scan ---
    info!("Scanning for new tokens using Helius...");
    match helius_client.get_recent_tokens(60).await { // TODO: Make age configurable
        Ok(tokens) => {
            if tokens.is_empty() {
                debug!("No new tokens found in this scan cycle.");
                return Ok(());
            }
            info!("Found {} potential new tokens via Helius.", tokens.len());

            for token in tokens {
                debug!("Processing potential token: {} ({})", token.name, token.address);
                match risk_analyzer.analyze_token(&token.address).await {
                    Ok(risk_analysis) => {
                        info!(
                            "Analyzed token {}: Risk Level {}, Liquidity {:.2} SOL, Holders {}",
                            token.symbol, risk_analysis.risk_level, risk_analysis.liquidity_sol, risk_analysis.holder_count
                        );

                        for strategy in &enabled_strategies {
                            if meets_strategy_criteria(&token, &risk_analysis, strategy) {
                                info!("Token {} meets criteria for strategy '{}'", token.symbol, strategy.name);
                                if should_execute_buy_task(&token, strategy, &position_manager).await? { // Added ? for error handling
                                    match execute_buy_task(
                                        &token,
                                        strategy,
                                        &position_manager,
                                        &jupiter_client,
                                        &wallet_manager, // Pass WalletManager which holds SolanaClient
                                        &config,
                                        None, // Pass None for notification_manager
                                    ).await {
                                        Ok(_) => info!("Successfully executed buy and confirmed for {} via strategy '{}'", token.symbol, strategy.name),
                                        Err(e) => error!("Failed to execute buy for {}: {:?}", token.symbol, e), // Error includes confirmation failure now
                                    }
                                } else {
                                     debug!("Buy condition not met for token {} and strategy '{}'", token.symbol, strategy.name);
                                }
                            } else {
                                 debug!("Token {} does not meet criteria for strategy '{}'", token.symbol, strategy.name);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to analyze token {}: {:?}", token.address, e);
                    }
                }
            }
        }
        Err(e) => {
            error!("Error fetching recent tokens from Helius: {:?}", e);
            // Don't return error, just log and continue scan next time
        }
    }
    Ok(())
}

/// Simulates the scanning process in demo mode.
async fn run_simulated_scan_cycle(
    enabled_strategies: &[Strategy],
    position_manager: &PositionManager, // Pass Arc<PositionManager>
    _config: &Config, // Pass Arc<Config> - Prefixed as unused for now
) -> Result<()> {
    info!("[DEMO MODE] Simulating scan for opportunities...");
    // Simulate finding a token occasionally
    if rand::random::<f64>() < 0.1 { // 10% chance per scan cycle
        let demo_token_addr = format!("DemoMint{}", rand::random::<u32>());
        let demo_token = TokenMetadata {
            address: demo_token_addr.clone(),
            name: format!("Demo Token {}", rand::random::<u16>()),
            symbol: format!("DEMO{}", rand::random::<u16>()),
            decimals: 9,
            supply: Some(1_000_000_000 * 10u64.pow(9)), // Example supply
            logo_uri: None,
            creation_time: Some(Utc::now()),
        };
        info!("[DEMO MODE] Simulated finding token: {} ({})", demo_token.name, demo_token.symbol);

        // Simulate analysis
        let risk_analysis = RiskAnalysis {
             token_address: demo_token_addr,
             risk_level: rand::random::<u32>() % 101, // 0-100
             liquidity_sol: (rand::random::<f64>() * 50.0) + 5.0, // 5-55 SOL
             holder_count: (rand::random::<u32>() % 500) + 10, // 10-509 holders
             has_mint_authority: rand::random::<bool>(),
             has_freeze_authority: rand::random::<bool>(),
             lp_tokens_burned: rand::random::<bool>(),
             transfer_tax_percent: if rand::random::<f64>() < 0.1 { rand::random::<f64>() * 10.0 } else { 0.0 },
             can_sell: rand::random::<f64>() > 0.1, // 90% chance can sell
             concentration_percent: rand::random::<f64>() * 50.0, // 0-50%
             details: vec!["Simulated analysis".to_string()],
        };
         info!("[DEMO MODE] Simulated analysis for {}: Risk {}, Liquidity {:.2}", demo_token.symbol, risk_analysis.risk_level, risk_analysis.liquidity_sol);


        for strategy in enabled_strategies {
            if meets_strategy_criteria(&demo_token, &risk_analysis, strategy) {
                info!("[DEMO MODE] Token {} meets criteria for strategy '{}'", demo_token.symbol, strategy.name);
                 if should_execute_buy_task(&demo_token, strategy, position_manager).await? {
                     info!("[DEMO MODE] Executing simulated buy for {} via strategy '{}'", demo_token.symbol, strategy.name);
                     // In demo, just log, maybe create a demo position entry
                     if let Err(e) = position_manager.create_demo_position(
                         &demo_token.address,
                         &demo_token.name,
                         &demo_token.symbol,
                         &strategy.id,
                         strategy.max_position_size_sol, // Use strategy defined size
                     ).await {
                         error!("[DEMO MODE] Error creating demo position: {}", e);
                     }
                 }
            }
        }
    } else {
         debug!("[DEMO MODE] No simulated token found this cycle.");
    }
    Ok(())
}

/// Checks if a token meets the criteria defined by a strategy based on risk analysis.
fn meets_strategy_criteria(
    token: &TokenMetadata,
    risk_analysis: &RiskAnalysis,
    strategy: &Strategy,
) -> bool {
    if risk_analysis.risk_level > strategy.max_risk_level {
        debug!("Token {} rejected by strategy '{}': Risk level {} > {}", token.symbol, strategy.name, risk_analysis.risk_level, strategy.max_risk_level);
        return false;
    }
    if risk_analysis.liquidity_sol < strategy.min_liquidity_sol as f64 {
         debug!("Token {} rejected by strategy '{}': Liquidity {:.2} < {}", token.symbol, strategy.name, risk_analysis.liquidity_sol, strategy.min_liquidity_sol);
        return false;
    }
    if let Some(creation_time) = token.creation_time {
        let age_minutes = Utc::now().signed_duration_since(creation_time).num_minutes();
        if age_minutes > 0 && age_minutes as u32 > strategy.max_token_age_minutes { // Check age > 0 to avoid issues with clock sync
             debug!("Token {} rejected by strategy '{}': Age {} mins > {}", token.symbol, strategy.name, age_minutes, strategy.max_token_age_minutes);
            return false;
        }
    } else {
         // If creation time is unknown, maybe reject or allow based on strategy config?
         // For now, allow if creation time is None.
         debug!("Token {} accepted by strategy '{}': Creation time unknown.", token.symbol, strategy.name);
    }
    if risk_analysis.holder_count < strategy.min_holders {
         debug!("Token {} rejected by strategy '{}': Holders {} < {}", token.symbol, strategy.name, risk_analysis.holder_count, strategy.min_holders);
        return false;
    }
    // Add more checks based on RiskAnalysis fields (mint/freeze authority, tax, etc.) if needed
    if !risk_analysis.can_sell && strategy.require_can_sell {
         debug!("Token {} rejected by strategy '{}': Cannot sell and strategy requires it", token.symbol, strategy.name);
        return false;
    }
    if risk_analysis.has_freeze_authority && strategy.reject_if_freeze_authority {
         debug!("Token {} rejected by strategy '{}': Has freeze authority and strategy rejects it", token.symbol, strategy.name);
        return false;
    }
    // ... other checks

    true
}

/// Checks if a buy should be executed based on strategy limits and existing positions.
async fn should_execute_buy_task(
    token: &TokenMetadata,
    strategy: &Strategy,
    position_manager: &PositionManager, // Pass Arc<PositionManager>
) -> Result<bool> { // Return Result
    // Check if already holding this token (across all strategies or just this one?)
    // Let's check across all active positions for simplicity first.
    if position_manager.has_active_position(&token.address).await {
        debug!("Skipping buy for {}: Already have an active position.", token.symbol);
        return Ok(false);
    }

    // Check strategy-specific limits (concurrent positions, budget)
    let strategy_positions = position_manager.get_active_positions_by_strategy(&strategy.id).await;

    if strategy_positions.len() >= strategy.max_concurrent_positions as usize {
        info!("Skipping buy for {}: Max concurrent positions ({}) reached for strategy '{}'.",
             token.symbol, strategy.max_concurrent_positions, strategy.name);
        return Ok(false);
    }

    let used_budget: f64 = strategy_positions.iter().map(|p| p.entry_value_sol).sum(); // Use entry value
    let position_size = strategy.max_position_size_sol; // Determine intended size first
    let remaining_budget = strategy.total_budget_sol - used_budget;

    if position_size > remaining_budget {
        warn!("Skipping buy for {}: Required size {:.4} SOL exceeds remaining budget {:.4} SOL for strategy '{}'.",
             token.symbol, position_size, remaining_budget, strategy.name);
        return Ok(false);
    }

    // Check overall wallet balance? Maybe not here, rely on swap failing if insufficient.

    Ok(true)
}

/// Executes the buy swap via Jupiter, confirms the transaction, and creates a position entry.
async fn execute_buy_task(
    token: &TokenMetadata,
    strategy: &Strategy,
    position_manager: &PositionManager, // Pass Arc<PositionManager>
    jupiter_client: &JupiterClient, // Pass Arc<JupiterClient>
    wallet_manager: &WalletManager, // Pass Arc<WalletManager> (holds SolanaClient)
    config: &Config, // Pass Arc<Config>
    notification_manager: Option<&Arc<crate::bot::notification::NotificationManager>>, // Add optional notification_manager
) -> Result<SwapResult> { // Return SwapResult
    info!(
        "Executing buy for token {} ({}) using strategy '{}'",
        token.symbol, token.address, strategy.name
    );

    // Determine position size based on strategy (consider risk adjustment?)
    let position_size_sol = strategy.max_position_size_sol; // Simple for now
    // TODO: Add risk-adjusted position sizing?
    // position_size_sol = position_size_sol * risk_adjustment_factor;

    // Ensure position size is not zero or negative
    if position_size_sol <= 0.0 {
        return Err(anyhow!("Calculated position size is zero or negative for token {}", token.symbol));
    }

    // Fetch token decimals if not already known (needed for Jupiter swap)
    // Assuming TokenMetadata now includes decimals correctly populated by Helius/RiskAnalyzer
    let token_decimals = token.decimals;

    // --- Execute Swap ---
    let swap_result = jupiter_client.swap_sol_to_token(
        &token.address,
        token_decimals,
        position_size_sol,
        strategy.slippage_bps.unwrap_or(config.default_slippage_bps), // Use strategy slippage or default
        strategy.priority_fee_micro_lamports.or(Some(config.default_priority_fee_micro_lamports)), // Use strategy priority fee or default
        wallet_manager.clone().into(), // Convert &WalletManager to Arc<WalletManager>
    ).await.context(format!("Failed to execute SOL to {} swap", token.symbol))?;

    info!(
        "Buy swap sent for {}. Signature: {}, Estimated Out: {:.6}",
        token.symbol, swap_result.transaction_signature, swap_result.out_amount_ui
    );

    // --- Confirm Transaction ---
    info!("Confirming buy transaction: {}", swap_result.transaction_signature);
    let signature = Signature::from_str(&swap_result.transaction_signature)
        .context("Failed to parse buy transaction signature")?;

    // Use the SolanaClient from WalletManager to confirm
    // TODO: Make confirmation timeout configurable
    match wallet_manager.solana_client().confirm_transaction(&signature, solana_sdk::commitment_config::CommitmentLevel::Confirmed, 60).await { // Use getter method
        Ok(_) => {
            info!("Buy transaction {} confirmed successfully.", signature);

            // --- Create Position Entry (Only after confirmation) ---
            // TODO: Get actual out amount after confirmation if possible (requires parsing tx details)
            let actual_out_amount = swap_result.actual_out_amount_ui.unwrap_or(swap_result.out_amount_ui); // Use estimate for now
            
            // Check fill rate - if it's too low, warn the user
            let fill_rate = if swap_result.out_amount_ui > 0.0 {
                (actual_out_amount / swap_result.out_amount_ui) * 100.0
            } else {
                100.0 // Default to 100% if expected is 0
            };
            
            // Log warning if fill rate is low
            if fill_rate < 95.0 {
                warn!(
                    "Low fill rate detected: Received {:.4} tokens ({:.1}% of expected {:.4})",
                    actual_out_amount, fill_rate, swap_result.out_amount_ui
                );
                
                // Send notification for significantly low fill rate
                if fill_rate < 50.0 && notification_manager.is_some() {
                    if let Some(notification_manager) = notification_manager {
                        notification_manager.send_error_alert(
                            &format!("⚠️ Very low fill rate in trade: only {:.1}% filled", fill_rate),
                            &format!("Trade for {} only received {:.4} tokens ({:.1}% of expected {:.4})", 
                                token.symbol, actual_out_amount, fill_rate, swap_result.out_amount_ui
                            )
                        ).await;
                    }
                }
            }

            position_manager.create_position(
                &token.address,
                &token.name,
                &token.symbol,
                token.decimals,
                &strategy.id,
                position_size_sol, // Entry value in SOL
                actual_out_amount, // Amount of token received
                Some(swap_result.out_amount_ui), // Expected amount as a separate parameter
                swap_result.price_impact_pct,
                &swap_result.transaction_signature,
                // Pass SL/TP/Trailing settings from strategy
                strategy.stop_loss_percent,
                strategy.take_profit_percent,
                strategy.trailing_stop_percent,
                Some(strategy.max_hold_time_minutes), // Wrap in Some()
            ).await.context("Failed to create position entry after successful swap confirmation")?;

            info!(
                "Position created for {} ({}) with {:.4} SOL entry value.",
                token.name, token.symbol, position_size_sol
            );

            // TODO: Send notification (Telegram?)

            Ok(swap_result) // Return original swap result on success
        }
        Err(e) => {
            error!("Failed to confirm buy transaction {}: {:?}", signature, e);
            // Don't create a position if confirmation fails
            Err(e).context(format!("Buy transaction {} failed confirmation", signature))
        }
    }
}


// Removed Clone derive, manual implementation was problematic
// Removed Debug derive as SolanaClient doesn't implement it
pub struct AutoTrader {
    wallet_manager: Arc<WalletManager>,
    solana_client: Arc<SolanaClient>,
    helius_client: Arc<HeliusClient>, // Use Arc for clients too if shared
    jupiter_client: Arc<JupiterClient>, // Use Arc
    birdeye_client: Arc<BirdeyeClient>,
    config: Arc<Config>,
    pub position_manager: Arc<PositionManager>, // Expose for references
    pub risk_analyzer: Arc<RiskAnalyzer>, // Expose for /analyze commands
    is_running: Arc<AtomicBool>,
    notification_manager: Option<Arc<crate::bot::notification::NotificationManager>>,
    strategies: Arc<RwLock<HashMap<String, Strategy>>>, // Use Arc<RwLock<..>> for shared mutable state
    running: Arc<RwLock<bool>>, // Use Arc<RwLock<..>>
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    strategies_path: PathBuf,
}

impl AutoTrader {
    // Update constructor to return Result<Self> and accept Config by value or Arc
    pub fn new(
        wallet_manager: Arc<WalletManager>,
        solana_client: Arc<SolanaClient>,
        config: Arc<Config>, // Keep Arc<Config>
    ) -> Result<Self> { // Return Result<Self>
        // Initialize clients and analyzers potentially shared via Arc
        let helius_client = Arc::new(HeliusClient::new(&config.helius_api_key));
        let jupiter_client = Arc::new(JupiterClient::new(config.jupiter_api_key.clone())); // Clone Option<String>

        // Initialize BirdeyeClient - require the API key for now
        let birdeye_api_key = config.birdeye_api_key.as_ref()
            .context("BIRDEYE_API_KEY is required but missing in config")?;
        let birdeye_client = Arc::new(BirdeyeClient::new(birdeye_api_key));

        let risk_analyzer = Arc::new(RiskAnalyzer::new(
            solana_client.clone(),
            helius_client.clone(),
            jupiter_client.clone(),
            birdeye_client.clone(), // Pass BirdeyeClient
            wallet_manager.clone(), // Pass WalletManager to RiskAnalyzer::new
        ));
        let position_manager = Arc::new(PositionManager::new(
            wallet_manager.clone(),
            jupiter_client.clone(),
            solana_client.clone(),
            config.clone(),
        )); // Corrected syntax: Ensure this parenthesis closes Arc::new
        
        // Set the default path for strategy persistence
        let strategies_path = PathBuf::from("data/strategies.json");
        
        // Create AutoTrader instance
        let autotrader = Self {
            wallet_manager,
            solana_client,
            helius_client,
            jupiter_client,
            birdeye_client,
            config,
            position_manager,
            risk_analyzer,
            is_running: Arc::new(AtomicBool::new(false)),
            notification_manager: None,
            strategies: Arc::new(RwLock::new(HashMap::new())), // Start with empty map, will load in init
            running: Arc::new(RwLock::new(false)),
            task_handle: Arc::new(Mutex::new(None)),
            strategies_path,
        };
        
        // Initialize by loading strategies
        match tokio::runtime::Handle::current().block_on(autotrader.load_strategies()) {
            Ok(_) => {
                info!("AutoTrader initialized successfully with strategies loaded");
                Ok(autotrader)
            },
            Err(e) => {
                error!("Failed to load strategies during AutoTrader initialization: {}", e);
                Err(e)
            }
        }
    }

    // --- Strategy Management ---
    
    /// Loads strategies from disk
    async fn load_strategies(&self) -> Result<()> {
        info!("Loading strategies from {:?}", self.strategies_path);
        
        let loaded_strategies = if self.strategies_path.exists() {
            match tokio::fs::read_to_string(&self.strategies_path).await {
                Ok(data) => {
                    if data.is_empty() {
                        HashMap::new()
                    } else {
                        match serde_json::from_str::<HashMap<String, Strategy>>(&data) {
                            Ok(strategies) => strategies,
                            Err(e) => {
                                error!("Failed to parse strategies file: {}", e);
                                HashMap::new()
                            }
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to read strategies file: {}", e);
                    HashMap::new()
                }
            }
        } else {
            // File doesn't exist yet
            HashMap::new()
        };
        
        // Update the in-memory HashMap
        let mut strategies = self.strategies.write().await;
        *strategies = loaded_strategies;
        
        info!("Loaded {} strategies", strategies.len());
        Ok(())
    }
    
    /// Saves strategies to disk
    async fn save_strategies(&self) -> Result<()> {
        debug!("Saving strategies to {:?}", self.strategies_path);
        
        // Get the current strategies
        let strategies = self.strategies.read().await;
        
        // Ensure directory exists
        if let Some(parent) = self.strategies_path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await
                    .context("Failed to create directory for strategies file")?;
            }
        }
        
        // Serialize to JSON
        let json = serde_json::to_string_pretty(&*strategies)
            .context("Failed to serialize strategies")?;
        
        // Write to file
        tokio::fs::write(&self.strategies_path, json).await
            .context("Failed to write strategies file")?;
        
        debug!("Saved {} strategies to disk", strategies.len());
        Ok(())
    }

    /// Adds a new strategy to the AutoTrader
    pub async fn add_strategy(&self, strategy: Strategy) -> Result<()> {
        // Validate the strategy first
        if let Err(validation_error) = strategy.validate() {
            return Err(anyhow!("Invalid strategy: {}", validation_error));
        }
        
        // Add strategy to the in-memory HashMap
        let mut strategies = self.strategies.write().await;
        info!("Adding strategy: {} ({})", strategy.name, strategy.id);
        strategies.insert(strategy.id.clone(), strategy);
        drop(strategies); // Release lock before saving
        
        // Save strategies to disk
        self.save_strategies().await?;
        
        Ok(())
    }
    
    /// Updates an existing strategy
    pub async fn update_strategy(&self, strategy: Strategy) -> Result<()> {
        // Validate the strategy first
        if let Err(validation_error) = strategy.validate() {
            return Err(anyhow!("Invalid strategy: {}", validation_error));
        }
        
        // Check if the strategy exists before updating
        let mut strategies = self.strategies.write().await;
        if !strategies.contains_key(&strategy.id) {
            return Err(anyhow!("Strategy with ID {} not found", strategy.id));
        }
        
        // Update the strategy
        info!("Updating strategy: {} ({})", strategy.name, strategy.id);
        strategies.insert(strategy.id.clone(), strategy);
        drop(strategies); // Release lock before saving
        
        // Save strategies to disk
        self.save_strategies().await?;
        
        Ok(())
    }
    
    /// Toggles a strategy's enabled state
    pub async fn toggle_strategy(&self, strategy_id: &str) -> Result<bool> {
        // Get the strategy
        let mut strategies = self.strategies.write().await;
        let strategy = strategies.get_mut(strategy_id)
            .ok_or_else(|| anyhow!("Strategy not found: {}", strategy_id))?;
        
        // Toggle the enabled flag
        strategy.enabled = !strategy.enabled;
        let new_status = strategy.enabled;
        drop(strategies);
        
        // Save changes to disk
        self.save_strategies().await?;
        
        info!("Strategy {} {} status: {}", strategy_id, 
            if new_status { "enabled" } else { "disabled" },
            new_status);
        
        Ok(new_status)
    }
    
    /// Deletes a strategy by ID
    pub async fn delete_strategy(&self, id: &str) -> Result<()> {
        // Remove the strategy from the in-memory HashMap
        let mut strategies = self.strategies.write().await;
        if let Some(strategy) = strategies.remove(id) {
            info!("Deleted strategy: {} ({})", strategy.name, strategy.id);
            drop(strategies); // Release lock before saving
            
            // Save strategies to disk
            self.save_strategies().await?;
            Ok(())
        } else {
            Err(anyhow!("Strategy with ID {} not found", id))
        }
    }

    pub async fn get_strategy(&self, id: &str) -> Option<Strategy> {
        let strategies = self.strategies.read().await;
        strategies.get(id).cloned()
    }

    pub async fn list_strategies(&self) -> Vec<Strategy> {
        let strategies = self.strategies.read().await;
        strategies.values().cloned().collect()
    }

    /// Sets the notification manager for trade alerts
    pub fn set_notification_manager(&mut self, notification_manager: Arc<crate::bot::notification::NotificationManager>) {
        self.notification_manager = Some(notification_manager);
        info!("Notification manager attached to AutoTrader");
    }

    // --- Control Methods ---

    // Changed to take &self
    pub async fn start(&self) -> Result<()> {
        // Check if already running *before* acquiring write lock if possible
        if *self.running.read().await {
             warn!("AutoTrader start requested but already running.");
             return Err(anyhow!("AutoTrader is already running"));
        }

        let mut running_guard = self.running.write().await;
        // Double check after acquiring write lock
        if *running_guard {
             warn!("AutoTrader start requested but already running (race condition).");
             return Ok(()); // Not an error, just already started
        }

        // Start the position manager's monitoring task
        // Ensure PositionManager::start_monitoring takes &self or Arc<Self> appropriately
        // Assuming it takes Arc<Self> based on previous implementation attempt
        self.position_manager.clone().start_monitoring().await?;

        // Set running flag to true
        *running_guard = true;
        // Drop the write guard before spawning the task
        drop(running_guard);

        info!("Starting AutoTrader background task...");

        // Clone necessary Arcs for the task
        let running_flag = self.running.clone();
        let strategies = self.strategies.clone();
        let helius_client = self.helius_client.clone();
        let risk_analyzer = self.risk_analyzer.clone();
        let position_manager = self.position_manager.clone();
        let config = self.config.clone();
        let wallet_manager = self.wallet_manager.clone();
        let jupiter_client = self.jupiter_client.clone();
        // Need solana_client too for RiskAnalyzer/PositionManager potentially
        let _solana_client = self.solana_client.clone(); // Prefixed as unused for now


        let handle = tokio::spawn(async move {
            let scan_interval = Duration::from_secs(60); // Configurable?
            let mut interval_timer = interval(scan_interval);
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip); // Skip ticks if busy

            info!("AutoTrader background task started.");

            loop {
                 // Check running status without holding lock for long
                if !*running_flag.read().await { // Use cloned running Arc
                    info!("AutoTrader running flag is false, stopping background task.");
                    break;
                }

                interval_timer.tick().await; // Wait for the next interval

                debug!("AutoTrader tick: Performing scan and manage cycle.");

                // Call the standalone scan cycle function
                if let Err(e) = run_scan_cycle(
                    strategies.clone(),
                    helius_client.clone(),
                    risk_analyzer.clone(),
                    position_manager.clone(),
                    config.clone(),
                    wallet_manager.clone(),
                    jupiter_client.clone(),
                    // solana_client is implicitly used by others
                ).await {
                    error!("Error during scan cycle: {}", e);
                }

                // Position management is handled by PositionManager's own task,
                // so no need to call manage_positions here.
                // if let Err(e) = self_clone.position_manager.manage_positions().await {
                //     error!("Error managing positions: {:?}", e);
                // }

                 // Add a small delay to prevent tight looping if ticks are skipped
                 // tokio::time::sleep(Duration::from_millis(100)).await;
            }
            info!("AutoTrader background task finished.");
        });

        // Store the task handle
        *self.task_handle.lock().await = Some(handle);

        info!("AutoTrader started successfully.");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut running_guard = self.running.write().await;
        if !*running_guard {
             warn!("AutoTrader stop requested but not running.");
            return Err(anyhow!("AutoTrader is not running"));
        }

        info!("Stopping AutoTrader...");
        *running_guard = false;
        // Drop the guard before waiting for the task
        drop(running_guard);

        // Stop the position manager's monitoring task
        self.position_manager.stop_monitoring().await?; // Assuming this returns Result

        // Wait for the background task to finish
        let mut handle_guard = self.task_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
             info!("Waiting for AutoTrader background task to complete...");
             // Abort the task to ensure it stops promptly
             handle.abort();
             // Optionally wait with a timeout, but abort is usually sufficient
             // if let Err(e) = tokio::time::timeout(Duration::from_secs(5), handle).await {
             //      error!("Error waiting for AutoTrader task after abort: {:?}", e);
             // } else {
                  info!("AutoTrader background task aborted.");
             // }
        } else {
             warn!("No running AutoTrader task handle found to wait for.");
        }


        info!("AutoTrader stopped successfully.");
        Ok(())
    }

    pub async fn get_status(&self) -> bool {
        *self.running.read().await
    }

    // --- Performance Stats ---

    pub async fn get_performance_stats(&self) -> Result<HashMap<String, f64>> { // Return Result
        let positions = self.position_manager.get_all_positions().await; // Assuming this returns Vec<Position>

        let total_positions = positions.len();
        let closed_positions: Vec<_> = positions
            .into_iter() // Take ownership
            .filter(|p| p.status == PositionStatus::Closed)
            .collect();

        let closed_count = closed_positions.len();
        if closed_count == 0 {
            // Return default stats if no closed positions yet
            let mut stats = HashMap::new();
            stats.insert("total_positions".to_string(), total_positions as f64);
            stats.insert("closed_positions".to_string(), 0.0);
            stats.insert("profitable_positions".to_string(), 0.0);
            stats.insert("win_rate".to_string(), 0.0);
            stats.insert("total_invested_sol".to_string(), 0.0);
            stats.insert("total_returned_sol".to_string(), 0.0);
            stats.insert("total_pnl_sol".to_string(), 0.0);
            stats.insert("average_pnl_percent".to_string(), 0.0);
            stats.insert("roi_percent".to_string(), 0.0);
            return Ok(stats);
        }


        let profit_positions: Vec<_> = closed_positions
            .iter()
            .filter(|p| p.pnl_sol.unwrap_or(0.0) > 0.0) // Use pnl_sol
            .collect();

        let profitable_count = profit_positions.len();

        let total_invested_sol: f64 = closed_positions
            .iter()
            .map(|p| p.entry_value_sol) // Sum entry value
            .sum();

        let total_returned_sol: f64 = closed_positions
            .iter()
            .map(|p| p.exit_value_sol.unwrap_or(0.0)) // Sum exit value
            .sum();

        let total_pnl_sol = total_returned_sol - total_invested_sol;

        let average_pnl_percent: f64 = closed_positions
            .iter()
            .filter_map(|p| p.pnl_percent) // Filter out None values
            .sum::<f64>() / closed_count as f64;


        let mut stats = HashMap::new();
        stats.insert("total_positions".to_string(), total_positions as f64);
        stats.insert("closed_positions".to_string(), closed_count as f64);
        stats.insert("profitable_positions".to_string(), profitable_count as f64);
        stats.insert("win_rate".to_string(), profitable_count as f64 / closed_count as f64 * 100.0);
        stats.insert("total_invested_sol".to_string(), total_invested_sol);
        stats.insert("total_returned_sol".to_string(), total_returned_sol);
        stats.insert("total_pnl_sol".to_string(), total_pnl_sol);
        stats.insert("average_pnl_percent".to_string(), average_pnl_percent);

        if total_invested_sol > 0.0 {
            stats.insert("roi_percent".to_string(), (total_pnl_sol / total_invested_sol) * 100.0);
        } else {
            stats.insert("roi_percent".to_string(), 0.0);
        }

        Ok(stats)
    }

    /// Executes a manual buy (snipe) triggered by a command.
    pub async fn execute_manual_buy(
        &self,
        token_address: &str,
        amount_sol: f64,
        // We need access to clients/managers, assuming they are available via self
    ) -> Result<SwapResult> {
        info!("Executing manual snipe for token {} with {} SOL", token_address, amount_sol);

        // 1. Get necessary token metadata (decimals primarily for Jupiter)
        //    We could fetch from Birdeye overview again, or rely on SolanaClient::get_mint_info
        let mint_info = self.solana_client.get_mint_info(&Pubkey::from_str(token_address)?)
            .await.context("Snipe: Failed to get mint info for decimals")?;
        let token_decimals = mint_info.decimals;
        // Use address as symbol/name for now, or fetch more details if needed
        let token_symbol = token_address.chars().take(6).collect::<String>();
        let token_name = format!("Manual Snipe {}", token_symbol);


        // 2. Execute Swap using JupiterClient
        //    Use default slippage/priority fee from config
        let swap_result = self.jupiter_client.swap_sol_to_token(
            token_address,
            token_decimals,
            amount_sol,
            self.config.default_slippage_bps,
            Some(self.config.default_priority_fee_micro_lamports),
            self.wallet_manager.clone(), // Pass Arc<WalletManager>
        ).await.context(format!("Snipe: Failed to execute SOL to {} swap", token_symbol))?;

        info!(
            "Snipe swap sent for {}. Signature: {}, Estimated Out: {:.6}",
            token_symbol, swap_result.transaction_signature, swap_result.out_amount_ui
        );

        // 3. Confirm Transaction
        info!("Snipe: Confirming transaction: {}", swap_result.transaction_signature);
        let signature = Signature::from_str(&swap_result.transaction_signature)
            .context("Snipe: Failed to parse transaction signature")?;

        match self.solana_client.confirm_transaction(&signature, solana_sdk::commitment_config::CommitmentLevel::Confirmed, 90).await { // Increased timeout slightly
            Ok(_) => {
                info!("Snipe transaction {} confirmed successfully.", signature);

                // 4. Create Position Entry (using defaults for SL/TP etc.)
                let actual_out_amount = swap_result.actual_out_amount_ui.unwrap_or(swap_result.out_amount_ui);
                
                // Check fill rate - if it's too low, warn the user
                let fill_rate = if swap_result.out_amount_ui > 0.0 {
                    (actual_out_amount / swap_result.out_amount_ui) * 100.0
                } else {
                    100.0 // Default to 100% if expected is 0
                };
                
                // Log warning if fill rate is low
                if fill_rate < 95.0 {
                    warn!(
                        "Snipe: Low fill rate detected: Received {:.4} tokens ({:.1}% of expected {:.4})",
                        actual_out_amount, fill_rate, swap_result.out_amount_ui
                    );
                    
                    // Send notification for significantly low fill rate
                    if fill_rate < 50.0 && self.notification_manager.is_some() {
                        if let Some(ref notification_manager) = self.notification_manager {
                            notification_manager.send_error_alert(
                                &format!("⚠️ Very low fill rate in snipe: only {:.1}% filled", fill_rate),
                                &format!("Snipe for {} only received {:.4} tokens ({:.1}% of expected {:.4})", 
                                    token_symbol, actual_out_amount, fill_rate, swap_result.out_amount_ui
                                )
                            ).await;
                        }
                    }
                }

                self.position_manager.create_position(
                    token_address,
                    &token_name,
                    &token_symbol,
                    token_decimals,
                    "MANUAL_SNIPE", // Use a specific strategy ID for manual snipes
                    amount_sol, // Entry value in SOL
                    actual_out_amount, // Amount of token received
                    Some(swap_result.out_amount_ui), // Expected amount as a separate parameter
                    swap_result.price_impact_pct,
                    &swap_result.transaction_signature,
                    None, // No default SL
                    None, // No default TP
                    None, // No default Trailing Stop
                    None, // No default Max Hold Time
                ).await.context("Snipe: Failed to create position entry after successful swap confirmation")?;

                info!(
                    "Snipe: Position created for {} ({}) with {:.4} SOL entry value.",
                    token_name, token_symbol, amount_sol
                );

                // Send trade notification if notification manager is available
                if let Some(notification_manager) = &self.notification_manager {
                    // Get the position we just created - use a position finder by token
                    let positions = self.position_manager.get_positions_by_token(token_address).await?;
                    if let Some(position) = positions.first() {
                        notification_manager.send_trade_notification(position, true).await;
                    }
                }

                Ok(swap_result) // Return original swap result on success
            }
            Err(e) => {
                error!("Snipe: Failed to confirm transaction {}: {:?}", signature, e);
                Err(e).context(format!("Snipe transaction {} failed confirmation", signature))
            }
        }
    }

} // This brace closes the impl AutoTrader block

// Note: Removed the manual Clone implementation as it was complex and likely incorrect.
// If AutoTrader needs to be cloned (e.g., for passing to multiple handlers),
// ensure all fields are properly handled (usually by cloning Arcs).
