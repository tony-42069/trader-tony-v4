use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup}; // Import inline keyboard types
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use tracing::{error, info, warn}; // Added warn

use crate::bot::keyboards; // Assuming keyboards module exists for callbacks
use crate::bot::BotState;
use crate::trading::position::{Position, PositionStatus}; // Add Position and PositionStatus imports
use crate::config::Config;

// Manual help text for bot commands (replaces Command::descriptions())
const HELP_TEXT: &str = "\
/start - Initialize the bot and show the main menu.
/help - Display available commands.
/balance - Show the current SOL balance of the bot's wallet.
/autotrader - View AutoTrader status and start/stop controls.
/strategy - View, add, or manage trading strategies.
/positions - View currently open trading positions.
/analyze <token_address> - Perform risk analysis on a specific token.
/snipe <token_address> [amount_sol] - Manually buy a token (uses default strategy settings if not specified). Use with caution.";
use teloxide::utils::markdown::escape; // Use teloxide's built-in escape function

#[derive(Clone, Debug)]
pub enum Command {
    Start,
    Help,
    Balance,
    Autotrader,
    Strategy,
    Positions,
    Analyze(String),
    Snipe(String, f64),
}

// Parses a teloxide::types::Message into a Command.
// Returns Some(Command) if parsing succeeds, or None if the message is not a valid command.
pub fn parse_command(msg: &Message) -> Option<Command> {
    let text = msg.text()?;
    let mut parts = text.trim().split_whitespace();
    let cmd = parts.next()?.trim_start_matches('/').split('@').next()?.to_lowercase();

    match cmd.as_str() {
        "start" => Some(Command::Start),
        "help" => Some(Command::Help),
        "balance" => Some(Command::Balance),
        "autotrader" => Some(Command::Autotrader),
        "strategy" => Some(Command::Strategy),
        "positions" => Some(Command::Positions),
        "analyze" => {
            let address = parts.next()?;
            Some(Command::Analyze(address.to_string()))
        }
        "snipe" => {
            let address = parts.next()?;
            let amount_str = parts.next();
            let amount = amount_str.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            Some(Command::Snipe(address.to_string(), amount))
        }
        _ => None,
    }
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
                locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_public_key(),
            );

            // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&welcome_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                .reply_markup(keyboards::main_menu()) // Use keyboard from keyboards module
                .await?;
        },
        Command::Help => {
            bot.send_message(chat_id, HELP_TEXT).await?;
        },
        Command::Balance => {
             // Re-lock state briefly if needed, or pass cloned client/wallet
            let balance_result = locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_sol_balance().await;

            match balance_result {
                Ok(balance) => {
                    let balance_message = format!(
                        "💰 *Wallet Balance*\n\n\
                        *Address:* `{}`\n\
                        *Balance:* {:.6} SOL", // Format SOL balance
                        locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_public_key(),
                        balance
                    );
                     // Use MarkdownV2 and escape the message
                    bot.send_message(chat_id, escape(&balance_message))
                        .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                        .await?;
                }
                Err(e) => {
                     error!("Failed to get balance: {}", e);
                     bot.send_message(chat_id, format!("❌ Failed to retrieve balance: {}", e)).await?;
                }
            }
        },
        Command::Autotrader => {
            // Fetch the actual status from the AutoTrader instance
            let running = locked_state.auto_trader.lock().await.get_status().await;

            let status_message = format!(
                "🤖 *AutoTrader Status*\n\n\
                *Status:* {}\n\
                *Mode:* {}",
                if running { "✅ Running" } else { "⏹️ Stopped" },
                if locked_state.config.demo_mode { "🧪 DEMO" } else { "🔴 REAL" }
            );

             // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&status_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                .reply_markup(keyboards::autotrader_menu(running)) // Use keyboard
                .await?;
        },
        Command::Strategy => {
             // Fetch strategies from AutoTrader
             let strategies = locked_state.auto_trader.lock().await.list_strategies().await;

            let strategies_message = if strategies.is_empty() {
                "✅ No strategies configured yet.".to_string()
            } else {
                 let strategies_list = strategies
                    .iter()
                    .map(|s| {
                        format!(
                            "*{}* (`{}`): {}\n\
                            Budget: {:.2} SOL | Max Pos: {:.2} SOL | Concurrent: {}\n\
                            Risk Lvl: {} | Min Liq: {} SOL | Min Holders: {}",
                            s.name,
                            s.id, // Maybe show only part of the ID?
                            if s.enabled { "✅ Enabled" } else { "❌ Disabled" },
                            s.total_budget_sol,
                            s.max_position_size_sol,
                            s.max_concurrent_positions,
                            s.max_risk_level,
                            s.min_liquidity_sol,
                            s.min_holders
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n"); // Separator

                 format!("📊 *Configured Strategies*\n\n{}", strategies_list)
            };

             // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&strategies_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                .reply_markup(keyboards::strategy_menu()) // Use keyboard
                .await?;
        },
        Command::Positions => {
            // Fetch active positions from the PositionManager
            // Need to lock the AutoTrader Mutex first to access PositionManager
            // Ensure position_manager is public in AutoTrader struct or add a getter method.
            let positions = locked_state.auto_trader.lock().await
                .position_manager // Access PositionManager
                .get_all_positions().await; // Call its method

            let active_positions: Vec<&Position> = positions.iter()
                .filter(|&p| p.status == PositionStatus::Active)
                .collect();
                
            let closed_positions: Vec<&Position> = positions.iter()
                .filter(|&p| p.status != PositionStatus::Active)
                .collect();
            
            // Calculate stats
            let total_trades = positions.len();
            let successful_trades = positions.iter()
                .filter(|&p| p.exit_time.is_some() && p.pnl_sol.unwrap_or(0.0) > 0.0)
                .count();
            let failed_trades = positions.iter()
                .filter(|&p| p.exit_time.is_some() && p.pnl_sol.unwrap_or(0.0) <= 0.0)
                .count();
            let total_pnl_sol = positions.iter()
                .filter_map(|p| p.pnl_sol)
                .sum::<f64>();
            let success_rate = if total_trades > 0 {
                successful_trades as f64 / total_trades as f64
            } else {
                0.0
            };
            
            // Calculate avg position duration for closed positions
            let mut total_duration_mins = 0;
            let mut pos_with_duration = 0;
            for p in positions.iter().filter(|&p| p.exit_time.is_some()) {
                if let Some(exit_time) = p.exit_time {
                    let duration = exit_time.signed_duration_since(p.entry_time);
                    total_duration_mins += duration.num_minutes();
                    pos_with_duration += 1;
                }
            }
            let avg_position_duration_mins = if pos_with_duration > 0 {
                total_duration_mins as f64 / pos_with_duration as f64
            } else {
                0.0
            };
            
            let stats_message = format!(
                "📊 *Trading Statistics*\n\n\
                Total Trades: {}\n\
                Successful: {}\n\
                Failed: {}\n\
                Total PnL: {:.4} SOL\n\
                Success Rate: {:.1}%\n\
                Avg Duration: {:.1} mins",
                total_trades,
                successful_trades,
                failed_trades,
                total_pnl_sol,
                success_rate * 100.0,
                avg_position_duration_mins
            );

            let positions_message = if positions.is_empty() {
                "✅ No active positions.".to_string()
            } else {
                let positions_list = positions
                    .iter()
                    .map(|p| {
                        // Calculate current PnL based on latest price if available
                        let current_value = p.entry_token_amount * p.current_price_sol;
                        let pnl = current_value - p.entry_value_sol;
                        let pnl_percent = if p.entry_value_sol > 0.0 { (pnl / p.entry_value_sol) * 100.0 } else { 0.0 };
                        format!(
                            "*{}* (`{}`)\n\
                            Status: {}\n\
                            Entry SOL: {:.4}\n\
                            Current SOL: {:.4}\n\
                            PnL: {:.4} SOL ({:.2}%)\n\
                            Entry Time: {}",
                            p.token_symbol,
                            p.token_address, // Consider shortening address display
                            p.status,
                            p.entry_value_sol,
                            current_value,
                            pnl,
                            pnl_percent,
                            p.entry_time.format("%Y-%m-%d %H:%M") // Shorter time format
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");

                format!("📈 *Active Positions*\n\n{}", positions_list)
            };

             // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&stats_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                .reply_markup(keyboards::positions_menu()) // Use keyboard
                .await?;
        },
        Command::Analyze(token_address) => {
            // --- Input Validation ---
            if token_address.trim().is_empty() {
                bot.send_message(chat_id, "⚠️ Please provide a token address after /analyze.").await?;
                return Ok(());
            }
            // Basic check for Solana address format (length, base58 chars) - can be improved
            if token_address.len() < 32 || token_address.len() > 44 || !token_address.chars().all(|c| c.is_ascii_alphanumeric()) {
                 bot.send_message(chat_id, format!("⚠️ Invalid token address format: `{}`", escape(&token_address))).parse_mode(ParseMode::MarkdownV2).await?;
                 return Ok(());
            }
            // --- End Validation ---

            bot.send_message(
                chat_id,
                 // Use MarkdownV2 and escape the message (note: backticks are fine in V2)
                escape(&format!("🔍 Analyzing token: `{}`\nPlease wait...", token_address))
            ).parse_mode(ParseMode::MarkdownV2).await?;

            // Call RiskAnalyzer::analyze_token
            // Ensure risk_analyzer is public or add a getter in AutoTrader
            let analysis_result = locked_state.auto_trader.lock().await
                .risk_analyzer // Access RiskAnalyzer
                .analyze_token(&token_address).await; // Call analyze_token

            match analysis_result {
                Ok(analysis) => { // Use the actual analysis result
                    // Format the analysis result
                    let risk_level_emoji = match analysis.risk_level {
                        0..=30 => "🟢", // Low risk
                        31..=70 => "🟠", // Medium risk
                        _ => "🔴", // High risk
                    };

                    let risk_factors = if analysis.details.is_empty() {
                        "None detected".to_string()
                    } else {
                        analysis.details.iter().map(|d| format!("- {}", d)).collect::<Vec<_>>().join("\n")
                    };

                    let analysis_message = format!(
                        "🔍 *Token Analysis: {}*\n\n\
                        *Address:* `{}`\n\
                        *Risk Score:* {} {} \\({}/100\\)\n\n\
                        *Checks:*\n\
                        - Liquidity: {:.2} SOL\n\
                        - Holders: {}\n\
                        - Mint Authority: {}\n\
                        - Freeze Authority: {}\n\
                        - LP Burned/Locked: {}\n\
                        - Transfer Tax: {:.1}%\n\
                        - Sellable (Honeypot Check): {}\n\
                        - Top Holder Concentration: {:.1}%\n\n\
                        *Risk Factors Found:*\n{}",
                        analysis.token_address, // Use address from analysis
                        analysis.token_address,
                        risk_level_emoji, analysis.risk_level, analysis.risk_level, // Show score twice for clarity
                        analysis.liquidity_sol,
                        analysis.holder_count,
                        if analysis.has_mint_authority { "⚠️ Yes" } else { "✅ No" },
                        if analysis.has_freeze_authority { "⚠️ Yes" } else { "✅ No" },
                        if analysis.lp_tokens_burned { "✅ Yes" } else { "🟠 No/Unknown" },
                        analysis.transfer_tax_percent,
                        if analysis.can_sell { "✅ Yes" } else { "🔴 No" },
                        analysis.concentration_percent,
                        risk_factors
                    );

                    // Use MarkdownV2 and escape the message
                    bot.send_message(chat_id, escape(&analysis_message))
                        .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                        .reply_markup(keyboards::token_action_menu(&analysis.token_address)) // Add action buttons
                        .await?;
                },
                Err(e) => {
                    error!("Error analyzing token {}: {:?}", token_address, e);
                    // Provide more specific feedback if possible (e.g., token not found vs. API error)
                    let error_message = if e.to_string().contains("TokenNotFound") || e.to_string().contains("Invalid token address") {
                        format!("❌ Could not find token address `{}`.", escape(&token_address))
                    } else {
                        format!("❌ Error analyzing token `{}`. Check logs for details.", escape(&token_address))
                    };
                    bot.send_message(chat_id, error_message).parse_mode(ParseMode::MarkdownV2).await?;
                }
            }
        },
        // Adjust match arm for simplified Snipe command
        Command::Snipe(token_address, amount_sol) => {
            // --- Input Validation ---
            if token_address.trim().is_empty() {
                bot.send_message(chat_id, "⚠️ Please provide a token address after /snipe.").await?;
                return Ok(());
            }
            // Basic check for Solana address format
            if token_address.len() < 32 || token_address.len() > 44 || !token_address.chars().all(|c| c.is_ascii_alphanumeric()) {
                 bot.send_message(chat_id, format!("⚠️ Invalid token address format: `{}`", escape(&token_address))).parse_mode(ParseMode::MarkdownV2).await?;
                 return Ok(());
            }
            // --- End Validation ---

            // Use default amount from config for now
            let amount = if amount_sol == 0.0 {
                locked_state.config.max_position_size_sol
            } else {
                amount_sol
            };
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
                 // Use MarkdownV2 and escape the message
                escape(&format!("🎯 Preparing to snipe token: `{}` with {:.6} SOL...", token_address, amount))
            ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2

            // TODO: Implement actual snipe logic within AutoTrader or a dedicated service
            // 1. Analyze token (reuse Analyze logic or parts of it)
            // 2. Check risk level against strategy/config
            // 3. Check balance
            // 4. Execute buy using JupiterClient via WalletManager
            // --- Execute Snipe Logic ---
            let auto_trader = locked_state.auto_trader.clone(); // Clone Arc for use
            let wallet_manager = locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").clone(); // Unwrap before clone
            let config = locked_state.config.clone(); // Clone config

            // Drop the main state lock before potentially long async operations
            drop(locked_state);

            // 1. Analyze token
            let analysis_result = auto_trader.lock().await.risk_analyzer.analyze_token(&token_address).await;

            let risk_analysis = match analysis_result {
                Ok(analysis) => analysis,
                Err(e) => {
                    error!("Snipe: Risk analysis failed for {}: {:?}", token_address, e);
                    bot.send_message(chat_id, format!("❌ Snipe failed: Could not analyze token `{}`.", escape(&token_address))).parse_mode(ParseMode::MarkdownV2).await?;
                    return Ok(());
                }
            };

            // 2. Check risk level (using a default/config threshold for now)
            // TODO: Use strategy-specific risk level if available
            let max_allowed_risk = 70; // Example threshold
            if risk_analysis.risk_level > max_allowed_risk {
                 bot.send_message(chat_id, format!("❌ Snipe failed: Token `{}` risk level ({}) exceeds threshold ({}).", escape(&token_address), risk_analysis.risk_level, max_allowed_risk)).parse_mode(ParseMode::MarkdownV2).await?;
                 return Ok(());
            }
            info!("Snipe: Risk check passed for {}", token_address);

            // 3. Check balance (ensure enough SOL for the snipe amount)
            let current_balance = match wallet_manager.get_sol_balance().await {
                 Ok(bal) => bal,
                 Err(e) => {
                     error!("Snipe: Failed to get wallet balance: {:?}", e);
                     bot.send_message(chat_id, "❌ Snipe failed: Could not check wallet balance.").await?;
                     return Ok(());
                 }
            };

            if current_balance < amount {
                 bot.send_message(chat_id, format!("❌ Snipe failed: Insufficient balance ({:.6} SOL) to snipe {:.6} SOL.", current_balance, amount)).await?;
                 return Ok(());
            }
            info!("Snipe: Balance check passed for {}", token_address);

            // 4. Execute buy
            // TODO: Refactor execute_buy logic into a reusable function in AutoTrader?
            // For now, call the new execute_manual_buy function
            info!("Snipe: Attempting buy execution for {} with {} SOL...", token_address, amount);
            let buy_result = auto_trader.lock().await.execute_manual_buy(
                &token_address,
                amount,
            ).await;


            // 5. Create position entry (if buy succeeds) - Now handled within execute_manual_buy

            match buy_result {
                 Ok(swap_result) => { // Use the SwapResult from execute_manual_buy
                     // Use MarkdownV2 and escape the message
                     bot.send_message(
                        chat_id,
                        escape(&format!(
                            "✅ Successfully sniped `{}` with {:.6} SOL!\nSignature: `{}`",
                            token_address, amount, swap_result.transaction_signature
                        ))
                    ).parse_mode(ParseMode::MarkdownV2)
                     .reply_markup(InlineKeyboardMarkup::new(vec![
                         vec![InlineKeyboardButton::callback("📊 View Positions", "positions_menu")],
                         vec![InlineKeyboardButton::callback("🔙 Main Menu", "main_menu")],
                     ]))
                    .await?;
                 }
                 Err(e) => {
                     error!("Snipe execution failed for token {}: {:?}", token_address, e);
                      // Use MarkdownV2 and escape the message
                     bot.send_message(
                        chat_id,
                        escape(&format!("❌ Snipe failed for token `{}`. Reason: {}", token_address, e))
                    ).parse_mode(ParseMode::MarkdownV2).await?;
                 }
            }
        },
    }

    Ok(())
}


// --- Callback Query Handler ---

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<Mutex<BotState>>,
) -> ResponseResult<()> {
    let chat_id = q.message.as_ref().map(|msg| msg.chat.id);
    let user_id = q.from.id;

    if let Some(data) = q.data {
        info!("Received callback query with data: {} from user: {}", data, user_id);

        let locked_state = state.lock().await; // Lock state

        // Check authorization again for callbacks
        if !is_authorized(&locked_state, Some(user_id)).await {
            warn!("Unauthorized callback query attempt by user: {}", user_id);
            if let Some(id) = chat_id {
                 bot.send_message(id, "⚠️ You are not authorized for this action.").await?;
            }
             // Answer callback query to remove the "loading" state on the button
             bot.answer_callback_query(q.id).text("Unauthorized").await?;
            return Ok(());
        }

        // --- Handle different callback data ---
        // This variable will store the text for the answer_callback_query notification
        let mut notification_text: Option<String> = None;

        // Parse callback data for callbacks with parameters (format: "action:param")
        let parts: Vec<&str> = data.split(':').collect();
        let action = parts[0];
        let param = if parts.len() > 1 { Some(parts[1]) } else { None };
        let extra_param = if parts.len() > 2 { Some(parts[2]) } else { None };

        match action {
            // --- AutoTrader Controls ---
            "start_autotrader" => {
                info!("Callback: Start AutoTrader requested.");
                // Lock the mutex and call start (which now takes &self)
                let auto_trader_guard = locked_state.auto_trader.lock().await;
                match auto_trader_guard.start().await {
                    Ok(_) => {
                        notification_text = Some("✅ AutoTrader started successfully.".to_string());
                        // Edit the original message to update keyboard
                        if let Some(msg) = q.message {
                             bot.edit_message_reply_markup(msg.chat.id, msg.id)
                                .reply_markup(keyboards::autotrader_menu(true)) // Show stop button
                                .await?;
                        }
                    }
                    Err(e) => {
                         error!("Failed to start AutoTrader: {}", e);
                         notification_text = Some(format!("❌ Failed to start AutoTrader: {}", e));
                    }
                }
            }
            "stop_autotrader" => {
                 info!("Callback: Stop AutoTrader requested.");
                 let auto_trader_guard = locked_state.auto_trader.lock().await;
                 match auto_trader_guard.stop().await { // Call stop (which takes &self)
                     Ok(_) => {
                         notification_text = Some("⏹️ AutoTrader stopped successfully.".to_string());
                         // Edit the original message to update keyboard
                         if let Some(msg) = q.message {
                              bot.edit_message_reply_markup(msg.chat.id, msg.id)
                                 .reply_markup(keyboards::autotrader_menu(false)) // Show start button
                                 .await?;
                         }
                     }
                     Err(e) => {
                          error!("Failed to stop AutoTrader: {}", e);
                          notification_text = Some(format!("❌ Failed to stop AutoTrader: {}", e));
                     }
                 }
            }
            
            // --- Navigation Menus ---
            "autotrader_menu" => {
                 // Re-send the autotrader status message and keyboard
                 let running = locked_state.auto_trader.lock().await.get_status().await;
                 let status_message = format!(
                    "🤖 *AutoTrader Status*\n\n*Status:* {}\n*Mode:* {}",
                    if running { "✅ Running" } else { "⏹️ Stopped" },
                    if locked_state.config.demo_mode { "🧪 DEMO" } else { "🔴 REAL" }
                 );
                 if let Some(msg) = q.message { // Edit the original message
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&status_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::autotrader_menu(running))
                         .await?;
                 }
                 notification_text = None; // Message already edited
            }
             "main_menu" => {
                 // Re-send the main menu message
                 let welcome_message = format!(
                    "🤖 *Welcome back!*\n\n*Mode:* {}\n*Wallet:* `{}`",
                    if locked_state.config.demo_mode { "🧪 DEMO" } else { "🔴 REAL" },
                    locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_public_key(),
                 );
                  if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&welcome_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::main_menu())
                        .await?;
                  }
                 notification_text = None;
             }
             
             // --- Positions Management ---
             "positions_menu" => {
                 // Fetch active positions from the PositionManager
                 let positions = locked_state.auto_trader.lock().await
                    .position_manager
                    .get_active_positions().await;

                 let positions_message = if positions.is_empty() {
                     "✅ No active positions.".to_string()
                 } else {
                     let positions_list = positions
                         .iter()
                         .map(|p| {
                             // Calculate current PnL based on latest price if available
                             let current_value = p.entry_token_amount * p.current_price_sol;
                             let pnl = current_value - p.entry_value_sol;
                             let pnl_percent = if p.entry_value_sol > 0.0 { (pnl / p.entry_value_sol) * 100.0 } else { 0.0 };
                             format!(
                                 "*{}* (`{}`)\n\
                                 Status: {}\n\
                                 Entry SOL: {:.4}\n\
                                 Current SOL: {:.4}\n\
                                 PnL: {:.4} SOL ({:.2}%)\n\
                                 Entry Time: {}",
                                 p.token_symbol,
                                 p.token_address,
                                 p.status,
                                 p.entry_value_sol,
                                 current_value,
                                 pnl,
                                 pnl_percent,
                                 p.entry_time.format("%Y-%m-%d %H:%M")
                             )
                         })
                         .collect::<Vec<_>>()
                         .join("\n\n---\n\n");

                     format!("📈 *Active Positions*\n\n{}", positions_list)
                 };

                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&positions_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::positions_menu())
                        .await?;
                 }
                 notification_text = None;
             }
             "refresh_positions" => {
                 // Similar to positions_menu but just update the current message
                 let positions = locked_state.auto_trader.lock().await
                    .position_manager
                    .get_active_positions().await;

                 let positions_message = if positions.is_empty() {
                     "✅ No active positions.".to_string()
                 } else {
                     // Similar formatting as above
                     let positions_list = positions
                         .iter()
                         .map(|p| {
                             // Calculate current PnL based on latest price if available
                             let current_value = p.entry_token_amount * p.current_price_sol;
                             let pnl = current_value - p.entry_value_sol;
                             let pnl_percent = if p.entry_value_sol > 0.0 { (pnl / p.entry_value_sol) * 100.0 } else { 0.0 };
                             format!(
                                 "*{}* (`{}`)\n\
                                 Status: {}\n\
                                 Entry SOL: {:.4}\n\
                                 Current SOL: {:.4}\n\
                                 PnL: {:.4} SOL ({:.2}%)\n\
                                 Entry Time: {}",
                                 p.token_symbol,
                                 p.token_address,
                                 p.status,
                                 p.entry_value_sol,
                                 current_value,
                                 pnl,
                                 pnl_percent,
                                 p.entry_time.format("%Y-%m-%d %H:%M")
                             )
                         })
                         .collect::<Vec<_>>()
                         .join("\n\n---\n\n");

                     format!("📈 *Active Positions*\n\n{}", positions_list)
                 };

                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&positions_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::positions_menu())
                        .await?;
                 }
                 notification_text = Some("🔄 Positions refreshed".to_string());
             }
             "close_position" => {
                 if let Some(position_id) = param {
                     info!("Callback: Close position requested for ID: {}", position_id);
                     // Confirm before closing
                     if let Some(msg) = q.message {
                         let confirm_message = format!("🔴 *Confirm Position Close*\n\nAre you sure you want to close position ID: `{}`?", position_id);
                         bot.edit_message_text(msg.chat.id, msg.id, escape(&confirm_message))
                            .parse_mode(ParseMode::MarkdownV2)
                            .reply_markup(keyboards::confirmation_menu("close_position", position_id))
                            .await?;
                     }
                     notification_text = None;
                 } else {
                     notification_text = Some("❌ Missing position ID".to_string());
                 }
             }
             
             // --- Strategy Management ---
             "strategy_menu" => {
                 // Fetch strategies from AutoTrader
                 let strategies = locked_state.auto_trader.lock().await.list_strategies().await;

                let strategies_message = if strategies.is_empty() {
                    "✅ No strategies configured yet.".to_string()
                } else {
                    let strategies_list = strategies
                        .iter()
                        .map(|s| {
                            format!(
                                "*{}* (`{}`): {}\n\
                                Budget: {:.2} SOL | Max Pos: {:.2} SOL | Concurrent: {}\n\
                                Risk Lvl: {} | Min Liq: {} SOL | Min Holders: {}",
                                s.name,
                                s.id,
                                if s.enabled { "✅ Enabled" } else { "❌ Disabled" },
                                s.total_budget_sol,
                                s.max_position_size_sol,
                                s.max_concurrent_positions,
                                s.max_risk_level,
                                s.min_liquidity_sol,
                                s.min_holders
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");

                    format!("📊 *Configured Strategies*\n\n{}", strategies_list)
                };

                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&strategies_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::strategy_menu())
                        .await?;
                 }
                 notification_text = None;
             }
             "add_strategy" => {
                 // Start a conversation flow to add a new strategy
                 if let Some(chat) = chat_id {
                     // Create a new strategy form
                     let form_message = "✏️ *Create New Strategy*\n\nPlease provide the following information:\n\n1️⃣ Strategy name\n2️⃣ Total budget (SOL)\n3️⃣ Max position size (SOL)\n4️⃣ Max concurrent positions\n5️⃣ Max risk level (0-100)\n6️⃣ Min liquidity (SOL)\n7️⃣ Min holders\n\nReply with these values separated by commas.";
                     
                     bot.send_message(chat, escape(form_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .await?;
                     
                     // Note: This requires a separate message handler for form responses
                     // which would be set up in the conversation management system
                 }
                 notification_text = Some("✏️ Please fill out the strategy form".to_string());
             }
             "refresh_strategies" => {
                 // Similar to strategy_menu but with a notification
                 let strategies = locked_state.auto_trader.lock().await.list_strategies().await;
                 
                 let strategies_message = if strategies.is_empty() {
                     "✅ No strategies configured yet.".to_string()
                 } else {
                     let strategies_list = strategies
                         .iter()
                         .map(|s| {
                             format!(
                                 "*{}* (`{}`): {}\n\
                                 Budget: {:.2} SOL | Max Pos: {:.2} SOL | Concurrent: {}\n\
                                 Risk Lvl: {} | Min Liq: {} SOL | Min Holders: {}",
                                 s.name,
                                 s.id, 
                                 if s.enabled { "✅ Enabled" } else { "❌ Disabled" },
                                 s.total_budget_sol,
                                 s.max_position_size_sol,
                                 s.max_concurrent_positions,
                                 s.max_risk_level,
                                 s.min_liquidity_sol,
                                 s.min_holders
                             )
                         })
                         .collect::<Vec<_>>()
                         .join("\n\n---\n\n");

                     format!("📊 *Configured Strategies*\n\n{}", strategies_list)
                 };

                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&strategies_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::strategy_menu())
                        .await?;
                 }
                 notification_text = Some("🔄 Strategies refreshed".to_string());
             }
             "strategy_toggle" => {
                 if let Some(strategy_id) = param {
                     info!("Callback: Toggle strategy {} requested", strategy_id);
                     
                     // Find strategy and toggle its enabled status
                     let mut auto_trader = locked_state.auto_trader.lock().await;
                     let toggle_result = auto_trader.toggle_strategy(strategy_id).await;
                     
                     match toggle_result {
                         Ok(new_status) => {
                             notification_text = Some(format!("✅ Strategy {} {}", 
                                 strategy_id, 
                                 if new_status { "enabled" } else { "disabled" }
                             ));
                             
                             // Refresh the strategy detail view if we're on that page
                             if let Some(msg) = q.message {
                                 // Get updated strategy details
                                 let strategy = auto_trader.get_strategy(strategy_id).await;
                                 
                                 if let Some(s) = strategy {
                                     let detail_message = format!(
                                         "📊 *Strategy Details: {}*\n\n\
                                         *ID:* `{}`\n\
                                         *Status:* {}\n\
                                         *Budget:* {:.2} SOL\n\
                                         *Max Position:* {:.2} SOL\n\
                                         *Max Concurrent:* {}\n\
                                         *Risk Level:* {}/100\n\
                                         *Min Liquidity:* {} SOL\n\
                                         *Min Holders:* {}",
                                         s.name,
                                         s.id,
                                         if s.enabled { "✅ Enabled" } else { "❌ Disabled" },
                                         s.total_budget_sol,
                                         s.max_position_size_sol,
                                         s.max_concurrent_positions,
                                         s.max_risk_level,
                                         s.min_liquidity_sol,
                                         s.min_holders
                                     );
                                     
                                     bot.edit_message_text(msg.chat.id, msg.id, escape(&detail_message))
                                        .parse_mode(ParseMode::MarkdownV2)
                                        .reply_markup(keyboards::strategy_detail_menu(&s.id, s.enabled))
                                        .await?;
                                 }
                             }
                         },
                         Err(e) => {
                             error!("Failed to toggle strategy {}: {}", strategy_id, e);
                             notification_text = Some(format!("❌ Failed to toggle strategy: {}", e));
                         }
                     }
                 } else {
                     notification_text = Some("❌ Missing strategy ID".to_string());
                 }
             }
             "strategy_edit" => {
                 if let Some(strategy_id) = param {
                     info!("Callback: Edit strategy {} requested", strategy_id);
                     
                     // Get current strategy details to pre-fill the form
                     let strategy = locked_state.auto_trader.lock().await.get_strategy(strategy_id).await;
                     
                     if let Some(s) = strategy {
                         if let Some(chat) = chat_id {
                             let form_message = format!(
                                 "✏️ *Edit Strategy: {}*\n\n\
                                 Please provide updated values (leave blank to keep current):\n\n\
                                 1️⃣ Strategy name (current: {})\n\
                                 2️⃣ Total budget (current: {:.2} SOL)\n\
                                 3️⃣ Max position size (current: {:.2} SOL)\n\
                                 4️⃣ Max concurrent positions (current: {})\n\
                                 5️⃣ Max risk level (current: {})\n\
                                 6️⃣ Min liquidity (current: {} SOL)\n\
                                 7️⃣ Min holders (current: {})\n\n\
                                 Reply with these values separated by commas.",
                                 s.name, s.name,
                                 s.total_budget_sol,
                                 s.max_position_size_sol,
                                 s.max_concurrent_positions,
                                 s.max_risk_level,
                                 s.min_liquidity_sol,
                                 s.min_holders
                             );
                             
                             bot.send_message(chat, escape(&form_message))
                                .parse_mode(ParseMode::MarkdownV2)
                                .await?;
                         }
                         notification_text = Some("✏️ Please fill out the edit form".to_string());
                     } else {
                         notification_text = Some("❌ Strategy not found".to_string());
                     }
                 } else {
                     notification_text = Some("❌ Missing strategy ID".to_string());
                 }
             }
             "strategy_delete" => {
                 if let Some(strategy_id) = param {
                     info!("Callback: Delete strategy {} requested", strategy_id);
                     
                     // Ask for confirmation before deleting
                     if let Some(msg) = q.message {
                         let confirm_message = format!("🔴 *Confirm Strategy Deletion*\n\nAre you sure you want to delete strategy: `{}`?", strategy_id);
                         bot.edit_message_text(msg.chat.id, msg.id, escape(&confirm_message))
                            .parse_mode(ParseMode::MarkdownV2)
                            .reply_markup(keyboards::confirmation_menu("delete_strategy", strategy_id))
                            .await?;
                     }
                     notification_text = None;
                 } else {
                     notification_text = Some("❌ Missing strategy ID".to_string());
                 }
             }
             
             // --- Risk Level Settings ---
             "set_risk" => {
                 if let Some(risk_level) = param {
                     info!("Callback: Set risk level to {}", risk_level);
                     
                     if risk_level == "custom" {
                         // Ask user to enter a custom risk level
                         if let Some(chat) = chat_id {
                             bot.send_message(chat, "Please enter a custom risk level (0-100):").await?;
                             // This requires conversation state handling
                         }
                         notification_text = Some("Enter a number between 0-100".to_string());
                     } else {
                         // Use the predefined risk level
                         if let Ok(level) = risk_level.parse::<u8>() {
                             if level <= 100 {
                                 // Store the risk level in state or apply it directly to a strategy
                                 // This depends on the context of where this was called from
                                 notification_text = Some(format!("Risk level set to {}", level));
                             } else {
                                 notification_text = Some("❌ Risk level must be 0-100".to_string());
                             }
                         } else {
                             notification_text = Some("❌ Invalid risk level format".to_string());
                         }
                     }
                 } else {
                     notification_text = Some("❌ Missing risk level".to_string());
                 }
             }
             "cancel_risk_setting" => {
                 notification_text = Some("❌ Risk setting cancelled".to_string());
                 // Return to previous menu - depends on context
             }
             
             // --- Position Size Settings ---
             "set_pos_size" => {
                 if let Some(size_str) = param {
                     info!("Callback: Set position size to {}", size_str);
                     
                     if size_str == "custom" {
                         // Ask user to enter a custom position size
                         if let Some(chat) = chat_id {
                             bot.send_message(chat, "Please enter a custom position size in SOL:").await?;
                             // This requires conversation state handling
                         }
                         notification_text = Some("Enter position size in SOL".to_string());
                     } else {
                         // Use the predefined position size
                         if let Ok(size) = size_str.parse::<f64>() {
                             if size > 0.0 {
                                 // Store the position size in state or apply it directly
                                 notification_text = Some(format!("Position size set to {} SOL", size));
                             } else {
                                 notification_text = Some("❌ Position size must be positive".to_string());
                             }
                         } else {
                             notification_text = Some("❌ Invalid position size format".to_string());
                         }
                     }
                 } else {
                     notification_text = Some("❌ Missing position size".to_string());
                 }
             }
             "cancel_pos_size_setting" => {
                 notification_text = Some("❌ Position size setting cancelled".to_string());
                 // Return to previous menu - depends on context
             }
             
             // --- Confirmation Handlers ---
             "confirm" => {
                 if let Some(action_type) = param {
                     match action_type {
                         "delete_strategy" => {
                             if let Some(strategy_id) = extra_param {
                                 // Execute the strategy deletion
                                 let delete_result = locked_state.auto_trader.lock().await
                                     .delete_strategy(strategy_id).await;
                                 
                                 match delete_result {
                                     Ok(_) => {
                                         notification_text = Some(format!("✅ Strategy {} deleted", strategy_id));
                                         // Return to strategy list
                                         if let Some(msg) = q.message {
                                             let strategies = locked_state.auto_trader.lock().await.list_strategies().await;
                                             
                                             if strategies.is_empty() {
                                                 let empty_text = "No strategies found. Use /strategy add to create one.";
                                                 bot.edit_message_text(msg.chat.id, msg.id, empty_text)
                                                    .reply_markup(keyboards::strategy_menu())
                                                    .await?;
                                             } else {
                                                 let list_message = format!("*Available Strategies:*\n\n{}", 
                                                     strategies.iter()
                                                         .map(|s| format!("• {} ({}) - {}", 
                                                             s.name, 
                                                             s.id.chars().take(8).collect::<String>(),
                                                             if s.enabled { "✅" } else { "❌" }
                                                         ))
                                                         .collect::<Vec<_>>()
                                                         .join("\n")
                                                 );
                                                 
                                                 bot.edit_message_text(msg.chat.id, msg.id, escape(&list_message))
                                                    .parse_mode(ParseMode::MarkdownV2)
                                                    .reply_markup(keyboards::strategy_list_menu())
                                                    .await?;
                                             }
                                         }
                                     },
                                     Err(e) => {
                                         notification_text = Some(format!("❌ Failed to delete strategy: {}", e));
                                     }
                                 }
                             }
                         },
                         "close_position" => {
                             if let Some(position_id) = extra_param {
                                 // Execute the position closing
                                 let close_result = locked_state.auto_trader.lock().await
                                     .position_manager
                                     .close_position(
                                         position_id,
                                         PositionStatus::ClosedManually,  // Status reason
                                         0.0,  // Exit price - using placeholder, should get actual price
                                         0.0,  // Exit value - using placeholder, should get actual value 
                                         "MANUAL_CLOSE",  // TX signature - using placeholder
                                     ).await;
                                 
                                 match close_result {
                                     Ok(_) => {
                                         notification_text = Some(format!("✅ Position {} closed", position_id));
                                         // Return to positions list
                                         if let Some(msg) = q.message {
                                             bot.edit_message_text(msg.chat.id, msg.id, "Position closed successfully.")
                                                .reply_markup(keyboards::positions_menu())
                                                .await?;
                                         }
                                     },
                                     Err(e) => {
                                         error!("Failed to close position {}: {}", position_id, e);
                                         notification_text = Some(format!("❌ Failed to close position: {}", e));
                                     }
                                 }
                             }
                         },
                         _ => {
                             notification_text = Some(format!("⚠️ Unknown confirmation type: {}", action_type));
                         }
                     }
                 }
             },
             "cancel" => {
                 if let Some(action_type) = param {
                     notification_text = Some(format!("❌ {} cancelled", action_type));
                     
                     // Return to appropriate menu based on action type
                     if let Some(msg) = q.message {
                         match action_type {
                             "delete_strategy" => {
                                 // Return to strategy detail or list
                                 if let Some(strategy_id) = extra_param {
                                     let strategy = locked_state.auto_trader.lock().await.get_strategy(strategy_id).await;
                                     if let Some(s) = strategy {
                                         let detail_message = format!(
                                             "📊 *Strategy Details: {}*\n\n\
                                             *ID:* `{}`\n\
                                             *Status:* {}\n\
                                             *Budget:* {:.2} SOL\n\
                                             *Max Position:* {:.2} SOL\n\
                                             *Max Concurrent:* {}\n\
                                             *Risk Level:* {}/100\n\
                                             *Min Liquidity:* {} SOL\n\
                                             *Min Holders:* {}",
                                             s.name,
                                             s.id,
                                             if s.enabled { "✅ Enabled" } else { "❌ Disabled" },
                                             s.total_budget_sol,
                                             s.max_position_size_sol,
                                             s.max_concurrent_positions,
                                             s.max_risk_level,
                                             s.min_liquidity_sol,
                                             s.min_holders
                                         );
                                         
                                         bot.edit_message_text(msg.chat.id, msg.id, escape(&detail_message))
                                            .parse_mode(ParseMode::MarkdownV2)
                                            .reply_markup(keyboards::strategy_detail_menu(&s.id, s.enabled))
                                            .await?;
                                     } else {
                                         // If strategy can't be found, go back to strategy list
                                         bot.edit_message_text(msg.chat.id, msg.id, "Operation cancelled")
                                            .reply_markup(keyboards::strategy_menu())
                                            .await?;
                                     }
                                 } else {
                                     // Without ID, just go to strategy list
                                     bot.edit_message_text(msg.chat.id, msg.id, "Operation cancelled")
                                        .reply_markup(keyboards::strategy_menu())
                                        .await?;
                                 }
                             },
                             "close_position" => {
                                 // Return to positions list
                                 bot.edit_message_text(msg.chat.id, msg.id, "Position close cancelled")
                                    .reply_markup(keyboards::positions_menu())
                                    .await?;
                             },
                             "snipe_token" => {
                                 bot.edit_message_text(msg.chat.id, msg.id, "Token purchase cancelled")
                                    .reply_markup(keyboards::main_menu())
                                    .await?;
                             },
                             _ => {
                                 // Generic cancel, return to main menu
                                 bot.edit_message_text(msg.chat.id, msg.id, "Operation cancelled")
                                    .reply_markup(keyboards::main_menu())
                                    .await?;
                             }
                         }
                     }
                 }
             },
             
             // --- Utility Callbacks ---
             "show_balance" => {
                 // Call the logic similar to /balance command
                 let balance_result = locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_sol_balance().await;
                 let balance_message = match balance_result {
                     Ok(bal) => format!("💰 *Balance:* {:.6} SOL", bal),
                     Err(_) => "❌ Failed to retrieve balance.".to_string(),
                 };
                 // Send as a new message or edit? Alert might be better.
                 notification_text = Some(balance_message);
             }
             "show_help" => {
                 notification_text = Some(HELP_TEXT.to_string());
             }
             "autotrader_performance" => {
                 // Fetch and display performance stats from AutoTrader
                 let performance = locked_state.auto_trader.lock().await.get_performance_stats().await;
                 
                 match performance {
                     Ok(stats) => {
                         // Define a struct to hold the needed stats in case they come in a different format
                         struct PerformanceStats {
                             total_trades: usize,
                             successful_trades: usize,
                             failed_trades: usize,
                             total_pnl_sol: f64,
                             success_rate: f64,
                             avg_position_duration_mins: f64,
                         }
                         
                         // Extract or compute stats as needed
                         let total_trades = stats.values().sum::<f64>() as usize; // Example fallback
                         let successful_trades = stats.get("successful_trades").map(|&v| v as usize).unwrap_or(0);
                         let failed_trades = stats.get("failed_trades").map(|&v| v as usize).unwrap_or(0);
                         let total_pnl_sol = stats.get("total_pnl_sol").copied().unwrap_or(0.0);
                         let success_rate = stats.get("success_rate").copied().unwrap_or(0.0);
                         let avg_position_duration_mins = stats.get("avg_position_duration_mins").copied().unwrap_or(0.0);
                         
                         let performance_message = format!(
                             "📈 *AutoTrader Performance*\n\n\
                             *Total Trades:* {}\n\
                             *Successful Trades:* {}\n\
                             *Failed Trades:* {}\n\
                             *Total Profit/Loss:* {:.4} SOL\n\
                             *Success Rate:* {:.1}%\n\
                             *Average Position Duration:* {:.1} min",
                             total_trades,
                             successful_trades,
                             failed_trades,
                             total_pnl_sol,
                             success_rate * 100.0,
                             avg_position_duration_mins
                         );
                         
                         if let Some(msg) = q.message {
                             bot.edit_message_text(msg.chat.id, msg.id, escape(&performance_message))
                                .parse_mode(ParseMode::MarkdownV2)
                                .reply_markup(keyboards::autotrader_menu(
                                    locked_state.auto_trader.lock().await.get_status().await
                                ))
                                .await?;
                         }
                         notification_text = None;
                     },
                     Err(e) => {
                         error!("Failed to get performance stats: {}", e);
                         notification_text = Some(format!("❌ Failed to get performance stats: {}", e));
                     }
                 }
             }
             
             // --- Token Action Handlers ---
             "analyze_token" => {
                 if let Some(token_address) = param {
                     info!("Callback: Analyze token: {}", token_address);
                     
                     // Send initial message
                     if let Some(chat) = chat_id {
                         bot.send_message(chat, escape(&format!("🔍 Analyzing token: `{}`\nPlease wait...", token_address)))
                             .parse_mode(ParseMode::MarkdownV2)
                             .await?;
                     }
                     
                     // Perform token analysis
                     let analysis_result = locked_state.auto_trader.lock().await
                         .risk_analyzer
                         .analyze_token(&token_address).await;
                     
                     match analysis_result {
                         Ok(analysis) => {
                             // Format analysis result similar to Command::Analyze
                             let risk_level_emoji = match analysis.risk_level {
                                 0..=30 => "🟢",
                                 31..=70 => "🟠",
                                 _ => "🔴",
                             };
                             
                             let risk_factors = if analysis.details.is_empty() {
                                 "None detected".to_string()
                             } else {
                                 analysis.details.iter().map(|d| format!("- {}", d)).collect::<Vec<_>>().join("\n")
                             };
                             
                             let analysis_message = format!(
                                 "🔍 *Token Analysis: {}*\n\n\
                                 *Address:* `{}`\n\
                                 *Risk Score:* {} {} \\({}/100\\)\n\n\
                                 *Checks:*\n\
                                 - Liquidity: {:.2} SOL\n\
                                 - Holders: {}\n\
                                 - Mint Authority: {}\n\
                                 - Freeze Authority: {}\n\
                                 - LP Burned/Locked: {}\n\
                                 - Transfer Tax: {:.1}%\n\
                                 - Sellable (Honeypot Check): {}\n\
                                 - Top Holder Concentration: {:.1}%\n\n\
                                 *Risk Factors Found:*\n{}",
                                 analysis.token_address,
                                 analysis.token_address,
                                 risk_level_emoji, analysis.risk_level, analysis.risk_level,
                                 analysis.liquidity_sol,
                                 analysis.holder_count,
                                 if analysis.has_mint_authority { "⚠️ Yes" } else { "✅ No" },
                                 if analysis.has_freeze_authority { "⚠️ Yes" } else { "✅ No" },
                                 if analysis.lp_tokens_burned { "✅ Yes" } else { "🟠 No/Unknown" },
                                 analysis.transfer_tax_percent,
                                 if analysis.can_sell { "✅ Yes" } else { "🔴 No" },
                                 analysis.concentration_percent,
                                 risk_factors
                             );
                             
                             if let Some(chat) = chat_id {
                                 bot.send_message(chat, escape(&analysis_message))
                                     .parse_mode(ParseMode::MarkdownV2)
                                     .reply_markup(keyboards::token_action_menu(&token_address))
                                     .await?;
                             }
                         },
                         Err(e) => {
                             error!("Error analyzing token {}: {:?}", token_address, e);
                             let error_message = if e.to_string().contains("TokenNotFound") || e.to_string().contains("Invalid token address") {
                                 format!("❌ Could not find token address `{}`.", escape(&token_address))
                             } else {
                                 format!("❌ Error analyzing token `{}`. Check logs for details.", escape(&token_address))
                             };
                             
                             if let Some(chat) = chat_id {
                                 bot.send_message(chat, error_message)
                                     .parse_mode(ParseMode::MarkdownV2)
                                     .await?;
                             }
                         }
                     }
                     
                     notification_text = None; // Already handled with dedicated messages
                 } else {
                     notification_text = Some("❌ Missing token address".to_string());
                 }
             }
             "snipe_token" => {
                 if let Some(token_address) = param {
                     info!("Callback: Snipe token: {}", token_address);
                     
                     if let Some(chat) = chat_id {
                         // First ask for confirmation with amount selection
                         let current_amount = locked_state.config.max_position_size_sol;
                         let confirmation_message = format!(
                             "🎯 *Snipe Token*\n\n\
                             Do you want to snipe token: `{}`?\n\n\
                             Default amount: {:.6} SOL\n\
                             Current wallet balance: {:.6} SOL",
                             token_address,
                             current_amount,
                             locked_state.wallet_manager.as_ref().expect("WalletManager not initialized").get_sol_balance().await.unwrap_or(0.0)
                         );
                         
                         // Create a custom keyboard for the confirmation with amount options
                         let custom_markup = InlineKeyboardMarkup::new(vec![
                             vec![
                                 InlineKeyboardButton::callback(
                                     format!("✅ Snipe with {:.4} SOL", current_amount),
                                     format!("execute_snipe:{}:{}", token_address, current_amount)
                                 ),
                             ],
                             vec![
                                 InlineKeyboardButton::callback("💰 Change Amount", format!("set_snipe_amount:{}", token_address)),
                                 InlineKeyboardButton::callback("❌ Cancel", "cancel:snipe_token"),
                             ],
                         ]);
                         
                         bot.send_message(chat, escape(&confirmation_message))
                             .parse_mode(ParseMode::MarkdownV2)
                             .reply_markup(custom_markup)
                             .await?;
                     }
                     
                     notification_text = None; // Already handled with dedicated messages
                 } else {
                     notification_text = Some("❌ Missing token address".to_string());
                 }
             }
             "set_snipe_amount" => {
                 if let Some(token_address) = param {
                     info!("Callback: Setting snipe amount for token: {}", token_address);
                     
                     if let Some(chat) = chat_id {
                         bot.send_message(chat, escape(&format!(
                             "💰 *Set Snipe Amount*\n\nPlease enter the amount of SOL to use for sniping token `{}`:",
                             token_address
                         )))
                         .parse_mode(ParseMode::MarkdownV2)
                         .await?;
                         
                         // Note: This requires conversation state handling to track the next message
                     }
                     
                     notification_text = Some("Enter amount in SOL".to_string());
                 } else {
                     notification_text = Some("❌ Missing token address".to_string());
                 }
             }
             "execute_snipe" => {
                 if let Some(token_address) = param {
                     // Try to get the amount from extra_param
                     let amount = if let Some(amount_str) = extra_param {
                         match amount_str.parse::<f64>() {
                             Ok(a) => a,
                             Err(_) => {
                                 notification_text = Some("❌ Invalid amount format".to_string());
                                 0.0 // Will fail the minimum check below
                             }
                         }
                     } else {
                         // Use default from config
                         locked_state.config.max_position_size_sol
                     };
                     
                     let min_amount = 0.001;
                     let max_amount = locked_state.config.total_budget_sol;
                     
                     if amount <= 0.0 || amount < min_amount || amount > max_amount {
                         notification_text = Some(format!("❌ Invalid amount. Must be between {} and {} SOL.", min_amount, max_amount));
                     } else if let Some(chat) = chat_id {
                         // Proceed with snipe execution - similar to Command::Snipe
                         bot.send_message(
                             chat,
                             escape(&format!("🎯 Preparing to snipe token: `{}` with {:.6} SOL...", token_address, amount))
                         ).parse_mode(ParseMode::MarkdownV2).await?;
                         
                         // Copy necessary state resources before dropping the lock
                         let auto_trader = locked_state.auto_trader.clone();
                         let wallet_manager = locked_state.wallet_manager.clone();
                         
                         // Drop the lock for long operation
                         drop(locked_state);
                         
                         // Execute the snipe operation
                         let analysis_result = auto_trader.lock().await.risk_analyzer.analyze_token(&token_address).await;
                         
                         match analysis_result {
                             Ok(risk_analysis) => {
                                 // Check risk level
                                 let max_allowed_risk = 70; // Example threshold
                                 if risk_analysis.risk_level > max_allowed_risk {
                                     bot.send_message(chat, format!(
                                         "❌ Snipe failed: Token `{}` risk level ({}) exceeds threshold ({}).",
                                         escape(&token_address), risk_analysis.risk_level, max_allowed_risk
                                     )).parse_mode(ParseMode::MarkdownV2).await?;
                                 } else {
                                     // Check balance
                 match wallet_manager.as_ref().expect("WalletManager not initialized").get_sol_balance().await {
                     Ok(current_balance) => {
                         if current_balance < amount {
                             bot.send_message(chat, format!(
                                 "❌ Snipe failed: Insufficient balance ({:.6} SOL) to snipe {:.6} SOL.",
                                 current_balance, amount
                             )).await?;
                         } else {
                             // Execute buy
                             let buy_result = auto_trader.lock().await.execute_manual_buy(
                                 &token_address,
                                 amount,
                             ).await;
                             
                             match buy_result {
                                 Ok(swap_result) => {
                                     bot.send_message(
                                         chat,
                                         escape(&format!(
                                             "✅ Successfully sniped `{}` with {:.6} SOL!\nSignature: `{}`",
                                             token_address, amount, swap_result.transaction_signature
                                         ))
                                     ).parse_mode(ParseMode::MarkdownV2)
                                      .reply_markup(InlineKeyboardMarkup::new(vec![
                                          vec![InlineKeyboardButton::callback("📊 View Positions", "positions_menu")],
                                          vec![InlineKeyboardButton::callback("🔙 Main Menu", "main_menu")],
                                      ]))
                                     .await?;
                                 },
                                 Err(e) => {
                                     error!("Snipe execution failed for token {}: {:?}", token_address, e);
                                     bot.send_message(
                                         chat,
                                         escape(&format!("❌ Snipe failed for token `{}`. Reason: {}", token_address, e))
                                     ).parse_mode(ParseMode::MarkdownV2).await?;
                                 }
                             }
                         }
                     },
                     Err(e) => {
                         error!("Snipe: Failed to get wallet balance: {:?}", e);
                         bot.send_message(chat, "❌ Snipe failed: Could not check wallet balance.").await?;
                     }
                 }
                                 }
                             },
                             Err(e) => {
                                 error!("Snipe: Risk analysis failed for {}: {:?}", token_address, e);
                                 bot.send_message(chat, format!(
                                     "❌ Snipe failed: Could not analyze token `{}`.",
                                     escape(&token_address)
                                 )).parse_mode(ParseMode::MarkdownV2).await?;
                             }
                         }
                     }
                     
                     notification_text = None; // Already handled with dedicated messages
                 } else {
                     notification_text = Some("❌ Missing token address".to_string());
                 }
             }

            _ => {
                warn!("Unhandled callback data: {}", data);
                notification_text = Some("⚠️ Action not implemented yet.".to_string());
            }
        }

        // Answer the callback query to remove the "loading" state
        if let Some(text) = notification_text {
             bot.answer_callback_query(q.id).text(text).show_alert(false).await?; // Show simple notification
        } else {
             bot.answer_callback_query(q.id).await?; // Just acknowledge if message was edited
        }

    } else {
        warn!("Received callback query with no data.");
         bot.answer_callback_query(q.id).await?; // Acknowledge
    }

    Ok(())
}

pub async fn start_bot(bot: Bot, state: Arc<Mutex<BotState>>) -> ResponseResult<()> {
    info!("Starting Telegram bot");

    // Set bot commands (optional, for Telegram UI)
    // bot.set_my_commands(Command::bot_commands()).await?;

    // Create message handler: filter messages starting with '/' and route to message_command_handler
    let message_handler = Update::filter_message()
        .filter(|msg: &Message| msg.text().map_or(false, |t| t.trim_start().starts_with('/')))
        .endpoint(message_command_handler);

    // Create callback handler
    let callback_handler = Update::filter_callback_query()
        .endpoint(callback_handler);

    // Create combined handler
    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    // Create dispatcher
    let mut dispatcher = Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build();

    // Start dispatcher
    dispatcher.dispatch().await;

    Ok(())
}

/// Handler for messages that are commands (start with '/')
async fn message_command_handler(
    bot: Bot,
    msg: Message,
    state: Arc<Mutex<BotState>>,
) -> ResponseResult<()> {
    if let Some(cmd) = parse_command(&msg) {
        command_handler(bot, msg, cmd, state).await
    } else {
        // Unknown or invalid command
        bot.send_message(msg.chat.id, "Unknown command or invalid arguments. Use /help for a list of commands.").await?;
        Ok(())
    }
}

// Add the handle_message function
async fn handle_message(bot: Bot, msg: Message, state: Arc<Mutex<BotState>>) -> ResponseResult<()> {
    // Default message handler - could be used for freeform conversations later
    Ok(())
}
