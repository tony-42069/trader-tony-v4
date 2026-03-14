---
name: trader-tony
description: Autonomous Solana memecoin trading bot with multi-strategy token discovery, advanced risk analysis, and automated position management via Jupiter aggregator.
metadata:
  author: trader-tony
  version: "4.0"
---

# TraderTony V4

TraderTony is an autonomous Solana memecoin trading bot that discovers, analyzes, and trades new tokens using multiple strategies with automated risk management.

## What TraderTony Does

1. **Token Discovery** — Finds new tokens via Helius DAS API, Pump.fun WebSocket monitoring, and Moralis API scanning
2. **Risk Analysis** — Evaluates tokens for honeypot risks, liquidity depth, mint/freeze authorities, holder distribution, and transfer taxes
3. **Trade Execution** — Executes buy/sell swaps via Jupiter aggregator with configurable slippage
4. **Position Management** — Monitors positions every 15 seconds with automated stop-loss, take-profit, and trailing stop

## Deployment

- **Backend**: Railway (Rust/Axum API server)
- **Frontend**: Vercel (Cyberpunk-themed trading dashboard)
- **Dashboard**: Real-time P&L charts, position tracking, strategy controls

## Revenue Model

TraderTony generates revenue through profitable trading. When positions close with profit:

1. Profit is calculated (exit value - entry value)
2. 100% of profits are sent to the Agent Deposit Address
3. Funds are used for automated $TONY token buybacks and burns

Revenue deposits occur after each profitable trade with a minimum threshold of 0.01 SOL.

## Trading Strategies

### New Pairs (Sniper)
- Catches tokens within milliseconds of creation on pump.fun
- Uses WebSocket CreateEvent monitoring
- Risk evaluation based on name quality, price sanity, scam patterns
- Position size: 0.1 SOL

### Final Stretch (Bonding Curve Momentum)
- Targets tokens with traction on the bonding curve (20-80% progress)
- Minimum criteria: $15k volume, 50+ holders, 55%+ buy ratio
- Scanned every 30 seconds via Moralis API
- Position size: 0.1 SOL

### Migrated (Graduated Tokens)
- Targets recently graduated tokens on PumpSwap/Raydium
- Minimum criteria: $40k volume, 75+ holders, established liquidity
- Scanned every 30 seconds via Moralis API
- Position size: 0.1 SOL

## Risk Management

Every token is evaluated for:
- **Mint Authority** — Rejected if mint authority exists
- **Freeze Authority** — Rejected if freeze authority exists
- **Liquidity** — Minimum threshold required (strategy-dependent)
- **LP Token Status** — Verified burned or locked on Raydium
- **Honeypot Detection** — Simulated buy/sell via Jupiter before trading
- **Holder Distribution** — Rejected if top holders exceed concentration threshold
- **Transfer Tax** — Detected via Token-2022 extension check

Risk scores range from 0-100. Strategies only trade tokens below their risk threshold.

## Position Management

- **Stop Loss**: Automated sell if price drops below threshold
- **Take Profit**: Automated sell if price rises above target
- **Trailing Stop**: Dynamic stop that follows price upward
- **Max Hold Time**: Force exit after configured duration
- **Monitoring Interval**: 15 seconds per cycle

## Security

- Non-custodial: Bot uses dedicated burner wallet only
- All trades are on-chain and verifiable via Solscan
- Demo mode available for testing without real funds
- Never stores or transmits main wallet credentials

## Required Configuration

- Solana wallet private key (base58, burner wallet only)
- Helius API key (token discovery)
- Birdeye API key (price data)
- Moralis API key (bonding curve scanning)
- Solana RPC endpoint (Helius or QuickNode recommended)

## Links

- Website: https://agenttony.xyz
- GitHub: https://github.com/tony-42069/trader-tony-v4
- Network: Solana Mainnet-Beta
