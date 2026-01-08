# TraderTony V4 - Claude Code Implementation Guide
## Project Path: D:\AI Projects\trader-tony-v4
## GitHub Repo: https://github.com/tony-42069/trader-tony-v4

---

## ⚠️ IMPORTANT: WORKFLOW RULES

**Claude Code MUST submit a Pull Request after each meaningful change/modification.**

```
Workflow:
1. Create feature branch from main (e.g., feat/phase-1-api-server)
2. Make changes
3. Commit with clear message
4. Push branch
5. Create PR with description of changes
6. Wait for review/merge before starting next task
```

---

## CONTEXT FOR CLAUDE CODE

TraderTony V4 is an autonomous Solana memecoin trading bot written in Rust. The bot:
- Discovers new tokens via Helius DAS API
- Analyzes risk (honeypot detection, liquidity, authorities, etc.)
- Executes trades via Jupiter aggregator
- Manages positions with SL/TP/Trailing Stop

**CURRENT STATE**: Core trading logic is ~90% complete. Telegram interface exists but needs to be REMOVED and replaced with a REST API + Web Dashboard.

**GOAL**: Fully autonomous bot with:
1. Public webapp showing bot's trades and performance
2. Manual copy trade signals (users see and copy manually)
3. Auto-copy trade feature (users connect wallet, trades execute automatically)
4. 10% profit fee on auto-copy trade sells

**DEPLOYMENT TARGET**:
- Backend: Railway (Rust binary)
- Frontend: Vercel (Static HTML/JS)

---

## ARCHITECTURE OVERVIEW

```
┌─────────────────────────────────────────────────────────────────┐
│                    VERCEL (FRONTEND)                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Web Dashboard (HTML/JS)                     │   │
│  │  • Bot Status & Stats                                    │   │
│  │  • Active Positions                                      │   │
│  │  • Trade History                                         │   │
│  │  • Copy Trade Signals                                    │   │
│  │  • Wallet Connect (Phantom/Solflare)                     │   │
│  │  • Auto-Copy Toggle                                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────┬────────────────────────────────────┘
                             │ HTTPS / WSS
┌────────────────────────────▼────────────────────────────────────┐
│                    RAILWAY (BACKEND)                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Rust API Server (Axum)                      │   │
│  │  • REST API Endpoints                                    │   │
│  │  • WebSocket for Real-time Updates                       │   │
│  │  • Copy Trade Transaction Builder                        │   │
│  │  • User Wallet Tracking                                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              AutoTrader Engine                           │   │
│  │  • Token Discovery (Helius)                              │   │
│  │  • Risk Analysis                                         │   │
│  │  • Trade Execution (Jupiter)                             │   │
│  │  • Position Management                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                             │
                             ▼
                    Solana Blockchain
```

---

## PHASE 1: REMOVE TELEGRAM & ADD REST API
**Priority: CRITICAL**
**Branch: `feat/phase-1-remove-telegram-add-api`**

### Task 1.1: Update Cargo.toml Dependencies
**File: Cargo.toml**
**PR Title: "Remove Telegram, add Axum web framework"**

```toml
# REMOVE:
teloxide = { version = "0.12", features = ["macros"] }

# ADD:
axum = "0.7"
axum-extra = { version = "0.9", features = ["typed-header"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "fs"] }
tokio-tungstenite = "0.21"
futures-util = "0.3"
```

### Task 1.2: Delete Telegram Bot Module
**PR Title: "Remove Telegram bot module"**

```
DELETE entire src/bot/ directory:
- src/bot/mod.rs
- src/bot/commands.rs
- src/bot/keyboards.rs
- src/bot/notification.rs
```

### Task 1.3: Create Web API Module Structure
**PR Title: "Add web API module structure"**

```
CREATE src/web/ directory:
- src/web/mod.rs           # Module exports + AppState
- src/web/server.rs        # Axum server setup
- src/web/routes.rs        # Route definitions
- src/web/handlers.rs      # Request handlers
- src/web/websocket.rs     # WebSocket handler
- src/web/models.rs        # Request/Response DTOs
```

### Task 1.4: Implement AppState and Server
**File: src/web/mod.rs, src/web/server.rs**
**PR Title: "Implement API server with AppState"**

```rust
// src/web/mod.rs
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct AppState {
    pub auto_trader: Arc<Mutex<AutoTrader>>,
    pub wallet_manager: Arc<WalletManager>,
    pub solana_client: Arc<SolanaClient>,
    pub config: Arc<Config>,
    pub ws_tx: broadcast::Sender<WsMessage>,
}

// src/web/server.rs
// - Bind to API_HOST:API_PORT from config
// - Setup CORS for Vercel frontend domain
// - Mount all routes
// - Setup WebSocket upgrade
```

### Task 1.5: Implement Core REST Endpoints
**File: src/web/routes.rs, src/web/handlers.rs**
**PR Title: "Implement core REST API endpoints"**

```
GET  /api/health                - Health check (for Railway)
GET  /api/wallet                - Bot wallet address + SOL balance
GET  /api/positions             - All positions (active + closed)
GET  /api/positions/active      - Active positions only
GET  /api/trades                - Trade history (paginated)
GET  /api/stats                 - Performance statistics
GET  /api/strategies            - List strategies
POST /api/strategies            - Add strategy
PUT  /api/strategies/:id        - Update strategy
DELETE /api/strategies/:id      - Delete strategy
POST /api/autotrader/start      - Start autonomous trading
POST /api/autotrader/stop       - Stop autonomous trading
GET  /api/autotrader/status     - Get status
POST /api/analyze               - Analyze token {address: string}
```

### Task 1.6: Implement WebSocket Handler
**File: src/web/websocket.rs**
**PR Title: "Implement WebSocket for real-time updates"**

```rust
// WebSocket broadcasts these message types:
enum WsMessage {
    PositionOpened { position: Position },
    PositionClosed { position: Position, pnl: f64 },
    PriceUpdate { token: String, price: f64 },
    StatusChange { running: bool },
    Error { message: String },
    TradeSignal { signal: TradeSignal },  // For copy trading
}
```

### Task 1.7: Update main.rs
**File: src/main.rs**
**PR Title: "Update main.rs for web server"**

```rust
// Remove all bot:: imports
// Add web:: imports
// Initialize AppState
// Start Axum server
// Optionally auto-start AutoTrader based on AUTO_START_TRADING env
```

### Task 1.8: Update Config
**File: src/config.rs, .env**
**PR Title: "Add web server configuration"**

```rust
// Add to Config struct:
pub api_host: String,        // default: "0.0.0.0"
pub api_port: u16,           // default: 3000
pub cors_origins: Vec<String>,
pub auto_start_trading: bool,
pub treasury_wallet: String, // For collecting copy trade fees
pub copy_trade_fee_percent: f64, // default: 10.0
```

---

## PHASE 2: BUILD WEB DASHBOARD (FRONTEND)
**Priority: HIGH**
**Branch: `feat/phase-2-web-dashboard`**
**Location: Separate repo or `webapp/` folder (for Vercel)**

### Task 2.1: Create Frontend Project Structure
**PR Title: "Initialize web dashboard structure"**

```
CREATE webapp/ directory (or separate repo):
webapp/
├── index.html
├── css/
│   └── styles.css
├── js/
│   ├── app.js
│   ├── api.js
│   ├── websocket.js
│   └── wallet.js      # Solana wallet adapter
└── vercel.json        # Vercel config
```

### Task 2.2: Implement Dashboard HTML
**File: webapp/index.html**
**PR Title: "Implement main dashboard layout"**

```html
<!-- Sections:
1. Header
   - Logo/Name
   - Bot Status (Running/Stopped indicator)
   - Connect Wallet button

2. Stats Cards Row
   - Total Trades
   - Win Rate
   - Total PnL (SOL)
   - ROI %

3. Bot Wallet Card
   - Address (clickable to Solscan)
   - Balance

4. Active Positions Table
   - Token | Entry Price | Current Price | PnL % | Time Held

5. Trade History Table
   - Token | Action | Amount | PnL | Timestamp

6. Copy Trade Section
   - Active Signals (current bot holdings)
   - Auto-Copy Toggle (requires wallet connection)
   - Fee disclosure (10% of profits)

7. Admin Controls (if needed)
   - Start/Stop AutoTrader
   - Add Strategy
-->
```

### Task 2.3: Implement API Client
**File: webapp/js/api.js**
**PR Title: "Implement API client functions"**

```javascript
const API_BASE = 'https://your-railway-app.up.railway.app';

export async function getWallet() { ... }
export async function getPositions() { ... }
export async function getStats() { ... }
export async function getSignals() { ... }
export async function startAutoTrader() { ... }
export async function stopAutoTrader() { ... }
export async function analyzeToken(address) { ... }
```

### Task 2.4: Implement WebSocket Client
**File: webapp/js/websocket.js**
**PR Title: "Implement WebSocket connection"**

```javascript
// Connect to wss://your-railway-app.up.railway.app/ws
// Handle reconnection with exponential backoff
// Parse messages and update UI accordingly
// Trigger auto-copy trades when signals received (if enabled)
```

### Task 2.5: Implement Wallet Connection
**File: webapp/js/wallet.js**
**PR Title: "Implement Solana wallet adapter"**

```javascript
// Use @solana/wallet-adapter or direct Phantom integration
// Functions:
// - connectWallet() -> returns publicKey
// - disconnectWallet()
// - signTransaction(tx) -> returns signed tx
// - isConnected() -> boolean

// Store connected wallet in localStorage for persistence
```

### Task 2.6: Implement Dashboard Logic
**File: webapp/js/app.js**
**PR Title: "Implement main dashboard logic"**

```javascript
// On page load:
// 1. Check for saved wallet connection
// 2. Fetch initial data (wallet, positions, stats)
// 3. Connect WebSocket
// 4. Setup polling fallback (every 30s)
// 5. Initialize auto-copy state from localStorage
```

### Task 2.7: Style Dashboard
**File: webapp/css/styles.css**
**PR Title: "Add dashboard styling"**

```css
/* 
- Dark theme (#0d1117 background, similar to GitHub dark)
- Green for profits (#238636)
- Red for losses (#da3633)
- Cards with subtle borders
- Responsive grid layout
- Status indicators (pulsing dot for running)
*/
```

### Task 2.8: Vercel Configuration
**File: webapp/vercel.json**
**PR Title: "Add Vercel deployment config"**

```json
{
  "rewrites": [
    { "source": "/api/:path*", "destination": "https://your-railway-app.up.railway.app/api/:path*" }
  ],
  "headers": [
    {
      "source": "/(.*)",
      "headers": [
        { "key": "X-Content-Type-Options", "value": "nosniff" }
      ]
    }
  ]
}
```

---

## PHASE 3: COPY TRADE SYSTEM
**Priority: HIGH**
**Branch: `feat/phase-3-copy-trade`**

### Task 3.1: Create Copy Trade Models
**File: src/models/copy_trade.rs**
**PR Title: "Add copy trade data models"**

```rust
pub struct TradeSignal {
    pub id: String,
    pub token_address: String,
    pub token_symbol: String,
    pub action: TradeAction,  // Buy or Sell
    pub amount_sol: f64,
    pub price_sol: f64,
    pub timestamp: DateTime<Utc>,
    pub bot_position_id: String,
}

pub enum TradeAction {
    Buy,
    Sell,
}

pub struct CopyTrader {
    pub wallet_address: String,
    pub registered_at: DateTime<Utc>,
    pub auto_copy_enabled: bool,
    pub copy_amount_sol: f64,  // How much SOL to use per copy trade
}

pub struct CopyPosition {
    pub id: String,
    pub copier_wallet: String,
    pub token_address: String,
    pub entry_price_sol: f64,
    pub entry_amount_sol: f64,
    pub token_amount: f64,
    pub bot_position_id: String,
    pub status: CopyPositionStatus,
}
```

### Task 3.2: Add Copy Trade Endpoints
**File: src/web/routes.rs, src/web/handlers.rs**
**PR Title: "Add copy trade API endpoints"**

```
GET  /api/signals              - Get recent trade signals
GET  /api/signals/active       - Get active signals (bot's open positions)
POST /api/copy/register        - Register wallet for copy trading
DELETE /api/copy/register      - Unregister wallet
GET  /api/copy/status          - Get copy trade status for wallet
PUT  /api/copy/settings        - Update copy settings (amount, auto-copy)
POST /api/copy/build-tx        - Build copy trade transaction for user to sign
GET  /api/copy/positions       - Get user's copy positions
```

### Task 3.3: Implement Transaction Builder
**File: src/web/copy_trade.rs**
**PR Title: "Implement copy trade transaction builder"**

```rust
// Build transaction for user's copy trade:
// 1. Get Jupiter quote for user's amount
// 2. Build swap transaction
// 3. For SELL: Add instruction to transfer fee to treasury
// 4. Return serialized transaction for user to sign

pub async fn build_copy_buy_transaction(
    user_wallet: &Pubkey,
    token_address: &str,
    amount_sol: f64,
    slippage_bps: u32,
) -> Result<VersionedTransaction>

pub async fn build_copy_sell_transaction(
    user_wallet: &Pubkey,
    token_address: &str,
    token_amount: f64,
    entry_price: f64,  // To calculate profit
    fee_percent: f64,  // 10%
    treasury_wallet: &Pubkey,
) -> Result<VersionedTransaction>
```

### Task 3.4: Emit Signals from AutoTrader
**File: src/trading/autotrader.rs**
**PR Title: "Emit trade signals for copy trading"**

```rust
// When bot opens position:
// 1. Create TradeSignal with action=Buy
// 2. Broadcast via WebSocket
// 3. Store in signals history

// When bot closes position:
// 1. Create TradeSignal with action=Sell
// 2. Broadcast via WebSocket
// 3. Store in signals history
```

### Task 3.5: Implement Frontend Copy Trade UI
**File: webapp/js/app.js, webapp/index.html**
**PR Title: "Implement copy trade UI"**

```javascript
// Copy Trade Section Features:
// 1. Show active signals (bot's current holdings)
// 2. "Copy" button for each signal (manual)
// 3. Auto-copy toggle (requires connected wallet)
// 4. Settings: copy amount per trade
// 5. User's copy positions table
// 6. Fee disclosure banner

// Auto-copy flow:
// 1. Receive TradeSignal via WebSocket
// 2. If auto-copy enabled and wallet connected:
//    a. Call /api/copy/build-tx with signal details
//    b. Prompt user to sign transaction
//    c. Submit signed transaction
//    d. Track copy position
```

### Task 3.6: Fee Collection Logic
**File: src/web/copy_trade.rs**
**PR Title: "Implement profit fee collection"**

```rust
// Fee calculation on SELL:
// 1. Calculate profit = (exit_value - entry_value)
// 2. If profit > 0:
//    fee = profit * (fee_percent / 100)
//    Add transfer instruction: user -> treasury for fee amount
// 3. If profit <= 0:
//    No fee collected
```

---

## PHASE 4: DEPLOYMENT
**Priority: HIGH**
**Branch: `feat/phase-4-deployment`**

### Task 4.1: Railway Configuration
**File: railway.toml, Dockerfile (optional)**
**PR Title: "Add Railway deployment configuration"**

```toml
# railway.toml
[build]
builder = "nixpacks"

[deploy]
startCommand = "./target/release/trader-tony-v4"
healthcheckPath = "/api/health"
healthcheckTimeout = 30

[variables]
# Set in Railway dashboard, not in file
```

### Task 4.2: Environment Variables for Railway
**PR Title: "Document Railway environment setup"**

```
Required Railway Environment Variables:
- SOLANA_RPC_URL
- SOLANA_PRIVATE_KEY
- HELIUS_API_KEY
- BIRDEYE_API_KEY
- API_HOST=0.0.0.0
- API_PORT=3000 (or $PORT)
- CORS_ORIGINS=https://your-vercel-app.vercel.app
- DEMO_MODE=false (for production)
- TREASURY_WALLET=<your-fee-collection-wallet>
- COPY_TRADE_FEE_PERCENT=10
- AUTO_START_TRADING=true
```

### Task 4.3: Vercel Deployment
**PR Title: "Configure Vercel deployment"**

```
1. Connect webapp/ folder (or separate repo) to Vercel
2. Set build settings:
   - Framework: None (static)
   - Output Directory: webapp (or root if separate repo)
3. Add environment variables if needed
4. Configure custom domain (optional)
```

### Task 4.4: GitHub Actions CI (Optional)
**File: .github/workflows/ci.yml**
**PR Title: "Add CI workflow"**

```yaml
name: CI
on: [push, pull_request]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - run: cargo test
```

---

## PHASE 5: TESTING & LAUNCH
**Priority: HIGH**

### Task 5.1: Local Testing
```powershell
# Build and run locally
cd "D:\AI Projects\trader-tony-v4"
cargo build --release
$env:DEMO_MODE="true"
.\target\release\trader-tony-v4.exe

# Test API
curl http://localhost:3000/api/health
curl http://localhost:3000/api/wallet
```

### Task 5.2: Frontend Testing
```
1. Open webapp/index.html in browser (or use local server)
2. Verify wallet info displays
3. Test WebSocket connection
4. Test wallet connection (Phantom)
5. Verify copy trade UI works
```

### Task 5.3: Integration Testing
```
1. Deploy backend to Railway (staging)
2. Deploy frontend to Vercel (preview)
3. Test full flow:
   - Dashboard loads
   - Real-time updates via WebSocket
   - Connect wallet
   - Enable auto-copy
   - Simulate trade signal
   - Verify transaction build works
```

### Task 5.4: Production Launch
```
1. Switch DEMO_MODE=false
2. Start with small budget (0.1 SOL)
3. Monitor first few trades
4. Verify fee collection works
5. Gradually increase budget
```

---

## FILE CHANGES SUMMARY

### DELETE:
```
src/bot/mod.rs
src/bot/commands.rs
src/bot/keyboards.rs
src/bot/notification.rs
```

### CREATE:
```
src/web/mod.rs
src/web/server.rs
src/web/routes.rs
src/web/handlers.rs
src/web/websocket.rs
src/web/models.rs
src/web/copy_trade.rs
src/models/copy_trade.rs
webapp/index.html
webapp/css/styles.css
webapp/js/app.js
webapp/js/api.js
webapp/js/websocket.js
webapp/js/wallet.js
webapp/vercel.json
railway.toml
```

### MODIFY:
```
Cargo.toml
src/main.rs
src/config.rs
src/trading/autotrader.rs (add signal emission)
.env
```

---

## PR CHECKLIST TEMPLATE

When creating PRs, include:

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing Done
- [ ] Compiled successfully (`cargo build`)
- [ ] Tested locally
- [ ] No new warnings

## Checklist
- [ ] Code follows project style
- [ ] Self-reviewed code
- [ ] Added comments where needed
- [ ] Updated documentation if needed
```

---

## QUICK REFERENCE

### GitHub Repo
```
https://github.com/tony-42069/trader-tony-v4
```

### Local Project Path
```
D:\AI Projects\trader-tony-v4
```

### Existing .env Location
```
D:\AI Projects\trader-tony-v4\.env
```

### Key Existing Files (DO NOT MODIFY unless specified):
```
src/trading/autotrader.rs  - Core trading logic
src/trading/position.rs    - Position management
src/trading/risk.rs        - Risk analysis
src/trading/strategy.rs    - Strategy definitions
src/solana/client.rs       - RPC client
src/solana/wallet.rs       - Wallet management
src/api/jupiter.rs         - Jupiter swaps
src/api/helius.rs          - Token discovery
src/api/birdeye.rs         - Price data
```

---

## SUCCESS CRITERIA

### Phase 1 ✓ When:
- [ ] Compiles without telegram dependencies
- [ ] API server starts on port 3000
- [ ] All REST endpoints return valid JSON
- [ ] WebSocket broadcasts messages

### Phase 2 ✓ When:
- [ ] Dashboard loads and displays data
- [ ] Real-time updates work
- [ ] Wallet connection works

### Phase 3 ✓ When:
- [ ] Trade signals broadcast to clients
- [ ] Copy trade transactions build correctly
- [ ] Fee collection works on profitable sells

### Phase 4 ✓ When:
- [ ] Backend runs on Railway
- [ ] Frontend deploys to Vercel
- [ ] End-to-end flow works

### Phase 5 ✓ When:
- [ ] Demo mode completes full trading cycle
- [ ] Real mode executes first trade
- [ ] Copy trade feature works with real user

---

*Document Version: 2.0*
*Updated: January 7, 2026*
*For use with Claude Code on TraderTony V4 project*
