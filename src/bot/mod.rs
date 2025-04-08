use std::sync::Arc;
use tokio::sync::Mutex; // Added Mutex import

use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;

pub mod commands;
pub mod keyboards;
pub mod notification; // Add notification module

// Added Clone derive for easier state management if needed
#[derive(Clone)] 
pub struct BotState {
    pub auto_trader: Arc<Mutex<AutoTrader>>, // Corrected Mutex path
    pub wallet_manager: Option<Arc<WalletManager>>, // Make this Option
    pub solana_client: Option<Arc<SolanaClient>>, // Make this Option
    pub config: Arc<Config>,
    pub authorized_users: Vec<i64>, // User IDs allowed to use the bot
    pub notification_manager: Option<Arc<notification::NotificationManager>>, // Add notification manager
}
