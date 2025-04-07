use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub address: String,                     // Token mint address
    pub name: String,                        // Token name
    pub symbol: String,                      // Token symbol
    pub decimals: u8,                        // Token decimals (usually 9 for Solana)
    pub supply: Option<u64>,                 // Total supply (use u64 for lamports/raw units)
    pub logo_uri: Option<String>,            // Logo URL
    pub creation_time: Option<DateTime<Utc>>, // Token creation time (if available)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub address: String,         // Token mint address
    pub price_usd: Option<f64>,  // Price in USD (might not always be available)
    pub price_sol: f64,          // Price in SOL (usually derived from a SOL pair)
    pub last_updated: DateTime<Utc>, // Last price update time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLiquidity {
    pub address: String,         // Token mint address (or pair address)
    pub liquidity_usd: Option<f64>, // Liquidity in USD (might not always be available)
    pub liquidity_sol: f64,      // Liquidity in SOL (usually from SOL pair)
    pub last_updated: DateTime<Utc>, // Last update time
}

// Potential future additions:
// - TokenRiskAssessment struct
// - TokenMarketData (volume, holders, etc.)
