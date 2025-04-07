use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid; // Import Uuid

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,              // Unique identifier (UUID string)
    pub telegram_id: i64,        // Telegram user ID
    pub username: Option<String>, // Telegram username (optional)
    pub is_admin: bool,          // Whether user is an admin (determined by config)
    pub created_at: DateTime<Utc>, // Account creation time (first seen)
    pub last_active: DateTime<Utc>, // Last activity time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub user_id: String,         // User ID (matches User.id)
    pub notify_on_trade: bool,   // Whether to notify on trades
    pub notify_on_position_update: bool, // Whether to notify on position updates (e.g., SL/TP hit)
    pub notify_on_close: bool,   // Whether to notify on position close
    pub default_strategy_id: Option<String>, // Default strategy ID for manual actions
    // Add other user-specific settings like default slippage, risk tolerance etc.
}

impl User {
    // Note: is_admin should likely be determined based on config, not stored directly here
    // unless you have a separate user management system.
    pub fn new(telegram_id: i64, username: Option<String>, is_admin: bool) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(), // Generate UUID v4
            telegram_id,
            username,
            is_admin, // Set based on authorization check
            created_at: now,
            last_active: now,
        }
    }
    
    pub fn update_activity(&mut self) {
        self.last_active = Utc::now();
    }
}

impl UserSettings {
    // Creates default settings for a given user ID
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            notify_on_trade: true,
            notify_on_position_update: true,
            notify_on_close: true,
            default_strategy_id: None,
        }
    }
}
