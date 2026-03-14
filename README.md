# TraderTony V4

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.83%2B-orange.svg)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/Solana-1.17-blue.svg)](https://solana.com/)
[![Web App](https://img.shields.io/badge/Web-Dashboard-purple.svg)](https://agenttony.xyz)

An autonomous trading bot for Solana memecoins with multi-strategy token discovery, advanced risk analysis, and a cyberpunk-themed web dashboard.

**Website**: https://agenttony.xyz

---

## Features

- 🚀 **Autonomous Trading**: Automatically discovers and trades new tokens on Solana using three configurable strategies
- 🎯 **Multi-Strategy Engine**: Choose between New Pairs (sniper), Final Stretch (bonding curve momentum), or Migrated (graduated tokens)
- 🔎 **Advanced Risk Analysis**: Evaluates tokens for honeypot risks, mint/freeze authority, liquidity, holder distribution, and transfer taxes
- 📊 **Web Dashboard**: Cyberpunk-themed real-time dashboard with P&L charts, position tracking, and strategy controls
- 📋 **Copy Trading**: Follow TraderTony's trades with your own wallet (10% fee on profitable trades)
- 🤖 **Tokenized Agent ($TONY)**: Pump.fun token with automated buybacks funded by trading profits
- 📈 **Position Management**: Automated stop-loss, take-profit, and trailing stop with 15-second monitoring
- ⚙️ **Dry Run Mode**: Test strategies with real market data without executing actual trades
- 🔌 **WebSocket Real-Time Updates**: Live price updates and trade notifications
- 💰 **Multi-Wallet Support**: Connect Phantom, Solflare, or Backpack wallets

---

## Trading Strategies

| Strategy | Discovery | Risk Level | Key Criteria |
|----------|-----------|------------|--------------|
| **New Pairs** | WebSocket (ms-level) | Highest | Brand new tokens, risk score filtering, scam detection |
| **Final Stretch** | Moralis (30s polling) | Medium | 20-80% bonding progress, $15k+ volume, 50+ holders |
| **Migrated** | Moralis (30s polling) | Lower | Graduated tokens, $40k+ volume, 75+ holders |

### Strategy Details

**New Pairs (Sniper)**
- Catches tokens within milliseconds of creation on pump.fun
- Uses Pump.fun WebSocket CreateEvent monitoring
- Risk evaluation based on name quality, price sanity, scam patterns
- Position size: 0.1 SOL

**Final Stretch (Bonding Curve Momentum)**
- Targets tokens with traction on the bonding curve
- Minimum: $15k volume, $15k market cap, 50+ holders, 55%+ buy ratio
- Scanned every 30 seconds via Moralis API
- Position size: 0.1 SOL

**Migrated (Graduated Tokens)**
- Targets recently graduated tokens on PumpSwap/Raydium
- Minimum: $40k volume, $40k market cap, 75+ holders
- Scanned every 30 seconds via Moralis API
- Position size: 0.1 SOL

---

## Risk Management

Every token is evaluated before trading:

- **Mint Authority** — Rejected if mint authority exists
- **Freeze Authority** — Rejected if freeze authority exists
- **Liquidity** — Minimum threshold required (strategy-dependent)
- **LP Token Status** — Verified burned or locked on Raydium
- **Honeypot Detection** — Simulated buy/sell via Jupiter before trading
- **Holder Distribution** — Rejected if top holders exceed concentration threshold
- **Transfer Tax** — Detected via Token-2022 extension check
- **Buy/Sell Ratio** — Minimum healthy demand required for Final Stretch/Migrated
- **Unique Wallets** — Filters out wash trading patterns

Risk scores range from 0-100. Strategies only trade tokens below their configured risk threshold.

---

## Tokenized Agent ($TONY)

TraderTony is a Pump.fun Tokenized Agent. When the bot closes profitable trades, 100% of profits are automatically sent to the Agent Deposit Address for automated $TONY token buybacks and burns.

**How it works:**
1. Bot discovers, analyzes, and executes trades autonomously
2. When a position closes with profit, funds are transferred to the Agent Deposit Address
3. Pump.fun's Tokenized Agent system performs hourly buybacks of $TONY
4. Bought tokens are burned, permanently removing them from supply

**Revenue Model:**
- Revenue source: Trading profits from autonomous trading
- Buyback percentage: 100% (all profits)
- Buyback frequency: Hourly (via Pump.fun)
- Minimum deposit: 0.01 SOL

---

## Copy Trading

Follow TraderTony's trades with your own wallet on the web dashboard.

**Features:**
- Real-time trade signals visible on dashboard
- One-click copy trade execution via connected wallet
- Your keys, your coins — TraderTony never has access to your wallet
- 10% fee on profitable trades only (no fee on losses)
- Track your copy positions and P&L separately

**How to use:**
1. Visit https://agenttony.xyz
2. Connect your wallet (Phantom, Solflare, or Backpack)
3. Enable copy trading in the dashboard
4. Receive trade signals and execute copies manually

---

## Setup

### Prerequisites

- Rust 1.83+ (check `Cargo.toml` for MSRV)
- Helius API Key (token discovery and analysis)
- Birdeye API Key (price data and market metrics)
- Moralis API Key (bonding curve scanning for Final Stretch/Migrated)
- Solana RPC endpoint (Helius or QuickNode recommended)
- Solana Wallet Private Key (Base58 encoded) — **USE A BURNER WALLET**

### Environment Variables

```bash
# Solana Configuration
SOLANA_RPC_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_KEY
SOLANA_WS_URL=wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY
WALLET_PRIVATE_KEY=YOUR_BURNER_WALLET_PRIVATE_KEY_BASE58

# API Keys
HELIUS_API_KEY=YOUR_HELIUS_API_KEY
BIRDEYE_API_KEY=YOUR_BIRDEYE_API_KEY
MORALIS_API_KEY=YOUR_MORALIS_API_KEY

# Trading Configuration
DEMO_MODE=true
DEFAULT_SLIPPAGE_BPS=100
DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS=50000
DRY_RUN_MODE=false

# Web Server
API_HOST=0.0.0.0
API_PORT=3000
CORS_ORIGINS=*

# Tokenized Agent (Optional - for $TONY buybacks)
TOKENIZED_AGENT_ENABLED=false
AGENT_DEPOSIT_ADDRESS=YOUR_AGENT_DEPOSIT_ADDRESS
AGENT_TOKEN_MINT=YOUR_TONY_TOKEN_MINT
AGENT_BUYBACK_PERCENT=100.0
AGENT_MIN_DEPOSIT_SOL=0.01

# Copy Trading (Optional)
COPY_TRADE_FEE_PERCENT=10.0
```

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/tony-42069/trader-tony-v4.git
   cd trader-tony-v4
   ```

2. Create `.env` file with your configuration (see above)

3. Build the project:
   ```bash
   cargo build --release
   ```

4. Create data directory:
   ```bash
   mkdir data
   ```

5. Run TraderTony:
   ```bash
   ./target/release/trader-tony-v4
   ```

6. Open the dashboard:
   - Local: `webapp/index.html` (or use `python -m http.server 8080` in `webapp/`)
   - Production: https://agenttony.xyz

---

## API Endpoints

### Trading
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/wallet` | GET | Bot wallet address and balance |
| `/api/stats` | GET | Trading statistics |
| `/api/positions` | GET | Active positions |
| `/api/positions/active` | GET | Active positions only |
| `/api/trades` | GET | Trade history (paginated) |
| `/api/analyze` | POST | Token risk analysis |

### AutoTrader Control
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/autotrader/status` | GET | AutoTrader status |
| `/api/autotrader/start` | POST | Start autonomous trading |
| `/api/autotrader/stop` | POST | Stop autonomous trading |

### Strategy Management
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/strategies` | GET | List all strategies |
| `/api/strategies` | POST | Create new strategy |
| `/api/strategies/:id` | GET | Get strategy by ID |
| `/api/strategies/:id` | PUT | Update strategy |
| `/api/strategies/:id` | DELETE | Delete strategy |
| `/api/strategy/active` | GET | Get active strategy type |
| `/api/strategy/active` | POST | Set active strategy type |

### Watchlist
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/watchlist` | GET | Get watchlist tokens |
| `/api/watchlist/stats` | GET | Watchlist statistics |

### Copy Trading
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/signals` | GET | Recent trade signals |
| `/api/signals/active` | GET | Active signals (bot's open positions) |
| `/api/copy/register` | POST | Register wallet for copy trading |
| `/api/copy/register` | DELETE | Unregister wallet |
| `/api/copy/status` | GET | Copy trade status for wallet |
| `/api/copy/settings` | PUT | Update copy settings |
| `/api/copy/positions` | GET | User's copy positions |
| `/api/copy/stats` | GET | Copy trade statistics |
| `/api/copy/build-tx` | POST | Build copy trade transaction |

### Simulation
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/simulation/stats` | GET | Simulation statistics |
| `/api/simulation/positions` | GET | Simulated positions |
| `/api/simulation/clear` | POST | Clear all simulations |
| `/api/simulation/close/:id` | POST | Close simulated position |

### Real-Time
| Endpoint | Type | Description |
|----------|------|-------------|
| `/ws` | WebSocket | Real-time updates (positions, trades, prices) |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  VERCEL (Frontend)                                              │
│  https://agenttony.xyz                                          │
│  Cyberpunk dashboard with real-time charts                      │
└──────────────────────────┬──────────────────────────────────────┘
                           │ HTTPS / WSS
┌──────────────────────────▼──────────────────────────────────────┐
│  RAILWAY (Backend)                                              │
│  https://trader-tony.up.railway.app                             │
│  Rust/Axum REST API + WebSocket                                 │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ AutoTrader Engine                                        │    │
│  │  • Token Discovery (Helius, Moralis, Pump.fun WebSocket)│    │
│  │  • Risk Analysis (Rust)                                  │    │
│  │  • Trade Execution (Jupiter)                             │    │
│  │  • Position Management (15s monitoring)                  │    │
│  └─────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Copy Trade Manager                                       │    │
│  │  • Signal Broadcasting                                   │    │
│  │  • Transaction Building                                  │    │
│  │  • Fee Collection (10% on profits)                       │    │
│  └─────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Tokenized Agent Manager                                  │    │
│  │  • Profit Tracking                                       │    │
│  │  • Auto-deposit to Agent Address                         │    │
│  │  • Revenue Statistics                                    │    │
│  └─────────────────────────────────────────────────────────┘    │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
                    Solana Blockchain
```

---

## Development

### Project Structure

```
src/
├── main.rs                 # Application entry point
├── config.rs               # Configuration from .env
├── error.rs                # Custom error types
├── api/
│   ├── birdeye.rs          # Price data and market metrics
│   ├── helius.rs           # Token discovery via DAS API
│   ├── jupiter.rs          # Swap execution
│   └── moralis.rs          # Bonding curve scanning
├── models/                 # Data structures
├── solana/
│   ├── client.rs           # Solana RPC client
│   └── wallet.rs           # Wallet management
├── trading/
│   ├── autotrader.rs       # Main trading orchestrator
│   ├── position.rs         # Position management
│   ├── strategy.rs         # Strategy definitions
│   ├── risk.rs             # Risk analysis engine
│   ├── scanner.rs          # Final Stretch/Migrated scanner
│   ├── watchlist.rs        # Token watchlist
│   ├── simulation.rs       # Dry-run simulation
│   ├── pumpfun.rs          # Pump.fun integration
│   ├── pumpfun_monitor.rs  # WebSocket token discovery
│   └── graduation_monitor.rs # Bonding curve graduation
├── web/
│   ├── mod.rs              # AppState and module exports
│   ├── server.rs           # Axum server setup
│   ├── routes.rs           # Route definitions
│   ├── handlers.rs         # Request handlers
│   ├── websocket.rs        # WebSocket handler
│   └── models.rs           # Request/Response DTOs

webapp/
├── index.html              # Main dashboard HTML
├── css/terminal.css        # Cyberpunk theme
├── js/
│   ├── app.js              # Main dashboard logic
│   ├── api.js              # API client
│   ├── websocket.js        # WebSocket client
│   ├── wallet.js           # Wallet connection (Phantom/Solflare/Backpack)
│   ├── chart.js            # P&L chart (Chart.js)
│   └── config.js           # Frontend configuration
└── vercel.json             # Vercel deployment config

data/
├── positions.json          # Position persistence
└── strategies.json         # Strategy persistence
```

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Running Locally

```bash
# Terminal 1: Start backend
cargo run

# Terminal 2: Start frontend (optional)
cd webapp && python -m http.server 8080
# Visit http://localhost:8080
```

---

## Security & Disclaimer

- **USE AT YOUR OWN RISK.** Cryptocurrency trading involves significant risk. This bot is experimental software.
- **NEVER use your main wallet.** Always use a dedicated burner wallet with limited funds.
- **Demo Mode**: Start with `DEMO_MODE=true` to simulate trades without real funds.
- **Review Code**: Understand the code before running with real funds.
- **No Guarantees**: The developers provide no guarantee of profit or protection against loss.
- **Tokenized Agent Risks**: $TONY token buybacks are automated but not guaranteed. Token value may fluctuate.

---

## Contributing

Contributions, bug reports, and feature requests are welcome! Please feel free to open an issue or submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
