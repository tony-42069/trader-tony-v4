use std::sync::Arc;
use tokio::sync::Mutex; // Added Mutex import

use crate::config::Config;
use crate::solana::client::SolanaClient;
use crate::solana::wallet::WalletManager;
use crate::trading::autotrader::AutoTrader;

pub mod commands;
pub mod keyboards;

// Added Clone derive for easier state management if needed
#[derive(Clone)] 
pub struct BotState {
    pub auto_trader: Arc<Mutex<AutoTrader>>, // Corrected Mutex path
    pub wallet_manager: Arc<WalletManager>,
    pub solana_client: Arc<SolanaClient>,
    pub config: Config,
    pub authorized_users: Vec<i64>,
}
