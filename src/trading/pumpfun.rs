// src/trading/pumpfun.rs
//
// Pump.fun token discovery data structures and utilities.
// This module provides types for parsing Pump.fun create events and
// monitoring bonding curve state for graduation detection.
//
// VERIFIED against real on-chain transactions:
// - SINGU: 5j7DsKdxmQRYDWKcspkUy5avyNFpqtSLnD29ChGoRxZjMFHs171nFrSXiC54TvwxEaLgUWBvYC8MmEELBZExk8Ww
// - Taki:  vY3Ajg8wEiCtGbH8xq4LLhiNsAuZPTENMTwJzxV1umEVoxMyEUoRQwmjXbyVoRdGxTHQKJ9cjW5jMD2gBoPwUpB

use borsh::BorshDeserialize;
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

/// CreateEvent discriminator - VERIFIED from real transactions
/// Derived from sha256("event:createEvent")[0..8] - note lowercase 'c'!
/// Base64 prefix: "G3KpTd7r"
pub const CREATE_DISCRIMINATOR: [u8; 8] = [27, 114, 169, 77, 222, 235, 99, 118];

/// Maximum allowed size for event data (prevents OOM on malformed data)
/// CreateEvent is typically 300-400 bytes, so 1024 is a safe upper bound
pub const MAX_EVENT_SIZE: usize = 1024;

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

/// The event emitted by Pump.fun when a new token is created (CreateV2 instruction).
/// This is parsed from the "Program data:" log line.
///
/// VERIFIED against real on-chain data from multiple transactions.
///
/// IMPORTANT:
/// - Skip the first 8 bytes (Anchor event discriminator) before deserializing
/// - Validate that those 8 bytes match CREATE_DISCRIMINATOR
/// - Field order matters! Borsh deserializes in declaration order
///
/// This struct has 14 fields total.
#[derive(Debug, Clone, BorshDeserialize)]
pub struct PumpCreateEvent {
    // --- Strings (variable length: 4-byte len prefix + UTF-8 data) ---
    /// Token name (e.g., "The Singularity")
    pub name: String,
    /// Token symbol (e.g., "SINGU")
    pub symbol: String,
    /// Metadata URI (usually IPFS)
    pub uri: String,

    // --- Pubkeys (32 bytes each) ---
    /// The SPL token mint address - THIS IS WHAT WE NEED FOR TRADING
    pub mint: Pubkey,
    /// The bonding curve PDA
    pub bonding_curve: Pubkey,
    /// The user who initiated the creation (transaction signer)
    pub user: Pubkey,
    /// The creator/dev wallet (often same as user)
    pub creator: Pubkey,

    // --- Numbers (8 bytes each) ---
    /// Unix timestamp of creation
    pub timestamp: i64,
    /// Initial virtual token reserves on the bonding curve
    pub virtual_token_reserves: u64,
    /// Initial virtual SOL reserves on the bonding curve (30 SOL = 30_000_000_000 lamports)
    pub virtual_sol_reserves: u64,
    /// Initial real token reserves
    pub real_token_reserves: u64,
    /// Total token supply (1 billion with 6 decimals = 1_000_000_000_000_000)
    pub token_total_supply: u64,

    // --- More Pubkeys ---
    /// Token program ID (Token-2022: TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb)
    pub token_program: Pubkey,

    // --- Bool (1 byte) ---
    /// Whether this token was created in "Mayhem mode"
    pub is_mayhem_mode: bool,
}

// ============================================================================
// BONDING CURVE STATE
// ============================================================================

/// The on-chain state of a Pump.fun bonding curve account.
/// Used to track graduation status, calculate price, and determine liquidity.
#[derive(BorshDeserialize, Debug, Clone)]
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
/// 2. Validates data size (prevents OOM attacks)
/// 3. Validates that the first 8 bytes match CREATE_DISCRIMINATOR
/// 4. Deserializes the remaining bytes using Borsh
///
/// Returns None if:
/// - Data is too short or too long
/// - Discriminator doesn't match (this is expected for Buy/Sell events)
/// - Borsh deserialization fails
pub fn parse_create_event(base64_data: &str) -> Option<PumpCreateEvent> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use tracing::{debug, warn};

    // Decode base64
    let data = match STANDARD.decode(base64_data) {
        Ok(d) => d,
        Err(e) => {
            debug!("Base64 decode failed: {:?}", e);
            return None;
        }
    };

    // Must have at least 8 bytes for discriminator
    if data.len() <= 8 {
        debug!("Data too short: {} bytes", data.len());
        return None;
    }

    // CRITICAL: Prevent OOM by rejecting suspiciously large data
    // CreateEvent is typically 300-400 bytes, max 1024 is safe
    if data.len() > MAX_EVENT_SIZE {
        warn!(
            "Data too large ({} bytes), rejecting to prevent OOM",
            data.len()
        );
        return None;
    }

    // Validate discriminator FIRST (before attempting deserialization)
    if data[0..8] != CREATE_DISCRIMINATOR {
        // Not a Create event - this is expected for Buy/Sell/other events
        return None;
    }

    // Found a Create event! Deserialize the rest (skip 8-byte discriminator)
    match PumpCreateEvent::try_from_slice(&data[8..]) {
        Ok(event) => {
            debug!(
                "✅ Parsed CreateEvent: {} ({}) mint={}",
                event.name, event.symbol, event.mint
            );
            Some(event)
        }
        Err(e) => {
            warn!(
                "Borsh deserialization failed: {} (data len: {})",
                e,
                data.len()
            );
            None
        }
    }
}

/// Calculate initial price from virtual reserves
pub fn calculate_initial_price(virtual_token_reserves: u64, virtual_sol_reserves: u64) -> f64 {
    if virtual_token_reserves == 0 {
        return 0.0;
    }
    // SOL per token
    (virtual_sol_reserves as f64 / 1_000_000_000.0) / (virtual_token_reserves as f64 / 1_000_000.0)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminator_value() {
        // Verify our discriminator matches expected bytes
        // This is sha256("event:createEvent")[0..8]
        assert_eq!(CREATE_DISCRIMINATOR[0], 27);  // 0x1b
        assert_eq!(CREATE_DISCRIMINATOR[1], 114); // 0x72
        assert_eq!(CREATE_DISCRIMINATOR[2], 169); // 0xa9
        assert_eq!(CREATE_DISCRIMINATOR[3], 77);  // 0x4d
        assert_eq!(CREATE_DISCRIMINATOR[4], 222); // 0xde
        assert_eq!(CREATE_DISCRIMINATOR[5], 235); // 0xeb
        assert_eq!(CREATE_DISCRIMINATOR[6], 99);  // 0x63
        assert_eq!(CREATE_DISCRIMINATOR[7], 118); // 0x76
    }

    #[test]
    fn test_parse_singu_token() {
        // Real data from tx 5j7DsKdxmQRYDWKcspkUy5avyNFpqtSLnD29ChGoRxZjMFHs171nFrSXiC54TvwxEaLgUWBvYC8MmEELBZExk8Ww
        let base64_data = "G3KpTd7rY3YPAAAAVGhlIFNpbmd1bGFyaXR5BQAAAFNJTkdVUAAAAGh0dHBzOi8vaXBmcy5pby9pcGZzL2JhZmtyZWliZGxmbDZmenZiYWR5cHJkZ2NoeWk3NGw2NGFxMmd4Z3g1N210ZHhzYXl5b2R2ZnFhcDdlj2ePFn0HrqpCzAvgSjQTneRkqT5WfdGJBBm/QNBvbK/6fcORCCovNiMFYMAzuw3uHzGoLFqEG+FkQWxcymf0oL22lg79rG+3N7A/+gaHwyckutJH39aYkUL0cjQ0dGimvbaWDv2sb7c3sD/6BofDJyS60kff1piRQvRyNDR0aKboJmlpAAAAAAAQ2EfjzwMAAKwj/AYAAAAAeMX7UdECAACAxqR+jQMABt324e51j94YQl285GzN2rYa/E2DuQ0n/r35KNihi/wA";

        let event = parse_create_event(base64_data).expect("Should parse SINGU token");

        assert_eq!(event.name, "The Singularity");
        assert_eq!(event.symbol, "SINGU");
        assert_eq!(
            event.mint.to_string(),
            "AentZ28d2APLu8U7Bnqonb8C2MWWvv4DnwraoTCypump"
        );
        assert_eq!(
            event.bonding_curve.to_string(),
            "Hrp9jxCqcUQFUJWN49AEMRUWgwbWA4Vdvb2jLArBFmnF"
        );
        assert_eq!(
            event.user.to_string(),
            "DmZY2mTm7Ci336qq7oNvts1Bffp8vb5WyHhi7cchTREd"
        );
        assert_eq!(event.user, event.creator); // Same for this token
        assert_eq!(event.timestamp, 1768498920);
        assert_eq!(event.virtual_token_reserves, 1073000000000000);
        assert_eq!(event.virtual_sol_reserves, 30000000000);
        assert_eq!(event.real_token_reserves, 793100000000000);
        assert_eq!(event.token_total_supply, 1000000000000000);
        assert_eq!(
            event.token_program.to_string(),
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        );
        assert_eq!(event.is_mayhem_mode, false);

        println!("✅ SINGU token parsed successfully!");
        println!("   Name: {}", event.name);
        println!("   Symbol: {}", event.symbol);
        println!("   Mint: {}", event.mint);
    }

    #[test]
    fn test_parse_taki_token() {
        // Real data from tx vY3Ajg8wEiCtGbH8xq4LLhiNsAuZPTENMTwJzxV1umEVoxMyEUoRQwmjXbyVoRdGxTHQKJ9cjW5jMD2gBoPwUpB
        let base64_data = "G3KpTd7rY3YEAAAAVGFraQQAAABUYWtpPQAAAGh0dHBzOi8vbWV0YWRhdGEuajd0cmFja2VyLmNvbS9tZXRhZGF0YS9iYjJhMGU3MWQyMjY0YWExLmpzb27FzfVxmN9cMINqwLCtIJRsjIJ1mBztr19XIUhnY27h8Q541h90lcZyZJNdZagw3fOqOEJTYbADjV6h5gOEMCRPxuFvr2iDGWfai1Lz8///dd0FExKKlO3gUhIcYC78rqLG4W+vaIMZZ9qLUvPz//913QUTEoqU7eBSEhxgLvyuonl7ZmkAAAAAABDYR+PPAwAArCP8BgAAAAB4xftR0QIAAIDGpH6NAwAG3fbh7nWP3hhCXbzkbM3athr8TYO5DSf+vfko2KGL/AA=";

        let event = parse_create_event(base64_data).expect("Should parse Taki token");

        assert_eq!(event.name, "Taki");
        assert_eq!(event.symbol, "Taki");
        assert_eq!(
            event.mint.to_string(),
            "EK9U7T5GFoNjYg8R5eBzpu8fNR6DCxD7ggYrqamR8eBa"
        );
        assert_eq!(
            event.bonding_curve.to_string(),
            "yVaR2kKvXEVVUQEqD7U7wZk7h47ePk3JvusmXz5uMgi"
        );
        assert_eq!(event.is_mayhem_mode, false);

        println!("✅ Taki token parsed successfully!");
    }

    #[test]
    fn test_parse_rejects_non_create_events() {
        use base64::{engine::general_purpose::STANDARD, Engine};

        // Wrong discriminator (tradeEvent starts with different bytes)
        let wrong_disc = STANDARD.encode([0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(parse_create_event(&wrong_disc).is_none());

        // Too short data
        let short_data = STANDARD.encode([0u8; 4]);
        assert!(parse_create_event(&short_data).is_none());

        // Invalid base64
        assert!(parse_create_event("not valid base64!!!").is_none());

        // Empty string
        assert!(parse_create_event("").is_none());
    }

    #[test]
    fn test_max_size_guard() {
        use base64::{engine::general_purpose::STANDARD, Engine};

        // Create data larger than MAX_EVENT_SIZE with correct discriminator
        let mut large_data = CREATE_DISCRIMINATOR.to_vec();
        large_data.extend(vec![0u8; MAX_EVENT_SIZE + 100]);
        let encoded = STANDARD.encode(&large_data);

        // Should be rejected due to size
        assert!(parse_create_event(&encoded).is_none());
    }

    #[test]
    fn test_derive_bonding_curve_pda() {
        let mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        let (pda, bump) = derive_bonding_curve_pda(&mint);

        // Verify it's a valid PDA
        assert!(bump <= 255);
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
    fn test_bonding_curve_graduated() {
        let curve = BondingCurveState {
            virtual_token_reserves: 1_000_000_000_000_000,
            virtual_sol_reserves: 85_000_000_000,
            real_token_reserves: 0, // All sold
            real_sol_reserves: 85_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: true,
        };

        assert!(curve.is_ready_to_graduate());
        assert_eq!(curve.get_progress_percent(), 100.0);
    }

    #[test]
    fn test_calculate_initial_price() {
        let price = calculate_initial_price(
            1_073_000_000_000_000, // virtual token reserves
            30_000_000_000,        // virtual SOL reserves (30 SOL)
        );

        // Price should be approximately 0.00000002796 SOL per token
        assert!(price > 0.0);
        assert!(price < 0.0001);
    }

    #[test]
    fn test_program_ids_valid() {
        assert!(Pubkey::from_str(PUMP_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(PUMPSWAP_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(RAYDIUM_AMM_V4).is_ok());
    }
}
