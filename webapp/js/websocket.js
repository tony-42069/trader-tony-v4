/**
 * TraderTony V4 - WebSocket Client
 * Handles real-time communication with the Rust backend
 */

const WebSocketClient = {
    // Connection state
    socket: null,
    url: null,
    reconnectAttempts: 0,
    maxReconnectAttempts: 10,
    reconnectDelay: 1000,
    reconnectTimer: null,
    heartbeatTimer: null,
    heartbeatInterval: 30000,
    isConnecting: false,
    isIntentionallyClosed: false,
    demoMode: false,
    demoTimer: null,

    // Event handlers
    handlers: {
        onConnect: [],
        onDisconnect: [],
        onMessage: [],
        onError: [],
        onReconnect: [],
        // Specific message type handlers
        onPositionUpdate: [],
        onTradeExecuted: [],
        onPriceUpdate: [],
        onStatusChange: [],
        onAlert: [],
    },

    /**
     * Initialize the WebSocket client
     * @param {string} url - WebSocket URL (optional, auto-detected)
     */
    init(url = null) {
        if (url) {
            this.url = url;
        } else if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
            this.url = 'ws://127.0.0.1:3000/ws';
        } else {
            // Production - use Railway backend directly (Vercel can't proxy WebSocket)
            this.url = window.WS_URL || 'wss://trader-tony.up.railway.app/ws';
        }

        console.log(`[WebSocket] Initialized with URL: ${this.url}`);
    },

    /**
     * Connect to the WebSocket server
     */
    connect() {
        if (this.isConnecting || (this.socket && this.socket.readyState === WebSocket.OPEN)) {
            console.log('[WebSocket] Already connected or connecting');
            return;
        }

        // Demo mode simulation
        if (this.demoMode) {
            this.simulateDemoConnection();
            return;
        }

        this.isConnecting = true;
        this.isIntentionallyClosed = false;

        console.log(`[WebSocket] Connecting to ${this.url}...`);

        try {
            this.socket = new WebSocket(this.url);
            this.setupEventListeners();
        } catch (error) {
            console.error('[WebSocket] Connection error:', error);
            this.isConnecting = false;
            this.triggerHandlers('onError', { error: error.message });
            this.scheduleReconnect();
        }
    },

    /**
     * Setup WebSocket event listeners
     */
    setupEventListeners() {
        this.socket.onopen = () => {
            console.log('[WebSocket] Connected');
            this.isConnecting = false;
            this.reconnectAttempts = 0;
            this.startHeartbeat();
            this.triggerHandlers('onConnect', {});
        };

        this.socket.onclose = (event) => {
            console.log(`[WebSocket] Disconnected (code: ${event.code})`);
            this.isConnecting = false;
            this.stopHeartbeat();
            this.triggerHandlers('onDisconnect', { code: event.code, reason: event.reason });

            if (!this.isIntentionallyClosed) {
                this.scheduleReconnect();
            }
        };

        this.socket.onerror = (error) => {
            console.error('[WebSocket] Error:', error);
            this.triggerHandlers('onError', { error: 'WebSocket error' });
        };

        this.socket.onmessage = (event) => {
            this.handleMessage(event.data);
        };
    },

    /**
     * Handle incoming WebSocket messages
     * @param {string} data - Raw message data
     */
    handleMessage(data) {
        try {
            const message = JSON.parse(data);

            // Log for debugging
            console.log('[WebSocket] Received:', message.type || 'unknown', message);

            // Trigger general message handlers
            this.triggerHandlers('onMessage', message);

            // Trigger type-specific handlers
            switch (message.type) {
                case 'position_update':
                case 'position_opened':
                case 'position_closed':
                    this.triggerHandlers('onPositionUpdate', message.data || message);
                    break;

                case 'trade_executed':
                case 'trade':
                    this.triggerHandlers('onTradeExecuted', message.data || message);
                    break;

                case 'price_update':
                case 'price':
                    this.triggerHandlers('onPriceUpdate', message.data || message);
                    break;

                case 'status_change':
                case 'status':
                    this.triggerHandlers('onStatusChange', message.data || message);
                    break;

                case 'alert':
                case 'notification':
                    this.triggerHandlers('onAlert', message.data || message);
                    break;

                case 'pong':
                case 'heartbeat':
                    // Heartbeat response, connection is alive
                    break;

                default:
                    console.log('[WebSocket] Unknown message type:', message.type);
            }
        } catch (error) {
            console.error('[WebSocket] Error parsing message:', error, data);
        }
    },

    /**
     * Send a message to the server
     * @param {string} type - Message type
     * @param {object} data - Message payload
     */
    send(type, data = {}) {
        if (!this.isConnected()) {
            console.warn('[WebSocket] Cannot send - not connected');
            return false;
        }

        const message = JSON.stringify({ type, data, timestamp: Date.now() });
        this.socket.send(message);
        return true;
    },

    /**
     * Send a ping to keep the connection alive
     */
    ping() {
        this.send('ping', {});
    },

    /**
     * Start the heartbeat timer
     */
    startHeartbeat() {
        this.stopHeartbeat();
        this.heartbeatTimer = setInterval(() => {
            if (this.isConnected()) {
                this.ping();
            }
        }, this.heartbeatInterval);
    },

    /**
     * Stop the heartbeat timer
     */
    stopHeartbeat() {
        if (this.heartbeatTimer) {
            clearInterval(this.heartbeatTimer);
            this.heartbeatTimer = null;
        }
    },

    /**
     * Schedule a reconnection attempt
     */
    scheduleReconnect() {
        if (this.reconnectTimer) {
            return;
        }

        if (this.reconnectAttempts >= this.maxReconnectAttempts) {
            console.error('[WebSocket] Max reconnection attempts reached');
            this.triggerHandlers('onError', { error: 'Max reconnection attempts reached' });
            return;
        }

        this.reconnectAttempts++;
        const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

        console.log(`[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

        this.triggerHandlers('onReconnect', {
            attempt: this.reconnectAttempts,
            maxAttempts: this.maxReconnectAttempts,
            delay,
        });

        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = null;
            this.connect();
        }, delay);
    },

    /**
     * Disconnect from the server
     */
    disconnect() {
        console.log('[WebSocket] Disconnecting...');
        this.isIntentionallyClosed = true;

        if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer);
            this.reconnectTimer = null;
        }

        this.stopHeartbeat();
        this.stopDemoMode();

        if (this.socket) {
            this.socket.close();
            this.socket = null;
        }
    },

    /**
     * Check if WebSocket is connected
     * @returns {boolean}
     */
    isConnected() {
        if (this.demoMode) {
            return true;
        }
        return this.socket && this.socket.readyState === WebSocket.OPEN;
    },

    /**
     * Get connection state as string
     * @returns {string}
     */
    getState() {
        if (this.demoMode) {
            return 'demo';
        }
        if (!this.socket) {
            return 'disconnected';
        }
        switch (this.socket.readyState) {
            case WebSocket.CONNECTING:
                return 'connecting';
            case WebSocket.OPEN:
                return 'connected';
            case WebSocket.CLOSING:
                return 'closing';
            case WebSocket.CLOSED:
                return 'disconnected';
            default:
                return 'unknown';
        }
    },

    // ==========================================
    // Event Handler Registration
    // ==========================================

    /**
     * Register an event handler
     * @param {string} event - Event name
     * @param {function} handler - Handler function
     * @returns {function} Unsubscribe function
     */
    on(event, handler) {
        const eventName = `on${event.charAt(0).toUpperCase()}${event.slice(1)}`;

        if (!this.handlers[eventName]) {
            console.warn(`[WebSocket] Unknown event: ${event}`);
            return () => {};
        }

        this.handlers[eventName].push(handler);

        // Return unsubscribe function
        return () => {
            const index = this.handlers[eventName].indexOf(handler);
            if (index > -1) {
                this.handlers[eventName].splice(index, 1);
            }
        };
    },

    /**
     * Remove an event handler
     * @param {string} event - Event name
     * @param {function} handler - Handler function to remove
     */
    off(event, handler) {
        const eventName = `on${event.charAt(0).toUpperCase()}${event.slice(1)}`;

        if (!this.handlers[eventName]) {
            return;
        }

        const index = this.handlers[eventName].indexOf(handler);
        if (index > -1) {
            this.handlers[eventName].splice(index, 1);
        }
    },

    /**
     * Trigger all handlers for an event
     * @param {string} eventName - Internal event name (e.g., 'onConnect')
     * @param {object} data - Event data
     */
    triggerHandlers(eventName, data) {
        if (!this.handlers[eventName]) {
            return;
        }

        for (const handler of this.handlers[eventName]) {
            try {
                handler(data);
            } catch (error) {
                console.error(`[WebSocket] Handler error for ${eventName}:`, error);
            }
        }
    },

    // ==========================================
    // Demo Mode
    // ==========================================

    /**
     * Enable demo mode with simulated updates
     */
    enableDemoMode() {
        this.demoMode = true;
        console.log('[WebSocket] Demo mode enabled');
    },

    /**
     * Disable demo mode
     */
    stopDemoMode() {
        this.demoMode = false;
        if (this.demoTimer) {
            clearInterval(this.demoTimer);
            this.demoTimer = null;
        }
    },

    /**
     * Simulate a WebSocket connection in demo mode
     */
    simulateDemoConnection() {
        console.log('[WebSocket] Simulating demo connection...');

        // Simulate connection delay
        setTimeout(() => {
            this.triggerHandlers('onConnect', {});
            this.startDemoUpdates();
        }, 500);
    },

    /**
     * Start sending simulated updates in demo mode
     */
    startDemoUpdates() {
        if (this.demoTimer) {
            return;
        }

        // Send periodic updates
        this.demoTimer = setInterval(() => {
            // Randomly send different update types
            const updateType = Math.random();

            if (updateType < 0.4) {
                // Price update
                this.triggerHandlers('onPriceUpdate', {
                    position_id: 'pos_001',
                    current_price_sol: 0.0000312 * (0.95 + Math.random() * 0.1),
                    pnl_percent: 30 + (Math.random() * 10 - 5),
                });
            } else if (updateType < 0.6) {
                // Status update
                this.triggerHandlers('onStatusChange', {
                    autotrader_running: true,
                    active_positions: 2,
                    last_scan: new Date().toISOString(),
                });
            } else if (updateType < 0.7) {
                // Occasional alert
                const alerts = [
                    { level: 'info', message: 'Scanning for new opportunities...' },
                    { level: 'success', message: 'Position updated' },
                    { level: 'warning', message: 'High volatility detected' },
                ];
                const alert = alerts[Math.floor(Math.random() * alerts.length)];
                this.triggerHandlers('onAlert', alert);
            }
        }, 5000);
    },

    /**
     * Subscribe to a specific topic
     * @param {string} topic - Topic to subscribe to
     */
    subscribe(topic) {
        this.send('subscribe', { topic });
    },

    /**
     * Unsubscribe from a topic
     * @param {string} topic - Topic to unsubscribe from
     */
    unsubscribe(topic) {
        this.send('unsubscribe', { topic });
    },
};

// Auto-initialize when script loads
WebSocketClient.init();
