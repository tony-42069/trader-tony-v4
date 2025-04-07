use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use tracing::{error, info, warn}; // Added warn

use crate::bot::keyboards; // Assuming keyboards module exists for callbacks
use crate::bot::BotState;
// Removed: use crate::trading::strategy::Strategy;
use teloxide::utils::markdown::escape; // Use teloxide's built-in escape function

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

            // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&welcome_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
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
                .get_active_positions().await; // Call its method

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
                    .join("\n\n---\n\n"); // Separator

                format!("📈 *Active Positions*\n\n{}", positions_list)
            };

             // Use MarkdownV2 and escape the message
            bot.send_message(chat_id, escape(&positions_message))
                .parse_mode(ParseMode::MarkdownV2) // Use MarkdownV2
                .reply_markup(keyboards::positions_menu()) // Use keyboard
                .await?;
        },
        Command::Analyze { token_address } => {
            bot.send_message(
                chat_id,
                 // Use MarkdownV2 and escape the message (note: backticks are fine in V2)
                escape(&format!("🔍 Analyzing token: `{}`\nPlease wait...", token_address))
            ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2

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
                        // Add keyboard for actions like Snipe?
                        .await?;
                },
                Err(e) => {
                    error!("Error analyzing token {}: {:?}", token_address, e);
                    bot.send_message(
                        chat_id,
                         // Use MarkdownV2 and escape the message
                        escape(&format!("❌ Error analyzing token `{}`.", token_address))
                    ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2
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
                 // Use MarkdownV2 and escape the message
                escape(&format!("🎯 Preparing to snipe token: `{}` with {:.6} SOL...", token_address, amount))
            ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2

            // TODO: Implement actual snipe logic within AutoTrader or a dedicated service
            // 1. Analyze token (reuse Analyze logic or parts of it)
            // 2. Check risk level against strategy/config
            // 3. Check balance
            // 4. Execute buy using JupiterClient via WalletManager
            // 5. Create position entry in PositionManager

            let snipe_result: Result<(), _> = Err(()); // Placeholder

            match snipe_result {
                 Ok(_) => {
                     // Use MarkdownV2 and escape the message
                     bot.send_message(
                        chat_id,
                        escape(&format!("✅ Successfully sniped `{}` with {:.6} SOL!", token_address, amount))
                    ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2
                 }
                 Err(e) => {
                     error!("Snipe failed for token {}: {:?}", token_address, e);
                      // Use MarkdownV2 and escape the message
                     bot.send_message(
                        chat_id,
                        escape(&format!("❌ Snipe failed for token `{}`.", token_address))
                    ).parse_mode(ParseMode::MarkdownV2).await?; // Use MarkdownV2
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

        match data.as_str() {
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
            // --- Add other callback handlers here ---
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
                    locked_state.wallet_manager.get_public_key(),
                 );
                  if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&welcome_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::main_menu())
                        .await?;
                  }
                 notification_text = None;
             }
             "positions_menu" => {
                 // Call the logic similar to /positions command
                 let positions = locked_state.auto_trader.lock().await.position_manager.get_active_positions().await;
                 let positions_message = if positions.is_empty() { "✅ No active positions.".to_string() } else { /* ... formatting ... */ "Formatted positions".to_string() }; // Simplified formatting
                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&positions_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::positions_menu())
                        .await?;
                 }
                 notification_text = None;
             }
             "strategy_menu" => {
                 // Call the logic similar to /strategy command
                 let strategies = locked_state.auto_trader.lock().await.list_strategies().await;
                 let strategies_message = if strategies.is_empty() { "✅ No strategies configured yet.".to_string() } else { /* ... formatting ... */ "Formatted strategies".to_string() }; // Simplified formatting
                 if let Some(msg) = q.message {
                     bot.edit_message_text(msg.chat.id, msg.id, escape(&strategies_message))
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(keyboards::strategy_menu())
                        .await?;
                 }
                 notification_text = None;
             }
             "show_balance" => {
                 // Call the logic similar to /balance command
                 let balance_result = locked_state.wallet_manager.get_sol_balance().await;
                 let balance_message = match balance_result {
                     Ok(bal) => format!("💰 *Balance:* {:.6} SOL", bal),
                     Err(_) => "❌ Failed to retrieve balance.".to_string(),
                 };
                 // Send as a new message or edit? Alert might be better.
                 notification_text = Some(balance_message);
             }
             "show_help" => {
                 notification_text = Some(Command::descriptions().to_string());
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
