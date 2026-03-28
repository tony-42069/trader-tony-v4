# Multi-Strategy Implementation Plan

## Overview

Implement three mutually exclusive trading strategies for the TraderTony V4 bot:

1. **New Pairs** (Sniper) - EXISTING, DO NOT MODIFY
2. **Final Stretch** (Bonding Curve with Traction) - NEW
3. **Migrated** (Graduated to PumpSwap/Raydium) - NEW

User selects ONE strategy at a time via UI toggle.

---

## Strategy Criteria

### New Pairs (EXISTING - NO CHANGES)
- Catches tokens at 0% within milliseconds of creation
- Uses CreateEvent data directly from WebSocket
- Already working perfectly

### Final Stretch (NEW)
| Criteria | Threshold |
|----------|-----------|
| Age | 0-60 minutes |
| Holders | >= 50 |
| Volume 24h | >= $20,000 USD |
| Market Cap | >= $20,000 USD |
| Bonding Progress | >= 20% |
| Status | `complete = FALSE` |
| Position Size | 0.1 SOL |

### Migrated (NEW)
| Criteria | Threshold |
|----------|-----------|
| Age | 0-1440 minutes (24h) |
| Holders | >= 75 |
| Volume 24h | >= $40,000 USD |
| Market Cap | >= $40,000 USD |
| Status | `complete = TRUE` |
| Position Size | 0.1 SOL |

---

## Implementation Steps

### Step 1: Enhance Strategy Struct
**File:** `src/trading/strategy.rs`

Add new fields to Strategy struct:
```rust
pub strategy_type: StrategyType,        // New Pairs, Final Stretch, Migrated
pub min_volume_usd: Option<f64>,        // Minimum 24h volume in USD
pub min_market_cap_usd: Option<f64>,    // Minimum market cap in USD
pub min_bonding_progress: Option<f64>,  // Minimum bonding curve progress (0-100)
pub require_migrated: Option<bool>,     // TRUE = must be migrated, FALSE = must NOT be migrated
```

Add StrategyType enum:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StrategyType {
    NewPairs,      // Sniper - catches at creation
    FinalStretch,  // Bonding curve with traction
    Migrated,      // Graduated to PumpSwap/Raydium
}
```

### Step 2: Enhance Birdeye Client
**File:** `src/api/birdeye.rs`

Add new methods for v3 API endpoints:
```rust
// Market data endpoint (price, market cap, liquidity)
pub async fn get_market_data(&self, mint: &str) -> Result<MarketData>

// Trade data endpoint (volume, buy/sell counts, holder count)
pub async fn get_trade_data(&self, mint: &str) -> Result<TradeData>

// Combined convenience method
pub async fn get_token_data(&self, mint: &str) -> Result<TokenData>
```

New structs:
```rust
pub struct TokenData {
    pub holders: u64,
    pub volume_24h_usd: f64,
    pub market_cap_usd: f64,
    pub price_usd: f64,
    pub liquidity_usd: f64,
}
```

### Step 3: Create Watchlist Module
**File:** `src/trading/watchlist.rs` (NEW)

```rust
pub struct WatchlistToken {
    pub mint: String,
    pub bonding_curve: String,
    pub name: String,
    pub symbol: String,
    pub created_at: DateTime<Utc>,
    pub creator: Option<String>,
    pub last_checked: DateTime<Utc>,
    pub traded: bool,  // Whether we already traded this token
}

pub struct Watchlist {
    tokens: Arc<RwLock<HashMap<String, WatchlistToken>>>,
    max_size: usize,           // Max 500 tokens
    max_age_minutes: u64,      // 24 hours
}

impl Watchlist {
    pub async fn add_token(&self, token: WatchlistToken) -> Result<()>
    pub async fn get_tokens(&self) -> Vec<WatchlistToken>
    pub async fn remove_token(&self, mint: &str)
    pub async fn mark_as_traded(&self, mint: &str)
    pub async fn cleanup(&self)  // Remove old tokens
    pub async fn get_tokens_for_strategy(&self, strategy_type: &StrategyType) -> Vec<WatchlistToken>
}
```

### Step 4: Create Scanner Module
**File:** `src/trading/scanner.rs` (NEW)

```rust
pub struct Scanner {
    watchlist: Arc<Watchlist>,
    birdeye_client: Arc<BirdeyeClient>,
    rpc_client: Arc<SolanaRpcClient>,
    scan_interval: Duration,  // 15 seconds
}

impl Scanner {
    pub async fn start(&self, active_strategy: Arc<RwLock<StrategyType>>) -> Result<()>

    async fn scan_cycle(&self, strategy_type: &StrategyType) -> Result<Vec<ScanResult>>

    async fn evaluate_final_stretch(&self, token: &WatchlistToken) -> Result<Option<ScanResult>>

    async fn evaluate_migrated(&self, token: &WatchlistToken) -> Result<Option<ScanResult>>
}

pub struct ScanResult {
    pub token: WatchlistToken,
    pub birdeye_data: TokenData,
    pub bonding_state: Option<BondingCurveState>,
    pub meets_criteria: bool,
    pub rejection_reason: Option<String>,
}
```

### Step 5: Integrate Into AutoTrader
**File:** `src/trading/autotrader.rs`

Changes:
1. Add `active_strategy_type: Arc<RwLock<StrategyType>>` field
2. Add `watchlist: Arc<Watchlist>` field
3. Add `scanner: Arc<Scanner>` field (for Final Stretch/Migrated)
4. Modify token flow:
   - New Pairs: Current WebSocket flow (unchanged)
   - Final Stretch/Migrated: Scanner polls watchlist every 15 seconds

New methods:
```rust
pub async fn set_active_strategy(&self, strategy_type: StrategyType) -> Result<()>
pub async fn get_active_strategy(&self) -> StrategyType
```

Modified start() flow:
```rust
// In start():
match active_strategy_type {
    NewPairs => start_pumpfun_discovery(),  // Existing WebSocket
    FinalStretch | Migrated => start_scanner(),  // New scanner
}
```

### Step 6: Add API Endpoints
**File:** `src/api/routes.rs` (or wherever routes are defined)

New endpoints:
```
GET  /api/strategy/active          - Get active strategy type
POST /api/strategy/active          - Set active strategy type
GET  /api/watchlist                - Get watchlist tokens
GET  /api/watchlist/stats          - Get watchlist statistics
```

### Step 7: Update Frontend
**File:** `webapp/js/app.js` and `webapp/index.html`

Add strategy selector:
```html
<div class="strategy-selector">
    <label>Active Strategy:</label>
    <select id="strategySelector" onchange="App.setActiveStrategy(this.value)">
        <option value="NewPairs">New Pairs (Sniper)</option>
        <option value="FinalStretch">Final Stretch</option>
        <option value="Migrated">Migrated</option>
    </select>
</div>
```

Add JavaScript:
```javascript
async setActiveStrategy(strategyType) {
    await API.setActiveStrategy(strategyType);
    this.showToast(`Strategy changed to: ${strategyType}`, 'success');
    await this.loadAutotraderStatus();
}

async loadActiveStrategy() {
    const response = await API.getActiveStrategy();
    document.getElementById('strategySelector').value = response.strategy_type;
}
```

### Step 8: Logging Format

**Final Stretch candidates:**
```
🔥 [FINAL STRETCH] {name} ({symbol}) meeting criteria!
   Age: {age} min | Holders: {holders} | Volume: ${volume}
   Market Cap: ${mc} | Progress: {progress}%
```

**Migrated candidates:**
```
🚀 [MIGRATED] {name} ({symbol}) meeting criteria!
   Age: {age} min | Holders: {holders} | Volume: ${volume}
   Market Cap: ${mc} | Status: Graduated
```

---

## File Changes Summary

### CREATE (New Files)
| File | Purpose |
|------|---------|
| `src/trading/watchlist.rs` | Token watchlist management |
| `src/trading/scanner.rs` | Periodic scanner for Final Stretch/Migrated |

### MODIFY (Existing Files)
| File | Changes |
|------|---------|
| `src/trading/strategy.rs` | Add StrategyType enum, new fields |
| `src/api/birdeye.rs` | Add v3 API methods, TokenData struct |
| `src/trading/autotrader.rs` | Integrate watchlist, scanner, strategy selection |
| `src/trading/mod.rs` | Export new modules |
| `src/api/routes.rs` | Add strategy/watchlist endpoints |
| `webapp/index.html` | Add strategy selector dropdown |
| `webapp/js/app.js` | Add strategy switching logic |
| `webapp/js/api.js` | Add API methods for strategy endpoints |

---

## Rate Limiting Considerations

- Birdeye API: Check rate limit headers, implement exponential backoff on 429
- Batch requests where possible
- Cache responses for 10-15 seconds
- Scanner runs every 15 seconds, batches multiple tokens per cycle

---

## Testing Plan

1. Test New Pairs (existing) - verify unchanged behavior
2. Test Final Stretch:
   - Add tokens to watchlist via WebSocket discovery
   - Wait for scanner to pick up tokens meeting criteria
   - Verify simulated buys with correct logging
3. Test Migrated:
   - Same flow, but with graduated token criteria
4. Test strategy switching:
   - Switch between strategies via UI
   - Verify correct scanner/WebSocket activation
5. Test dry run mode for all strategies

---

## Implementation Order & Git Strategy

Create a new feature branch: `feature/multi-strategy`

Each step will have its own commit with a descriptive message:

| Step | Commit Message | Files |
|------|----------------|-------|
| 1 | `feat: add StrategyType enum and new strategy fields` | `src/trading/strategy.rs` |
| 2 | `feat: enhance Birdeye client with v3 API endpoints` | `src/api/birdeye.rs` |
| 3 | `feat: add watchlist module for token tracking` | `src/trading/watchlist.rs`, `src/trading/mod.rs` |
| 4 | `feat: add scanner module for Final Stretch/Migrated strategies` | `src/trading/scanner.rs`, `src/trading/mod.rs` |
| 5 | `feat: integrate watchlist and scanner into AutoTrader` | `src/trading/autotrader.rs` |
| 6 | `feat: add API endpoints for strategy selection and watchlist` | `src/api/routes.rs` |
| 7 | `feat: add strategy selector UI to frontend` | `webapp/index.html`, `webapp/js/app.js`, `webapp/js/api.js` |
| 8 | `test: verify all three strategies in dry run mode` | Testing only |

After all commits, push branch and optionally create PR.

---

## Notes

- Keep New Pairs (sniper) COMPLETELY UNCHANGED
- Position size is 0.1 SOL for ALL strategies
- DRY_RUN mode should work for all strategies
- Only ONE strategy active at a time
- Do NOT implement "Global Fees Paid" - skip this metric
- Use existing Birdeye API key from .env (BIRDEYE_API_KEY)
