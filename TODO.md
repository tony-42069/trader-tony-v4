# TraderTony V4 - TODO List

## Development Plan (Logical Order & Reasoning)

This plan outlines the logical steps for completing the remaining features, prioritizing core functionality and dependencies.

**Phase 1: Core Position Management & Execution** (Get the basic trading loop working reliably)
1.  [X] **Implement Real-time Price Fetching (`position.rs`):** Fetch prices for open positions (likely using `JupiterClient`). *(Implemented in `manage_positions_cycle`)*
    *   **Why:** Essential for knowing the current value of open positions. Needed for PnL calculation and exit condition checks (SL/TP/Trailing).
    *   **How:** Use `JupiterClient` to fetch token prices against SOL periodically within `PositionManager`.
2.  [X] **Implement Accurate PnL Calculation (`position.rs`):** *(Implemented in `manage_positions_cycle` & `close_position`)*
    *   **Why:** Directly depends on real-time prices. Needed for accurate status reporting and exit logic.
    *   **How:** Update `Position` struct and `PositionManager` methods to calculate PnL based on fetched prices.
3.  [ ] **Implement `execute_sell` Logic:**
    *   **Why:** The counterpart to `execute_buy`. Needed to close positions based on triggers or commands.
    *   **How:** Create a function (in `position.rs` or `autotrader.rs`?) using `JupiterClient::swap_token_to_sol` and `WalletManager`. *(Implemented as `PositionManager::execute_exit`)*
4.  [X] **Verify/Implement V0 Transaction Signing (`wallet.rs` / `jupiter.rs`):** *(Verified Jupiter client, implemented signing in WalletManager)*
    *   **Why:** Ensure buy/sell swaps use modern `VersionedTransaction`s for reliability.
    *   **How:** Review swap logic, confirm `VersionedTransaction` usage, and add `WalletManager::sign_versioned_transaction` if needed.
5.  [X] **Implement Transaction Confirmation Tracking (`position.rs` / `autotrader.rs`):** *(Confirmation logic exists in `SolanaClient::confirm_transaction`, integrated into `execute_buy_task` and `execute_exit`)*
    *   **Why:** Makes trading robust by waiting for on-chain confirmation. Avoids premature state updates.
    *   **How:** After sending tx in buy/sell functions, call `SolanaClient::confirm_transaction` before updating `Position` status.

**Phase 2: Enhancing Decision Making & Robustness** (Improve bot intelligence and stability)
6.  [ ] **Implement Remaining Risk Analysis Checks (`risk.rs`):**
    *   **Why:** Improves automated trading decisions with comprehensive risk assessment.
    *   **How:** Implement actual logic for `check_liquidity`, `check_lp_tokens_burned`, `check_holder_distribution`, `check_transfer_tax`.
7.  [ ] **Robust Error Handling & Retries (`solana/client.rs`):**
    *   **Why:** Makes RPC interactions less prone to temporary network issues.
    *   **How:** Wrap RPC calls with retry logic (e.g., `tokio-retry`).

**Phase 3: User Interface & Persistence** (Flesh out user interaction and save configurations)
8.  [ ] **Implement Full Telegram Commands/Callbacks (`bot/`):**
    *   **Why:** Provides the full user experience.
    *   **How:** Implement logic for `/snipe`, `/analyze`, strategy management callbacks, and notifications.
9.  [ ] **Implement Strategy Loading/Persistence (`autotrader.rs`, `models/strategy.rs`):**
    *   **Why:** Allows strategies to persist across restarts.
    *   **How:** Implement saving/loading strategies (e.g., to JSON file or `sled` DB).

**Phase 4: Refinements & Edge Cases** (Clean up and finalize)
10. [ ] **Address Remaining Warnings & Add Tests/Docs:**
    *   **Why:** Improves code quality, maintainability, and reliability.
    *   **How:** Fix `dead_code`, add tests, comments, `LICENSE`.
11. [ ] **Handle Partial Fills (`position.rs`):**
    *   **Why:** Addresses potential swap edge cases.
    *   **How:** Investigate Jupiter API for partial fill info and adjust position tracking if needed.

---

## Original TODO Items (Categorized)

## Core Functionality
- [ ] **Risk Analysis (`risk.rs`):**
    - [ ] Implement `check_liquidity` (using DEX APIs/SDKs) - *Partially proxied via price check*
    - [ ] Implement `check_lp_tokens_burned` (find LP, check holders/burn address)
    - [ ] Implement `check_holder_distribution` (using RPC/Helius)
    - [ ] Implement `check_transfer_tax` (Token-2022/simulation)
    - [X] Implement `check_sellability` simulation (honeypot check) - *Basic simulation added*
    - [X] Implement `check_mint_freeze_authority` - *Implemented using `get_mint_info`*
- [ ] **Position Management (`position.rs`):**
    - [X] Implement real-time price fetching for active positions. *(Implemented in `manage_positions_cycle`)*
    - [X] Implement accurate PnL calculation based on current price. *(Implemented in `manage_positions_cycle` & `close_position`)*
    - [X] Implement exit condition checking (SL/TP/Trailing/Time) - *Basic checks added*
    - [X] Implement persistence (saving/loading `data/positions.json`) - *Basic implementation added*
    - [ ] Implement logic for handling partially filled orders (if applicable).
    - [X] Implement transaction confirmation tracking for buys/sells. *(Logic exists, integrated)*
- [ ] **AutoTrader Logic (`autotrader.rs`):**
    - [ ] Refactor `scan_for_opportunities` task loop logic (pass Arcs correctly).
    - [ ] Implement `execute_buy` logic (call Jupiter swap, create position).
    - [X] Implement `execute_sell` logic (called by `PositionManager` or `AutoTrader`?). *(Implemented as `PositionManager::execute_exit`)*
    - [X] Implement `start`/`stop`/`get_status` functionality fully (incl. task handling). *(Refactored in `autotrader.rs`)*
    - [ ] Implement strategy loading/persistence.
- [ ] **Solana Integration (`wallet.rs` / `client.rs`):**
    - [X] Implement correct V0 `VersionedTransaction` signing in `wallet.rs`. *(Implemented)*
    - [ ] Add robust error handling and retries for RPC calls in `client.rs`.
    - [X] Implement transaction confirmation logic in `client.rs` or `wallet.rs`. *(Implemented in `SolanaClient`, integrated)*
- [ ] **Telegram Bot (`commands.rs` / `keyboards.rs`):**
    - [X] Connect `/autotrader` command & callbacks to `AutoTrader::start/stop/status`.
    - [X] Connect `/strategy` command to `AutoTrader::list_strategies`.
    - [X] Connect `/positions` command to `PositionManager::get_active_positions`.
    - [ ] Implement full `/snipe` logic (analysis, buy execution).
    - [ ] Implement `/analyze` logic (call `RiskAnalyzer`).
    - [X] Implement basic callback query handler structure & start/stop/menu logic.
    - [ ] Implement other callback handlers (strategy add/edit/delete, etc.).
    - [ ] Implement notifications for trades, errors, etc.
    - [X] Address `ParseMode::Markdown` deprecation - *Switched to MarkdownV2*

## Refinements & Other
- [ ] Address remaining `cargo check` warnings (dead_code, etc.).
- [ ] Add comprehensive unit and integration tests.
- [ ] Improve error handling and reporting throughout.
- [ ] Add more detailed logging where needed.
- [ ] Create `LICENSE` file.
- [ ] Add comments and documentation to code.
