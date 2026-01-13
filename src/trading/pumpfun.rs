// src/trading/pumpfun.rs
//
// Pump.fun token discovery data structures and utilities.
// This module provides types for parsing Pump.fun create events and
// monitoring bonding curve state for graduation detection.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Pump.fun Program ID (confirmed from multiple research sources)
pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/// PumpSwap AMM Program ID (for graduated tokens)
pub const PUMPSWAP_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

/// Raydium AMM V4 Program ID (alternative graduation target, legacy)
pub const RAYDIUM_AMM_V4: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

/// Bonding curve seed for PDA derivation
pub const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";

/// Create instruction discriminator (first 8 bytes of event data)
/// Used to identify PumpCreateEvent vs other events (Buy, Sell, etc.)
pub const CREATE_DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];

/// Default token decimals for Pump.fun tokens
pub const DEFAULT_DECIMALS: u8 = 6;

/// Initial virtual token reserves (for price calculation)
pub const INITIAL_VIRTUAL_TOKEN_RESERVES: u64 = 1_073_000_000_000_000;

/// Initial virtual SOL reserves (30 SOL in lamports)
pub const INITIAL_VIRTUAL_SOL_RESERVES: u64 = 30_000_000_000;

/// Initial real token reserves
pub const INITIAL_REAL_TOKEN_RESERVES: u64 = 793_100_000_000_000;

/// Graduation threshold - approximately 85 SOL in lamports
pub const GRADUATION_THRESHOLD_LAMPORTS: u64 = 85_000_000_000;

// ============================================================================
// EVENT STRUCTURES
// ============================================================================

/// The event emitted by Pump.fun when a new token is created.
/// This is parsed from the "Program data:" log line.
///
/// IMPORTANT: Skip the first 8 bytes (Anchor event discriminator) before deserializing.
/// Also validate that those 8 bytes match CREATE_DISCRIMINATOR.
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct PumpCreateEvent {
    /// Token name (e.g., "PEPE Coin")
    pub name: String,
    /// Token symbol (e.g., "PEPE")
    pub symbol: String,
    /// Metadata URI (usually IPFS)
    pub uri: String,
    /// The SPL token mint address - THIS IS WHAT WE NEED FOR TRADING
    pub mint: Pubkey,
    /// The bonding curve PDA
    pub bonding_curve: Pubkey,
    /// The creator (dev) wallet address
    pub user: Pubkey,
}

// ============================================================================
// BONDING CURVE STATE
// ============================================================================

/// The on-chain state of a Pump.fun bonding curve account.
/// Used to track graduation status, calculate price, and determine liquidity.
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct BondingCurveState {
    /// Virtual token reserves (for price calculation via constant product)
    pub virtual_token_reserves: u64,
    /// Virtual SOL reserves (for price calculation via constant product)
    pub virtual_sol_reserves: u64,
    /// Real token reserves (actual tokens remaining in curve)
    pub real_token_reserves: u64,
    /// Real SOL reserves (actual SOL deposited in curve)
    pub real_sol_reserves: u64,
    /// Total token supply
    pub token_total_supply: u64,
    /// Whether the token has graduated (bonding curve complete)
    pub complete: bool,
}

impl BondingCurveState {
    /// Calculate the current price in SOL per token.
    /// Uses the constant product formula: virtual_token * virtual_sol = k
    /// Price = virtual_sol_reserves / virtual_token_reserves
    pub fn get_price_sol(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }
        // Convert lamports to SOL and adjust for token decimals (6)
        let sol = self.virtual_sol_reserves as f64 / 1_000_000_000.0;
        let tokens = self.virtual_token_reserves as f64 / 1_000_000.0;
        sol / tokens
    }

    /// Calculate bonding curve progress (0-100%).
    /// Progress is based on how many tokens have been sold from the curve.
    pub fn get_progress_percent(&self) -> f64 {
        if self.real_token_reserves >= INITIAL_REAL_TOKEN_RESERVES {
            return 0.0;
        }
        (1.0 - (self.real_token_reserves as f64 / INITIAL_REAL_TOKEN_RESERVES as f64)) * 100.0
    }

    /// Get the current liquidity in SOL.
    /// This is the actual SOL deposited in the bonding curve.
    pub fn get_liquidity_sol(&self) -> f64 {
        self.real_sol_reserves as f64 / 1_000_000_000.0
    }

    /// Check if token is ready to graduate (bonding curve complete).
    /// Graduation happens when `complete` is true OR all real tokens are sold.
    pub fn is_ready_to_graduate(&self) -> bool {
        self.complete || self.real_token_reserves == 0
    }

    /// Get the market cap in SOL (approximate).
    /// Market cap = price * total supply
    pub fn get_market_cap_sol(&self) -> f64 {
        let price = self.get_price_sol();
        let supply = self.token_total_supply as f64 / 1_000_000.0; // 6 decimals
        price * supply
    }
}

// ============================================================================
// DISCOVERED TOKEN
// ============================================================================

/// A discovered Pump.fun token with all metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpfunToken {
    /// The SPL token mint address
    pub mint: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Metadata URI (IPFS)
    pub uri: String,
    /// Creator (dev) wallet address
    pub creator: String,
    /// Bonding curve PDA address
    pub bonding_curve: String,
    /// Associated token account for bonding curve
    pub bonding_curve_ata: String,
    /// When the token was discovered (Unix timestamp)
    pub discovered_at: i64,
    /// Transaction signature that created this token
    pub creation_signature: String,
    /// Whether the token has graduated
    pub is_graduated: bool,
    /// Bonding curve progress (0-100%)
    pub bonding_progress: f64,
    /// Current price in SOL (from bonding curve)
    pub price_sol: f64,
    /// Current liquidity in SOL (from bonding curve)
    pub liquidity_sol: f64,
}

// ============================================================================
// PDA DERIVATION FUNCTIONS
// ============================================================================

/// Derive the bonding curve PDA from a token mint.
/// Returns (PDA pubkey, bump seed).
pub fn derive_bonding_curve_pda(mint: &Pubkey) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID).expect("Invalid PUMP_PROGRAM_ID");
    Pubkey::find_program_address(&[BONDING_CURVE_SEED, mint.as_ref()], &program_id)
}

/// Derive the associated token account for the bonding curve.
/// This is where the tokens are held before being sold.
pub fn derive_bonding_curve_ata(bonding_curve: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(bonding_curve, mint)
}

/// Get the Pump.fun program ID as a Pubkey.
pub fn get_pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).expect("Invalid PUMP_PROGRAM_ID")
}

/// Get the PumpSwap program ID as a Pubkey.
pub fn get_pumpswap_program_id() -> Pubkey {
    Pubkey::from_str(PUMPSWAP_PROGRAM_ID).expect("Invalid PUMPSWAP_PROGRAM_ID")
}

// ============================================================================
// EVENT PARSING
// ============================================================================

/// Parse a base64-encoded "Program data:" log into a PumpCreateEvent.
///
/// This function:
/// 1. Decodes the base64 data
/// 2. Validates that the first 8 bytes match CREATE_DISCRIMINATOR
/// 3. Deserializes the remaining bytes using Borsh
///
/// Returns None if:
/// - Data is too short
/// - Discriminator doesn't match (this is expected for Buy/Sell events)
/// - Borsh deserialization fails
pub fn parse_create_event(base64_data: &str) -> Option<PumpCreateEvent> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use tracing::{debug, warn};

    // Decode base64
    let data = match STANDARD.decode(base64_data) {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to decode base64: {:?}", e);
            return None;
        }
    };

    // Must have at least 8 bytes for discriminator
    if data.len() <= 8 {
        debug!("Data too short: {} bytes", data.len());
        return None;
    }

    // Validate discriminator FIRST (before attempting deserialization)
    if data[0..8] != CREATE_DISCRIMINATOR {
        // Not a Create event - this is expected for Buy/Sell/other events
        // Only log at debug level since this is very common
        return None;
    }

    // Found a Create event! Log the discriminator match
    debug!("✅ Create discriminator matched! Attempting deserialization...");

    // Deserialize the rest (skip discriminator)
    match PumpCreateEvent::try_from_slice(&data[8..]) {
        Ok(event) => Some(event),
        Err(e) => {
            warn!("⚠️ Create discriminator matched but Borsh deserialization failed: {:?}", e);
            None
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_bonding_curve_pda() {
        // Test with a known mint address
        let mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        let (pda, bump) = derive_bonding_curve_pda(&mint);

        // Verify it's a valid PDA
        assert!(bump > 0 && bump <= 255);
        assert_ne!(pda, mint);
    }

    #[test]
    fn test_bonding_curve_initial_state() {
        let curve = BondingCurveState {
            virtual_token_reserves: INITIAL_VIRTUAL_TOKEN_RESERVES,
            virtual_sol_reserves: INITIAL_VIRTUAL_SOL_RESERVES,
            real_token_reserves: INITIAL_REAL_TOKEN_RESERVES,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        // Initial progress should be 0%
        let progress = curve.get_progress_percent();
        assert!(progress < 1.0, "Initial progress should be ~0%, got {}", progress);

        // Initial liquidity should be 0 SOL
        let liquidity = curve.get_liquidity_sol();
        assert_eq!(liquidity, 0.0);

        // Should not be ready to graduate
        assert!(!curve.is_ready_to_graduate());
    }

    #[test]
    fn test_bonding_curve_50_percent() {
        let curve = BondingCurveState {
            virtual_token_reserves: 1_000_000_000_000_000,
            virtual_sol_reserves: 45_000_000_000, // 45 SOL
            real_token_reserves: INITIAL_REAL_TOKEN_RESERVES / 2, // 50% sold
            real_sol_reserves: 40_000_000_000,                    // 40 SOL
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        let progress = curve.get_progress_percent();
        assert!(
            progress > 45.0 && progress < 55.0,
            "Progress should be ~50%, got {}",
            progress
        );

        let liquidity = curve.get_liquidity_sol();
        assert_eq!(liquidity, 40.0);
    }

    #[test]
    fn test_bonding_curve_graduated() {
        let curve = BondingCurveState {
            virtual_token_reserves: 1_000_000_000_000_000,
            virtual_sol_reserves: 85_000_000_000,
            real_token_reserves: 0, // All sold
            real_sol_reserves: 85_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: true,
        };

        // Should be ready to graduate
        assert!(curve.is_ready_to_graduate());

        // Progress should be 100%
        let progress = curve.get_progress_percent();
        assert_eq!(progress, 100.0);
    }

    #[test]
    fn test_parse_create_event_short_data() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        // Data that's too short should return None
        let short_data = STANDARD.encode([0u8; 4]);
        assert!(parse_create_event(&short_data).is_none());
    }

    #[test]
    fn test_parse_create_event_wrong_discriminator() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        // Data with wrong discriminator should return None
        let wrong_disc = STANDARD.encode([0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(parse_create_event(&wrong_disc).is_none());
    }

    #[test]
    fn test_program_ids_valid() {
        // Ensure program IDs are valid pubkeys
        assert!(Pubkey::from_str(PUMP_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(PUMPSWAP_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(RAYDIUM_AMM_V4).is_ok());
    }
}
