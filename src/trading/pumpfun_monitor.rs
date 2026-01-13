// src/trading/pumpfun_monitor.rs
//
// Real-time Pump.fun token discovery using Helius WebSocket logsSubscribe.
// This replaces the broken DAS API approach that was returning NFT metadata.

use anyhow::{anyhow, Result};
use futures::StreamExt;
use solana_client::{
    nonblocking::pubsub_client::PubsubClient,
    rpc_config::RpcTransactionLogsConfig,
    rpc_response::{RpcLogsResponse, Response},
};
use solana_sdk::commitment_config::CommitmentConfig;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::trading::pumpfun::{
    derive_bonding_curve_ata, parse_create_event, PumpfunToken, PUMP_PROGRAM_ID,
};

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Configuration for the Pump.fun monitor
#[derive(Debug, Clone)]
pub struct PumpfunMonitorConfig {
    /// Helius WebSocket URL (include API key)
    pub websocket_url: String,
    /// Commitment level for subscriptions
    pub commitment: CommitmentConfig,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Reconnection delay in milliseconds
    pub reconnect_delay_ms: u64,
}

impl Default for PumpfunMonitorConfig {
    fn default() -> Self {
        Self {
            websocket_url: String::new(),
            commitment: CommitmentConfig::confirmed(),
            max_reconnect_attempts: 10,
            reconnect_delay_ms: 5000,
        }
    }
}

// ============================================================================
// PUMP.FUN MONITOR
// ============================================================================

/// Real-time Pump.fun token discovery monitor using WebSocket logsSubscribe.
///
/// This monitor:
/// 1. Connects to Helius WebSocket
/// 2. Subscribes to logs mentioning the Pump.fun program
/// 3. Parses "Program data:" logs to extract PumpCreateEvent
/// 4. Sends discovered tokens through a channel
pub struct PumpfunMonitor {
    config: PumpfunMonitorConfig,
    /// Channel to send discovered tokens
    token_sender: mpsc::Sender<PumpfunToken>,
    /// Flag to control the monitor loop
    running: Arc<RwLock<bool>>,
    /// Shutdown signal broadcaster
    shutdown_tx: broadcast::Sender<()>,
    /// Statistics
    stats: Arc<RwLock<MonitorStats>>,
}

/// Statistics for the monitor
#[derive(Debug, Default, Clone)]
pub struct MonitorStats {
    /// Total logs received
    pub logs_received: u64,
    /// Total tokens discovered
    pub tokens_discovered: u64,
    /// Total parse failures (expected for non-create events)
    pub parse_failures: u64,
    /// Total reconnection attempts
    pub reconnect_attempts: u32,
}

impl PumpfunMonitor {
    /// Create a new Pump.fun monitor.
    ///
    /// # Arguments
    /// * `helius_api_key` - Your Helius API key
    /// * `token_sender` - Channel to send discovered tokens
    pub fn new(helius_api_key: &str, token_sender: mpsc::Sender<PumpfunToken>) -> Self {
        let websocket_url = format!("wss://mainnet.helius-rpc.com/?api-key={}", helius_api_key);

        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config: PumpfunMonitorConfig {
                websocket_url,
                ..Default::default()
            },
            token_sender,
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
            stats: Arc::new(RwLock::new(MonitorStats::default())),
        }
    }

    /// Start the monitor (runs in background).
    ///
    /// Returns immediately after spawning the background task.
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(anyhow!("Pump.fun monitor is already running"));
        }
        *running = true;
        drop(running);

        info!("🚀 Starting Pump.fun token discovery monitor...");
        info!("📡 Subscribing to program: {}", PUMP_PROGRAM_ID);

        let config = self.config.clone();
        let token_sender = self.token_sender.clone();
        let running = self.running.clone();
        let stats = self.stats.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut reconnect_attempts = 0u32;

            loop {
                // Check if we should stop
                if !*running.read().await {
                    info!("Pump.fun monitor stopped by request");
                    break;
                }

                // Try to connect and subscribe
                match Self::run_subscription(&config, &token_sender, &stats, &mut shutdown_rx).await
                {
                    Ok(_) => {
                        info!("WebSocket subscription ended normally");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {:?}", e);
                        reconnect_attempts += 1;

                        {
                            let mut s = stats.write().await;
                            s.reconnect_attempts = reconnect_attempts;
                        }

                        if reconnect_attempts >= config.max_reconnect_attempts {
                            error!("Max reconnection attempts reached. Stopping Pump.fun monitor.");
                            *running.write().await = false;
                            break;
                        }

                        warn!(
                            "Reconnecting in {}ms (attempt {}/{})",
                            config.reconnect_delay_ms,
                            reconnect_attempts,
                            config.max_reconnect_attempts
                        );

                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            config.reconnect_delay_ms,
                        ))
                        .await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the monitor.
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping Pump.fun monitor...");
        *self.running.write().await = false;
        let _ = self.shutdown_tx.send(());
        Ok(())
    }

    /// Check if the monitor is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get current statistics.
    pub async fn get_stats(&self) -> MonitorStats {
        self.stats.read().await.clone()
    }

    /// Run the WebSocket subscription loop.
    async fn run_subscription(
        config: &PumpfunMonitorConfig,
        token_sender: &mpsc::Sender<PumpfunToken>,
        stats: &Arc<RwLock<MonitorStats>>,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Connecting to Helius WebSocket...");

        // Create WebSocket client
        let pubsub_client = PubsubClient::new(&config.websocket_url).await?;

        info!("✅ Connected! Subscribing to Pump.fun logs...");

        // Subscribe to logs mentioning the Pump.fun program
        let (mut logs_stream, unsubscribe) = pubsub_client
            .logs_subscribe(
                solana_client::rpc_config::RpcTransactionLogsFilter::Mentions(vec![
                    PUMP_PROGRAM_ID.to_string()
                ]),
                RpcTransactionLogsConfig {
                    commitment: Some(config.commitment),
                },
            )
            .await?;

        info!("✅ Subscribed! Listening for new Pump.fun tokens...");

        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal");
                    break;
                }

                // Process incoming logs
                log_result = logs_stream.next() => {
                    match log_result {
                        Some(log_response) => {
                            {
                                let mut s = stats.write().await;
                                s.logs_received += 1;
                            }

                            if let Err(e) = Self::process_log_response(
                                log_response,
                                token_sender,
                                stats
                            ).await {
                                debug!("Error processing log: {:?}", e);
                            }
                        }
                        None => {
                            warn!("Log stream ended unexpectedly");
                            break;
                        }
                    }
                }
            }
        }

        // Unsubscribe
        unsubscribe().await;

        Ok(())
    }

    /// Process a single log response from the WebSocket.
    async fn process_log_response(
        response: Response<RpcLogsResponse>,
        token_sender: &mpsc::Sender<PumpfunToken>,
        stats: &Arc<RwLock<MonitorStats>>,
    ) -> Result<()> {
        let log_response = response.value;

        // Skip failed transactions
        if log_response.err.is_some() {
            return Ok(());
        }

        let logs = &log_response.logs;
        let signature = &log_response.signature;

        // Look for "Instruction: Create" log to identify token creation
        let mut is_create_instruction = false;

        for log in logs {
            // Detect create instruction
            if log.contains("Program log: Instruction: Create") {
                is_create_instruction = true;
                debug!("Found Create instruction in tx: {}", signature);
            }

            // Extract event data (only if we're in a Create context)
            if is_create_instruction && log.starts_with("Program data: ") {
                let base64_data = log.trim_start_matches("Program data: ");

                match parse_create_event(base64_data) {
                    Some(event) => {
                        info!("🚀 NEW PUMP.FUN TOKEN DISCOVERED!");
                        info!("   Mint: {}", event.mint);
                        info!("   Name: {}", event.name);
                        info!("   Symbol: {}", event.symbol);
                        info!("   Creator: {}", event.user);
                        info!("   Bonding Curve: {}", event.bonding_curve);
                        info!("   TX: {}", signature);

                        // Derive the bonding curve ATA
                        let bonding_curve_ata =
                            derive_bonding_curve_ata(&event.bonding_curve, &event.mint);

                        let token = PumpfunToken {
                            mint: event.mint.to_string(),
                            name: event.name,
                            symbol: event.symbol,
                            uri: event.uri,
                            creator: event.user.to_string(),
                            bonding_curve: event.bonding_curve.to_string(),
                            bonding_curve_ata: bonding_curve_ata.to_string(),
                            discovered_at: chrono::Utc::now().timestamp(),
                            creation_signature: signature.clone(),
                            is_graduated: false,
                            bonding_progress: 0.0,
                            price_sol: 0.0,      // Will be updated from bonding curve state
                            liquidity_sol: 0.0,  // Will be updated from bonding curve state
                        };

                        // Update stats
                        {
                            let mut s = stats.write().await;
                            s.tokens_discovered += 1;
                        }

                        // Send to channel
                        if let Err(e) = token_sender.send(token).await {
                            error!("Failed to send token to channel: {:?}", e);
                        }
                    }
                    None => {
                        // Not a create event (could be Buy, Sell, etc.) - this is expected
                        let mut s = stats.write().await;
                        s.parse_failures += 1;
                    }
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_config_default() {
        let config = PumpfunMonitorConfig::default();
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.reconnect_delay_ms, 5000);
        assert!(config.websocket_url.is_empty());
    }

    #[tokio::test]
    async fn test_monitor_stats_default() {
        let stats = MonitorStats::default();
        assert_eq!(stats.logs_received, 0);
        assert_eq!(stats.tokens_discovered, 0);
        assert_eq!(stats.parse_failures, 0);
        assert_eq!(stats.reconnect_attempts, 0);
    }
}
