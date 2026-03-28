# Token Discovery Mechanism Redesign

**Document Created:** 2026-01-10
**Status:** Planning Phase
**Priority:** High - Core Functionality

---

## Table of Contents

1. [Current Issues Discovered](#current-issues-discovered)
2. [Current Implementation Analysis](#current-implementation-analysis)
3. [Target Platforms](#target-platforms)
4. [Proposed Architecture](#proposed-architecture)
5. [Information Needed for Implementation](#information-needed-for-implementation)
6. [Technical Considerations](#technical-considerations)
7. [Next Steps](#next-steps)

---

## Current Issues Discovered

### Issue 1: Wrong API for Token Discovery

**Problem:** Using Helius DAS (Digital Asset Standard) `searchAssets` API to find tradeable tokens.

**Why It Fails:**
- DAS is designed for **NFTs and compressed NFTs**, not SPL tokens
- Returns metadata accounts, not token mint addresses
- Addresses returned are often:
  - NFT metadata accounts
  - Compressed NFT leaf nodes
  - Token metadata (Metaplex) - not the actual mint
  - Dead/deprecated tokens with no liquidity

**Evidence from logs:**
```
AccountNotFound: pubkey=DUcKzvaJuvB8S8Gj6QbuhGQpsya7PETLqJG9FKLwFKnD
Failed to unpack mint account: An account's data contents was invalid
```

**Result:** 100% of tokens get rejected with 100/100 risk score because they're not valid SPL token mints.

---

### Issue 2: Birdeye API Rate Limiting

**Problem:** Free tier Birdeye API has strict rate limits.

**Error:**
```
Birdeye Token Overview API error: 429 Too Many Requests
Birdeye SOL Price API error: 429 Too Many Requests
```

**Impact:**
- Cannot fetch token price data
- Cannot calculate liquidity
- Risk analysis defaults to worst-case assumptions

**Solutions to Consider:**
- Implement request rate limiting/throttling in code
- Cache SOL price (doesn't change frequently)
- Upgrade to paid Birdeye tier for production
- Use alternative price sources (Jupiter, Raydium API)

---

### Issue 3: Jupiter API DNS Resolution Failure

**Problem:** Railway cannot resolve `quote-api.jup.ag` hostname.

**Error:**
```
dns error: failed to lookup address information: No address associated with hostname
```

**Impact:**
- Cannot check if tokens are sellable (honeypot detection)
- Cannot get swap quotes for trading

**Possible Causes:**
- Railway DNS configuration issue
- Temporary network issue
- May need to use IP address or alternative endpoint

**Solutions to Consider:**
- Try Jupiter v6 API: `https://api.jup.ag/`
- Use Jupiter Price API v2: `https://api.jup.ag/price/v2`
- Configure custom DNS in Railway
- Implement fallback endpoints

---

## Current Implementation Analysis

### Technical Flow (Current - Broken)

```
┌─────────────────────────────────────────────────────────────────┐
│                     CURRENT FLOW (PROBLEMATIC)                  │
└─────────────────────────────────────────────────────────────────┘

1. AutoTrader starts scan cycle (every 60 seconds)
          │
          ▼
2. Helius DAS API: searchAssets()
   - Searches for "assets" (NFTs, compressed NFTs, metadata)
   - Returns up to 50 items
   - Items are NOT tradeable SPL tokens
          │
          ▼
3. For each "token" returned:
   │
   ├──► Birdeye: Get token price/overview
   │    └── FAILS: Rate limited (429) or no data for invalid tokens
   │
   ├──► Birdeye: Get SOL price
   │    └── FAILS: Rate limited (429)
   │
   ├──► Solana RPC: Get mint account info
   │    └── FAILS: AccountNotFound (not a valid mint)
   │
   ├──► Jupiter: Check sellability
   │    └── FAILS: DNS resolution error
   │
   └──► Risk Score Calculation
        └── Result: 100/100 (all checks fail = max risk)
          │
          ▼
4. Token rejected: "Risk too high: 100/100"
          │
          ▼
5. No simulated positions created
```

### File Locations

| Component | File | Function |
|-----------|------|----------|
| Token Discovery | `src/api/helius.rs` | `get_recent_tokens()` |
| Risk Analysis | `src/trading/risk.rs` | `analyze_token()` |
| AutoTrader Loop | `src/trading/autotrader.rs` | `scan_for_tokens()` |
| Price Data | `src/api/birdeye.rs` | `get_token_overview()`, `get_sol_price_usd()` |
| Sellability Check | `src/api/jupiter.rs` | `get_quote()` |

---

## Target Platforms

### Primary Target: Pump.fun

**Platform Overview:**
- Pump.fun is the leading memecoin launchpad on Solana
- Uses a **bonding curve model** for initial token phase
- Tokens "graduate" at 100% bonding curve progress
- After graduation, tokens migrate to **PumpSwap** (pump.fun's native AMM DEX)

**Token Lifecycle:**
```
┌─────────────────────────────────────────────────────────────────┐
│                    PUMP.FUN TOKEN LIFECYCLE                     │
└─────────────────────────────────────────────────────────────────┘

Phase 1: Bonding Curve (0-100%)
├── Token is tradeable ONLY on pump.fun
├── Price determined by bonding curve formula
├── No DEX liquidity yet
└── High risk, high reward opportunity

          │
          ▼ (Token reaches 100% / ~$69k market cap)

Phase 2: Graduation
├── Token "graduates" from bonding curve
├── Liquidity automatically created on PumpSwap
├── Token becomes tradeable on PumpSwap AMM
└── Can be aggregated via Jupiter

          │
          ▼

Phase 3: Mature Trading
├── Full DEX liquidity on PumpSwap
├── May gain additional liquidity on Raydium/Orca
├── Aggregated across DEXes via Jupiter
└── Standard memecoin trading
```

**What We Need:**
1. **Pump.fun API** - To discover new tokens on bonding curve
2. **PumpSwap Integration** - To trade graduated tokens
3. **Bonding Curve Status** - To track graduation progress
4. **Real-time Events** - WebSocket for new token launches

---

### Secondary Target: BONK Ecosystem

**Platform Overview:**
- BONK is an established Solana memecoin (not pump.fun launched)
- Has its own ecosystem including **BonkSwap** (AMM DEX)
- Primary liquidity on **Raydium** and **Orca**
- Often used as a quote token for other memecoins

**Liquidity Distribution:**
| DEX | Pool | Approximate Liquidity |
|-----|------|----------------------|
| Raydium | BONK/SOL | High |
| Orca | BONK/SOL | ~$2M+ |
| BonkSwap | Various | Medium |
| Jupiter | Aggregated | Best execution |

**What We Need:**
1. **Raydium API** - Monitor BONK pairs and new pools
2. **Orca API** - Monitor BONK pairs
3. **BonkSwap Integration** - If targeting BONK ecosystem tokens
4. **New Pair Detection** - Alert when new tokens pair with BONK

---

## Proposed Architecture

### New Token Discovery Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    PROPOSED FLOW (MULTI-SOURCE)                 │
└─────────────────────────────────────────────────────────────────┘

                    ┌─────────────────┐
                    │  Token Sources  │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   Pump.fun API  │ │  Raydium API    │ │  Helius Webhooks│
│                 │ │                 │ │                 │
│ • New launches  │ │ • New pools     │ │ • Mint events   │
│ • Bonding curve │ │ • BONK pairs    │ │ • LP creation   │
│ • Graduations   │ │ • SOL pairs     │ │                 │
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                   │                   │
         └───────────────────┼───────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │ Token Processor │
                    │                 │
                    │ • Deduplication │
                    │ • Validation    │
                    │ • Prioritization│
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │  Risk Analysis  │
                    │                 │
                    │ • Liquidity     │
                    │ • Holders       │
                    │ • Honeypot      │
                    │ • Rug indicators│
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │ Trading Decision│
                    │                 │
                    │ DRY RUN:        │
                    │ • Simulate buy  │
                    │ • Track P&L     │
                    │                 │
                    │ LIVE:           │
                    │ • Execute swap  │
                    │ • Manage position│
                    └─────────────────┘
```

### Source Priority

| Priority | Source | Token Type | Risk Level |
|----------|--------|------------|------------|
| 1 | Pump.fun (Bonding Curve) | Pre-graduation memecoins | Very High |
| 2 | PumpSwap (Graduated) | Post-graduation memecoins | High |
| 3 | Raydium (New Pools) | New SOL/BONK pairs | Medium-High |
| 4 | Orca (New Pools) | New SOL/BONK pairs | Medium-High |

---

## Information Needed for Implementation

### Pump.fun Integration

**API Documentation Needed:**
- [ ] Pump.fun REST API endpoints (if available)
- [ ] Pump.fun WebSocket API for real-time events
- [ ] Bonding curve formula/calculation
- [ ] Graduation detection mechanism
- [ ] PumpSwap contract addresses
- [ ] PumpSwap swap instruction format

**Specific Questions:**
1. Does pump.fun have a public API? Or do we need to use on-chain data?
2. What is the program ID for pump.fun bonding curve contracts?
3. What is the PumpSwap AMM program ID?
4. How do we detect when a token graduates?
5. What are the swap fees on PumpSwap?

**On-Chain Data Needed:**
- Pump.fun program ID
- PumpSwap program ID
- Bonding curve account structure
- LP pool account structure

---

### Raydium/Orca Integration (for BONK pairs)

**API Documentation Needed:**
- [ ] Raydium SDK or API for new pool detection
- [ ] Raydium pool creation event monitoring
- [ ] Orca Whirlpool API for new pools
- [ ] Orca pool monitoring

**Specific Questions:**
1. How to subscribe to new pool creation events?
2. What's the best way to filter for BONK and SOL pairs only?
3. Rate limits on these APIs?
4. WebSocket vs polling approach?

---

### Price Data Sources

**Current Issues:**
- Birdeye rate limiting on free tier
- Need reliable SOL price
- Need token price for P&L calculation

**Alternative Sources to Evaluate:**
| Source | Pros | Cons |
|--------|------|------|
| Jupiter Price API | Free, aggregated | May not have new tokens |
| Raydium API | Direct from DEX | Only Raydium pools |
| Orca API | Direct from DEX | Only Orca pools |
| Pyth Network | Oracle price | Limited tokens |
| CoinGecko | Free tier available | Rate limits, delay |

---

### Helius Webhooks (Alternative Approach)

Instead of polling, use webhooks to get real-time notifications:

**Webhook Events to Monitor:**
- `TOKEN_MINT` - New token created
- `NFT_MINT` - Filter these out
- `SWAP` - New trading activity
- `CREATE_POOL` - New liquidity pool

**Questions:**
1. Can Helius webhooks filter by program ID (pump.fun, Raydium)?
2. What's the latency on webhook delivery?
3. How to handle webhook failures/retries?

---

## Technical Considerations

### Rate Limiting Strategy

```rust
// Proposed rate limiter configuration
struct RateLimitConfig {
    birdeye_requests_per_minute: u32,      // Free: 100, Paid: 1000+
    helius_requests_per_second: u32,        // Based on plan
    jupiter_requests_per_second: u32,       // Generally generous
    solana_rpc_requests_per_second: u32,    // Depends on provider
}

// Implementation: Token bucket or sliding window
```

### Caching Strategy

**What to Cache:**
| Data | TTL | Reason |
|------|-----|--------|
| SOL price | 10 seconds | Doesn't change rapidly |
| Token metadata | 5 minutes | Static data |
| Pool addresses | 1 hour | Rarely changes |
| Holder count | 1 minute | Changes with trades |

### Error Handling

**Graceful Degradation:**
- If Birdeye fails → Use Jupiter Price API
- If Jupiter fails → Use Raydium direct
- If all price sources fail → Skip token, don't assume worst case
- If RPC fails → Retry with backoff, then skip

---

## Next Steps

### Phase 1: Research & Documentation
- [ ] Research pump.fun API/on-chain structure
- [ ] Research PumpSwap contract details
- [ ] Document Raydium new pool detection methods
- [ ] Evaluate Helius webhook capabilities
- [ ] Test alternative price APIs

### Phase 2: Architecture Design
- [ ] Design multi-source token discovery system
- [ ] Design caching layer
- [ ] Design rate limiting system
- [ ] Design error handling strategy

### Phase 3: Implementation
- [ ] Implement pump.fun token discovery
- [ ] Implement PumpSwap trading integration
- [ ] Implement Raydium pool monitoring
- [ ] Implement improved risk analysis
- [ ] Update dry run simulation

### Phase 4: Testing
- [ ] Test with real pump.fun tokens
- [ ] Validate risk scoring accuracy
- [ ] Performance testing
- [ ] Error scenario testing

---

## Resources to Gather

### Official Documentation
- [ ] Pump.fun developer docs (if available)
- [ ] PumpSwap documentation
- [ ] Raydium SDK documentation
- [ ] Orca Whirlpool documentation
- [ ] Helius webhook documentation
- [ ] Jupiter API v6 documentation

### Program IDs Needed
- [ ] Pump.fun bonding curve program
- [ ] PumpSwap AMM program
- [ ] Raydium AMM v4 program
- [ ] Orca Whirlpool program

### Community Resources
- [ ] Pump.fun Discord/community for API info
- [ ] Solana developer Discord
- [ ] Example implementations/open source bots

---

## Notes

*This document will be updated as more information is gathered. The goal is to create a robust, multi-source token discovery system that can reliably find and evaluate new memecoin opportunities on Solana.*

---

**Document Version:** 1.0
**Last Updated:** 2026-01-10
**Author:** TraderTony Development Team
