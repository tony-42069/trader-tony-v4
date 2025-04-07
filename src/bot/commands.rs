use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, ParseMode,
};
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use tracing::{error, info, warn}; // Added warn

use crate::bot::keyboards; // Assuming keyboards module exists for callbacks
use crate::bot::BotState;
use crate::trading::strategy::Strategy; // Assuming Strategy struct exists

#[derive(BotCommands, Clone, Debug)] // Added Debug
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "Start the bot & show main menu")]
    Start,
    #[command(description = "Show this help message")]
    Help,
    #[command(description = "Show wallet balance")]
    Balance,
    #[command(description = "Control auto-trader status")]
    Autotrader,
    #[command(description = "View and manage strategies")]
    Strategy,
    #[command(description = "View current open positions")]
    Positions,
    #[command(description = "Analyze a token's risk")]
    Analyze { token_address: String },
    #[command(description = "Manually buy a token (snipe)")]
    // Temporarily simplify to one argument to check derive macro issue
    Snipe { token_address: String }, // Removed amount_sol for now
}

// --- Authorization Check ---

async fn is_authorized(state: &BotState, user_id: Option<UserId>) -> bool {
    match user_id {
        Some(id) => state.authorized_users.contains(&(id.0 as i64)),
        None => false,
    }
}

// --- Command Handler ---

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<Mutex<BotState>>, // Renamed bot_state to state for clarity
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let user_id = msg.from().map(|user| user.id);
    
    let locked_state = state.lock().await; // Lock state once

    // Check authorization
    if !is_authorized(&locked_state, user_id).await {
        warn!("Unauthorized access attempt by user: {:?}", user_id);
        bot.send_message(
            chat_id,
            "⚠️ You are not authorized to use this bot. Please contact the administrator."
        ).await?;
        return Ok(());
    }
    
    info!("Received command: {:?} from user: {:?}", cmd, user_id);

    // Drop the lock before potentially long-running async operations inside match arms
    // Clone necessary parts of the state if needed within the arms, or re-lock briefly.
    // For simplicity here, we keep the lock, but be mindful of this in complex handlers.
    
    match cmd {
        Command::Start => {
            let welcome_message = format!(
                "🤖 *Welcome to TraderTony V4*\n\nYour autonomous trading bot for Solana.\n\n\
                *Current Mode:* {}\n\
                *Wallet:* `{}`\n\n\
                Use /help to see commands or the buttons below.",
                if locked_state.config.demo_mode { "🧪 DEMO" } else { "🔴 REAL" },
                locked_state.wallet_manager.get_public_key(),
            );
            
            bot.send_message(chat_id, welcome_message)
                .parse_mode(ParseMode::Markdown)
                .reply_markup(keyboards::main_menu()) // Use keyboard from keyboards module
                .await?;
        },
        Command::Help => {
            bot.send_message(chat_id, Command::descriptions().to_string()).await?;
        },
        Command::Balance => {
             // Re-lock state briefly if needed, or pass cloned client/wallet
            let balance_result = locked_state.wallet_manager.get_sol_balance().await;
            
            match balance_result {
                Ok(balance) => {
                    let balance_message = format!(
                        "💰 *Wallet Balance*\n\n\
                        *Address:* `{}`\n\
                        *Balance:* {:.6} SOL", // Format SOL balance
                        locked_state.wallet_manager.get_public_key(),
                        balance
                    );
                    bot.send_message(chat_id, balance_message)
                        .parse_mode(ParseMode::Markdown)
                        .await?;
                }
                Err(e) => {
                     error!("Failed to get balance: {}", e);
                     bot.send_message(chat_id, format!("❌ Failed to retrieve balance: {}", e)).await?;
                }
            }
        },
        Command::Autotrader => {
            // Need to implement get_status on AutoTrader
            // let running = locked_state.auto_trader.lock().await.get_status().await; // Example lock if AutoTrader is Mutex guarded
            let running = false; // Placeholder
            
            let status_message = format!(
                "🤖 *AutoTrader Status*\n\n\
                *Status:* {}\n\
                *Mode:* {}",
                if running { "✅ Running" } else { "⏹️ Stopped" },
                if locked_state.config.demo_mode { "🧪 DEMO" } else { "🔴 REAL" }
            );
            
            bot.send_message(chat_id, status_message)
                .parse_mode(ParseMode::Markdown)
                .reply_markup(keyboards::autotrader_menu(running)) // Use keyboard
                .await?;
        },
        Command::Strategy => {
             // Need to implement list_strategies on AutoTrader
             // let strategies = locked_state.auto_trader.lock().await.list_strategies().await; // Example lock
             let strategies: Vec<Strategy> = vec![]; // Placeholder

            let strategies_message = if strategies.is_empty() {
                "No strategies configured yet.".to_string()
            } else {
                 // Format strategies... (implementation omitted for brevity)
                 "Existing strategies:\n- Strategy 1\n- Strategy 2".to_string() // Placeholder
            };
            
            bot.send_message(chat_id, strategies_message)
                .parse_mode(ParseMode::Markdown)
                .reply_markup(keyboards::strategy_menu()) // Use keyboard
                .await?;
        },
        Command::Positions => {
            // Need PositionManager and get_active_positions implementation
            // let positions = locked_state.auto_trader.lock().await.position_manager.get_active_positions().await; // Example lock
            let positions: Vec<()> = vec![]; // Placeholder

            let positions_message = if positions.is_empty() {
                "No active positions.".to_string()
            } else {
                // Format positions... (implementation omitted)
                "Active positions:\n- Position A\n- Position B".to_string() // Placeholder
            };
            
            bot.send_message(chat_id, positions_message)
                .parse_mode(ParseMode::Markdown)
                .reply_markup(keyboards::positions_menu()) // Use keyboard
                .await?;
        },
        Command::Analyze { token_address } => {
            bot.send_message(
                chat_id,
                format!("🔍 Analyzing token: `{}`\nPlease wait...", token_address)
            ).parse_mode(ParseMode::Markdown).await?;

            // Need RiskAnalyzer and analyze_token implementation
            // let analysis_result = locked_state.auto_trader.lock().await.risk_analyzer.analyze_token(&token_address).await; // Example lock
            let analysis_result: Result<(), _> = Err(()); // Placeholder

            match analysis_result {
                Ok(_analysis) => {
                    // Format analysis... (implementation omitted)
                    let analysis_message = format!("Analysis results for `{}`:\nRisk: Medium\nLiquidity: Good", token_address); // Placeholder
                    
                    bot.send_message(chat_id, analysis_message)
                        .parse_mode(ParseMode::Markdown)
                        // Add keyboard for actions like Snipe?
                        .await?;
                },
                Err(e) => {
                    error!("Error analyzing token {}: {:?}", token_address, e);
                    bot.send_message(
                        chat_id,
                        format!("❌ Error analyzing token `{}`.", token_address)
                    ).parse_mode(ParseMode::Markdown).await?;
                }
            }
        },
        // Adjust match arm for simplified Snipe command
        Command::Snipe { token_address } => {
            // Use default amount from config for now
            let amount = locked_state.config.max_position_size_sol;
            let min_amount = 0.001; // Example minimum
            // Max amount check might need refinement based on available budget, not just total
            let max_amount = locked_state.config.total_budget_sol;

            // Add check: Ensure default amount is within reasonable bounds
            if amount <= 0.0 {
                 error!("Default snipe amount (max_position_size_sol) is zero or negative in config.");
                 bot.send_message(chat_id, "❌ Internal configuration error: Invalid default snipe amount.").await?;
                 return Ok(());
            }


            if amount < min_amount || amount > max_amount { // Keep basic bounds check
                bot.send_message(
                    chat_id,
                    format!("❌ Invalid amount. Must be between {} and {} SOL.", min_amount, max_amount)
                ).await?;
                return Ok(());
            }
            
            bot.send_message(
                chat_id,
                format!("🎯 Preparing to snipe token: `{}` with {:.6} SOL...", token_address, amount)
            ).parse_mode(ParseMode::Markdown).await?;
            
            // TODO: Implement actual snipe logic within AutoTrader or a dedicated service
            // 1. Analyze token (reuse Analyze logic or parts of it)
            // 2. Check risk level against strategy/config
            // 3. Check balance
            // 4. Execute buy using JupiterClient via WalletManager
            // 5. Create position entry in PositionManager

            let snipe_result: Result<(), _> = Err(()); // Placeholder

            match snipe_result {
                 Ok(_) => {
                     bot.send_message(
                        chat_id,
                        format!("✅ Successfully sniped `{}` with {:.6} SOL!", token_address, amount)
                    ).parse_mode(ParseMode::Markdown).await?;
                 }
                 Err(e) => {
                     error!("Snipe failed for token {}: {:?}", token_address, e);
                     bot.send_message(
                        chat_id,
                        format!("❌ Snipe failed for token `{}`.", token_address)
                    ).parse_mode(ParseMode::Markdown).await?;
                 }
            }
        },
    }
    
    Ok(())
}
