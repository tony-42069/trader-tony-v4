//! Telegram-driven sniper.
//!
//! Consumes messages from a Telegram channel listener, identifies call-outs,
//! executes a buy via Jupiter, and spawns a fast-exit task per position.

use serde::{Deserialize, Serialize};

/// A parsed call-out signal from the monitored Telegram channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallSignal {
    /// Solana mint address (base58, always ends with "pump" for pump.fun tokens).
    pub mint: String,
    /// Optional ticker symbol (e.g. "RETARD") extracted from "$TICKER" mention.
    pub ticker: Option<String>,
    /// The trigger keyword that fired ("Gamboled" or "Gamboling").
    pub trigger: String,
}

pub mod parser {
    use super::CallSignal;
    use regex::Regex;
    use std::sync::OnceLock;

    /// Regex for a pump.fun mint: base58 chars (excluding 0, O, I, l)
    /// of length 30-44, ending in literal "pump", on its own line.
    fn mint_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r"(?m)^\s*([1-9A-HJ-NP-Za-km-z]{30,40}pump)\s*$").unwrap()
        })
    }

    /// Regex for the trigger keyword: "Gamboled" or "Gamboling" at the very
    /// start of the message (case-insensitive, allowing leading whitespace).
    fn trigger_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r"^\s*(?i)(Gamboled|Gamboling)\b").unwrap()
        })
    }

    /// Regex for a ticker mention like "$RETARD".
    fn ticker_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\$([A-Z][A-Z0-9_]{1,15})\b").unwrap())
    }

    /// Parse a message body into a `CallSignal` if it matches the call pattern.
    ///
    /// Rules:
    /// 1. Message must start with "Gamboled" or "Gamboling" (case-insensitive).
    /// 2. Message must contain a pump.fun mint (base58, ends in "pump") on its
    ///    own line.
    /// 3. Returns the first matching mint and the first ticker mention found.
    pub fn parse_call_message(text: &str) -> Option<CallSignal> {
        let trigger_match = trigger_regex().captures(text)?;
        let trigger = trigger_match.get(1)?.as_str().to_string();

        let mint = mint_regex().captures(text)?.get(1)?.as_str().to_string();

        let ticker = ticker_regex()
            .captures(text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));

        Some(CallSignal { mint, ticker, trigger })
    }
}

use crate::api::jupiter::JupiterClient;
use crate::config::Config;
use crate::solana::wallet::WalletManager;
use crate::trading::position::PositionManager;
use crate::trading::strategy::Strategy;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

/// Pump.fun tokens use 9 decimals. This matches the assumption in the existing
/// `autotrader.rs` code path that processes Pump.fun tokens.
const PUMP_FUN_TOKEN_DECIMALS: u8 = 9;

/// Snipe orchestrator. Owns the references needed to buy + exit.
pub struct Sniper {
    pub config: Arc<Config>,
    pub jupiter: Arc<JupiterClient>,
    pub wallet: Arc<WalletManager>,
    pub position_manager: Arc<PositionManager>,
    pub strategy: Strategy,
}

impl Sniper {
    pub fn new(
        config: Arc<Config>,
        jupiter: Arc<JupiterClient>,
        wallet: Arc<WalletManager>,
        position_manager: Arc<PositionManager>,
        strategy: Strategy,
    ) -> Self {
        Self { config, jupiter, wallet, position_manager, strategy }
    }

    /// Consume call signals and fire snipes. Loops forever; returns only on
    /// receiver close.
    pub async fn run(self: Arc<Self>, mut rx: mpsc::Receiver<CallSignal>) {
        info!("🎯 Sniper running — waiting for Telegram calls");
        while let Some(signal) = rx.recv().await {
            let me = self.clone();
            tokio::spawn(async move {
                if let Err(e) = me.execute_snipe(signal).await {
                    error!("Snipe failed: {:?}", e);
                }
            });
        }
        warn!("Sniper signal channel closed — exiting");
    }

    /// Public entry point for one-shot snipe execution
    /// (called from AutoTrader's select! branch in Task 8).
    pub async fn execute_snipe_public(self: Arc<Self>, signal: CallSignal) -> Result<()> {
        self.execute_snipe(signal).await
    }

    /// One snipe lifecycle: buy → wait → dump 90% → record 10% moonbag in PositionManager.
    async fn execute_snipe(self: Arc<Self>, signal: CallSignal) -> Result<()> {
        let mint = &signal.mint;
        let amount_sol = self.config.snipe_amount_sol;
        let slippage_bps = self.config.snipe_slippage_bps;
        let priority_fee = Some(self.config.snipe_priority_fee_micro_lamports);
        let symbol_for_log = signal.ticker.as_deref().unwrap_or("?");

        info!(
            "🚨 SNIPE FIRING: trigger={} ticker={} mint={} amount={} SOL slippage={}bps",
            signal.trigger, symbol_for_log, mint, amount_sol, slippage_bps
        );

        // --- BUY ---
        let buy_start = std::time::Instant::now();
        let buy_result = self
            .jupiter
            .swap_sol_to_token(
                mint,
                PUMP_FUN_TOKEN_DECIMALS,
                amount_sol,
                slippage_bps,
                priority_fee,
                self.wallet.clone(),
            )
            .await
            .context("Jupiter buy failed")?;

        let buy_latency_ms = buy_start.elapsed().as_millis();
        let tokens_acquired = buy_result.actual_out_amount_ui.unwrap_or(buy_result.out_amount_ui);
        info!(
            "✅ Buy landed: tx={} latency={}ms acquired={:.6} tokens",
            buy_result.transaction_signature, buy_latency_ms, tokens_acquired
        );

        if tokens_acquired <= 0.0 {
            return Err(anyhow::anyhow!(
                "Buy succeeded (tx {}) but token amount is zero or negative — aborting fast-exit",
                buy_result.transaction_signature
            ));
        }

        // --- HOLD ---
        let exit_delay_ms = self.config.snipe_exit_delay_ms;
        let exit_percent = self.config.snipe_exit_percent.clamp(1, 100);
        info!(
            "⏱  Holding {} for {}ms before dumping {}%",
            mint, exit_delay_ms, exit_percent
        );
        sleep(Duration::from_millis(exit_delay_ms)).await;

        // --- DUMP 90% ---
        let dump_amount = tokens_acquired * (exit_percent as f64) / 100.0;
        let moonbag_amount = tokens_acquired - dump_amount;
        info!(
            "💸 Dumping {:.6} of {:.6} tokens ({}%) for {} — moonbag {:.6}",
            dump_amount, tokens_acquired, exit_percent, mint, moonbag_amount
        );

        let dump_result = self
            .jupiter
            .swap_token_to_sol(
                mint,
                PUMP_FUN_TOKEN_DECIMALS,
                dump_amount,
                slippage_bps,
                priority_fee,
                self.wallet.clone(),
            )
            .await;

        // --- RECORD MOONBAG ---
        // Regardless of whether dump succeeded, we have the moonbag (10%) in the
        // wallet. Record it in PositionManager so SL/TP/trailing applies.
        // entry_value_sol = the SOL we effectively spent on the moonbag share.
        let moonbag_entry_value_sol = amount_sol * (100 - exit_percent) as f64 / 100.0;

        match &dump_result {
            Ok(sell) => {
                info!(
                    "✅ Dump landed: tx={} sol_received={:.6}",
                    sell.transaction_signature,
                    sell.actual_out_amount_ui.unwrap_or(sell.out_amount_ui)
                );
            }
            Err(e) => {
                error!(
                    "❌ Dump FAILED for {}: {:?} — full position still in wallet, recording all as moonbag",
                    mint, e
                );
                // If dump failed, the WHOLE position is still in the wallet.
                // Adjust the moonbag amount/value to reflect that.
            }
        }

        let (final_token_amount, final_entry_value) = if dump_result.is_ok() {
            (moonbag_amount, moonbag_entry_value_sol)
        } else {
            // Dump failed: treat the entire bought amount as the position.
            (tokens_acquired, amount_sol)
        };

        if final_token_amount > 0.0 {
            if let Err(e) = self
                .position_manager
                .create_position(
                    mint,
                    signal.ticker.as_deref().unwrap_or(mint),
                    signal.ticker.as_deref().unwrap_or("?"),
                    PUMP_FUN_TOKEN_DECIMALS,
                    &self.strategy.id,
                    final_entry_value,
                    final_token_amount,
                    None,
                    buy_result.price_impact_pct,
                    &buy_result.transaction_signature,
                    self.strategy.stop_loss_percent,
                    self.strategy.take_profit_percent,
                    self.strategy.trailing_stop_percent,
                    Some(self.strategy.max_hold_time_minutes),
                )
                .await
            {
                warn!("create_position for moonbag failed: {:?}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parser::parse_call_message;

    const REAL_SAMPLE: &str = "Gamboled a bag here on $RETARD. Everybody in crypto feeling like this right now with the current market so heavy mindshare here, could be a runner. DYOR NFA.\n\nhttps://dexscreener.com/solana/J4kiZJMAge9qendsAfDeQXoanHqLHdR6RcCybeitcHJo\n\nhttps://retardcoin.wtf/\n\nhttps://x.com/OfficialRetardX\n\nhttps://t.me/BunchOfRetards\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";

    #[test]
    fn parses_the_real_retard_sample() {
        let signal = parse_call_message(REAL_SAMPLE).expect("should parse");
        assert_eq!(signal.mint, "ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump");
        assert_eq!(signal.ticker.as_deref(), Some("RETARD"));
        assert_eq!(signal.trigger, "Gamboled");
    }

    #[test]
    fn parses_gambling_present_tense() {
        // Mint uses only base58 chars (excludes 0, O, I, l).
        let msg = "Gamboling on $WIF\n\nHzAJ8x9QYpDsmZ3hRdWvL4kKbFntYg7uMcVjpump";
        let signal = parse_call_message(msg).expect("should parse");
        assert_eq!(signal.trigger, "Gamboling");
        assert_eq!(signal.ticker.as_deref(), Some("WIF"));
        assert!(signal.mint.ends_with("pump"));
    }

    #[test]
    fn rejects_message_without_trigger() {
        let msg = "Just thinking about $RETARD\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn rejects_message_without_mint() {
        let msg = "Gamboled on $RETARD but no contract address attached";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn rejects_non_pump_address() {
        // Valid base58 chars but ends in 'xyz' not 'pump' — should not match.
        let msg = "Gamboled\n\nHzAJ8x9QYpDsmZ3hRdWvL4kKbFntYg7uMcVjxyz";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn trigger_is_case_insensitive() {
        let msg = "GAMBOLED\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_some());
    }

    #[test]
    fn mint_inline_with_other_text_is_rejected() {
        // The mint must be on its own line. An inline mention isn't a call.
        let msg = "Gamboled on this token ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump btw";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn ticker_optional_signal_still_parses() {
        let msg = "Gamboled hard\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        let signal = parse_call_message(msg).expect("should parse without ticker");
        assert_eq!(signal.ticker, None);
        assert_eq!(signal.mint, "ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump");
    }

    #[test]
    fn trigger_must_be_at_start_not_buried() {
        // "Gamboled" appears but not as the first word — should NOT parse.
        let msg = "Yesterday I gamboled on something, today\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_none());
    }
}
