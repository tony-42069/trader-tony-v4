use thiserror::Error;

#[derive(Debug, Error)]
pub enum TraderbotError {
    #[error("Solana error: {0}")]
    SolanaError(String),
    
    #[error("Wallet error: {0}")]
    WalletError(String),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Token not found: {0}")]
    TokenNotFound(String),
    
    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),
    
    #[error("Transaction error: {0}")]
    TransactionError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Position error: {0}")]
    PositionError(String),
    
    #[error("Risk analysis error: {0}")]
    RiskAnalysisError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}
