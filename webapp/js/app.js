/**
 * TraderTony V4 - Main Application
 * Orchestrates UI updates and user interactions
 */

const App = {
    // State
    isInitialized: false,
    isLoading: true,
    demoMode: false,
    refreshInterval: null,
    refreshRate: 30000, // 30 seconds

    // Cache
    cache: {
        stats: null,
        positions: [],
        trades: [],
        wallet: null,
        autotraderStatus: null,
    },

    /**
     * Initialize the application
     */
    async init() {
        console.log('[App] Initializing TraderTony V4 Dashboard...');

        // Setup event listeners
        this.setupUIEventListeners();

        // Setup WebSocket handlers
        this.setupWebSocketHandlers();

        // Setup wallet handlers
        this.setupWalletHandlers();

        // Try to connect to backend
        await this.connectToBackend();

        // Start periodic refresh
        this.startAutoRefresh();

        this.isInitialized = true;
        console.log('[App] Initialization complete');
    },

    /**
     * Connect to the backend API
     */
    async connectToBackend() {
        this.updateConnectionStatus('connecting');

        try {
            // Health check
            const health = await API.healthCheck();
            console.log('[App] Backend health:', health);

            // Backend is available
            this.demoMode = false;
            this.updateConnectionStatus('connected');
            this.hideDemoBadge();

            // Connect WebSocket
            WebSocketClient.connect();

            // Load initial data
            await this.loadAllData();
        } catch (error) {
            console.warn('[App] Backend not available, switching to demo mode:', error.message);

            // Enable demo mode
            this.enableDemoMode();
        }
    },

    /**
     * Enable demo mode with mock data
     */
    enableDemoMode() {
        this.demoMode = true;
        API.enableDemoMode();
        WebSocketClient.enableDemoMode();

        this.updateConnectionStatus('demo');
        this.showDemoBadge();

        // Simulate WebSocket connection
        WebSocketClient.connect();

        // Load mock data
        this.loadAllData();
    },

    /**
     * Load all dashboard data
     */
    async loadAllData() {
        this.setLoading(true);

        try {
            await Promise.all([
                this.loadWallet(),
                this.loadStats(),
                this.loadPositions(),
                this.loadTrades(),
                this.loadAutotraderStatus(),
            ]);
        } catch (error) {
            console.error('[App] Error loading data:', error);
            this.showToast('Failed to load data', 'error');
        }

        this.setLoading(false);
        this.updateLastRefreshTime();
    },

    /**
     * Load wallet information
     */
    async loadWallet() {
        try {
            const wallet = await API.getWallet();
            this.cache.wallet = wallet;
            this.updateWalletDisplay(wallet);
        } catch (error) {
            console.error('[App] Error loading wallet:', error);
        }
    },

    /**
     * Load trading statistics
     */
    async loadStats() {
        try {
            const stats = await API.getStats();
            this.cache.stats = stats;
            this.updateStatsDisplay(stats);
        } catch (error) {
            console.error('[App] Error loading stats:', error);
        }
    },

    /**
     * Load active positions
     */
    async loadPositions() {
        try {
            const response = await API.getPositions();
            this.cache.positions = response.positions || [];
            this.updatePositionsTable(this.cache.positions);
        } catch (error) {
            console.error('[App] Error loading positions:', error);
        }
    },

    /**
     * Load trade history
     */
    async loadTrades() {
        try {
            const response = await API.getTrades(20);
            this.cache.trades = response.trades || [];
            this.updateTradesTable(this.cache.trades);
        } catch (error) {
            console.error('[App] Error loading trades:', error);
        }
    },

    /**
     * Load autotrader status
     */
    async loadAutotraderStatus() {
        try {
            const status = await API.getAutotraderStatus();
            this.cache.autotraderStatus = status;
            this.updateAutotraderDisplay(status);
        } catch (error) {
            console.error('[App] Error loading autotrader status:', error);
        }
    },

    // ==========================================
    // UI Update Methods
    // ==========================================

    /**
     * Update wallet display
     */
    updateWalletDisplay(wallet) {
        const addressEl = document.getElementById('walletAddress');
        const balanceEl = document.getElementById('walletBalance');
        const solscanLink = document.getElementById('solscanLink');
        const networkBadge = document.getElementById('networkBadge');

        if (addressEl) {
            addressEl.textContent = this.shortenAddress(wallet.address);
            addressEl.title = wallet.address;
        }

        if (balanceEl) {
            balanceEl.textContent = this.formatNumber(wallet.balance_sol, 4);
        }

        if (solscanLink) {
            solscanLink.href = `https://solscan.io/account/${wallet.address}`;
        }

        if (networkBadge) {
            networkBadge.textContent = wallet.network === 'mainnet-beta' ? 'Mainnet' : 'Devnet';
            networkBadge.className = `badge ${wallet.network === 'mainnet-beta' ? 'badge-info' : 'badge-warning'}`;
        }
    },

    /**
     * Update statistics display
     */
    updateStatsDisplay(stats) {
        // Total trades
        const totalTradesEl = document.getElementById('totalTrades');
        if (totalTradesEl) {
            totalTradesEl.textContent = stats.total_trades || 0;
        }

        // Win rate
        const winRateEl = document.getElementById('winRate');
        const winRateBarEl = document.getElementById('winRateBar');
        if (winRateEl) {
            winRateEl.textContent = `${this.formatNumber(stats.win_rate || 0, 1)}%`;
        }
        if (winRateBarEl) {
            winRateBarEl.style.width = `${stats.win_rate || 0}%`;
        }

        // Total PnL
        const totalPnlEl = document.getElementById('totalPnl');
        if (totalPnlEl) {
            const pnl = stats.total_pnl_sol || 0;
            totalPnlEl.textContent = `${pnl >= 0 ? '+' : ''}${this.formatNumber(pnl, 3)}`;
            totalPnlEl.className = `stat-value ${pnl >= 0 ? 'positive' : 'negative'}`;
        }

        // Average ROI
        const avgRoiEl = document.getElementById('avgRoi');
        if (avgRoiEl) {
            const roi = stats.avg_roi_percent || 0;
            avgRoiEl.textContent = `${roi >= 0 ? '+' : ''}${this.formatNumber(roi, 1)}%`;
            avgRoiEl.className = `stat-value ${roi >= 0 ? 'positive' : 'negative'}`;
        }
    },

    /**
     * Update positions table
     */
    updatePositionsTable(positions) {
        const tbody = document.getElementById('positionsBody');
        const countBadge = document.getElementById('positionCount');

        if (countBadge) {
            countBadge.textContent = positions.length;
        }

        if (!tbody) return;

        if (positions.length === 0) {
            tbody.innerHTML = '<tr class="empty-row"><td colspan="6">No active positions</td></tr>';
            return;
        }

        tbody.innerHTML = positions.map(pos => `
            <tr>
                <td>
                    <div class="token-cell">
                        <span class="token-symbol">${pos.token_symbol || 'Unknown'}</span>
                        <span class="token-mint" title="${pos.token_mint}">${this.shortenAddress(pos.token_mint, 4)}</span>
                    </div>
                </td>
                <td>${this.formatNumber(pos.entry_price_sol, 8)} SOL</td>
                <td>${this.formatNumber(pos.current_price_sol, 8)} SOL</td>
                <td class="${pos.pnl_sol >= 0 ? 'positive' : 'negative'}">
                    ${pos.pnl_sol >= 0 ? '+' : ''}${this.formatNumber(pos.pnl_sol, 4)} SOL
                    <span class="pnl-percent">(${pos.pnl_percent >= 0 ? '+' : ''}${this.formatNumber(pos.pnl_percent, 1)}%)</span>
                </td>
                <td>${this.formatTimeAgo(pos.opened_at)}</td>
                <td><span class="status-badge status-${pos.status}">${pos.status}</span></td>
            </tr>
        `).join('');
    },

    /**
     * Update trades table
     */
    updateTradesTable(trades) {
        const tbody = document.getElementById('tradesBody');
        const countBadge = document.getElementById('tradeCount');

        if (countBadge) {
            countBadge.textContent = trades.length;
        }

        if (!tbody) return;

        if (trades.length === 0) {
            tbody.innerHTML = '<tr class="empty-row"><td colspan="5">No trades yet</td></tr>';
            return;
        }

        tbody.innerHTML = trades.map(trade => `
            <tr>
                <td>
                    <span class="token-symbol">${trade.token_symbol || 'Unknown'}</span>
                </td>
                <td>
                    <span class="action-badge action-${trade.action}">${trade.action.toUpperCase()}</span>
                </td>
                <td>${this.formatNumber(trade.amount_sol, 4)} SOL</td>
                <td class="${trade.pnl_sol >= 0 ? 'positive' : 'negative'}">
                    ${trade.pnl_sol !== 0 ? (trade.pnl_sol >= 0 ? '+' : '') + this.formatNumber(trade.pnl_sol, 4) + ' SOL' : '-'}
                </td>
                <td>${this.formatTimeAgo(trade.timestamp)}</td>
            </tr>
        `).join('');
    },

    /**
     * Update autotrader display
     */
    updateAutotraderDisplay(status) {
        const statusEl = document.getElementById('autotraderStatus');
        const strategiesEl = document.getElementById('activeStrategies');
        const positionsEl = document.getElementById('openPositions');
        const startBtn = document.getElementById('startTradingBtn');
        const stopBtn = document.getElementById('stopTradingBtn');

        if (statusEl) {
            const isRunning = status?.running || false;
            statusEl.innerHTML = `
                <span class="status-dot ${isRunning ? 'active' : ''}"></span>
                <span>${isRunning ? 'Running' : 'Stopped'}</span>
            `;
            statusEl.className = `autotrader-status ${isRunning ? 'running' : 'stopped'}`;
        }

        if (strategiesEl) {
            strategiesEl.textContent = status?.active_strategies || 0;
        }

        if (positionsEl) {
            positionsEl.textContent = status?.open_positions || 0;
        }

        if (startBtn && stopBtn) {
            const isRunning = status?.running || false;
            startBtn.disabled = isRunning;
            stopBtn.disabled = !isRunning;
        }
    },

    /**
     * Update connection status indicator
     */
    updateConnectionStatus(status) {
        const statusEl = document.getElementById('botStatus');
        const footerStatus = document.getElementById('connectionStatus');

        const statusConfig = {
            connecting: { text: 'Connecting...', class: 'connecting' },
            connected: { text: 'Connected', class: 'connected' },
            disconnected: { text: 'Disconnected', class: 'disconnected' },
            demo: { text: 'Demo Mode', class: 'demo' },
        };

        const config = statusConfig[status] || statusConfig.disconnected;

        if (statusEl) {
            statusEl.innerHTML = `
                <span class="status-dot ${config.class}"></span>
                <span class="status-text">${config.text}</span>
            `;
        }

        if (footerStatus) {
            footerStatus.textContent = config.text;
        }
    },

    /**
     * Show/hide demo badge
     */
    showDemoBadge() {
        const badge = document.getElementById('demoBadge');
        if (badge) badge.style.display = 'block';
    },

    hideDemoBadge() {
        const badge = document.getElementById('demoBadge');
        if (badge) badge.style.display = 'none';
    },

    /**
     * Update last refresh time
     */
    updateLastRefreshTime() {
        const el = document.getElementById('lastUpdate');
        if (el) {
            el.textContent = `Last update: ${new Date().toLocaleTimeString()}`;
        }
    },

    /**
     * Set loading state
     */
    setLoading(loading) {
        this.isLoading = loading;
        document.body.classList.toggle('loading', loading);
    },

    // ==========================================
    // UI Event Listeners
    // ==========================================

    /**
     * Setup UI event listeners
     */
    setupUIEventListeners() {
        // Refresh button
        const refreshBtn = document.getElementById('refreshBtn');
        if (refreshBtn) {
            refreshBtn.addEventListener('click', () => this.loadAllData());
        }

        // Start trading button
        const startBtn = document.getElementById('startTradingBtn');
        if (startBtn) {
            startBtn.addEventListener('click', () => this.startTrading());
        }

        // Stop trading button
        const stopBtn = document.getElementById('stopTradingBtn');
        if (stopBtn) {
            stopBtn.addEventListener('click', () => this.stopTrading());
        }

        // Analyze token button
        const analyzeBtn = document.getElementById('analyzeTokenBtn');
        if (analyzeBtn) {
            analyzeBtn.addEventListener('click', () => this.analyzeToken());
        }

        // Analyze token input - enter key
        const analyzeInput = document.getElementById('analyzeTokenInput');
        if (analyzeInput) {
            analyzeInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter') this.analyzeToken();
            });
        }

        // Copy address button
        const copyBtn = document.getElementById('copyAddressBtn');
        if (copyBtn) {
            copyBtn.addEventListener('click', () => this.copyBotAddress());
        }

        // Connect wallet button
        const connectWalletBtn = document.getElementById('connectWalletBtn');
        if (connectWalletBtn) {
            connectWalletBtn.addEventListener('click', () => this.handleWalletConnect());
        }

        // Enable copy trade button
        const enableCopyTradeBtn = document.getElementById('enableCopyTradeBtn');
        if (enableCopyTradeBtn) {
            enableCopyTradeBtn.addEventListener('click', () => this.handleEnableCopyTrade());
        }
    },

    /**
     * Setup WebSocket event handlers
     */
    setupWebSocketHandlers() {
        WebSocketClient.on('connect', () => {
            console.log('[App] WebSocket connected');
            if (!this.demoMode) {
                this.updateConnectionStatus('connected');
            }
        });

        WebSocketClient.on('disconnect', () => {
            console.log('[App] WebSocket disconnected');
            if (!this.demoMode) {
                this.updateConnectionStatus('disconnected');
            }
        });

        WebSocketClient.on('positionUpdate', (data) => {
            console.log('[App] Position update:', data);
            this.loadPositions();
        });

        WebSocketClient.on('tradeExecuted', (data) => {
            console.log('[App] Trade executed:', data);
            this.loadTrades();
            this.loadStats();
            this.showToast(`Trade executed: ${data.action} ${data.token_symbol}`, 'success');
        });

        WebSocketClient.on('priceUpdate', (data) => {
            // Update position prices in real-time
            this.updatePositionPrice(data);
        });

        WebSocketClient.on('statusChange', (data) => {
            this.updateAutotraderDisplay(data);
        });

        WebSocketClient.on('alert', (data) => {
            this.showToast(data.message, data.level || 'info');
        });
    },

    /**
     * Setup wallet event handlers
     */
    setupWalletHandlers() {
        WalletManager.on('connect', (data) => {
            console.log('[App] Wallet connected:', data);
            this.updateWalletButton(true, data.publicKey);
            this.updateCopyTradeButton(true);
            this.showToast(`Wallet connected: ${WalletManager.shortenAddress(data.publicKey)}`, 'success');
        });

        WalletManager.on('disconnect', () => {
            console.log('[App] Wallet disconnected');
            this.updateWalletButton(false);
            this.updateCopyTradeButton(false);
            this.showToast('Wallet disconnected', 'info');
        });

        WalletManager.on('error', (data) => {
            this.showToast(data.error, 'error');
        });
    },

    // ==========================================
    // User Actions
    // ==========================================

    /**
     * Start autotrader
     */
    async startTrading() {
        try {
            const startBtn = document.getElementById('startTradingBtn');
            if (startBtn) startBtn.disabled = true;

            await API.startAutotrader();
            await this.loadAutotraderStatus();

            this.showToast('AutoTrader started', 'success');
        } catch (error) {
            console.error('[App] Error starting autotrader:', error);
            this.showToast('Failed to start AutoTrader', 'error');
        }
    },

    /**
     * Stop autotrader
     */
    async stopTrading() {
        try {
            const stopBtn = document.getElementById('stopTradingBtn');
            if (stopBtn) stopBtn.disabled = true;

            await API.stopAutotrader();
            await this.loadAutotraderStatus();

            this.showToast('AutoTrader stopped', 'success');
        } catch (error) {
            console.error('[App] Error stopping autotrader:', error);
            this.showToast('Failed to stop AutoTrader', 'error');
        }
    },

    /**
     * Analyze a token
     */
    async analyzeToken() {
        const input = document.getElementById('analyzeTokenInput');
        const resultDiv = document.getElementById('analysisResult');
        const analyzeBtn = document.getElementById('analyzeTokenBtn');

        if (!input || !input.value.trim()) {
            this.showToast('Please enter a token address', 'warning');
            return;
        }

        const tokenAddress = input.value.trim();

        try {
            if (analyzeBtn) {
                analyzeBtn.disabled = true;
                analyzeBtn.textContent = 'Analyzing...';
            }

            const analysis = await API.analyzeToken(tokenAddress);

            // Update result display
            if (resultDiv) {
                resultDiv.style.display = 'block';

                document.getElementById('analysisToken').textContent = analysis.symbol || tokenAddress.slice(0, 8);

                const riskBadge = document.getElementById('riskBadge');
                riskBadge.textContent = analysis.risk_level?.toUpperCase() || 'UNKNOWN';
                riskBadge.className = `risk-badge risk-${analysis.risk_level || 'unknown'}`;

                document.getElementById('analysisRiskLevel').textContent = `${analysis.risk_score || 0}/100`;
                document.getElementById('analysisLiquidity').textContent = `$${this.formatNumber(analysis.liquidity_sol * 150, 0)}`; // Rough USD estimate
                document.getElementById('analysisHolders').textContent = this.formatNumber(analysis.holder_count || 0, 0);
                document.getElementById('analysisCanSell').textContent = analysis.can_sell ? 'Yes' : 'No';
                document.getElementById('analysisRecommendation').textContent = analysis.recommendation || 'No recommendation';
            }

            this.showToast('Token analyzed', 'success');
        } catch (error) {
            console.error('[App] Error analyzing token:', error);
            this.showToast('Failed to analyze token', 'error');
        } finally {
            if (analyzeBtn) {
                analyzeBtn.disabled = false;
                analyzeBtn.textContent = 'Analyze';
            }
        }
    },

    /**
     * Copy bot wallet address
     */
    async copyBotAddress() {
        const address = this.cache.wallet?.address;
        if (!address) return;

        try {
            await navigator.clipboard.writeText(address);
            this.showToast('Address copied to clipboard', 'success');
        } catch (error) {
            console.error('[App] Copy failed:', error);
            this.showToast('Failed to copy address', 'error');
        }
    },

    /**
     * Handle wallet connect button click
     */
    async handleWalletConnect() {
        if (WalletManager.connected) {
            await WalletManager.disconnect();
        } else {
            try {
                await WalletManager.connect();
            } catch (error) {
                // Error already handled by wallet manager
            }
        }
    },

    /**
     * Update wallet connect button state
     */
    updateWalletButton(connected, publicKey = null) {
        const btn = document.getElementById('connectWalletBtn');
        if (!btn) return;

        if (connected) {
            btn.innerHTML = `
                <span class="btn-icon">&#128275;</span>
                ${WalletManager.shortenAddress(publicKey)}
            `;
            btn.classList.add('connected');
        } else {
            btn.innerHTML = `
                <span class="btn-icon">&#128279;</span>
                Connect Wallet
            `;
            btn.classList.remove('connected');
        }
    },

    /**
     * Update copy trade button state
     */
    updateCopyTradeButton(walletConnected) {
        const btn = document.getElementById('enableCopyTradeBtn');
        if (!btn) return;

        if (walletConnected) {
            btn.disabled = false;
            btn.textContent = 'Enable Copy Trading';
        } else {
            btn.disabled = true;
            btn.textContent = 'Connect Wallet to Enable';
        }
    },

    /**
     * Handle enable copy trade button click
     */
    async handleEnableCopyTrade() {
        if (!WalletManager.connected) {
            this.showToast('Please connect your wallet first', 'warning');
            return;
        }

        try {
            // Generate verification signature
            const verification = await WalletManager.generateVerification();

            // Register with backend
            await API.registerCopyTrader(verification.publicKey, verification.signature);

            this.showToast('Copy trading enabled!', 'success');
        } catch (error) {
            console.error('[App] Error enabling copy trade:', error);
            this.showToast('Failed to enable copy trading', 'error');
        }
    },

    /**
     * Update position price in real-time
     */
    updatePositionPrice(data) {
        const position = this.cache.positions.find(p => p.id === data.position_id);
        if (position) {
            position.current_price_sol = data.current_price_sol;
            position.pnl_percent = data.pnl_percent;
            // Re-render positions table
            this.updatePositionsTable(this.cache.positions);
        }
    },

    // ==========================================
    // Auto Refresh
    // ==========================================

    /**
     * Start auto-refresh timer
     */
    startAutoRefresh() {
        this.stopAutoRefresh();
        this.refreshInterval = setInterval(() => {
            if (!document.hidden) {
                this.loadAllData();
            }
        }, this.refreshRate);
    },

    /**
     * Stop auto-refresh timer
     */
    stopAutoRefresh() {
        if (this.refreshInterval) {
            clearInterval(this.refreshInterval);
            this.refreshInterval = null;
        }
    },

    // ==========================================
    // Toast Notifications
    // ==========================================

    /**
     * Show a toast notification
     * @param {string} message - Toast message
     * @param {string} type - Toast type (success, error, warning, info)
     * @param {number} duration - Duration in ms
     */
    showToast(message, type = 'info', duration = 4000) {
        const container = document.getElementById('toastContainer');
        if (!container) return;

        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;
        toast.innerHTML = `
            <span class="toast-icon">${this.getToastIcon(type)}</span>
            <span class="toast-message">${message}</span>
            <button class="toast-close" onclick="this.parentElement.remove()">×</button>
        `;

        container.appendChild(toast);

        // Trigger animation
        requestAnimationFrame(() => {
            toast.classList.add('show');
        });

        // Auto remove
        setTimeout(() => {
            toast.classList.remove('show');
            setTimeout(() => toast.remove(), 300);
        }, duration);
    },

    /**
     * Get toast icon based on type
     */
    getToastIcon(type) {
        const icons = {
            success: '&#10003;',
            error: '&#10007;',
            warning: '&#9888;',
            info: '&#8505;',
        };
        return icons[type] || icons.info;
    },

    // ==========================================
    // Utility Methods
    // ==========================================

    /**
     * Shorten an address for display
     */
    shortenAddress(address, chars = 4) {
        if (!address) return '';
        return `${address.slice(0, chars)}...${address.slice(-chars)}`;
    },

    /**
     * Format a number with specified decimal places
     */
    formatNumber(num, decimals = 2) {
        if (num === null || num === undefined) return '-';
        return Number(num).toLocaleString(undefined, {
            minimumFractionDigits: decimals,
            maximumFractionDigits: decimals,
        });
    },

    /**
     * Format a timestamp as relative time
     */
    formatTimeAgo(timestamp) {
        if (!timestamp) return '-';

        const date = new Date(timestamp);
        const now = new Date();
        const diffMs = now - date;
        const diffMins = Math.floor(diffMs / 60000);
        const diffHours = Math.floor(diffMins / 60);
        const diffDays = Math.floor(diffHours / 24);

        if (diffMins < 1) return 'Just now';
        if (diffMins < 60) return `${diffMins}m ago`;
        if (diffHours < 24) return `${diffHours}h ago`;
        if (diffDays < 7) return `${diffDays}d ago`;

        return date.toLocaleDateString();
    },
};

// Initialize app when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    App.init();
});

// Handle visibility change (pause/resume refresh when tab is hidden/visible)
document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
        App.stopAutoRefresh();
    } else {
        App.loadAllData();
        App.startAutoRefresh();
    }
});
