//! Request handlers for all API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use tracing::{error, info};

use super::models::*;
use super::websocket::WsMessage;
use super::AppState;
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
