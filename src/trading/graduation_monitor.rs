// src/trading/graduation_monitor.rs
//
// Monitors Pump.fun bonding curves to detect token graduation.
// Graduation occurs when a token completes its bonding curve and
// migrates to PumpSwap for standard AMM trading.

use anyhow::{anyhow, Result};
use borsh::BorshDeserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::trading::pumpfun::{BondingCurveState, PumpfunToken};

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Configuration for the graduation monitor
#[derive(Debug, Clone)]
pub struct GraduationMonitorConfig {
    /// How often to poll bonding curves (in seconds)
    pub poll_interval_secs: u64,
    /// Maximum tokens to track simultaneously
    pub max_tracked_tokens: usize,
}

impl Default for GraduationMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 10, // Default 10 seconds, configurable
            max_tracked_tokens: 100,
        }
    }
}

// ============================================================================
// GRADUATION EVENT
// ============================================================================

/// Event emitted when a token graduates from bonding curve to PumpSwap.
#[derive(Debug, Clone)]
pub struct GraduationEvent {
    /// Token mint address
    pub mint: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Final SOL reserves in the bonding curve (lamports)
    pub final_sol_reserves: u64,
    /// Final price in SOL
    pub final_price_sol: f64,
    /// Time of graduation (Unix timestamp)
    pub graduated_at: i64,
}

// ============================================================================
// TRACKED TOKEN STATE
// ============================================================================

/// Internal state for a tracked token
#[derive(Debug, Clone)]
struct TrackedToken {
    /// The token info
    pub token: PumpfunToken,
    /// Last known bonding curve progress
    pub last_progress: f64,
    /// Last known price
    pub last_price_sol: f64,
    /// Last known liquidity
    pub last_liquidity_sol: f64,
    /// When we started tracking
    pub tracked_since: i64,
}

// ============================================================================
// GRADUATION MONITOR
// ============================================================================

/// Monitors bonding curves and detects when tokens graduate.
///
/// This monitor:
/// 1. Receives newly discovered tokens from PumpfunMonitor
/// 2. Periodically polls their bonding curve accounts
/// 3. Detects when `complete` flag is set or reserves are exhausted
/// 4. Emits GraduationEvent when graduation is detected
pub struct GraduationMonitor {
    config: GraduationMonitorConfig,
    rpc_client: Arc<RpcClient>,
    /// Tokens being tracked: mint -> tracked state
    tracked_tokens: Arc<RwLock<HashMap<String, TrackedToken>>>,
    /// Channel to receive new tokens to track
    token_receiver: Arc<RwLock<mpsc::Receiver<PumpfunToken>>>,
    /// Channel to send graduation events
    graduation_sender: mpsc::Sender<GraduationEvent>,
    /// Running flag
    running: Arc<RwLock<bool>>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

impl GraduationMonitor {
    /// Create a new graduation monitor.
    ///
    /// # Arguments
    /// * `rpc_url` - Solana RPC URL (should be Helius for consistency)
    /// * `token_receiver` - Channel to receive new tokens to track
    /// * `graduation_sender` - Channel to send graduation events
    pub fn new(
        rpc_url: &str,
        token_receiver: mpsc::Receiver<PumpfunToken>,
        graduation_sender: mpsc::Sender<GraduationEvent>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config: GraduationMonitorConfig::default(),
            rpc_client: Arc::new(RpcClient::new(rpc_url.to_string())),
            tracked_tokens: Arc::new(RwLock::new(HashMap::new())),
            token_receiver: Arc::new(RwLock::new(token_receiver)),
            graduation_sender,
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(
        rpc_url: &str,
        config: GraduationMonitorConfig,
        token_receiver: mpsc::Receiver<PumpfunToken>,
        graduation_sender: mpsc::Sender<GraduationEvent>,
    ) -> Self {
        let mut monitor = Self::new(rpc_url, token_receiver, graduation_sender);
        monitor.config = config;
        monitor
    }

    /// Start the graduation monitor.
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(anyhow!("Graduation monitor is already running"));
        }
        *running = true;
        drop(running);

        info!("🎓 Starting graduation monitor...");
        info!(
            "📊 Poll interval: {} seconds, Max tracked: {}",
            self.config.poll_interval_secs, self.config.max_tracked_tokens
        );

        // Spawn token receiver task
        self.spawn_token_receiver_task();

        // Spawn polling task
        self.spawn_polling_task();

        Ok(())
    }

    /// Spawn the task that receives new tokens to track.
    fn spawn_token_receiver_task(&self) {
        let tracked_tokens = self.tracked_tokens.clone();
        let token_receiver = self.token_receiver.clone();
        let max_tracked = self.config.max_tracked_tokens;
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut receiver = token_receiver.write().await;

            while *running.read().await {
                tokio::select! {
                    result = receiver.recv() => {
                        match result {
                            Some(token) => {
                                let mut tokens = tracked_tokens.write().await;

                                // Check if we're at capacity
                                if tokens.len() >= max_tracked {
                                    // Remove oldest token (by tracked_since)
                                    if let Some(oldest_key) = tokens
                                        .iter()
                                        .min_by_key(|(_, t)| t.tracked_since)
                                        .map(|(k, _)| k.clone())
                                    {
                                        debug!("Removing oldest tracked token: {}", oldest_key);
                                        tokens.remove(&oldest_key);
                                    }
                                }

                                info!(
                                    "📝 Tracking new token for graduation: {} ({})",
                                    token.symbol, token.mint
                                );

                                let tracked = TrackedToken {
                                    token: token.clone(),
                                    last_progress: 0.0,
                                    last_price_sol: 0.0,
                                    last_liquidity_sol: 0.0,
                                    tracked_since: chrono::Utc::now().timestamp(),
                                };

                                tokens.insert(token.mint.clone(), tracked);
                            }
                            None => {
                                warn!("Token receiver channel closed");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // Check running flag periodically
                        if !*running.read().await {
                            break;
                        }
                    }
                }
            }
        });
    }

    /// Spawn the task that polls bonding curves.
    fn spawn_polling_task(&self) {
        let tracked_tokens = self.tracked_tokens.clone();
        let rpc_client = self.rpc_client.clone();
        let graduation_sender = self.graduation_sender.clone();
        let poll_interval = self.config.poll_interval_secs;
        let running = self.running.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut poll_timer = interval(Duration::from_secs(poll_interval));

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Graduation monitor received shutdown signal");
                        break;
                    }

                    _ = poll_timer.tick() => {
                        if !*running.read().await {
                            break;
                        }

                        Self::poll_bonding_curves(
                            &tracked_tokens,
                            &rpc_client,
                            &graduation_sender,
                        ).await;
                    }
                }
            }
        });
    }

    /// Stop the graduation monitor.
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping graduation monitor...");
        *self.running.write().await = false;
        let _ = self.shutdown_tx.send(());
        Ok(())
    }

    /// Check if the monitor is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Poll all tracked bonding curves.
    async fn poll_bonding_curves(
        tracked_tokens: &Arc<RwLock<HashMap<String, TrackedToken>>>,
        rpc_client: &RpcClient,
        graduation_sender: &mpsc::Sender<GraduationEvent>,
    ) {
        let tokens = tracked_tokens.read().await;
        let token_count = tokens.len();

        if token_count == 0 {
            return;
        }

        debug!("Polling {} bonding curves...", token_count);

        let mut graduated_mints = Vec::new();
        let mut updates: Vec<(String, f64, f64, f64)> = Vec::new(); // mint, progress, price, liquidity

        for (mint, tracked) in tokens.iter() {
            // Parse bonding curve address
            let bonding_curve = match Pubkey::from_str(&tracked.token.bonding_curve) {
                Ok(pk) => pk,
                Err(e) => {
                    debug!("Invalid bonding curve address for {}: {:?}", mint, e);
                    continue;
                }
            };

            // Fetch account data
            match rpc_client.get_account(&bonding_curve).await {
                Ok(account) => {
                    // Skip first 8 bytes (discriminator) when parsing
                    if account.data.len() > 8 {
                        let data = &account.data[8..];

                        if let Ok(state) = BondingCurveState::try_from_slice(data) {
                            let progress = state.get_progress_percent();
                            let price = state.get_price_sol();
                            let liquidity = state.get_liquidity_sol();

                            // Check for graduation
                            if state.complete || state.is_ready_to_graduate() {
                                info!(
                                    "🎓 TOKEN GRADUATED: {} ({})",
                                    tracked.token.name, tracked.token.symbol
                                );
                                info!("   Final SOL reserves: {} SOL", liquidity);
                                info!("   Final price: {} SOL", price);

                                graduated_mints.push(mint.clone());

                                let event = GraduationEvent {
                                    mint: mint.clone(),
                                    name: tracked.token.name.clone(),
                                    symbol: tracked.token.symbol.clone(),
                                    final_sol_reserves: state.real_sol_reserves,
                                    final_price_sol: price,
                                    graduated_at: chrono::Utc::now().timestamp(),
                                };

                                if let Err(e) = graduation_sender.send(event).await {
                                    error!("Failed to send graduation event: {:?}", e);
                                }
                            } else {
                                // Log progress if significantly changed
                                if (progress - tracked.last_progress).abs() > 5.0 {
                                    debug!(
                                        "{} ({}) - Progress: {:.1}%, Price: {:.10} SOL, Liquidity: {:.2} SOL",
                                        tracked.token.name, tracked.token.symbol,
                                        progress, price, liquidity
                                    );
                                }

                                updates.push((mint.clone(), progress, price, liquidity));
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch bonding curve for {}: {:?}", mint, e);
                }
            }
        }

        drop(tokens);

        // Apply updates
        if !updates.is_empty() {
            let mut tokens = tracked_tokens.write().await;
            for (mint, progress, price, liquidity) in updates {
                if let Some(tracked) = tokens.get_mut(&mint) {
                    tracked.last_progress = progress;
                    tracked.last_price_sol = price;
                    tracked.last_liquidity_sol = liquidity;
                }
            }
        }

        // Remove graduated tokens from tracking
        if !graduated_mints.is_empty() {
            let mut tokens = tracked_tokens.write().await;
            for mint in graduated_mints {
                tokens.remove(&mint);
                info!("Removed graduated token from tracking: {}", mint);
            }
        }
    }

    /// Get the number of currently tracked tokens.
    pub async fn get_tracked_count(&self) -> usize {
        self.tracked_tokens.read().await.len()
    }

    /// Get all currently tracked tokens with their state.
    pub async fn get_tracked_tokens(&self) -> Vec<PumpfunToken> {
        self.tracked_tokens
            .read()
            .await
            .values()
            .map(|t| {
                let mut token = t.token.clone();
                token.bonding_progress = t.last_progress;
                token.price_sol = t.last_price_sol;
                token.liquidity_sol = t.last_liquidity_sol;
                token
            })
            .collect()
    }

    /// Manually add a token to track (useful for testing).
    pub async fn track_token(&self, token: PumpfunToken) {
        let mut tokens = self.tracked_tokens.write().await;

        if tokens.len() >= self.config.max_tracked_tokens {
            warn!("Max tracked tokens reached, cannot add more");
            return;
        }

        let tracked = TrackedToken {
            token: token.clone(),
            last_progress: 0.0,
            last_price_sol: 0.0,
            last_liquidity_sol: 0.0,
            tracked_since: chrono::Utc::now().timestamp(),
        };

        tokens.insert(token.mint.clone(), tracked);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graduation_monitor_config_default() {
        let config = GraduationMonitorConfig::default();
        assert_eq!(config.poll_interval_secs, 10);
        assert_eq!(config.max_tracked_tokens, 100);
    }

    #[test]
    fn test_graduation_event() {
        let event = GraduationEvent {
            mint: "test_mint".to_string(),
            name: "Test Token".to_string(),
            symbol: "TEST".to_string(),
            final_sol_reserves: 85_000_000_000,
            final_price_sol: 0.0001,
            graduated_at: 1234567890,
        };

        assert_eq!(event.symbol, "TEST");
        assert_eq!(event.final_sol_reserves, 85_000_000_000);
    }
}
