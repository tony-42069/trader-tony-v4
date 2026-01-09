/**
 * TraderTony V4 - API Client
 * Handles all HTTP communication with the Rust backend
 */

const API = {
    // Configuration
    baseUrl: null,
    demoMode: false,

    /**
     * Initialize the API client
     * @param {string} baseUrl - Backend API base URL
     */
    init(baseUrl = null) {
        // Auto-detect API URL based on environment
        if (baseUrl) {
            this.baseUrl = baseUrl;
        } else if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
            // Development - connect to local Rust server
            this.baseUrl = 'http://127.0.0.1:3030';
        } else {
            // Production - use Railway backend directly (Vercel can't proxy reliably)
            this.baseUrl = window.API_BASE_URL || 'https://trader-tony.up.railway.app';
        }

        console.log(`[API] Initialized with base URL: ${this.baseUrl}`);
    },

    /**
     * Enable demo mode with mock data
     */
    enableDemoMode() {
        this.demoMode = true;
        console.log('[API] Demo mode enabled');
    },

    /**
     * Disable demo mode
     */
    disableDemoMode() {
        this.demoMode = false;
        console.log('[API] Demo mode disabled');
    },

    /**
     * Make an API request
     * @param {string} endpoint - API endpoint
     * @param {object} options - Fetch options
     * @returns {Promise<object>} Response data
     */
    async request(endpoint, options = {}) {
        // Return mock data in demo mode
        if (this.demoMode) {
            return this.getMockData(endpoint, options);
        }

        const url = `${this.baseUrl}${endpoint}`;
        const defaultOptions = {
            headers: {
                'Content-Type': 'application/json',
            },
        };

        const mergedOptions = {
            ...defaultOptions,
            ...options,
            headers: {
                ...defaultOptions.headers,
                ...options.headers,
            },
        };

        try {
            const response = await fetch(url, mergedOptions);

            if (!response.ok) {
                const errorData = await response.json().catch(() => ({}));
                throw new APIError(
                    errorData.error || `HTTP ${response.status}`,
                    response.status,
                    errorData
                );
            }

            return await response.json();
        } catch (error) {
            if (error instanceof APIError) {
                throw error;
            }
            // Network error or other issue
            throw new APIError(
                `Network error: ${error.message}`,
                0,
                { originalError: error.message }
            );
        }
    },

    /**
     * GET request helper
     */
    async get(endpoint) {
        return this.request(endpoint, { method: 'GET' });
    },

    /**
     * POST request helper
     */
    async post(endpoint, data = {}) {
        return this.request(endpoint, {
            method: 'POST',
            body: JSON.stringify(data),
        });
    },

    /**
     * PUT request helper
     */
    async put(endpoint, data = {}) {
        return this.request(endpoint, {
            method: 'PUT',
            body: JSON.stringify(data),
        });
    },

    /**
     * DELETE request helper
     */
    async delete(endpoint, data = {}) {
        return this.request(endpoint, {
            method: 'DELETE',
            body: JSON.stringify(data),
        });
    },

    // ==========================================
    // Health & Status Endpoints
    // ==========================================

    /**
     * Check API health
     */
    async healthCheck() {
        return this.get('/api/health');
    },

    /**
     * Get full bot status
     */
    async getStatus() {
        return this.get('/api/status');
    },

    // ==========================================
    // Wallet Endpoints
    // ==========================================

    /**
     * Get bot wallet information
     */
    async getWallet() {
        return this.get('/api/wallet');
    },

    /**
     * Get wallet balance
     */
    async getBalance() {
        return this.get('/api/wallet/balance');
    },

    // ==========================================
    // Trading Endpoints
    // ==========================================

    /**
     * Get all active positions
     */
    async getPositions() {
        return this.get('/api/positions');
    },

    /**
     * Get trade history
     * @param {number} limit - Maximum number of trades to return
     */
    async getTrades(limit = 50) {
        return this.get(`/api/trades?limit=${limit}`);
    },

    /**
     * Get trading statistics
     */
    async getStats() {
        return this.get('/api/stats');
    },

    // ==========================================
    // AutoTrader Endpoints
    // ==========================================

    /**
     * Get autotrader status
     */
    async getAutotraderStatus() {
        return this.get('/api/autotrader/status');
    },

    /**
     * Start the autotrader
     */
    async startAutotrader() {
        return this.post('/api/autotrader/start');
    },

    /**
     * Stop the autotrader
     */
    async stopAutotrader() {
        return this.post('/api/autotrader/stop');
    },

    // ==========================================
    // Token Analysis Endpoints
    // ==========================================

    /**
     * Analyze a token
     * @param {string} mint - Token mint address
     */
    async analyzeToken(mint) {
        return this.post('/api/analyze', { mint });
    },

    // ==========================================
    // Copy Trade Endpoints
    // ==========================================

    /**
     * Get trade signals (recent)
     */
    async getSignals() {
        return this.get('/api/signals');
    },

    /**
     * Get active signals (bot's current open positions)
     */
    async getActiveSignals() {
        return this.get('/api/signals/active');
    },

    /**
     * Register wallet for copy trading
     * @param {string} walletAddress - User's wallet address
     * @param {string} signature - Signed message for verification
     * @param {string} message - Original signed message
     */
    async registerCopyTrader(walletAddress, signature, message) {
        return this.post('/api/copy/register', {
            wallet_address: walletAddress,
            signature,
            message,
        });
    },

    /**
     * Unregister from copy trading
     * @param {string} walletAddress - User's wallet address
     */
    async unregisterCopyTrader(walletAddress) {
        return this.delete('/api/copy/register', {
            wallet_address: walletAddress,
        });
    },

    /**
     * Get copy trade status for a wallet
     * @param {string} walletAddress - User's wallet address
     */
    async getCopyTradeStatus(walletAddress) {
        return this.get(`/api/copy/status?wallet=${walletAddress}`);
    },

    /**
     * Update copy trade settings
     * @param {string} walletAddress - User's wallet address
     * @param {object} settings - Copy trade settings
     */
    async updateCopyTradeSettings(walletAddress, settings) {
        return this.put(`/api/copy/settings?wallet=${walletAddress}`, settings);
    },

    /**
     * Get copy positions for a wallet
     * @param {string} walletAddress - User's wallet address
     * @param {string} status - Optional status filter (open/closed)
     */
    async getCopyPositions(walletAddress, status = null) {
        let url = `/api/copy/positions?wallet=${walletAddress}`;
        if (status) url += `&status=${status}`;
        return this.get(url);
    },

    /**
     * Get copy trade statistics for a wallet
     * @param {string} walletAddress - User's wallet address
     */
    async getCopyTradeStats(walletAddress) {
        return this.get(`/api/copy/stats?wallet=${walletAddress}`);
    },

    /**
     * Build a copy trade transaction
     * @param {object} params - Transaction parameters
     */
    async buildCopyTransaction(params) {
        return this.post('/api/copy/build-tx', params);
    },

    // ==========================================
    // Simulation (Dry Run Mode) Endpoints
    // ==========================================

    /**
     * Get all simulated positions
     */
    async getSimulatedPositions() {
        return this.get('/api/simulation/positions');
    },

    /**
     * Get open simulated positions only
     */
    async getOpenSimulatedPositions() {
        return this.get('/api/simulation/positions/open');
    },

    /**
     * Get simulation statistics
     */
    async getSimulationStats() {
        return this.get('/api/simulation/stats');
    },

    /**
     * Clear all simulated positions
     */
    async clearSimulation() {
        return this.post('/api/simulation/clear');
    },

    /**
     * Manually close a simulated position
     * @param {string} positionId - Simulated position ID
     */
    async closeSimulatedPosition(positionId) {
        return this.post(`/api/simulation/close/${positionId}`);
    },

    // ==========================================
    // Manual Trading Endpoints
    // ==========================================

    /**
     * Execute a manual buy
     * @param {string} mint - Token mint address
     * @param {number} amountSol - Amount in SOL to spend
     */
    async manualBuy(mint, amountSol) {
        return this.post('/api/trade/buy', {
            mint,
            amount_sol: amountSol,
        });
    },

    /**
     * Execute a manual sell
     * @param {string} positionId - Position ID to sell
     * @param {number} percentage - Percentage of position to sell (0-100)
     */
    async manualSell(positionId, percentage = 100) {
        return this.post('/api/trade/sell', {
            position_id: positionId,
            percentage,
        });
    },

    // ==========================================
    // Mock Data for Demo Mode
    // ==========================================

    getMockData(endpoint, options) {
        const method = options.method || 'GET';

        // Simulate network delay
        return new Promise((resolve) => {
            setTimeout(() => {
                resolve(this.generateMockResponse(endpoint, method));
            }, 200 + Math.random() * 300);
        });
    },

    generateMockResponse(endpoint, method) {
        // Health check
        if (endpoint === '/api/health') {
            return {
                status: 'healthy',
                version: '4.0.0',
                uptime_seconds: 86400,
            };
        }

        // Status
        if (endpoint === '/api/status') {
            return {
                is_running: true,
                wallet_connected: true,
                network: 'mainnet-beta',
                active_positions: 2,
                autotrader_running: true,
            };
        }

        // Wallet
        if (endpoint === '/api/wallet') {
            return {
                address: 'DemoWa11etAddressXXXXXXXXXXXXXXXXXXXXXXXX',
                balance_sol: 5.234,
                balance_usd: 783.45,
                network: 'mainnet-beta',
            };
        }

        // Positions
        if (endpoint === '/api/positions') {
            return {
                positions: [
                    {
                        id: 'pos_001',
                        token_symbol: 'BONK',
                        token_mint: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                        entry_price_sol: 0.0000234,
                        current_price_sol: 0.0000312,
                        entry_sol_amount: 0.5,
                        token_amount: 21367521,
                        current_value_sol: 0.667,
                        pnl_sol: 0.167,
                        pnl_percent: 33.4,
                        opened_at: new Date(Date.now() - 3600000).toISOString(),
                        status: 'open',
                    },
                    {
                        id: 'pos_002',
                        token_symbol: 'WIF',
                        token_mint: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
                        entry_price_sol: 0.0145,
                        current_price_sol: 0.0132,
                        entry_sol_amount: 1.0,
                        token_amount: 68.97,
                        current_value_sol: 0.91,
                        pnl_sol: -0.09,
                        pnl_percent: -9.0,
                        opened_at: new Date(Date.now() - 7200000).toISOString(),
                        status: 'open',
                    },
                ],
            };
        }

        // Trades
        if (endpoint.startsWith('/api/trades')) {
            return {
                trades: [
                    {
                        id: 'trade_001',
                        token_symbol: 'MYRO',
                        action: 'sell',
                        amount_sol: 1.5,
                        pnl_sol: 0.45,
                        pnl_percent: 30.0,
                        timestamp: new Date(Date.now() - 1800000).toISOString(),
                        tx_signature: '4xDemo...Sig1',
                    },
                    {
                        id: 'trade_002',
                        token_symbol: 'BONK',
                        action: 'buy',
                        amount_sol: 0.5,
                        pnl_sol: 0,
                        pnl_percent: 0,
                        timestamp: new Date(Date.now() - 3600000).toISOString(),
                        tx_signature: '4xDemo...Sig2',
                    },
                    {
                        id: 'trade_003',
                        token_symbol: 'POPCAT',
                        action: 'sell',
                        amount_sol: 2.0,
                        pnl_sol: -0.3,
                        pnl_percent: -15.0,
                        timestamp: new Date(Date.now() - 5400000).toISOString(),
                        tx_signature: '4xDemo...Sig3',
                    },
                ],
            };
        }

        // Stats
        if (endpoint === '/api/stats') {
            return {
                total_trades: 47,
                winning_trades: 31,
                losing_trades: 16,
                win_rate: 65.96,
                total_pnl_sol: 12.45,
                avg_roi_percent: 18.7,
                best_trade_pnl: 2.5,
                worst_trade_pnl: -0.8,
                avg_hold_time_minutes: 45,
            };
        }

        // Autotrader status
        if (endpoint === '/api/autotrader/status') {
            return {
                running: true,
                active_strategies: 3,
                open_positions: 2,
                last_scan: new Date(Date.now() - 30000).toISOString(),
            };
        }

        // Autotrader start/stop
        if (endpoint === '/api/autotrader/start' || endpoint === '/api/autotrader/stop') {
            return {
                success: true,
                message: endpoint.includes('start') ? 'AutoTrader started' : 'AutoTrader stopped',
            };
        }

        // Token analysis
        if (endpoint === '/api/analyze') {
            return {
                token_address: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                symbol: 'BONK',
                name: 'Bonk',
                risk_level: 'medium',
                risk_score: 45,
                liquidity_sol: 125000,
                holder_count: 45678,
                can_sell: true,
                is_honeypot: false,
                top_holder_percent: 12.5,
                recommendation: 'Moderate risk. Good liquidity and holder distribution. Proceed with caution.',
            };
        }

        // Copy trade signals
        if (endpoint === '/api/signals') {
            return {
                signals: [
                    {
                        id: 'sig_001',
                        token_address: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                        token_symbol: 'BONK',
                        token_name: 'Bonk',
                        action: 'buy',
                        amount_sol: 0.5,
                        price_sol: 0.0000234,
                        timestamp: new Date(Date.now() - 3600000).toISOString(),
                        bot_position_id: 'pos_001',
                        is_active: true,
                        current_price_sol: 0.0000312,
                        current_pnl_percent: 33.4,
                    },
                    {
                        id: 'sig_002',
                        token_address: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
                        token_symbol: 'WIF',
                        token_name: 'dogwifhat',
                        action: 'buy',
                        amount_sol: 1.0,
                        price_sol: 0.0145,
                        timestamp: new Date(Date.now() - 7200000).toISOString(),
                        bot_position_id: 'pos_002',
                        is_active: true,
                        current_price_sol: 0.0132,
                        current_pnl_percent: -9.0,
                    },
                ],
                total: 2,
            };
        }

        // Active signals
        if (endpoint === '/api/signals/active') {
            return {
                signals: [
                    {
                        id: 'sig_001',
                        token_address: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                        token_symbol: 'BONK',
                        token_name: 'Bonk',
                        action: 'buy',
                        amount_sol: 0.5,
                        price_sol: 0.0000234,
                        timestamp: new Date(Date.now() - 3600000).toISOString(),
                        bot_position_id: 'pos_001',
                        is_active: true,
                        current_price_sol: 0.0000312,
                        current_pnl_percent: 33.4,
                    },
                ],
                total: 1,
            };
        }

        // Copy trade status
        if (endpoint.startsWith('/api/copy/status')) {
            return {
                is_registered: true,
                wallet_address: 'DemoUserWallet123...',
                auto_copy_enabled: false,
                copy_amount_sol: 0.1,
                max_positions: 5,
                slippage_bps: 300,
                total_copy_trades: 12,
                active_copy_positions: 1,
                total_fees_paid_sol: 0.05,
            };
        }

        // Copy trade stats
        if (endpoint.startsWith('/api/copy/stats')) {
            return {
                total_trades: 12,
                winning_trades: 8,
                losing_trades: 4,
                win_rate: 66.67,
                total_pnl_sol: 0.85,
                total_fees_paid_sol: 0.05,
                avg_pnl_percent: 15.2,
                best_trade_pnl_sol: 0.45,
                worst_trade_pnl_sol: -0.12,
            };
        }

        // Copy positions
        if (endpoint.startsWith('/api/copy/positions')) {
            return {
                positions: [
                    {
                        id: 'cpos_001',
                        copier_wallet: 'DemoUserWallet123...',
                        token_address: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                        token_symbol: 'BONK',
                        entry_price_sol: 0.0000234,
                        entry_amount_sol: 0.1,
                        token_amount: 4273504,
                        bot_position_id: 'pos_001',
                        status: 'open',
                        current_price_sol: 0.0000312,
                        current_pnl_percent: 33.4,
                        pnl_sol: null,
                        fee_paid_sol: null,
                        opened_at: new Date(Date.now() - 3500000).toISOString(),
                        closed_at: null,
                    },
                ],
                total: 1,
            };
        }

        // Copy trade registration
        if (endpoint === '/api/copy/register') {
            return {
                success: true,
                message: 'Wallet registered for copy trading',
            };
        }

        // Build copy transaction
        if (endpoint === '/api/copy/build-tx') {
            return {
                success: true,
                transaction: 'MOCK_TRANSACTION_BASE64_STRING',
                error: null,
                estimated_output: 4273504,
                estimated_fee: null,
                estimated_pnl: null,
            };
        }

        // Simulation positions
        if (endpoint === '/api/simulation/positions' || endpoint === '/api/simulation/positions/open') {
            return {
                positions: [
                    {
                        id: 'sim_001',
                        token_address: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                        token_symbol: 'BONK',
                        token_name: 'Bonk',
                        entry_price_sol: 0.0000234,
                        entry_amount_sol: 0.5,
                        token_amount: 21367521,
                        entry_time: new Date(Date.now() - 3600000).toISOString(),
                        current_price_sol: 0.0000312,
                        current_value_sol: 0.667,
                        unrealized_pnl_sol: 0.167,
                        unrealized_pnl_percent: 33.4,
                        risk_score: 35,
                        risk_details: ['Good liquidity', 'LP burned'],
                        selection_reason: "Passed 'Aggressive' strategy criteria",
                        strategy_id: 'strat_001',
                        status: 'Open',
                        highest_price_sol: 0.0000320,
                    },
                    {
                        id: 'sim_002',
                        token_address: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
                        token_symbol: 'WIF',
                        token_name: 'dogwifhat',
                        entry_price_sol: 0.0145,
                        entry_amount_sol: 1.0,
                        token_amount: 68.97,
                        entry_time: new Date(Date.now() - 7200000).toISOString(),
                        current_price_sol: 0.0132,
                        current_value_sol: 0.91,
                        unrealized_pnl_sol: -0.09,
                        unrealized_pnl_percent: -9.0,
                        risk_score: 42,
                        risk_details: ['Moderate liquidity', 'Active development'],
                        selection_reason: "Passed 'Conservative' strategy criteria",
                        strategy_id: 'strat_002',
                        status: 'Open',
                        highest_price_sol: 0.0148,
                    },
                ],
                total: 2,
                dry_run_mode: true,
            };
        }

        // Simulation stats
        if (endpoint === '/api/simulation/stats') {
            return {
                stats: {
                    total_simulated_trades: 15,
                    open_positions: 2,
                    closed_positions: 13,
                    winning_trades: 9,
                    losing_trades: 4,
                    total_realized_pnl_sol: 2.45,
                    total_unrealized_pnl_sol: 0.077,
                    win_rate: 69.23,
                    would_have_spent_sol: 8.5,
                    would_have_returned_sol: 10.95,
                    average_pnl_percent: 18.7,
                    best_trade_pnl_percent: 85.5,
                    worst_trade_pnl_percent: -22.3,
                },
                dry_run_mode: true,
            };
        }

        // Simulation clear
        if (endpoint === '/api/simulation/clear') {
            return {
                success: true,
                message: 'All simulated positions cleared',
            };
        }

        // Default response
        return { success: true };
    },
};

/**
 * Custom API Error class
 */
class APIError extends Error {
    constructor(message, status, data = {}) {
        super(message);
        this.name = 'APIError';
        this.status = status;
        this.data = data;
    }
}

// Make APIError available globally
window.APIError = APIError;

// Auto-initialize when script loads
API.init();
