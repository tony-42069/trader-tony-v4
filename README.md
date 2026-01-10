# TraderTony V4

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/Solana-1.17-blue.svg)](https://solana.com/)
[![Telegram Bot](https://img.shields.io/badge/Telegram-Bot-blue.svg)](https://core.telegram.org/bots)

An autonomous trading bot for Solana memecoins with advanced risk analysis, built in Rust.

## Features

- 🚀 **Autonomous Trading**: Automatically discovers and trades new tokens on the Solana blockchain using configurable strategies.
- 🔎 **Advanced Risk Analysis**: Evaluates tokens for common risks (mint/freeze authority, LP status, honeypot potential, holder concentration, transfer tax) using Helius and on-chain data.
- 📱 **Telegram Interface**: Control the bot, manage strategies, view positions, and receive notifications via Telegram.
- 📊 **Position Management**: Automatic take profit, stop loss, and trailing stop loss based on strategy settings. Includes max hold time limits.
- 📈 **Strategy Configuration**: Define multiple trading strategies with distinct risk parameters, budget allocation, and entry/exit rules. (Persistence planned).
- ⚙️ **Configuration**: Manage API keys, wallet details, and bot settings via a `.env` file.
- 🧪 **Demo Mode**: Simulate trading logic without executing real transactions on the blockchain.
- 🔍 **Dry Run Mode**: Scan real tokens and simulate trades with live price tracking - no actual execution.

## Setup

### Prerequisites

- Rust (latest stable recommended, check `Cargo.toml` for MSRV if specified)
- Solana CLI tools (optional, for wallet management)
- Telegram Bot Token (obtain from @BotFather on Telegram)
- Helius API Key (for token discovery and analysis)
- Solana Wallet Private Key (Base58 encoded) - **USE A BURNER WALLET FOR TESTING/DEVELOPMENT**

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/trader-tony-v4.git
   cd trader-tony-v4
   ```

2. Create a `.env` file from the example:
   ```bash
   # Create the file (e.g., on Linux/macOS)
   touch .env
   # Or create it manually on Windows
   ```
   Paste the following content into `.env` and fill in your details:
   ```dotenv
   # Telegram Configuration
   TELEGRAM_BOT_TOKEN=YOUR_TELEGRAM_BOT_TOKEN
   TELEGRAM_ADMIN_USER_ID=YOUR_TELEGRAM_USER_ID # Get from @userinfobot

   # Solana Configuration (Testnet/Mainnet)
   # Testnet Example:
   SOLANA_RPC_URL=https://api.testnet.solana.com
   SOLANA_WS_URL=wss://api.testnet.solana.com
   # Mainnet Example (use a reliable provider like Helius, QuickNode, Triton):
   # SOLANA_RPC_URL=YOUR_MAINNET_RPC_URL
   # SOLANA_WS_URL=YOUR_MAINNET_WS_URL
   WALLET_PRIVATE_KEY=YOUR_WALLET_PRIVATE_KEY_BASE58 # Use a burner wallet!

   # API Keys
   HELIUS_API_KEY=YOUR_HELIUS_API_KEY
   # JUPITER_API_KEY=YOUR_JUPITER_API_KEY # Optional, for potential future use

   # Trading Configuration (Defaults)
   DEMO_MODE=true # Set to false for real trading (USE WITH EXTREME CAUTION)
   DEFAULT_SLIPPAGE_BPS=100 # Default slippage (1%) if not set in strategy
   DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS=50000 # Default priority fee if not set in strategy

   # Optional: Strategy defaults (can be overridden per strategy)
   # MAX_POSITION_SIZE_SOL=0.05
   # TOTAL_BUDGET_SOL=0.2
   # DEFAULT_STOP_LOSS_PERCENT=15
   # DEFAULT_TAKE_PROFIT_PERCENT=50
   # DEFAULT_TRAILING_STOP_PERCENT=5
   # MAX_HOLD_TIME_MINUTES=240
   # MIN_LIQUIDITY_SOL=10
   # MAX_RISK_LEVEL=60
   # MIN_HOLDERS=50
   # MAX_TOKEN_AGE_MINUTES=120
   ```

3. Build the project:
   ```bash
   cargo build --release
   ```

4. Create data directory (for position persistence):
    ```bash
    mkdir data
    ```

5. Run TraderTony:
   ```bash
   ./target/release/trader-tony-v4
   ```
   The bot will start and connect to Telegram.

## Usage

Interact with the bot via Telegram using the following commands:

### Telegram Commands

- `/start` - Initialize the bot and show the main menu.
- `/help` - Display available commands.
- `/balance` - Show the current SOL balance of the bot's wallet.
- `/autotrader` - View AutoTrader status and start/stop controls.
- `/strategy` - View, add, or manage trading strategies.
- `/positions` - View currently open trading positions.
- `/analyze <token_address>` - Perform risk analysis on a specific token.
- `/snipe <token_address> [amount_sol]` - Manually buy a token (uses default strategy settings if not specified). Use with caution.

### Trading Strategies

Strategies define the rules for automatic trading. You can manage them via the `/strategy` command in Telegram. Key parameters include:

- **Risk Limits**: `max_risk_level`, `min_liquidity_sol`, `min_holders`, checks for mint/freeze authority, LP status, etc.
- **Budgeting**: `max_concurrent_positions`, `max_position_size_sol`, `total_budget_sol`.
- **Exits**: `stop_loss_percent`, `take_profit_percent`, `trailing_stop_percent`, `max_hold_time_minutes`.

*(Strategy persistence is currently basic - stored in `data/positions.json` indirectly. A dedicated strategy store is planned).*

## Security & Disclaimer

- **USE AT YOUR OWN RISK.** Cryptocurrency trading involves significant risk. This bot is experimental software.
- **NEVER use your main wallet.** Always use a dedicated burner wallet with limited funds for testing and operation.
- **Demo Mode**: Start with `DEMO_MODE=true` in your `.env` file to simulate trades without real funds.
- **Review Code**: Understand the code, especially trading and wallet logic, before running with real funds.
- **Configuration**: Double-check your `.env` settings, especially RPC URLs and API keys.
- **No Guarantees**: The developers provide no guarantee of profit or protection against loss.

## Development

### Project Structure

- `src/`
  - `main.rs`: Application entry point, initializes components, starts bot.
  - `config.rs`: Loads configuration from `.env`.
  - `error.rs`: Defines custom error types.
  - `api/`: Clients for external APIs (Helius, Jupiter).
  - `bot/`: Telegram bot interaction logic (commands, keyboards, handlers).
  - `models/`: Data structures (Token, User, Position, Strategy).
  - `solana/`: Solana blockchain interaction (RPC client, wallet management).
  - `trading/`: Core trading logic (AutoTrader, PositionManager, RiskAnalyzer, Strategy).
- `data/`: (Created at runtime) Stores persistent data like positions.

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```
*(Note: More comprehensive tests, including integration tests, are needed).*

## Contributing

Contributions, bug reports, and feature requests are welcome! Please feel free to open an issue or submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file (to be created) for details.
