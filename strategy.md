# TraderTony V4 Strategy Configuration Guide

This document explains the parameters available for configuring trading strategies in TraderTony V4. Strategies define the rules the bot uses to select tokens, enter positions, manage risk, and exit trades.

## Strategy Parameters Explained

Each strategy is defined by a set of parameters. You can manage strategies via the `/strategy` command in Telegram.

| Parameter                       | Type          | Description                                                                 | Default (Example) | Notes                                                                 |
| :------------------------------ | :------------ | :-------------------------------------------------------------------------- | :---------------- | :-------------------------------------------------------------------- |
| `id`                            | String (UUID) | Unique identifier automatically generated for the strategy.                 | (Auto-generated)  | Read-only.                                                            |
| `name`                          | String        | A user-friendly name for the strategy (e.g., "Aggressive", "Low Risk").     | "Default"         | Required when creating.                                               |
| `enabled`                       | Boolean       | If `true`, the AutoTrader will use this strategy for scanning and trading.  | `true`            |                                                                       |
| **Position Sizing & Budget**    |               |                                                                             |                   |                                                                       |
| `max_concurrent_positions`      | u32           | Maximum number of open positions allowed simultaneously for this strategy.  | 3                 | Prevents over-exposure.                                               |
| `max_position_size_sol`         | f64           | Maximum amount of SOL to use when entering a single position.               | 0.05              | Risk management per trade.                                            |
| `total_budget_sol`              | f64           | Total SOL allocated to this strategy across all its concurrent positions.   | 0.2               | Strategy stops opening new positions if this budget is fully utilized. |
| **Exit Conditions**             |               |                                                                             |                   |                                                                       |
| `stop_loss_percent`             | Option\<u32>  | Percentage below entry price to trigger an automatic sell (stop loss).      | `Some(15)`        | `None` disables stop loss.                                            |
| `take_profit_percent`           | Option\<u32>  | Percentage above entry price to trigger an automatic sell (take profit).    | `Some(50)`        | `None` disables take profit.                                          |
| `trailing_stop_percent`         | Option\<u32>  | Percentage below the *highest price reached* to trigger a trailing stop.    | `Some(5)`         | `None` disables trailing stop. Adjusts stop loss upwards as price rises. |
| `max_hold_time_minutes`         | u32           | Maximum duration (in minutes) to hold a position before forcing an exit.    | 240 (4 hours)     | Prevents holding stagnant positions indefinitely.                     |
| **Entry Filters (Token Selection)** |           |                                                                             |                   |                                                                       |
| `min_liquidity_sol`             | u32           | Minimum required liquidity (in SOL) for a token's primary pair.             | 10                | Filters out extremely illiquid tokens.                                |
| `max_risk_level`                | u32           | Maximum acceptable risk score (0-100) from the Risk Analyzer.               | 60                | Filters out tokens deemed too risky based on analysis.                |
| `min_holders`                   | u32           | Minimum number of unique token holders required.                            | 50                | Filters out tokens with very few holders (potential manipulation).    |
| `max_token_age_minutes`         | u32           | Maximum age (in minutes) since token creation to consider trading it.       | 120 (2 hours)     | Focuses on newly launched tokens.                                     |
| `require_lp_burned`             | Boolean       | If `true`, only trade tokens where LP tokens appear burned or locked.       | `true`            | Reduces rug pull risk.                                                |
| `reject_if_mint_authority`      | Boolean       | If `true`, reject tokens where the mint authority has not been revoked.     | `true`            | Reduces inflation risk.                                               |
| `reject_if_freeze_authority`    | Boolean       | If `true`, reject tokens where the freeze authority has not been revoked.   | `true`            | Reduces risk of accounts being frozen.                                |
| `require_can_sell`              | Boolean       | If `true`, require the token to pass the sellability (honeypot) check.      | `true`            | Avoids honeypots.                                                     |
| `max_transfer_tax_percent`      | Option\<f64>  | Maximum acceptable transfer tax (buy/sell). `None` disables this check.     | `Some(5.0)`       | Avoids tokens with excessive taxes.                                   |
| `max_concentration_percent`     | Option\<f64>  | Maximum acceptable % held by top N holders. `None` disables this check.   | `Some(60.0)`      | Avoids tokens heavily controlled by a few wallets.                    |
| **Transaction Parameters**      |               |                                                                             |                   |                                                                       |
| `slippage_bps`                  | Option\<u32>  | Slippage tolerance (in basis points, 100 = 1%) for Jupiter swaps.         | `None`            | Overrides global default if set.                                      |
| `priority_fee_micro_lamports`   | Option\<u64>  | Solana priority fee (in micro-lamports) for transactions.                 | `None`            | Overrides global default if set. Helps transaction inclusion.         |
| **Metadata**                    |               |                                                                             |                   |                                                                       |
| `created_at`                    | DateTime\<Utc>| Timestamp when the strategy was created.                                    | (Auto-generated)  | Read-only.                                                            |
| `updated_at`                    | DateTime\<Utc>| Timestamp when the strategy was last modified.                              | (Auto-updated)    | Read-only.                                                            |

## Strategy Templates (Examples)

These are starting points. Adjust them based on your risk tolerance and market analysis.

### Conservative

Prioritizes lower risk over high returns. Suitable for cautious traders or uncertain market conditions.

-   **Lower Risk Score:** `max_risk_level: 30`
-   **Higher Liquidity/Holders:** `min_liquidity_sol: 20`, `min_holders: 100`
-   **Smaller Positions/Budget:** `max_position_size_sol: 0.01`, `total_budget_sol: 0.1`
-   **Tighter Exits:** `stop_loss_percent: 10`, `take_profit_percent: 30`, `trailing_stop_percent: 3`
-   **Stricter Filters:** Enable all `reject_if...`, `require...` flags, low `max_transfer_tax_percent`, low `max_concentration_percent`.

### Balanced

A middle-ground approach aiming for moderate risk and reward. Uses default values as a base.

-   **Moderate Risk Score:** `max_risk_level: 60`
-   **Standard Liquidity/Holders:** `min_liquidity_sol: 10`, `min_holders: 50`
-   **Standard Positions/Budget:** `max_position_size_sol: 0.05`, `total_budget_sol: 0.2`
-   **Standard Exits:** `stop_loss_percent: 15`, `take_profit_percent: 50`, `trailing_stop_percent: 5`
-   **Standard Filters:** Enable most safety flags.

### Aggressive

Willing to take on higher risk for potentially higher (but less certain) rewards. Suitable for strong bull markets or higher risk tolerance.

-   **Higher Risk Score:** `max_risk_level: 75`
-   **Lower Liquidity/Holders:** `min_liquidity_sol: 5`, `min_holders: 30`
-   **Larger Positions/Budget:** `max_position_size_sol: 0.1`, `total_budget_sol: 0.5`
-   **Wider Exits:** `stop_loss_percent: 20`, `take_profit_percent: 100`, `trailing_stop_percent: 10`
-   **Looser Filters:** Maybe disable `reject_if_freeze_authority` or allow slightly higher tax/concentration, but keep `require_can_sell`. **Use extreme caution.**

## Strategy Optimization Tips

-   **Market Conditions:** Adjust parameters based on whether the market is bullish, bearish, or sideways. Aggressive strategies may perform better in bull runs, while conservative ones might be safer in bear markets.
-   **Risk Tolerance:** Align strategy parameters with your personal comfort level for risk. Don't use an aggressive strategy if you can't stomach potential losses.
-   **Backtesting (Future):** Ideally, strategies should be backtested against historical data to gauge potential performance (Note: Backtesting functionality is not yet implemented).
-   **Demo Mode:** **Always** test new or modified strategies extensively in Demo Mode (`DEMO_MODE=true` in `.env`) before using real funds.
-   **Start Small:** When transitioning to live trading, start with a very small `total_budget_sol` and `max_position_size_sol` and gradually increase as you gain confidence in the strategy's performance.
-   **Monitor Performance:** Regularly review the performance of your active strategies (PnL, win rate, etc.) using bot commands or external analysis. Adapt parameters based on results.
-   **Iterate:** Strategy optimization is an ongoing process. Don't be afraid to tweak parameters based on observations and changing market dynamics.

By carefully configuring and testing your strategies, you can tailor TraderTony V4 to your specific trading goals and risk profile.
