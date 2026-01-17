use anyhow::{anyhow, Context, Result};
use borsh::BorshDeserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::interval;
use chrono::Utc;
use tracing::{debug, error, info, warn};
use solana_client::nonblocking::rpc_client::RpcClient as SolanaRpcClient;

use crate::api::birdeye::BirdeyeClient;
use crate::api::helius::HeliusClient;
use crate::api::jupiter::{JupiterClient, SwapResult};
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::config::Config;
use crate::trading::position::PositionManager;
use crate::trading::risk::{RiskAnalysis, RiskAnalyzer};
use crate::trading::strategy::Strategy;
use crate::trading::simulation::SimulationManager;
use crate::trading::pumpfun::{PumpfunToken, BondingCurveState};
use crate::trading::pumpfun_monitor::PumpfunMonitor;
use crate::trading::graduation_monitor::{GraduationMonitor, GraduationEvent};
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
    simulation_manager: Option<Arc<SimulationManager>>,
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

    // --- Dry Run or Real Mode Scan ---
    // In dry run mode, we scan real tokens but simulate trades instead of executing
    if config.dry_run_mode {
        info!("🔍 [DRY RUN] Scanning for real tokens (simulation mode)...");
    } else {
        info!("Scanning for new tokens using Helius...");
    }
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
                                info!("✅ [CANDIDATE] Token {} meets criteria for strategy '{}' - Risk: {}/100",
                                    token.symbol, strategy.name, risk_analysis.risk_level);

                                // DRY RUN MODE: Simulate the trade instead of executing
                                if config.dry_run_mode {
                                    if let Some(ref sim_mgr) = simulation_manager {
                                        // Check if we already have a simulated position
                                        if !sim_mgr.has_open_position(&token.address).await {
                                            match sim_mgr.simulate_buy(
                                                &token.address,
                                                &token.symbol,
                                                &token.name,
                                                risk_analysis.liquidity_sol / 1000.0, // Estimate price from liquidity
                                                strategy.max_position_size_sol,
                                                risk_analysis.risk_level,
                                                risk_analysis.details.clone(),
                                                format!("Passed '{}' strategy criteria", strategy.name),
                                                strategy.id.clone(),
                                            ).await {
                                                Ok(_) => info!("🔍 [DRY RUN] Successfully simulated buy for {} via strategy '{}'", token.symbol, strategy.name),
                                                Err(e) => warn!("🔍 [DRY RUN] Failed to simulate buy for {}: {:?}", token.symbol, e),
                                            }
                                        } else {
                                            debug!("🔍 [DRY RUN] Already have simulated position for {}", token.symbol);
                                        }
                                    }
                                } else {
                                    // REAL MODE: Execute actual trade
                                    if should_execute_buy_task(&token, strategy, &position_manager).await? {
                                        match execute_buy_task(
                                            &token,
                                            strategy,
                                            &position_manager,
                                            &jupiter_client,
                                            &wallet_manager,
                                            &config,
                                            None,
                                        ).await {
                                            Ok(_) => info!("Successfully executed buy and confirmed for {} via strategy '{}'", token.symbol, strategy.name),
                                            Err(e) => error!("Failed to execute buy for {}: {:?}", token.symbol, e),
                                        }
                                    } else {
                                        debug!("Buy condition not met for token {} and strategy '{}'", token.symbol, strategy.name);
                                    }
                                }
                            } else {
                                // Enhanced logging for rejected tokens
                                if risk_analysis.risk_level > strategy.max_risk_level {
                                    info!("❌ [REJECT] {} - Risk too high: {}/100 (max: {})",
                                        token.symbol, risk_analysis.risk_level, strategy.max_risk_level);
                                } else if risk_analysis.liquidity_sol < strategy.min_liquidity_sol as f64 {
                                    info!("❌ [REJECT] {} - Liquidity too low: {:.2} SOL (min: {})",
                                        token.symbol, risk_analysis.liquidity_sol, strategy.min_liquidity_sol);
                                } else if risk_analysis.holder_count < strategy.min_holders {
                                    info!("❌ [REJECT] {} - Not enough holders: {} (min: {})",
                                        token.symbol, risk_analysis.holder_count, strategy.min_holders);
                                } else {
                                    debug!("Token {} does not meet criteria for strategy '{}'", token.symbol, strategy.name);
                                }
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
    _notification_tx: Option<()>, // Placeholder for future WebSocket notification channel
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

                // TODO: Send notification via WebSocket when implemented
                if fill_rate < 50.0 {
                    warn!(
                        "Very low fill rate in trade: only {:.1}% filled for {}",
                        fill_rate, token.symbol
                    );
                }
            }

            position_manager.create_position(
                &token.address,
                &token.name,
                &token.symbol,
                token_decimals,
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
    pub simulation_manager: Option<Arc<SimulationManager>>, // For DRY_RUN_MODE
    is_running: Arc<AtomicBool>,
    // notification_tx will be used for WebSocket broadcasts in future
    // notification_tx: Option<broadcast::Sender<WsMessage>>,
    strategies: Arc<RwLock<HashMap<String, Strategy>>>, // Use Arc<RwLock<..>> for shared mutable state
    running: Arc<RwLock<bool>>, // Use Arc<RwLock<..>>
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    strategies_path: PathBuf,

    // Pump.fun real-time discovery (for DRY_RUN_MODE)
    pumpfun_token_rx: Arc<Mutex<Option<mpsc::Receiver<PumpfunToken>>>>,
    graduation_rx: Arc<Mutex<Option<mpsc::Receiver<GraduationEvent>>>>,
    pumpfun_monitor: Arc<Mutex<Option<PumpfunMonitor>>>,
    graduation_monitor: Arc<Mutex<Option<GraduationMonitor>>>,

    // Multi-strategy support (NewPairs, FinalStretch, Migrated)
    active_strategy_type: Arc<RwLock<crate::trading::strategy::StrategyType>>,
    watchlist: Arc<crate::trading::watchlist::Watchlist>,
    scanner: Arc<Mutex<Option<crate::trading::scanner::Scanner>>>,
}

impl AutoTrader {
    // FIXED VERSION: Changed to async to avoid block_on issues
    pub async fn new(
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

        // Initialize SimulationManager if dry_run_mode is enabled
        let simulation_manager = if config.dry_run_mode {
            info!("🔍 [DRY RUN] Mode enabled - trades will be simulated, not executed");
            let sim_mgr = Arc::new(SimulationManager::new(birdeye_client.clone()));
            // Load existing simulated positions
            if let Err(e) = sim_mgr.load().await {
                warn!("Failed to load simulated positions: {}", e);
            }
            Some(sim_mgr)
        } else {
            None
        };

        // Set the default path for strategy persistence
        let strategies_path = PathBuf::from("data/strategies.json");

        // Initialize watchlist and load existing tokens
        let watchlist = Arc::new(crate::trading::watchlist::Watchlist::new());
        if let Err(e) = watchlist.load().await {
            warn!("Failed to load watchlist: {}", e);
        }

        // Create AutoTrader instance
        let autotrader = Self {
            wallet_manager,
            solana_client: solana_client.clone(),
            helius_client,
            jupiter_client,
            birdeye_client: birdeye_client.clone(),
            config: config.clone(),
            position_manager,
            risk_analyzer,
            simulation_manager,
            is_running: Arc::new(AtomicBool::new(false)),
            strategies: Arc::new(RwLock::new(HashMap::new())), // Start with empty map, will load in init
            running: Arc::new(RwLock::new(false)),
            task_handle: Arc::new(Mutex::new(None)),
            strategies_path,
            // Pump.fun discovery initialized to None - will be set up in init_pumpfun_discovery()
            pumpfun_token_rx: Arc::new(Mutex::new(None)),
            graduation_rx: Arc::new(Mutex::new(None)),
            pumpfun_monitor: Arc::new(Mutex::new(None)),
            graduation_monitor: Arc::new(Mutex::new(None)),
            // Multi-strategy support
            active_strategy_type: Arc::new(RwLock::new(crate::trading::strategy::StrategyType::NewPairs)),
            watchlist,
            scanner: Arc::new(Mutex::new(None)), // Scanner initialized in start() when needed
        };
        
        // Initialize by loading strategies - use await directly since we're in an async function
        match autotrader.load_strategies().await {
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

        // If no strategies loaded, create a default one for Pump.fun discovery
        if strategies.is_empty() {
            info!("📋 No strategies found - creating default 'Pump.fun Scout' strategy...");

            let default_strategy = Strategy {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Pump.fun Scout".to_string(),
                enabled: true,
                strategy_type: crate::trading::strategy::StrategyType::NewPairs,
                max_concurrent_positions: 5,
                max_position_size_sol: 0.1,
                total_budget_sol: 1.0,
                stop_loss_percent: Some(20),
                take_profit_percent: Some(50),
                trailing_stop_percent: Some(10),
                max_hold_time_minutes: 60,
                min_liquidity_sol: 1,
                max_risk_level: 70,
                min_holders: 1,
                max_token_age_minutes: 30,
                require_lp_burned: false,
                reject_if_mint_authority: false,
                reject_if_freeze_authority: false,
                require_can_sell: false,
                max_transfer_tax_percent: Some(5.0),
                max_concentration_percent: Some(90.0),
                // NewPairs doesn't use these criteria
                min_volume_usd: None,
                min_market_cap_usd: None,
                min_bonding_progress: None,
                require_migrated: None,
                slippage_bps: None,
                priority_fee_micro_lamports: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            strategies.insert(default_strategy.id.clone(), default_strategy);
            info!("✅ Default 'Pump.fun Scout' strategy created and enabled");

            // Save the default strategy to disk
            drop(strategies); // Release lock before saving
            if let Err(e) = self.save_strategies().await {
                warn!("Failed to save default strategy to disk: {}", e);
            }
        } else {
            info!("Loaded {} strategies", strategies.len());
        }

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

    // --- Active Strategy Type Management ---

    /// Get the currently active strategy type
    pub async fn get_active_strategy_type(&self) -> crate::trading::strategy::StrategyType {
        self.active_strategy_type.read().await.clone()
    }

    /// Set the active strategy type
    /// This determines which discovery method is used:
    /// - NewPairs: WebSocket CreateEvent monitoring (sniper)
    /// - FinalStretch/Migrated: Scanner with Birdeye data
    pub async fn set_active_strategy_type(&self, strategy_type: crate::trading::strategy::StrategyType) -> Result<()> {
        let old_type = self.get_active_strategy_type().await;
        if old_type == strategy_type {
            debug!("Strategy type already set to {:?}", strategy_type);
            return Ok(());
        }

        info!("🔄 Switching active strategy from {:?} to {:?}", old_type, strategy_type);

        // Update the strategy type
        let mut active = self.active_strategy_type.write().await;
        *active = strategy_type.clone();
        drop(active);

        info!("✅ Active strategy type set to: {:?}", strategy_type);
        Ok(())
    }

    /// Get watchlist reference
    pub fn get_watchlist(&self) -> Arc<crate::trading::watchlist::Watchlist> {
        self.watchlist.clone()
    }

    /// Get watchlist statistics
    pub async fn get_watchlist_stats(&self) -> crate::trading::watchlist::WatchlistStats {
        self.watchlist.get_stats().await
    }

    // TODO: Add method to set WebSocket broadcast channel for notifications
    // pub fn set_notification_tx(&mut self, tx: broadcast::Sender<WsMessage>) {
    //     self.notification_tx = Some(tx);
    //     info!("Notification channel attached to AutoTrader");
    // }

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

        // Initialize and start Pump.fun discovery in dry run mode
        if self.config.dry_run_mode {
            info!("🔍 [DRY RUN] Initializing Pump.fun real-time discovery...");
            if let Err(e) = self.init_pumpfun_discovery().await {
                warn!("Failed to initialize Pump.fun discovery: {:?}", e);
            } else if let Err(e) = self.start_pumpfun_discovery().await {
                warn!("Failed to start Pump.fun discovery: {:?}", e);
            }
        }

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
        let simulation_manager = self.simulation_manager.clone();
        // Need solana_client too for RiskAnalyzer/PositionManager potentially
        // solana_client is used implicitly by other components


        // Take the Pump.fun token receiver for use in the task (if in dry run mode)
        let pumpfun_token_rx = if config.dry_run_mode {
            let mut rx_guard = self.pumpfun_token_rx.lock().await;
            rx_guard.take()
        } else {
            None
        };

        // Clone watchlist for use in the task
        let watchlist = self.watchlist.clone();

        // Clone config API key for RPC client in token processing
        let helius_api_key = config.helius_api_key.clone();

        let handle = tokio::spawn(async move {
            // Main scanning loop
            let mut scan_interval = interval(Duration::from_secs(60)); // Scan every 60 seconds
            let mut price_update_counter: u32 = 0;

            // Create RPC client for Pump.fun token processing
            let rpc_client = if config.dry_run_mode {
                Some(SolanaRpcClient::new(format!(
                    "https://mainnet.helius-rpc.com/?api-key={}",
                    helius_api_key
                )))
            } else {
                None
            };

            // Wrap the receiver in an Option so we can use it in the select!
            let mut token_rx = pumpfun_token_rx;

            loop {
                // Check if we should stop
                if !*running_flag.read().await {
                    info!("AutoTrader scanning task stopped.");
                    break;
                }

                // Use tokio::select! to handle both timer events and incoming tokens
                tokio::select! {
                    // Handle Pump.fun token discovery (dry run mode only)
                    token = async {
                        if let Some(ref mut rx) = token_rx {
                            rx.recv().await
                        } else {
                            // If no receiver, wait forever (this branch won't be selected)
                            std::future::pending::<Option<PumpfunToken>>().await
                        }
                    } => {
                        if let Some(token) = token {
                            info!("📥 Received token from WebSocket channel: {} ({})", token.symbol, token.mint);

                            // Process the discovered token
                            if let (Some(ref sim_mgr), Some(ref rpc)) = (&simulation_manager, &rpc_client) {
                                let enabled_strategies: Vec<Strategy> = {
                                    let strats = strategies.read().await;
                                    strats.values().filter(|s| s.enabled).cloned().collect()
                                };

                                if let Err(e) = AutoTrader::process_pumpfun_token(
                                    &token,
                                    &enabled_strategies,
                                    sim_mgr,
                                    rpc,
                                    Some(&watchlist),
                                ).await {
                                    warn!("Error processing Pump.fun token {}: {:?}", token.symbol, e);
                                }
                            } else {
                                warn!("Cannot process token - simulation_manager or rpc_client not available");
                            }
                        } else {
                            warn!("Token channel closed - no more tokens will be received");
                        }
                    }

                    // Regular scan cycle timer
                    _ = scan_interval.tick() => {
                        // In dry_run mode, skip the DAS API scan - we use WebSocket discovery instead
                        // The DAS API returns NFT metadata, not tradeable Pump.fun tokens
                        if !config.dry_run_mode {
                            // Run the regular scan cycle (uses Helius DAS for non-Pump.fun tokens)
                            if let Err(e) = run_scan_cycle(
                                strategies.clone(),
                                helius_client.clone(),
                                risk_analyzer.clone(),
                                position_manager.clone(),
                                config.clone(),
                                wallet_manager.clone(),
                                jupiter_client.clone(),
                                simulation_manager.clone(),
                            ).await {
                                error!("Error in scan cycle: {:?}", e);
                                // Continue running even if one cycle fails
                            }
                        }

                        // In dry run mode, update prices and check exit conditions every 5 scan cycles
                        if config.dry_run_mode {
                            price_update_counter += 1;
                            if price_update_counter >= 5 {
                                price_update_counter = 0;
                                if let Some(ref sim_mgr) = simulation_manager {
                                    // Update prices for all open simulated positions
                                    if let Err(e) = sim_mgr.update_prices().await {
                                        warn!("🔍 [DRY RUN] Failed to update simulated prices: {}", e);
                                    }

                                    // Check exit conditions using default strategy settings
                                    let stop_loss = config.default_stop_loss_percent as f64;
                                    let take_profit = config.default_take_profit_percent as f64;
                                    let trailing_stop = Some(config.default_trailing_stop_percent as f64);
                                    let max_hold = Some(config.max_hold_time_minutes);

                                    match sim_mgr.check_exit_conditions(
                                        stop_loss,
                                        take_profit,
                                        trailing_stop,
                                        max_hold,
                                    ).await {
                                        Ok(closed) => {
                                            if !closed.is_empty() {
                                                info!("🔍 [DRY RUN] Closed {} simulated positions", closed.len());
                                            }
                                        }
                                        Err(e) => warn!("🔍 [DRY RUN] Failed to check exit conditions: {}", e),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        // Store the task handle
        let mut task_handle_guard = self.task_handle.lock().await;
        *task_handle_guard = Some(handle);
        drop(task_handle_guard);

        info!("AutoTrader started successfully");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        // Set running flag to false
        let mut running_guard = self.running.write().await;
        *running_guard = false;
        drop(running_guard);

        // Stop Pump.fun monitors if running
        if self.config.dry_run_mode {
            if let Err(e) = self.stop_pumpfun_discovery().await {
                warn!("Error stopping Pump.fun discovery: {:?}", e);
            }
        }

        // Wait for the task to finish
        let mut task_handle_guard = self.task_handle.lock().await;
        if let Some(handle) = task_handle_guard.take() {
            handle.await.context("Failed to wait for AutoTrader task to finish")?;
        }
        drop(task_handle_guard);

        // Stop position manager monitoring
        self.position_manager.stop_monitoring().await?;

        info!("AutoTrader stopped successfully");
        Ok(())
    }

    pub async fn get_status(&self) -> bool {
        *self.running.read().await
    }

    /// Executes a manual buy for a specific token address
    pub async fn execute_manual_buy(
        &self,
        token_address: &str,
        amount_sol: f64,
    ) -> Result<SwapResult> {
        info!("Executing manual buy for token: {} with amount: {} SOL", token_address, amount_sol);

        // Use the default strategy for manual buys
        let strategies = self.strategies.read().await;
        let default_strategy = strategies.values().find(|s| s.name.to_lowercase() == "default").cloned();

        let strategy = match default_strategy {
            Some(s) => s,
            None => {
                // Create a temporary default strategy if none exists
                drop(strategies);
                return self.create_default_strategy_and_buy(token_address, amount_sol).await;
            }
        };

        drop(strategies);

        // Check if we already have a position in this token
        if self.position_manager.has_active_position(token_address).await {
            return Err(anyhow!("Already have an active position in token {}", token_address));
        }

        // Get token metadata
        let token_metadata = self.get_token_metadata(token_address).await?;

        // Execute the buy using the existing execute_buy_task function
        execute_buy_task(
            &token_metadata,
            &strategy,
            &self.position_manager,
            &self.jupiter_client,
            &self.wallet_manager,
            &self.config,
            None, // TODO: Pass WebSocket tx when implemented
        ).await
    }

    /// Creates a default strategy and executes a manual buy
    async fn create_default_strategy_and_buy(
        &self,
        token_address: &str,
        amount_sol: f64,
    ) -> Result<SwapResult> {
        // Create a basic default strategy
        let default_strategy = Strategy {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default".to_string(),
            enabled: true,
            strategy_type: crate::trading::strategy::StrategyType::NewPairs,
            max_concurrent_positions: 10,
            max_position_size_sol: amount_sol,
            total_budget_sol: amount_sol * 2.0,
            stop_loss_percent: Some(15),
            take_profit_percent: Some(50),
            trailing_stop_percent: Some(5),
            max_hold_time_minutes: 240,
            min_liquidity_sol: 1,
            max_risk_level: 80,
            min_holders: 10,
            max_token_age_minutes: 1440, // 24 hours
            require_lp_burned: false,
            reject_if_mint_authority: true,
            reject_if_freeze_authority: true,
            require_can_sell: true,
            max_transfer_tax_percent: Some(5.0),
            max_concentration_percent: Some(80.0),
            min_volume_usd: None,
            min_market_cap_usd: None,
            min_bonding_progress: None,
            require_migrated: None,
            slippage_bps: None,
            priority_fee_micro_lamports: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Add the strategy
        self.add_strategy(default_strategy.clone()).await?;

        // Get token metadata
        let token_metadata = self.get_token_metadata(token_address).await?;

        // Execute the buy
        execute_buy_task(
            &token_metadata,
            &default_strategy,
            &self.position_manager,
            &self.jupiter_client,
            &self.wallet_manager,
            &self.config,
            None, // TODO: Pass WebSocket tx when implemented
        ).await
    }

    /// Gets token metadata for a given address
    async fn get_token_metadata(&self, token_address: &str) -> Result<TokenMetadata> {
        // Try to get from Helius first
        match self.helius_client.get_token_metadata(token_address).await {
            Ok(metadata) => Ok(metadata),
            Err(_) => {
                // If Helius fails, create basic metadata
                Ok(TokenMetadata {
                    address: token_address.to_string(),
                    name: format!("Token {}", token_address),
                    symbol: "UNKNOWN".to_string(),
                    decimals: 9,
                    supply: None,
                    logo_uri: None,
                    creation_time: None,
                })
            }
        }
    }

    // =========================================================================
    // PUMP.FUN REAL-TIME DISCOVERY (for DRY_RUN_MODE)
    // =========================================================================

    /// Initialize Pump.fun real-time token discovery.
    /// This sets up the WebSocket monitor and graduation tracker.
    /// Call this before start() when using DRY_RUN_MODE.
    pub async fn init_pumpfun_discovery(&self) -> Result<()> {
        if !self.config.dry_run_mode {
            info!("Pump.fun discovery is only available in DRY_RUN_MODE");
            return Ok(());
        }

        info!("🚀 Initializing Pump.fun real-time discovery...");

        // Create channels for token discovery and graduation events
        let (token_tx, token_rx) = mpsc::channel::<PumpfunToken>(100);
        let (graduation_tx, graduation_rx) = mpsc::channel::<GraduationEvent>(50);

        // Create channel for token flow: PumpfunMonitor -> GraduationMonitor
        let (_token_for_grad_tx, token_for_grad_rx) = mpsc::channel::<PumpfunToken>(100);

        // Create the Pump.fun monitor
        let pumpfun_monitor = PumpfunMonitor::new(
            &self.config.helius_api_key,
            token_tx,
        );

        // Build RPC URL for graduation monitor
        let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", self.config.helius_api_key);

        // Create the graduation monitor
        let graduation_monitor = GraduationMonitor::new(
            &rpc_url,
            token_for_grad_rx,
            graduation_tx,
        );

        // Store the monitors and receivers
        {
            let mut monitor_guard = self.pumpfun_monitor.lock().await;
            *monitor_guard = Some(pumpfun_monitor);
        }
        {
            let mut grad_monitor_guard = self.graduation_monitor.lock().await;
            *grad_monitor_guard = Some(graduation_monitor);
        }
        {
            let mut token_rx_guard = self.pumpfun_token_rx.lock().await;
            *token_rx_guard = Some(token_rx);
        }
        {
            let mut grad_rx_guard = self.graduation_rx.lock().await;
            *grad_rx_guard = Some(graduation_rx);
        }

        info!("✅ Pump.fun discovery initialized");
        Ok(())
    }

    /// Start the Pump.fun monitors (call after init_pumpfun_discovery and start).
    pub async fn start_pumpfun_discovery(&self) -> Result<()> {
        if !self.config.dry_run_mode {
            return Ok(());
        }

        info!("🎯 Starting Pump.fun real-time monitors...");

        // Start Pump.fun monitor
        {
            let monitor_guard = self.pumpfun_monitor.lock().await;
            if let Some(ref monitor) = *monitor_guard {
                monitor.start().await?;
                info!("✅ Pump.fun WebSocket monitor started");
            }
        }

        // Start graduation monitor
        {
            let grad_monitor_guard = self.graduation_monitor.lock().await;
            if let Some(ref monitor) = *grad_monitor_guard {
                monitor.start().await?;
                info!("✅ Graduation monitor started");
            }
        }

        Ok(())
    }

    /// Stop the Pump.fun monitors.
    pub async fn stop_pumpfun_discovery(&self) -> Result<()> {
        info!("Stopping Pump.fun monitors...");

        // Stop Pump.fun monitor
        {
            let monitor_guard = self.pumpfun_monitor.lock().await;
            if let Some(ref monitor) = *monitor_guard {
                monitor.stop().await?;
            }
        }

        // Stop graduation monitor
        {
            let grad_monitor_guard = self.graduation_monitor.lock().await;
            if let Some(ref monitor) = *grad_monitor_guard {
                monitor.stop().await?;
            }
        }

        info!("Pump.fun monitors stopped");
        Ok(())
    }

    /// Process a discovered Pump.fun token.
    /// Evaluates the token against enabled strategies and simulates buys if criteria are met.
    /// Also adds tokens to the watchlist for later evaluation by Final Stretch/Migrated strategies.
    ///
    /// IMPORTANT: For NEW tokens, we use the data from CreateEvent directly!
    /// - real_sol_reserves = 0 is EXPECTED (no one has bought yet)
    /// - We use virtual_sol_reserves (30 SOL) for initial liquidity assessment
    /// - We skip bonding curve fetch to avoid race condition
    async fn process_pumpfun_token(
        token: &PumpfunToken,
        strategies: &[Strategy],
        simulation_manager: &SimulationManager,
        _rpc_client: &solana_client::nonblocking::rpc_client::RpcClient,
        watchlist: Option<&crate::trading::watchlist::Watchlist>,
    ) -> Result<()> {
        info!("🔍 Processing Pump.fun token: {} ({})", token.symbol, token.mint);

        // Add to watchlist for Final Stretch/Migrated strategy evaluation
        if let Some(wl) = watchlist {
            let watchlist_token = crate::trading::watchlist::WatchlistToken::from_create_event(
                &token.mint,
                &token.bonding_curve,
                &token.name,
                &token.symbol,
                token.price_sol,
                None, // creator not available from PumpfunToken
            );
            if let Err(e) = wl.add_token(watchlist_token).await {
                warn!("Failed to add {} to watchlist: {:?}", token.symbol, e);
            }
        }

        // Skip if bonding curve is already complete
        if token.is_graduated {
            debug!("Token {} already graduated, skipping", token.symbol);
            return Ok(());
        }

        // USE CreateEvent DATA DIRECTLY!
        // The token.price_sol is already calculated from CreateEvent's virtual reserves
        // This avoids the race condition where bonding curve account isn't ready yet
        let price_sol = token.price_sol;

        // For NEW tokens, progress is 0% (no one has bought yet) - THIS IS EXPECTED!
        let progress = token.bonding_progress;

        // For NEW tokens, real liquidity is 0 (no SOL deposited yet) - THIS IS EXPECTED!
        // Use virtual liquidity (30 SOL) for initial assessment instead
        const VIRTUAL_SOL_RESERVES: f64 = 30.0; // 30 SOL virtual liquidity at creation
        let virtual_liquidity_sol = VIRTUAL_SOL_RESERVES;

        info!("   Progress: {:.1}%, Price: {:.10} SOL, Virtual Liquidity: {:.2} SOL",
            progress, price_sol, virtual_liquidity_sol);

        // Calculate risk score for NEW tokens
        // Don't penalize 0 real liquidity - it's EXPECTED for brand new tokens!
        // Instead, use a simpler risk assessment based on token characteristics
        let risk_score = calculate_new_token_risk_score(token);
        info!("   Risk Score: {}/100 (new token scoring)", risk_score);

        // Check against each enabled strategy
        for strategy in strategies {
            if !strategy.enabled {
                continue;
            }

            // Check if token meets strategy criteria
            // For NEW tokens, use virtual liquidity (30 SOL) for assessment
            let meets_criteria =
                risk_score <= strategy.max_risk_level &&
                virtual_liquidity_sol >= strategy.min_liquidity_sol as f64;

            if meets_criteria {
                info!("✅ [CANDIDATE] {} meets criteria for strategy '{}' - Risk: {}/100, Virtual Liquidity: {:.2} SOL",
                    token.symbol, strategy.name, risk_score, virtual_liquidity_sol);

                // Check if we already have a simulated position
                if !simulation_manager.has_open_position(&token.mint).await {
                    // Simulate the buy
                    let entry_reason = format!(
                        "Pump.fun NEW token - Price: {:.10} SOL, Strategy: '{}'",
                        price_sol, strategy.name
                    );

                    match simulation_manager.simulate_buy(
                        &token.mint,
                        &token.symbol,
                        &token.name,
                        price_sol,
                        strategy.max_position_size_sol,
                        risk_score,
                        vec![
                            format!("NEW TOKEN - Just created!"),
                            format!("Virtual Liquidity: {:.2} SOL", virtual_liquidity_sol),
                            format!("Price: {:.10} SOL", price_sol),
                        ],
                        entry_reason,
                        strategy.id.clone(),
                    ).await {
                        Ok(_) => info!("🎯 [DRY RUN] Simulated buy for {} via strategy '{}'", token.symbol, strategy.name),
                        Err(e) => warn!("🔍 [DRY RUN] Failed to simulate buy for {}: {:?}", token.symbol, e),
                    }
                } else {
                    debug!("Already have simulated position for {}", token.symbol);
                }
            } else {
                // Log why it was rejected
                if risk_score > strategy.max_risk_level {
                    info!("❌ {} rejected - Risk too high: {}/100 (max: {})",
                        token.symbol, risk_score, strategy.max_risk_level);
                } else if virtual_liquidity_sol < strategy.min_liquidity_sol as f64 {
                    info!("❌ {} rejected - Virtual Liquidity too low: {:.2} SOL (min: {})",
                        token.symbol, virtual_liquidity_sol, strategy.min_liquidity_sol);
                }
            }
        }

        Ok(())
    }

    /// Gets performance statistics for the trading bot
    pub async fn get_performance_stats(&self) -> Result<PerformanceStats> {
        let positions = self.position_manager.get_all_positions().await;
        let mut total_pnl = 0.0;
        let mut total_trades = 0;
        let mut winning_trades = 0;
        let mut total_entry_value = 0.0;

        for position in positions {
            if let Some(exit_value) = position.exit_value_sol {
                let pnl = exit_value - position.entry_value_sol;
                total_pnl += pnl;
                total_entry_value += position.entry_value_sol;
                total_trades += 1;

                if pnl > 0.0 {
                    winning_trades += 1;
                }
            }
        }

        let win_rate = if total_trades > 0 {
            (winning_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let avg_roi = if total_entry_value > 0.0 {
            (total_pnl / total_entry_value) * 100.0
        } else {
            0.0
        };

        Ok(PerformanceStats {
            total_trades,
            winning_trades,
            total_pnl,
            win_rate,
            avg_roi,
            total_entry_value,
        })
    }
}

/// Performance statistics structure
#[derive(Debug, serde::Serialize)]
pub struct PerformanceStats {
    pub total_trades: u32,
    pub winning_trades: u32,
    pub total_pnl: f64,
    pub win_rate: f64,
    pub avg_roi: f64,
    pub total_entry_value: f64,
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Calculate risk score for a NEWLY CREATED Pump.fun token.
/// For new tokens, real_sol_reserves = 0 and progress = 0% is EXPECTED!
/// We use different criteria than established tokens.
/// Returns a score from 0-100 where higher = more risky.
fn calculate_new_token_risk_score(token: &PumpfunToken) -> u32 {
    let mut risk_score: f64 = 30.0; // Start at moderate-low risk for new tokens

    // 1. Price sanity check - initial price should be ~0.000000028 SOL
    let price = token.price_sol;
    if price <= 0.0 {
        risk_score += 40.0; // Invalid price
    } else if price < 0.000000001 || price > 0.001 {
        risk_score += 20.0; // Unusual starting price
    }

    // 2. Name/Symbol quality (basic heuristics)
    if token.name.len() < 2 || token.symbol.len() < 2 {
        risk_score += 15.0; // Very short name/symbol
    }
    if token.name.len() > 50 || token.symbol.len() > 15 {
        risk_score += 10.0; // Unusually long
    }

    // 3. Check for suspicious patterns in name/symbol
    let name_lower = token.name.to_lowercase();
    let symbol_lower = token.symbol.to_lowercase();

    // Common scam patterns
    let scam_keywords = ["rug", "scam", "honeypot", "free", "airdrop", "giveaway"];
    for keyword in scam_keywords {
        if name_lower.contains(keyword) || symbol_lower.contains(keyword) {
            risk_score += 30.0;
            break;
        }
    }

    // 4. Bonus: Tokens mimicking popular projects
    let popular_tokens = ["bonk", "wif", "pepe", "doge", "shib", "trump", "melania"];
    for popular in popular_tokens {
        if symbol_lower == popular || name_lower == popular {
            // Exact match to popular token name - suspicious
            risk_score += 15.0;
            break;
        }
    }

    // Clamp to 0-100 range
    risk_score.clamp(0.0, 100.0) as u32
}

/// Calculate risk score for a Pump.fun token based on bonding curve state.
/// Returns a score from 0-100 where higher = more risky.
#[allow(dead_code)]
fn calculate_pumpfun_risk_score(progress_percent: f64, liquidity_sol: f64) -> u32 {
    let mut risk_score: f64 = 50.0; // Start at moderate risk

    // Progress-based risk: Very new tokens (< 10%) are highest risk
    // Tokens close to graduation (> 80%) are lower risk
    if progress_percent < 5.0 {
        risk_score += 30.0; // Very early = very risky
    } else if progress_percent < 10.0 {
        risk_score += 20.0;
    } else if progress_percent < 25.0 {
        risk_score += 10.0;
    } else if progress_percent > 80.0 {
        risk_score -= 20.0; // Near graduation = lower risk
    } else if progress_percent > 50.0 {
        risk_score -= 10.0;
    }

    // Liquidity-based risk: More liquidity = lower risk
    if liquidity_sol < 1.0 {
        risk_score += 25.0; // Very low liquidity
    } else if liquidity_sol < 5.0 {
        risk_score += 15.0;
    } else if liquidity_sol < 10.0 {
        risk_score += 5.0;
    } else if liquidity_sol > 50.0 {
        risk_score -= 15.0; // High liquidity = lower risk
    } else if liquidity_sol > 25.0 {
        risk_score -= 10.0;
    }

    // Clamp to 0-100 range
    risk_score.clamp(0.0, 100.0) as u32
}
