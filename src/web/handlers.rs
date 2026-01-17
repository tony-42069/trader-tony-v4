//! Request handlers for all API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use tracing::{error, info, warn};

use super::models::*;
use super::websocket::WsMessage;
use super::AppState;
use crate::models::copy_trade::CopyTradeSettings;
use crate::trading::strategy::Strategy;

// ============================================================================
// Health Check
// ============================================================================

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now(),
    })
}

// ============================================================================
// Wallet
// ============================================================================

pub async fn get_wallet(
    State(state): State<AppState>,
) -> Result<Json<WalletResponse>, (StatusCode, Json<ErrorResponse>)> {
    let address = state.wallet_manager.get_public_key().to_string();

    // Get SOL balance
    let balance_sol = match state.solana_client.get_sol_balance(&state.wallet_manager.get_public_key()).await {
        Ok(balance) => balance,
        Err(e) => {
            error!("Failed to get wallet balance: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to get wallet balance".to_string(),
                    details: Some(e.to_string()),
                }),
            ));
        }
    };

    Ok(Json(WalletResponse { address, balance_sol }))
}

// ============================================================================
// Positions
// ============================================================================

pub async fn get_positions(
    State(state): State<AppState>,
) -> Result<Json<PositionsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;
    let positions = auto_trader.position_manager.get_all_positions().await;

    let position_responses: Vec<PositionResponse> = positions
        .iter()
        .map(|p| {
            // Calculate current value
            let current_value = p.current_price_sol * p.entry_token_amount;

            PositionResponse {
                id: p.id.clone(),
                token_address: p.token_address.clone(),
                token_name: p.token_name.clone(),
                token_symbol: p.token_symbol.clone(),
                strategy_id: p.strategy_id.clone(),
                entry_value_sol: p.entry_value_sol,
                current_value_sol: Some(current_value),
                token_amount: p.entry_token_amount,
                entry_price: p.entry_price_sol,
                current_price: Some(p.current_price_sol),
                pnl_percent: p.pnl_percent,
                pnl_sol: p.pnl_sol,
                status: format!("{}", p.status),
                opened_at: p.entry_time,
                closed_at: p.exit_time,
                exit_reason: Some(format!("{}", p.status)),
            }
        })
        .collect();

    let total = position_responses.len();

    Ok(Json(PositionsListResponse {
        positions: position_responses,
        total,
    }))
}

pub async fn get_active_positions(
    State(state): State<AppState>,
) -> Result<Json<PositionsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;
    let positions = auto_trader.position_manager.get_active_positions().await;

    let position_responses: Vec<PositionResponse> = positions
        .iter()
        .map(|p| {
            let current_value = p.current_price_sol * p.entry_token_amount;

            PositionResponse {
                id: p.id.clone(),
                token_address: p.token_address.clone(),
                token_name: p.token_name.clone(),
                token_symbol: p.token_symbol.clone(),
                strategy_id: p.strategy_id.clone(),
                entry_value_sol: p.entry_value_sol,
                current_value_sol: Some(current_value),
                token_amount: p.entry_token_amount,
                entry_price: p.entry_price_sol,
                current_price: Some(p.current_price_sol),
                pnl_percent: p.pnl_percent,
                pnl_sol: p.pnl_sol,
                status: format!("{}", p.status),
                opened_at: p.entry_time,
                closed_at: p.exit_time,
                exit_reason: Some(format!("{}", p.status)),
            }
        })
        .collect();

    let total = position_responses.len();

    Ok(Json(PositionsListResponse {
        positions: position_responses,
        total,
    }))
}

// ============================================================================
// Trades
// ============================================================================

pub async fn get_trades(
    State(state): State<AppState>,
    Query(query): Query<TradesQuery>,
) -> Result<Json<TradesListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(50).min(100);

    let auto_trader = state.auto_trader.lock().await;
    let positions = auto_trader.position_manager.get_all_positions().await;

    // Convert closed positions to trades
    let mut trades: Vec<TradeResponse> = positions
        .iter()
        .filter(|p| p.exit_time.is_some())
        .map(|p| TradeResponse {
            id: p.id.clone(),
            token_address: p.token_address.clone(),
            token_symbol: p.token_symbol.clone(),
            action: "sell".to_string(),
            amount_sol: p.exit_value_sol.unwrap_or(0.0),
            token_amount: p.entry_token_amount,
            price: p.exit_price_sol.unwrap_or(0.0),
            pnl_sol: p.pnl_sol,
            pnl_percent: p.pnl_percent,
            transaction_signature: p.exit_tx_signature.clone().unwrap_or_default(),
            timestamp: p.exit_time.unwrap_or(p.entry_time),
        })
        .collect();

    // Sort by timestamp descending
    trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total = trades.len();

    // Paginate
    let start = ((page - 1) * limit) as usize;
    let trades: Vec<TradeResponse> = trades.into_iter().skip(start).take(limit as usize).collect();

    Ok(Json(TradesListResponse {
        trades,
        total,
        page,
        limit,
    }))
}

// ============================================================================
// Statistics
// ============================================================================

pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<StatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.get_performance_stats().await {
        Ok(stats) => {
            let losing_trades = stats.total_trades.saturating_sub(stats.winning_trades);

            Ok(Json(StatsResponse {
                total_trades: stats.total_trades,
                winning_trades: stats.winning_trades,
                losing_trades,
                win_rate: stats.win_rate,
                total_pnl_sol: stats.total_pnl,
                avg_roi_percent: stats.avg_roi,
                total_volume_sol: stats.total_entry_value,
                best_trade_pnl: 0.0,  // TODO: Calculate from positions
                worst_trade_pnl: 0.0, // TODO: Calculate from positions
            }))
        }
        Err(e) => {
            error!("Failed to get performance stats: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to get statistics".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// Strategies
// ============================================================================

pub async fn list_strategies(
    State(state): State<AppState>,
) -> Result<Json<StrategiesListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;
    let strategies = auto_trader.list_strategies().await;

    let strategy_responses: Vec<StrategyResponse> = strategies
        .iter()
        .map(|s| StrategyResponse {
            id: s.id.clone(),
            name: s.name.clone(),
            enabled: s.enabled,
            max_concurrent_positions: s.max_concurrent_positions,
            max_position_size_sol: s.max_position_size_sol,
            total_budget_sol: s.total_budget_sol,
            stop_loss_percent: s.stop_loss_percent,
            take_profit_percent: s.take_profit_percent,
            trailing_stop_percent: s.trailing_stop_percent,
            max_hold_time_minutes: s.max_hold_time_minutes,
            min_liquidity_sol: s.min_liquidity_sol,
            max_risk_level: s.max_risk_level,
            min_holders: s.min_holders,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect();

    let total = strategy_responses.len();

    Ok(Json(StrategiesListResponse {
        strategies: strategy_responses,
        total,
    }))
}

pub async fn get_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StrategyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.get_strategy(&id).await {
        Some(s) => Ok(Json(StrategyResponse {
            id: s.id.clone(),
            name: s.name.clone(),
            enabled: s.enabled,
            max_concurrent_positions: s.max_concurrent_positions,
            max_position_size_sol: s.max_position_size_sol,
            total_budget_sol: s.total_budget_sol,
            stop_loss_percent: s.stop_loss_percent,
            take_profit_percent: s.take_profit_percent,
            trailing_stop_percent: s.trailing_stop_percent,
            max_hold_time_minutes: s.max_hold_time_minutes,
            min_liquidity_sol: s.min_liquidity_sol,
            max_risk_level: s.max_risk_level,
            min_holders: s.min_holders,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Strategy not found".to_string(),
                details: None,
            }),
        )),
    }
}

pub async fn create_strategy(
    State(state): State<AppState>,
    Json(req): Json<CreateStrategyRequest>,
) -> Result<Json<StrategyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = Utc::now();

    let strategy = Strategy {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        enabled: true,
        strategy_type: crate::trading::strategy::StrategyType::NewPairs,
        max_concurrent_positions: req.max_concurrent_positions.unwrap_or(5),
        max_position_size_sol: req.max_position_size_sol.unwrap_or(0.1),
        total_budget_sol: req.total_budget_sol.unwrap_or(1.0),
        stop_loss_percent: req.stop_loss_percent,
        take_profit_percent: req.take_profit_percent,
        trailing_stop_percent: req.trailing_stop_percent,
        max_hold_time_minutes: req.max_hold_time_minutes.unwrap_or(240),
        min_liquidity_sol: req.min_liquidity_sol.unwrap_or(10),
        max_risk_level: req.max_risk_level.unwrap_or(50),
        min_holders: req.min_holders.unwrap_or(50),
        max_token_age_minutes: 60,
        require_lp_burned: false,
        reject_if_mint_authority: true,
        reject_if_freeze_authority: true,
        require_can_sell: true,
        max_transfer_tax_percent: Some(5.0),
        max_concentration_percent: Some(50.0),
        min_volume_usd: None,
        min_market_cap_usd: None,
        min_bonding_progress: None,
        require_migrated: None,
        slippage_bps: None,
        priority_fee_micro_lamports: None,
        created_at: now,
        updated_at: now,
    };

    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.add_strategy(strategy.clone()).await {
        Ok(_) => {
            info!("Created strategy: {} ({})", strategy.name, strategy.id);
            Ok(Json(StrategyResponse {
                id: strategy.id,
                name: strategy.name,
                enabled: strategy.enabled,
                max_concurrent_positions: strategy.max_concurrent_positions,
                max_position_size_sol: strategy.max_position_size_sol,
                total_budget_sol: strategy.total_budget_sol,
                stop_loss_percent: strategy.stop_loss_percent,
                take_profit_percent: strategy.take_profit_percent,
                trailing_stop_percent: strategy.trailing_stop_percent,
                max_hold_time_minutes: strategy.max_hold_time_minutes,
                min_liquidity_sol: strategy.min_liquidity_sol,
                max_risk_level: strategy.max_risk_level,
                min_holders: strategy.min_holders,
                created_at: strategy.created_at,
                updated_at: strategy.updated_at,
            }))
        }
        Err(e) => {
            error!("Failed to create strategy: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to create strategy".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

pub async fn update_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateStrategyRequest>,
) -> Result<Json<StrategyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    // Get existing strategy
    let existing = match auto_trader.get_strategy(&id).await {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Strategy not found".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Update fields
    let updated = Strategy {
        id: existing.id.clone(),
        name: req.name.unwrap_or(existing.name),
        enabled: req.enabled.unwrap_or(existing.enabled),
        strategy_type: existing.strategy_type,
        max_concurrent_positions: req.max_concurrent_positions.unwrap_or(existing.max_concurrent_positions),
        max_position_size_sol: req.max_position_size_sol.unwrap_or(existing.max_position_size_sol),
        total_budget_sol: req.total_budget_sol.unwrap_or(existing.total_budget_sol),
        stop_loss_percent: req.stop_loss_percent.or(existing.stop_loss_percent),
        take_profit_percent: req.take_profit_percent.or(existing.take_profit_percent),
        trailing_stop_percent: req.trailing_stop_percent.or(existing.trailing_stop_percent),
        max_hold_time_minutes: req.max_hold_time_minutes.unwrap_or(existing.max_hold_time_minutes),
        min_liquidity_sol: req.min_liquidity_sol.unwrap_or(existing.min_liquidity_sol),
        max_risk_level: req.max_risk_level.unwrap_or(existing.max_risk_level),
        min_holders: req.min_holders.unwrap_or(existing.min_holders),
        max_token_age_minutes: existing.max_token_age_minutes,
        require_lp_burned: existing.require_lp_burned,
        reject_if_mint_authority: existing.reject_if_mint_authority,
        reject_if_freeze_authority: existing.reject_if_freeze_authority,
        require_can_sell: existing.require_can_sell,
        max_transfer_tax_percent: existing.max_transfer_tax_percent,
        max_concentration_percent: existing.max_concentration_percent,
        min_volume_usd: existing.min_volume_usd,
        min_market_cap_usd: existing.min_market_cap_usd,
        min_bonding_progress: existing.min_bonding_progress,
        require_migrated: existing.require_migrated,
        slippage_bps: existing.slippage_bps,
        priority_fee_micro_lamports: existing.priority_fee_micro_lamports,
        created_at: existing.created_at,
        updated_at: Utc::now(),
    };

    match auto_trader.update_strategy(updated.clone()).await {
        Ok(_) => {
            info!("Updated strategy: {} ({})", updated.name, updated.id);
            Ok(Json(StrategyResponse {
                id: updated.id,
                name: updated.name,
                enabled: updated.enabled,
                max_concurrent_positions: updated.max_concurrent_positions,
                max_position_size_sol: updated.max_position_size_sol,
                total_budget_sol: updated.total_budget_sol,
                stop_loss_percent: updated.stop_loss_percent,
                take_profit_percent: updated.take_profit_percent,
                trailing_stop_percent: updated.trailing_stop_percent,
                max_hold_time_minutes: updated.max_hold_time_minutes,
                min_liquidity_sol: updated.min_liquidity_sol,
                max_risk_level: updated.max_risk_level,
                min_holders: updated.min_holders,
                created_at: updated.created_at,
                updated_at: updated.updated_at,
            }))
        }
        Err(e) => {
            error!("Failed to update strategy: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to update strategy".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

pub async fn delete_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.delete_strategy(&id).await {
        Ok(_) => {
            info!("Deleted strategy: {}", id);
            Ok(Json(SuccessResponse {
                success: true,
                message: format!("Strategy {} deleted", id),
            }))
        }
        Err(e) => {
            error!("Failed to delete strategy {}: {}", id, e);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Failed to delete strategy".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

pub async fn toggle_strategy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.toggle_strategy(&id).await {
        Ok(new_status) => {
            let status_str = if new_status { "enabled" } else { "disabled" };
            info!("Toggled strategy {}: now {}", id, status_str);
            Ok(Json(SuccessResponse {
                success: true,
                message: format!("Strategy {} is now {}", id, status_str),
            }))
        }
        Err(e) => {
            error!("Failed to toggle strategy {}: {}", id, e);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Failed to toggle strategy".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// AutoTrader Control
// ============================================================================

pub async fn get_autotrader_status(
    State(state): State<AppState>,
) -> Result<Json<AutoTraderStatus>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    let running = auto_trader.get_status().await;
    let strategies = auto_trader.list_strategies().await;
    let active_strategies = strategies.iter().filter(|s| s.enabled).count();
    let positions = auto_trader.position_manager.get_active_positions().await;

    Ok(Json(AutoTraderStatus {
        running,
        demo_mode: state.config.demo_mode,
        dry_run_mode: state.config.dry_run_mode,
        active_strategies,
        active_positions: positions.len(),
    }))
}

pub async fn start_autotrader(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.start().await {
        Ok(_) => {
            info!("AutoTrader started via API");

            // Broadcast status change
            state.broadcast(WsMessage::StatusChange {
                running: true,
                timestamp: Utc::now(),
            });

            Ok(Json(SuccessResponse {
                success: true,
                message: "AutoTrader started".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to start AutoTrader: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to start AutoTrader".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

pub async fn stop_autotrader(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.stop().await {
        Ok(_) => {
            info!("AutoTrader stopped via API");

            // Broadcast status change
            state.broadcast(WsMessage::StatusChange {
                running: false,
                timestamp: Utc::now(),
            });

            Ok(Json(SuccessResponse {
                success: true,
                message: "AutoTrader stopped".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to stop AutoTrader: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to stop AutoTrader".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// Token Analysis
// ============================================================================

pub async fn analyze_token(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match auto_trader.risk_analyzer.analyze_token(&req.address).await {
        Ok(analysis) => {
            let risk_rating = match analysis.risk_level {
                0..=25 => "Low",
                26..=50 => "Medium",
                51..=75 => "High",
                _ => "Very High",
            };

            let recommendation = if analysis.risk_level <= 30 && analysis.can_sell && analysis.liquidity_sol >= 10.0 {
                "Consider trading with caution"
            } else if analysis.risk_level <= 50 && analysis.can_sell {
                "High risk - small position only"
            } else if !analysis.can_sell {
                "DO NOT TRADE - Cannot sell (honeypot)"
            } else {
                "Avoid - Too risky"
            };

            Ok(Json(AnalyzeResponse {
                token_address: analysis.token_address,
                risk_level: analysis.risk_level,
                risk_rating: risk_rating.to_string(),
                liquidity_sol: analysis.liquidity_sol,
                holder_count: analysis.holder_count,
                has_mint_authority: analysis.has_mint_authority,
                has_freeze_authority: analysis.has_freeze_authority,
                lp_tokens_burned: analysis.lp_tokens_burned,
                transfer_tax_percent: analysis.transfer_tax_percent,
                can_sell: analysis.can_sell,
                concentration_percent: analysis.concentration_percent,
                details: analysis.details,
                recommendation: recommendation.to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to analyze token {}: {}", req.address, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to analyze token".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// Copy Trade - Signals
// ============================================================================

/// Get all trade signals (recent)
pub async fn get_signals(
    State(state): State<AppState>,
) -> Result<Json<SignalsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let signals = state.copy_trade_manager.get_recent_signals(100).await;

    let signal_responses: Vec<SignalResponse> = signals
        .iter()
        .map(|s| SignalResponse {
            id: s.id.clone(),
            token_address: s.token_address.clone(),
            token_symbol: s.token_symbol.clone(),
            token_name: s.token_name.clone(),
            action: format!("{}", s.action),
            amount_sol: s.amount_sol,
            price_sol: s.price_sol,
            timestamp: s.timestamp,
            bot_position_id: s.bot_position_id.clone(),
            is_active: s.is_active,
            current_price_sol: s.current_price_sol,
            current_pnl_percent: s.current_pnl_percent,
        })
        .collect();

    let total = signal_responses.len();

    Ok(Json(SignalsListResponse {
        signals: signal_responses,
        total,
    }))
}

/// Get active signals (bot's current open positions)
pub async fn get_active_signals(
    State(state): State<AppState>,
) -> Result<Json<SignalsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let signals = state.copy_trade_manager.get_active_signals().await;

    let signal_responses: Vec<SignalResponse> = signals
        .iter()
        .map(|s| SignalResponse {
            id: s.id.clone(),
            token_address: s.token_address.clone(),
            token_symbol: s.token_symbol.clone(),
            token_name: s.token_name.clone(),
            action: format!("{}", s.action),
            amount_sol: s.amount_sol,
            price_sol: s.price_sol,
            timestamp: s.timestamp,
            bot_position_id: s.bot_position_id.clone(),
            is_active: s.is_active,
            current_price_sol: s.current_price_sol,
            current_pnl_percent: s.current_pnl_percent,
        })
        .collect();

    let total = signal_responses.len();

    Ok(Json(SignalsListResponse {
        signals: signal_responses,
        total,
    }))
}

// ============================================================================
// Copy Trade - Registration
// ============================================================================

/// Register a wallet for copy trading
pub async fn register_copy_trader(
    State(state): State<AppState>,
    Json(req): Json<CopyTradeRegisterRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .copy_trade_manager
        .register_trader(&req.wallet_address, &req.signature, &req.message)
        .await
    {
        Ok(_) => {
            info!("Registered copy trader: {}", req.wallet_address);
            Ok(Json(SuccessResponse {
                success: true,
                message: format!("Wallet {} registered for copy trading", req.wallet_address),
            }))
        }
        Err(e) => {
            warn!("Failed to register copy trader: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to register".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

/// Unregister a wallet from copy trading
pub async fn unregister_copy_trader(
    State(state): State<AppState>,
    Json(req): Json<CopyTradeRegisterRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .copy_trade_manager
        .unregister_trader(&req.wallet_address)
        .await
    {
        Ok(_) => {
            info!("Unregistered copy trader: {}", req.wallet_address);
            Ok(Json(SuccessResponse {
                success: true,
                message: format!("Wallet {} unregistered from copy trading", req.wallet_address),
            }))
        }
        Err(e) => {
            warn!("Failed to unregister copy trader: {}", e);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Failed to unregister".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// Copy Trade - Status & Settings
// ============================================================================

/// Get copy trade status for a wallet
pub async fn get_copy_trade_status(
    State(state): State<AppState>,
    Query(query): Query<CopyPositionsQuery>,
) -> Result<Json<CopyTradeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let trader = state.copy_trade_manager.get_trader(&query.wallet).await;
    let active_positions = state
        .copy_trade_manager
        .get_active_copy_positions(&query.wallet)
        .await;

    match trader {
        Some(t) => Ok(Json(CopyTradeStatusResponse {
            is_registered: true,
            wallet_address: Some(t.wallet_address),
            auto_copy_enabled: t.auto_copy_enabled,
            copy_amount_sol: t.copy_amount_sol,
            max_positions: t.max_positions,
            slippage_bps: t.slippage_bps,
            total_copy_trades: t.total_copy_trades,
            active_copy_positions: active_positions.len(),
            total_fees_paid_sol: t.total_fees_paid_sol,
        })),
        None => Ok(Json(CopyTradeStatusResponse {
            is_registered: false,
            wallet_address: None,
            auto_copy_enabled: false,
            copy_amount_sol: 0.1,
            max_positions: 5,
            slippage_bps: 300,
            total_copy_trades: 0,
            active_copy_positions: 0,
            total_fees_paid_sol: 0.0,
        })),
    }
}

/// Update copy trade settings
pub async fn update_copy_trade_settings(
    State(state): State<AppState>,
    Query(query): Query<CopyPositionsQuery>,
    Json(req): Json<CopyTradeSettingsRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get existing settings
    let trader = match state.copy_trade_manager.get_trader(&query.wallet).await {
        Some(t) => t,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not registered".to_string(),
                    details: None,
                }),
            ));
        }
    };

    let settings = CopyTradeSettings {
        auto_copy_enabled: req.auto_copy_enabled.unwrap_or(trader.auto_copy_enabled),
        copy_amount_sol: req.copy_amount_sol.unwrap_or(trader.copy_amount_sol),
        max_positions: req.max_positions.unwrap_or(trader.max_positions),
        slippage_bps: req.slippage_bps.unwrap_or(trader.slippage_bps),
    };

    match state
        .copy_trade_manager
        .update_settings(&query.wallet, settings)
        .await
    {
        Ok(_) => {
            info!("Updated copy trade settings for: {}", query.wallet);
            Ok(Json(SuccessResponse {
                success: true,
                message: "Settings updated".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to update settings: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to update settings".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

// ============================================================================
// Copy Trade - Positions
// ============================================================================

/// Get copy positions for a wallet
pub async fn get_copy_positions(
    State(state): State<AppState>,
    Query(query): Query<CopyPositionsQuery>,
) -> Result<Json<CopyPositionsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let positions = state
        .copy_trade_manager
        .get_copy_positions(&query.wallet)
        .await;

    // Filter by status if provided
    let filtered_positions: Vec<_> = match query.status.as_deref() {
        Some("open") => positions
            .into_iter()
            .filter(|p| p.status == crate::models::copy_trade::CopyPositionStatus::Open)
            .collect(),
        Some("closed") => positions
            .into_iter()
            .filter(|p| p.status == crate::models::copy_trade::CopyPositionStatus::Closed)
            .collect(),
        _ => positions,
    };

    let position_responses: Vec<CopyPositionResponse> = filtered_positions
        .iter()
        .map(|p| CopyPositionResponse {
            id: p.id.clone(),
            copier_wallet: p.copier_wallet.clone(),
            token_address: p.token_address.clone(),
            token_symbol: p.token_symbol.clone(),
            entry_price_sol: p.entry_price_sol,
            entry_amount_sol: p.entry_amount_sol,
            token_amount: p.token_amount,
            bot_position_id: p.bot_position_id.clone(),
            status: format!("{}", p.status),
            current_price_sol: None, // TODO: Fetch current price
            current_pnl_percent: None, // TODO: Calculate current PnL
            pnl_sol: p.pnl_sol,
            fee_paid_sol: p.fee_paid_sol,
            opened_at: p.opened_at,
            closed_at: p.closed_at,
        })
        .collect();

    let total = position_responses.len();

    Ok(Json(CopyPositionsListResponse {
        positions: position_responses,
        total,
    }))
}

/// Get copy trade statistics for a wallet
pub async fn get_copy_trade_stats(
    State(state): State<AppState>,
    Query(query): Query<CopyPositionsQuery>,
) -> Result<Json<CopyTradeStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let stats = state
        .copy_trade_manager
        .get_trader_stats(&query.wallet)
        .await;

    Ok(Json(CopyTradeStatsResponse {
        total_trades: stats.total_trades,
        winning_trades: stats.winning_trades,
        losing_trades: stats.losing_trades,
        win_rate: stats.win_rate,
        total_pnl_sol: stats.total_pnl_sol,
        total_fees_paid_sol: stats.total_fees_paid_sol,
        avg_pnl_percent: stats.avg_pnl_percent,
        best_trade_pnl_sol: stats.best_trade_pnl_sol,
        worst_trade_pnl_sol: stats.worst_trade_pnl_sol,
    }))
}

// ============================================================================
// Copy Trade - Transaction Builder
// ============================================================================

/// Build a copy trade transaction for the user to sign
pub async fn build_copy_transaction(
    State(state): State<AppState>,
    Json(req): Json<BuildCopyTxRequest>,
) -> Result<Json<BuildCopyTxResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get the signal
    let signal = match state.copy_trade_manager.get_signal(&req.signal_id).await {
        Some(s) => s,
        None => {
            return Ok(Json(BuildCopyTxResponse {
                success: false,
                transaction: None,
                error: Some("Signal not found".to_string()),
                estimated_output: None,
                estimated_fee: None,
                estimated_pnl: None,
            }));
        }
    };

    // For BUY signals
    if signal.action == crate::models::copy_trade::TradeAction::Buy {
        let amount_sol = req.amount_sol.unwrap_or(0.1);

        // TODO: Build actual Jupiter swap transaction
        // For now, return a placeholder response
        info!(
            "Building copy BUY tx for {} - {} SOL for {}",
            req.user_wallet, amount_sol, signal.token_symbol
        );

        // In production, this would:
        // 1. Get Jupiter quote
        // 2. Build swap transaction
        // 3. Return serialized transaction

        Ok(Json(BuildCopyTxResponse {
            success: true,
            transaction: Some("PLACEHOLDER_TX_BASE64".to_string()), // TODO: Real transaction
            error: None,
            estimated_output: Some(amount_sol / signal.price_sol), // Estimated token amount
            estimated_fee: None,
            estimated_pnl: None,
        }))
    }
    // For SELL signals
    else {
        // Get the copy position to sell
        let copy_position_id = match req.copy_position_id {
            Some(id) => id,
            None => {
                return Ok(Json(BuildCopyTxResponse {
                    success: false,
                    transaction: None,
                    error: Some("copy_position_id required for sell".to_string()),
                    estimated_output: None,
                    estimated_fee: None,
                    estimated_pnl: None,
                }));
            }
        };

        // Find the copy position
        let positions = state
            .copy_trade_manager
            .get_copy_positions(&req.user_wallet)
            .await;

        let copy_position = match positions.iter().find(|p| p.id == copy_position_id) {
            Some(p) => p,
            None => {
                return Ok(Json(BuildCopyTxResponse {
                    success: false,
                    transaction: None,
                    error: Some("Copy position not found".to_string()),
                    estimated_output: None,
                    estimated_fee: None,
                    estimated_pnl: None,
                }));
            }
        };

        // Calculate estimated values
        let exit_value = copy_position.token_amount * signal.price_sol;
        let pnl = exit_value - copy_position.entry_amount_sol;
        let fee = state
            .copy_trade_manager
            .calculate_fee(copy_position.entry_amount_sol, exit_value);

        info!(
            "Building copy SELL tx for {} - {} {} (est PnL: {} SOL, fee: {} SOL)",
            req.user_wallet,
            copy_position.token_amount,
            signal.token_symbol,
            pnl,
            fee
        );

        // TODO: Build actual Jupiter swap transaction with fee transfer

        Ok(Json(BuildCopyTxResponse {
            success: true,
            transaction: Some("PLACEHOLDER_TX_BASE64".to_string()), // TODO: Real transaction
            error: None,
            estimated_output: Some(exit_value - fee),
            estimated_fee: Some(fee),
            estimated_pnl: Some(pnl - fee),
        }))
    }
}

// ============================================================================
// Simulation (Dry Run Mode)
// ============================================================================

/// Get all simulated positions
pub async fn get_simulated_positions(
    State(state): State<AppState>,
) -> Result<Json<SimulatedPositionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    let positions = match &auto_trader.simulation_manager {
        Some(sim_mgr) => sim_mgr.get_positions().await,
        None => vec![],
    };

    let total = positions.len();
    let is_dry_run_mode = state.config.dry_run_mode;

    Ok(Json(SimulatedPositionsResponse {
        positions,
        total,
        dry_run_mode: is_dry_run_mode,
    }))
}

/// Get only open simulated positions
pub async fn get_open_simulated_positions(
    State(state): State<AppState>,
) -> Result<Json<SimulatedPositionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    let positions = match &auto_trader.simulation_manager {
        Some(sim_mgr) => sim_mgr.get_open_positions().await,
        None => vec![],
    };

    let total = positions.len();
    let is_dry_run_mode = state.config.dry_run_mode;

    Ok(Json(SimulatedPositionsResponse {
        positions,
        total,
        dry_run_mode: is_dry_run_mode,
    }))
}

/// Get simulation statistics
pub async fn get_simulation_stats(
    State(state): State<AppState>,
) -> Result<Json<SimulationStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    let stats = match &auto_trader.simulation_manager {
        Some(sim_mgr) => sim_mgr.get_stats().await,
        None => crate::models::SimulationStats::default(),
    };

    let is_dry_run_mode = state.config.dry_run_mode;

    Ok(Json(SimulationStatsResponse {
        stats,
        dry_run_mode: is_dry_run_mode,
    }))
}

/// Clear all simulated positions
pub async fn clear_simulation(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match &auto_trader.simulation_manager {
        Some(sim_mgr) => {
            match sim_mgr.clear().await {
                Ok(_) => {
                    info!("Cleared all simulated positions via API");
                    Ok(Json(SuccessResponse {
                        success: true,
                        message: "All simulated positions cleared".to_string(),
                    }))
                }
                Err(e) => {
                    error!("Failed to clear simulated positions: {}", e);
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Failed to clear simulated positions".to_string(),
                            details: Some(e.to_string()),
                        }),
                    ))
                }
            }
        }
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Simulation not enabled".to_string(),
                details: Some("DRY_RUN_MODE is not enabled".to_string()),
            }),
        )),
    }
}

/// Manually close a simulated position
pub async fn close_simulated_position(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auto_trader = state.auto_trader.lock().await;

    match &auto_trader.simulation_manager {
        Some(sim_mgr) => {
            match sim_mgr.close_position(&id).await {
                Ok(pos) => {
                    info!(
                        "Manually closed simulated position {} - P&L: {:.2}%",
                        pos.token_symbol,
                        pos.realized_pnl_percent.unwrap_or(0.0)
                    );
                    Ok(Json(SuccessResponse {
                        success: true,
                        message: format!(
                            "Position {} closed with P&L: {:.2}%",
                            pos.token_symbol,
                            pos.realized_pnl_percent.unwrap_or(0.0)
                        ),
                    }))
                }
                Err(e) => {
                    error!("Failed to close simulated position {}: {}", id, e);
                    Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse {
                            error: "Failed to close position".to_string(),
                            details: Some(e.to_string()),
                        }),
                    ))
                }
            }
        }
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Simulation not enabled".to_string(),
                details: Some("DRY_RUN_MODE is not enabled".to_string()),
            }),
        )),
    }
}
