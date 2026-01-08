/**
 * TraderTony V4 - Wallet Connection
 * Handles Solana wallet integration (Phantom, Solflare, etc.)
 */

const WalletManager = {
    // State
    connected: false,
    publicKey: null,
    walletName: null,
    provider: null,

    // Supported wallets
    supportedWallets: {
        phantom: {
            name: 'Phantom',
            url: 'https://phantom.app/',
            icon: 'https://phantom.app/img/phantom-icon-purple.svg',
            getProvider: () => window.solana?.isPhantom ? window.solana : null,
        },
        solflare: {
            name: 'Solflare',
            url: 'https://solflare.com/',
            icon: 'https://solflare.com/favicon.ico',
            getProvider: () => window.solflare?.isSolflare ? window.solflare : null,
        },
        backpack: {
            name: 'Backpack',
            url: 'https://backpack.app/',
            icon: 'https://backpack.app/favicon.ico',
            getProvider: () => window.backpack?.isBackpack ? window.backpack : null,
        },
    },

    // Event handlers
    handlers: {
        onConnect: [],
        onDisconnect: [],
        onAccountChange: [],
        onError: [],
    },

    /**
     * Initialize wallet manager
     */
    init() {
        console.log('[Wallet] Initializing wallet manager...');

        // Check for available wallets
        const available = this.getAvailableWallets();
        console.log('[Wallet] Available wallets:', available.map(w => w.name).join(', ') || 'none');

        // Listen for wallet events on window load
        if (document.readyState === 'complete') {
            this.setupEventListeners();
        } else {
            window.addEventListener('load', () => this.setupEventListeners());
        }

        // Check if previously connected
        this.checkExistingConnection();
    },

    /**
     * Setup event listeners for wallet changes
     */
    setupEventListeners() {
        // Phantom events
        if (window.solana?.isPhantom) {
            window.solana.on('connect', (publicKey) => {
                console.log('[Wallet] Phantom connected:', publicKey.toString());
                this.handleConnect(publicKey.toString(), 'phantom');
            });

            window.solana.on('disconnect', () => {
                console.log('[Wallet] Phantom disconnected');
                this.handleDisconnect();
            });

            window.solana.on('accountChanged', (publicKey) => {
                if (publicKey) {
                    console.log('[Wallet] Phantom account changed:', publicKey.toString());
                    this.handleAccountChange(publicKey.toString());
                } else {
                    this.handleDisconnect();
                }
            });
        }

        // Solflare events
        if (window.solflare?.isSolflare) {
            window.solflare.on('connect', (publicKey) => {
                console.log('[Wallet] Solflare connected:', publicKey.toString());
                this.handleConnect(publicKey.toString(), 'solflare');
            });

            window.solflare.on('disconnect', () => {
                console.log('[Wallet] Solflare disconnected');
                this.handleDisconnect();
            });
        }
    },

    /**
     * Check if wallet was previously connected
     */
    async checkExistingConnection() {
        // Try to reconnect to previously used wallet
        const lastWallet = localStorage.getItem('trader-tony-wallet');

        if (lastWallet && this.supportedWallets[lastWallet]) {
            const provider = this.supportedWallets[lastWallet].getProvider();

            if (provider && provider.isConnected) {
                try {
                    // Silently reconnect
                    const resp = await provider.connect({ onlyIfTrusted: true });
                    this.handleConnect(resp.publicKey.toString(), lastWallet);
                } catch (err) {
                    // User hasn't previously connected, or revoked permission
                    console.log('[Wallet] No existing connection');
                }
            }
        }
    },

    /**
     * Get list of available wallet providers
     * @returns {Array} Available wallet info
     */
    getAvailableWallets() {
        const available = [];

        for (const [key, wallet] of Object.entries(this.supportedWallets)) {
            if (wallet.getProvider()) {
                available.push({
                    id: key,
                    name: wallet.name,
                    icon: wallet.icon,
                    url: wallet.url,
                });
            }
        }

        return available;
    },

    /**
     * Connect to a specific wallet
     * @param {string} walletId - Wallet identifier (phantom, solflare, etc.)
     */
    async connect(walletId = null) {
        try {
            // If no wallet specified, try the first available
            if (!walletId) {
                const available = this.getAvailableWallets();
                if (available.length === 0) {
                    throw new Error('No wallet found. Please install Phantom or Solflare.');
                }
                walletId = available[0].id;
            }

            const walletInfo = this.supportedWallets[walletId];
            if (!walletInfo) {
                throw new Error(`Unknown wallet: ${walletId}`);
            }

            const provider = walletInfo.getProvider();
            if (!provider) {
                throw new Error(`${walletInfo.name} is not installed. Visit ${walletInfo.url}`);
            }

            console.log(`[Wallet] Connecting to ${walletInfo.name}...`);
            this.provider = provider;

            const response = await provider.connect();
            const publicKey = response.publicKey.toString();

            this.handleConnect(publicKey, walletId);
            localStorage.setItem('trader-tony-wallet', walletId);

            return { publicKey, wallet: walletId };
        } catch (error) {
            console.error('[Wallet] Connection error:', error);
            this.triggerHandlers('onError', { error: error.message });
            throw error;
        }
    },

    /**
     * Disconnect current wallet
     */
    async disconnect() {
        try {
            if (this.provider && this.provider.disconnect) {
                await this.provider.disconnect();
            }

            this.handleDisconnect();
            localStorage.removeItem('trader-tony-wallet');
        } catch (error) {
            console.error('[Wallet] Disconnect error:', error);
            // Still clear local state even if provider disconnect fails
            this.handleDisconnect();
        }
    },

    /**
     * Handle successful connection
     */
    handleConnect(publicKey, walletId) {
        this.connected = true;
        this.publicKey = publicKey;
        this.walletName = this.supportedWallets[walletId]?.name || walletId;

        this.triggerHandlers('onConnect', {
            publicKey,
            wallet: walletId,
            walletName: this.walletName,
        });
    },

    /**
     * Handle disconnection
     */
    handleDisconnect() {
        this.connected = false;
        this.publicKey = null;
        this.walletName = null;
        this.provider = null;

        this.triggerHandlers('onDisconnect', {});
    },

    /**
     * Handle account change
     */
    handleAccountChange(publicKey) {
        this.publicKey = publicKey;

        this.triggerHandlers('onAccountChange', {
            publicKey,
            wallet: this.walletName,
        });
    },

    /**
     * Sign a message with the connected wallet
     * @param {string} message - Message to sign
     * @returns {Promise<string>} Base64 encoded signature
     */
    async signMessage(message) {
        if (!this.connected || !this.provider) {
            throw new Error('Wallet not connected');
        }

        try {
            const encodedMessage = new TextEncoder().encode(message);
            const { signature } = await this.provider.signMessage(encodedMessage, 'utf8');

            // Convert to base64
            return btoa(String.fromCharCode.apply(null, signature));
        } catch (error) {
            console.error('[Wallet] Sign message error:', error);
            throw error;
        }
    },

    /**
     * Sign and send a transaction (for copy trading)
     * @param {object} transaction - Serialized transaction
     * @returns {Promise<string>} Transaction signature
     */
    async signAndSendTransaction(transaction) {
        if (!this.connected || !this.provider) {
            throw new Error('Wallet not connected');
        }

        try {
            const { signature } = await this.provider.signAndSendTransaction(transaction);
            return signature;
        } catch (error) {
            console.error('[Wallet] Transaction error:', error);
            throw error;
        }
    },

    /**
     * Get shortened wallet address for display
     * @param {string} address - Full wallet address
     * @param {number} chars - Number of characters to show on each end
     * @returns {string} Shortened address
     */
    shortenAddress(address = this.publicKey, chars = 4) {
        if (!address) return '';
        return `${address.slice(0, chars)}...${address.slice(-chars)}`;
    },

    /**
     * Get Solscan URL for address
     * @param {string} address - Wallet address
     * @returns {string} Solscan URL
     */
    getSolscanUrl(address = this.publicKey) {
        if (!address) return '#';
        return `https://solscan.io/account/${address}`;
    },

    // ==========================================
    // Event Handler Registration
    // ==========================================

    /**
     * Register an event handler
     * @param {string} event - Event name (connect, disconnect, accountChange, error)
     * @param {function} handler - Handler function
     * @returns {function} Unsubscribe function
     */
    on(event, handler) {
        const eventName = `on${event.charAt(0).toUpperCase()}${event.slice(1)}`;

        if (!this.handlers[eventName]) {
            console.warn(`[Wallet] Unknown event: ${event}`);
            return () => {};
        }

        this.handlers[eventName].push(handler);

        return () => {
            const index = this.handlers[eventName].indexOf(handler);
            if (index > -1) {
                this.handlers[eventName].splice(index, 1);
            }
        };
    },

    /**
     * Trigger all handlers for an event
     */
    triggerHandlers(eventName, data) {
        if (!this.handlers[eventName]) {
            return;
        }

        for (const handler of this.handlers[eventName]) {
            try {
                handler(data);
            } catch (error) {
                console.error(`[Wallet] Handler error for ${eventName}:`, error);
            }
        }
    },

    // ==========================================
    // Verification for Copy Trading
    // ==========================================

    /**
     * Generate and sign a verification message for copy trade registration
     * @returns {Promise<object>} Wallet address and signature
     */
    async generateVerification() {
        if (!this.connected) {
            throw new Error('Wallet not connected');
        }

        const timestamp = Date.now();
        const message = `TraderTony V4 Copy Trade Registration\nWallet: ${this.publicKey}\nTimestamp: ${timestamp}`;

        const signature = await this.signMessage(message);

        return {
            publicKey: this.publicKey,
            message,
            signature,
            timestamp,
        };
    },
};

// Initialize when script loads
WalletManager.init();
