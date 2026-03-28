//! Token Watchlist Module
//!
//! Tracks discovered Pump.fun tokens for evaluation by Final Stretch and Migrated strategies.
//! When the "New Pairs" sniper discovers a token, it gets added to this watchlist.
//! The scanner then periodically evaluates watchlist tokens against strategy criteria.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum number of tokens to track in the watchlist
const MAX_WATCHLIST_SIZE: usize = 500;

/// Maximum age of tokens to keep (24 hours in minutes)
const MAX_TOKEN_AGE_MINUTES: i64 = 1440;

/// Represents a token being tracked in the watchlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistToken {
    /// Token mint address
    pub mint: String,
    /// Bonding curve account address (for on-chain queries)
    pub bonding_curve: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// When the token was created (from CreateEvent)
    pub created_at: DateTime<Utc>,
    /// Creator wallet address (optional)
    pub creator: Option<String>,
    /// Last time this token was checked by the scanner
    pub last_checked: Option<DateTime<Utc>>,
    /// Whether we've already traded this token
    pub traded: bool,
    /// Initial price in SOL (from CreateEvent)
    pub initial_price_sol: f64,
    /// Last known bonding curve progress (0-100%)
    pub last_known_progress: Option<f64>,
    /// Whether the token has migrated (graduated)
    pub is_migrated: bool,
}

impl WatchlistToken {
    /// Create a new watchlist token from Pump.fun CreateEvent data
    pub fn from_create_event(
        mint: &str,
        bonding_curve: &str,
        name: &str,
        symbol: &str,
        price_sol: f64,
        creator: Option<String>,
    ) -> Self {
        Self {
            mint: mint.to_string(),
            bonding_curve: bonding_curve.to_string(),
            name: name.to_string(),
            symbol: symbol.to_string(),
            created_at: Utc::now(),
            creator,
            last_checked: None,
            traded: false,
            initial_price_sol: price_sol,
            last_known_progress: Some(0.0), // New tokens start at 0%
            is_migrated: false,
        }
    }

    /// Get the age of this token in minutes
    pub fn age_minutes(&self) -> i64 {
        Utc::now().signed_duration_since(self.created_at).num_minutes()
    }

    /// Check if this token is still within the maximum age
    pub fn is_within_max_age(&self) -> bool {
        self.age_minutes() <= MAX_TOKEN_AGE_MINUTES
    }
}

/// Token watchlist manager
/// Thread-safe storage for tracking discovered tokens
pub struct Watchlist {
    /// Token storage: mint -> WatchlistToken
    tokens: Arc<RwLock<HashMap<String, WatchlistToken>>>,
    /// Path for persistence
    persistence_path: PathBuf,
    /// Maximum number of tokens to track
    max_size: usize,
}

impl Watchlist {
    /// Create a new watchlist
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            persistence_path: PathBuf::from("data/watchlist.json"),
            max_size: MAX_WATCHLIST_SIZE,
        }
    }

    /// Create a new watchlist with custom persistence path
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            persistence_path: path,
            max_size: MAX_WATCHLIST_SIZE,
        }
    }

    /// Add a token to the watchlist
    /// Returns true if the token was added, false if it already exists or watchlist is full
    pub async fn add_token(&self, token: WatchlistToken) -> Result<bool> {
        let mut tokens = self.tokens.write().await;

        // Check if already exists
        if tokens.contains_key(&token.mint) {
            debug!("Token {} already in watchlist", token.symbol);
            return Ok(false);
        }

        // Check size limit
        if tokens.len() >= self.max_size {
            // Try to remove old tokens first
            drop(tokens);
            self.cleanup().await?;
            tokens = self.tokens.write().await;

            if tokens.len() >= self.max_size {
                warn!("Watchlist full ({} tokens), cannot add {}", tokens.len(), token.symbol);
                return Ok(false);
            }
        }

        info!("📝 Adding to watchlist: {} ({}) - Age: {} min",
            token.name, token.symbol, token.age_minutes());

        tokens.insert(token.mint.clone(), token);
        drop(tokens);

        // Persist to disk
        self.save().await?;

        Ok(true)
    }

    /// Get a specific token from the watchlist
    pub async fn get_token(&self, mint: &str) -> Option<WatchlistToken> {
        let tokens = self.tokens.read().await;
        tokens.get(mint).cloned()
    }

    /// Get all tokens in the watchlist
    pub async fn get_all_tokens(&self) -> Vec<WatchlistToken> {
        let tokens = self.tokens.read().await;
        tokens.values().cloned().collect()
    }

    /// Get tokens that haven't been traded and are within age limits
    pub async fn get_active_tokens(&self) -> Vec<WatchlistToken> {
        let tokens = self.tokens.read().await;
        tokens.values()
            .filter(|t| !t.traded && t.is_within_max_age())
            .cloned()
            .collect()
    }

    /// Get tokens suitable for Final Stretch strategy (not migrated, within 60 min)
    pub async fn get_tokens_for_final_stretch(&self) -> Vec<WatchlistToken> {
        let tokens = self.tokens.read().await;
        tokens.values()
            .filter(|t| !t.traded && !t.is_migrated && t.age_minutes() <= 60)
            .cloned()
            .collect()
    }

    /// Get tokens suitable for Migrated strategy (migrated, within 24 hours)
    pub async fn get_tokens_for_migrated(&self) -> Vec<WatchlistToken> {
        let tokens = self.tokens.read().await;
        tokens.values()
            .filter(|t| !t.traded && t.is_migrated && t.age_minutes() <= 1440)
            .cloned()
            .collect()
    }

    /// Update a token's last checked time and progress
    pub async fn update_token_status(
        &self,
        mint: &str,
        progress: Option<f64>,
        is_migrated: bool,
    ) -> Result<()> {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.get_mut(mint) {
            token.last_checked = Some(Utc::now());
            if let Some(p) = progress {
                token.last_known_progress = Some(p);
            }
            token.is_migrated = is_migrated;
        }
        drop(tokens);
        self.save().await
    }

    /// Mark a token as traded
    pub async fn mark_as_traded(&self, mint: &str) -> Result<()> {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.get_mut(mint) {
            info!("✅ Marking {} as traded", token.symbol);
            token.traded = true;
        }
        drop(tokens);
        self.save().await
    }

    /// Remove a token from the watchlist
    pub async fn remove_token(&self, mint: &str) -> Result<Option<WatchlistToken>> {
        let mut tokens = self.tokens.write().await;
        let removed = tokens.remove(mint);
        if let Some(ref t) = removed {
            info!("🗑️ Removed {} from watchlist", t.symbol);
        }
        drop(tokens);
        self.save().await?;
        Ok(removed)
    }

    /// Clean up old tokens and traded tokens
    pub async fn cleanup(&self) -> Result<usize> {
        let mut tokens = self.tokens.write().await;
        let initial_count = tokens.len();

        // Remove tokens older than 24 hours or already traded
        tokens.retain(|_, t| t.is_within_max_age() && !t.traded);

        let removed_count = initial_count - tokens.len();
        if removed_count > 0 {
            info!("🧹 Cleaned up {} old/traded tokens from watchlist", removed_count);
        }

        drop(tokens);
        self.save().await?;

        Ok(removed_count)
    }

    /// Get watchlist statistics
    pub async fn get_stats(&self) -> WatchlistStats {
        let tokens = self.tokens.read().await;
        let total = tokens.len();
        let active = tokens.values().filter(|t| !t.traded && t.is_within_max_age()).count();
        let traded = tokens.values().filter(|t| t.traded).count();
        let migrated = tokens.values().filter(|t| t.is_migrated).count();

        WatchlistStats {
            total_tokens: total,
            active_tokens: active,
            traded_tokens: traded,
            migrated_tokens: migrated,
            max_capacity: self.max_size,
        }
    }

    /// Load watchlist from disk
    pub async fn load(&self) -> Result<()> {
        if !self.persistence_path.exists() {
            debug!("Watchlist file not found, starting with empty watchlist");
            return Ok(());
        }

        let data = tokio::fs::read_to_string(&self.persistence_path).await?;
        if data.trim().is_empty() {
            return Ok(());
        }

        let loaded: HashMap<String, WatchlistToken> = serde_json::from_str(&data)?;
        let mut tokens = self.tokens.write().await;
        *tokens = loaded;

        info!("📂 Loaded {} tokens from watchlist", tokens.len());
        Ok(())
    }

    /// Save watchlist to disk
    pub async fn save(&self) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.persistence_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let tokens = self.tokens.read().await;
        let data = serde_json::to_string_pretty(&*tokens)?;
        tokio::fs::write(&self.persistence_path, data).await?;

        debug!("💾 Saved {} tokens to watchlist", tokens.len());
        Ok(())
    }
}

impl Default for Watchlist {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the watchlist
#[derive(Debug, Clone, Serialize)]
pub struct WatchlistStats {
    pub total_tokens: usize,
    pub active_tokens: usize,
    pub traded_tokens: usize,
    pub migrated_tokens: usize,
    pub max_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watchlist_add_and_get() {
        let watchlist = Watchlist::new();

        let token = WatchlistToken::from_create_event(
            "TestMint123",
            "TestBondingCurve",
            "Test Token",
            "TEST",
            0.0000000280,
            None,
        );

        let added = watchlist.add_token(token.clone()).await.unwrap();
        assert!(added);

        let retrieved = watchlist.get_token("TestMint123").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().symbol, "TEST");
    }

    #[tokio::test]
    async fn test_watchlist_duplicate() {
        let watchlist = Watchlist::new();

        let token = WatchlistToken::from_create_event(
            "TestMint123",
            "TestBondingCurve",
            "Test Token",
            "TEST",
            0.0000000280,
            None,
        );

        let first = watchlist.add_token(token.clone()).await.unwrap();
        let second = watchlist.add_token(token).await.unwrap();

        assert!(first);
        assert!(!second); // Duplicate should return false
    }

    #[tokio::test]
    async fn test_mark_as_traded() {
        let watchlist = Watchlist::new();

        let token = WatchlistToken::from_create_event(
            "TestMint123",
            "TestBondingCurve",
            "Test Token",
            "TEST",
            0.0000000280,
            None,
        );

        watchlist.add_token(token).await.unwrap();
        watchlist.mark_as_traded("TestMint123").await.unwrap();

        let retrieved = watchlist.get_token("TestMint123").await.unwrap();
        assert!(retrieved.traded);
    }
}
