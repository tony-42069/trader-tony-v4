/**
 * TraderTony V4 - Configuration
 * Environment-specific settings
 */

// API Base URL - Railway backend
window.API_BASE_URL = window.API_BASE_URL || 'https://trader-tony.up.railway.app';

// WebSocket URL - Railway backend
window.WS_URL = window.WS_URL || 'wss://trader-tony.up.railway.app/ws';

// Feature flags
window.FEATURES = {
    // Enable copy trading feature
    copyTrading: false,

    // Enable manual trading controls
    manualTrading: true,

    // Enable demo mode by default if backend is unavailable
    autoEnableDemoMode: true,

    // Show detailed debug logs in console
    debugMode: true,
};

// Network configuration
window.NETWORK_CONFIG = {
    // 'mainnet-beta' or 'devnet'
    network: 'mainnet-beta',

    // RPC endpoint (for wallet interactions)
    rpcEndpoint: 'https://api.mainnet-beta.solana.com',

    // Solscan base URL
    solscanBaseUrl: 'https://solscan.io',
};

// Copy trade settings
window.COPY_TRADE_CONFIG = {
    // Fee percentage on profits
    feePercent: 10,

    // Minimum SOL balance required
    minBalance: 0.1,

    // Maximum position size in SOL
    maxPositionSize: 5.0,
};

console.log('[Config] TraderTony V4 Dashboard Configuration loaded');
console.log('[Config] Network:', window.NETWORK_CONFIG.network);
console.log('[Config] Features:', window.FEATURES);
