use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::markdown::escape;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::bot::BotState;
use crate::trading::position::Position;

/// Handles sending notifications to authorized users
pub struct NotificationManager {
    bot: Bot,
    state: Arc<Mutex<BotState>>,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new(bot: Bot, state: Arc<Mutex<BotState>>) -> Self {
        Self { bot, state }
    }
    
    /// Send a trade execution notification
    pub async fn send_trade_notification(&self, position: &Position, is_buy: bool) {
        let emoji = if is_buy { "🟢" } else { "🔴" };
        let action = if is_buy { "Buy" } else { "Sell" };
        
        // Get the transaction signature string once to avoid temporary value issues
        let tx_signature = if is_buy { 
            &position.entry_tx_signature 
        } else { 
            match &position.exit_tx_signature {
                Some(sig) => sig,
                None => "N/A"
            }
        };
        
        let message = format!(
            "{} *Trade Executed*\n\n\
            Action: {}\n\
            Token: `{}`\n\
            Amount: {:.6} SOL\n\
            Price: {:.8} SOL\n\
            Transaction: `{}`",
            emoji,
            action,
            position.token_address,
            if is_buy { position.entry_value_sol } else { position.exit_value_sol.unwrap_or(0.0) },
            if is_buy { position.entry_price_sol } else { position.exit_price_sol.unwrap_or(0.0) },
            tx_signature
        );
        
        self.send_to_all_users(&message).await;
    }
    
    /// Send a position update notification (for significant PnL changes)
    pub async fn send_position_update(&self, position: &Position, pnl_percent: f64) {
        let emoji = if pnl_percent >= 10.0 {
            "🚀"
        } else if pnl_percent >= 5.0 {
            "📈"
        } else if pnl_percent <= -10.0 {
            "💥"
        } else if pnl_percent <= -5.0 {
            "📉"
        } else {
            "➖"
        };
        
        let message = format!(
            "{} *Position Update*\n\n\
            Token: `{}`\n\
            Current Value: {:.6} SOL\n\
            PnL: {:.2}% ({:.6} SOL)\n\
            Price: {:.8} SOL",
            emoji,
            position.token_address,
            position.entry_token_amount * position.current_price_sol,
            pnl_percent,
            (position.entry_token_amount * position.current_price_sol) - position.entry_value_sol,
            position.current_price_sol
        );
        
        self.send_to_all_users(&message).await;
    }
    
    /// Send error alert
    pub async fn send_error_alert(&self, error_type: &str, message: &str) {
        let alert = format!(
            "❌ *Error Alert: {}*\n\n{}",
            error_type,
            message
        );
        
        self.send_to_all_users(&alert).await;
    }
    
    /// Send system status update
    pub async fn send_status_update(&self, status_type: &str, message: &str) {
        let alert = format!(
            "ℹ️ *{} Update*\n\n{}",
            status_type,
            message
        );
        
        self.send_to_all_users(&alert).await;
    }
    
    /// Send a custom notification
    pub async fn send_custom_notification(&self, title: &str, message: &str, emoji: &str) {
        let notification = format!(
            "{} *{}*\n\n{}",
            emoji,
            title,
            message
        );
        
        self.send_to_all_users(&notification).await;
    }
    
    /// Helper to send message to all authorized users
    async fn send_to_all_users(&self, message: &str) {
        // Get a copy of the authorized users to avoid holding the lock during API calls
        let authorized_users = {
            let locked_state = self.state.lock().await;
            locked_state.authorized_users.clone()
        };
        
        for &user_id in &authorized_users {
            match self.bot.send_message(ChatId(user_id), escape(message))
                .parse_mode(ParseMode::MarkdownV2)
                .await 
            {
                Ok(_) => {
                    info!("Notification sent to user {}", user_id);
                },
                Err(e) => {
                    error!("Failed to send notification to user {}: {}", user_id, e);
                }
            }
        }
    }
} 