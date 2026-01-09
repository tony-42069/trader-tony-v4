# TraderTony V4

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.78%2B-orange.svg)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/Solana-Mainnet-blue.svg)](https://solana.com/)

An autonomous trading bot for Solana memecoins with REST API, web dashboard, and copy trading.

## Features

- **Autonomous Trading**: Automatically discovers and trades new tokens on Solana using configurable strategies
- **REST API**: Full HTTP API for controlling the bot and retrieving data
- **Web Dashboard**: Real-time dashboard showing performance, positions, and controls
- **Copy Trading**: Users can copy bot trades with automatic 10% profit fee
- **Risk Analysis**: Evaluates tokens for common risks (mint/freeze authority, LP status, honeypot, holder concentration)
- **Position Management**: Automatic take profit, stop loss, and trailing stop loss
- **Demo Mode**: Simulate trading without executing real transactions

## Architecture

```
Frontend (Vercel)          Backend (Railway)
┌──────────────────┐      ┌──────────────────┐
│  Web Dashboard   │─────▶│   REST API       │
│  - Stats         │ HTTP │   - /api/*       │
│  - Positions     │◀─────│                  │
│  - Copy Trade    │ WSS  │   WebSocket      │
└──────────────────┘      │   - Real-time    │
                          │                  │
                          │   AutoTrader     │
                          │   - Scanning     │
                          │   - Trading      │
                          └────────┬─────────┘
                                   │
                                   ▼
                            Solana Blockchain
```

## Quick Start

### Prerequisites

- Rust 1.78+ (for Cargo.lock v4 support)
- Helius API Key
- Birdeye API Key
- Solana Wallet Private Key (Base58) - **USE A BURNER WALLET**

### Local Development

```bash
# Clone and setup
git clone https://github.com/tony-42069/trader-tony-v4.git
cd trader-tony-v4
cp .env.example .env  # Fill in your API keys

# Build and run
cargo build --release
mkdir data
./target/release/trader-tony-v4

# Open dashboard
cd webapp && python -m http.server 8080
# Navigate to http://localhost:8080
```

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `SOLANA_RPC_URL` | Yes | Helius/QuickNode RPC endpoint |
| `SOLANA_PRIVATE_KEY` | Yes | Bot wallet private key (base58) |
| `HELIUS_API_KEY` | Yes | Helius API key |
| `BIRDEYE_API_KEY` | Yes | Birdeye API key |
| `DEMO_MODE` | No | Set to `true` for simulation (default: false) |
| `API_PORT` | No | API port (default: 3030) |

See `.env.example` for all options.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/wallet` | GET | Wallet balance |
| `/api/stats` | GET | Trading statistics |
| `/api/positions` | GET | Current positions |
| `/api/config` | GET/PUT | AutoTrader config |
| `/api/autotrader/start` | POST | Start trading |
| `/api/autotrader/stop` | POST | Stop trading |
| `/api/signals` | GET | Trade signals |
| `/api/copy/register` | POST | Register for copy trading |
| `/ws` | WebSocket | Real-time updates |

## Deployment

See [DEPLOYMENT.md](DEPLOYMENT.md) for full Railway + Vercel deployment guide.

## Security

- **USE AT YOUR OWN RISK** - Cryptocurrency trading involves significant risk
- **NEVER use your main wallet** - Always use a dedicated burner wallet
- **Start with Demo Mode** - Test thoroughly before live trading
- **Review the code** - Understand the trading logic before deploying

## License

MIT License - see LICENSE file for details.
