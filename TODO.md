# TraderTony V4 - TODO List

## Core Functionality
- [ ] **Risk Analysis (`risk.rs`):**
    - [ ] Implement `check_liquidity` (using DEX APIs/SDKs) - *Partially proxied via price check*
    - [ ] Implement `check_lp_tokens_burned` (find LP, check holders/burn address)
    - [ ] Implement `check_holder_distribution` (using RPC/Helius)
    - [ ] Implement `check_transfer_tax` (Token-2022/simulation)
    - [X] Implement `check_sellability` simulation (honeypot check) - *Basic simulation added*
    - [X] Implement `check_mint_freeze_authority` - *Implemented using `get_mint_info`*
- [ ] **Position Management (`position.rs`):**
    - [ ] Implement real-time price fetching for active positions.
    - [ ] Implement accurate PnL calculation based on current price.
    - [X] Implement exit condition checking (SL/TP/Trailing/Time) - *Basic checks added*
    - [X] Implement persistence (saving/loading `data/positions.json`) - *Basic implementation added*
    - [ ] Implement logic for handling partially filled orders (if applicable).
    - [ ] Implement transaction confirmation tracking for buys/sells.
- [ ] **AutoTrader Logic (`autotrader.rs`):**
    - [ ] Refactor `scan_for_opportunities` task loop logic (pass Arcs correctly).
    - [ ] Implement `execute_buy` logic (call Jupiter swap, create position).
    - [ ] Implement `execute_sell` logic (called by `PositionManager` or `AutoTrader`?).
    - [ ] Implement `start`/`stop`/`get_status` functionality fully (incl. task handling).
    - [ ] Implement strategy loading/persistence.
- [ ] **Solana Integration (`wallet.rs` / `client.rs`):**
    - [ ] Implement correct V0 `VersionedTransaction` signing in `wallet.rs`.
    - [ ] Add robust error handling and retries for RPC calls in `client.rs`.
    - [ ] Implement transaction confirmation logic in `client.rs` or `wallet.rs`.
- [ ] **Telegram Bot (`commands.rs` / `keyboards.rs`):**
    - [ ] Connect `/autotrader` commands to `AutoTrader::start/stop/status`.
    - [ ] Connect `/strategy` commands to `AutoTrader` strategy methods.
    - [ ] Connect `/positions` command to `PositionManager::get_active_positions`.
    - [ ] Implement full `/snipe` logic (analysis, buy execution).
    - [ ] Implement `/analyze` logic (call `RiskAnalyzer`).
    - [ ] Implement callback query handlers for inline keyboard buttons.
    - [ ] Implement notifications for trades, errors, etc.
    - [X] Address `ParseMode::Markdown` deprecation - *Switched to MarkdownV2*

## Refinements & Other
- [ ] Address remaining `cargo check` warnings (dead_code, etc.).
- [ ] Add comprehensive unit and integration tests.
- [ ] Improve error handling and reporting throughout.
- [ ] Add more detailed logging where needed.
- [ ] Create `LICENSE` file.
- [ ] Add comments and documentation to code.
