/**
 * TraderTony V4 - PNL Chart
 * Displays cumulative PNL over time with cyberpunk styling
 */

const PNLChart = {
    chart: null,
    ctx: null,
    data: [],

    // Cyberpunk color scheme
    colors: {
        profit: '#00ff9d',
        loss: '#ff0055',
        zero: 'rgba(255, 255, 255, 0.2)',
        grid: 'rgba(255, 255, 255, 0.05)',
        text: '#8892b0',
        gradientProfit: 'rgba(0, 255, 157, 0.1)',
        gradientLoss: 'rgba(255, 0, 85, 0.1)',
    },

    /**
     * Initialize the PNL chart
     */
    init() {
        const canvas = document.getElementById('pnlChart');
        if (!canvas) {
            console.warn('[Chart] Canvas element not found');
            return;
        }

        this.ctx = canvas.getContext('2d');
        this.createChart();
        console.log('[Chart] PNL Chart initialized');
    },

    /**
     * Create the Chart.js instance
     */
    createChart() {
        // Demo data - will be replaced with real data
        const demoData = this.generateDemoData();

        // Create gradient for the line fill
        const gradientProfit = this.ctx.createLinearGradient(0, 0, 0, 300);
        gradientProfit.addColorStop(0, 'rgba(0, 255, 157, 0.3)');
        gradientProfit.addColorStop(1, 'rgba(0, 255, 157, 0)');

        const gradientLoss = this.ctx.createLinearGradient(0, 0, 0, 300);
        gradientLoss.addColorStop(0, 'rgba(255, 0, 85, 0)');
        gradientLoss.addColorStop(1, 'rgba(255, 0, 85, 0.3)');

        this.chart = new Chart(this.ctx, {
            type: 'line',
            data: {
                labels: demoData.labels,
                datasets: [{
                    label: 'Cumulative PNL (SOL)',
                    data: demoData.values,
                    borderColor: (context) => {
                        const value = context.raw;
                        return value >= 0 ? this.colors.profit : this.colors.loss;
                    },
                    backgroundColor: (context) => {
                        const chart = context.chart;
                        const {ctx, chartArea} = chart;
                        if (!chartArea) return null;

                        // Create gradient based on data
                        const gradient = ctx.createLinearGradient(0, chartArea.top, 0, chartArea.bottom);
                        gradient.addColorStop(0, 'rgba(0, 255, 157, 0.2)');
                        gradient.addColorStop(0.5, 'rgba(0, 255, 157, 0.05)');
                        gradient.addColorStop(0.5, 'rgba(255, 0, 85, 0.05)');
                        gradient.addColorStop(1, 'rgba(255, 0, 85, 0.2)');
                        return gradient;
                    },
                    borderWidth: 2,
                    fill: true,
                    tension: 0.4,
                    pointRadius: 0,
                    pointHoverRadius: 6,
                    pointHoverBackgroundColor: '#fff',
                    pointHoverBorderColor: this.colors.profit,
                    pointHoverBorderWidth: 2,
                    segment: {
                        borderColor: (ctx) => {
                            const p0 = ctx.p0.parsed.y;
                            const p1 = ctx.p1.parsed.y;
                            // Color segment based on whether it's going up or down
                            if (p1 >= 0 && p0 >= 0) return this.colors.profit;
                            if (p1 < 0 && p0 < 0) return this.colors.loss;
                            // Transition segment
                            return p1 >= p0 ? this.colors.profit : this.colors.loss;
                        }
                    }
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                interaction: {
                    intersect: false,
                    mode: 'index',
                },
                plugins: {
                    legend: {
                        display: false,
                    },
                    tooltip: {
                        backgroundColor: 'rgba(11, 13, 18, 0.95)',
                        titleColor: '#fff',
                        bodyColor: '#8892b0',
                        borderColor: 'rgba(0, 242, 255, 0.3)',
                        borderWidth: 1,
                        padding: 12,
                        displayColors: false,
                        callbacks: {
                            title: (items) => {
                                return items[0].label;
                            },
                            label: (item) => {
                                const value = item.raw;
                                const sign = value >= 0 ? '+' : '';
                                const color = value >= 0 ? '🟢' : '🔴';
                                return `${color} PNL: ${sign}${value.toFixed(4)} SOL`;
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        grid: {
                            color: this.colors.grid,
                            drawBorder: false,
                        },
                        ticks: {
                            color: this.colors.text,
                            font: {
                                family: "'JetBrains Mono', monospace",
                                size: 10,
                            },
                            maxRotation: 0,
                            autoSkip: true,
                            maxTicksLimit: 7,
                        }
                    },
                    y: {
                        grid: {
                            color: (context) => {
                                if (context.tick.value === 0) {
                                    return 'rgba(255, 255, 255, 0.3)';
                                }
                                return this.colors.grid;
                            },
                            lineWidth: (context) => {
                                return context.tick.value === 0 ? 2 : 1;
                            },
                            drawBorder: false,
                        },
                        ticks: {
                            color: (context) => {
                                if (context.tick.value > 0) return this.colors.profit;
                                if (context.tick.value < 0) return this.colors.loss;
                                return this.colors.text;
                            },
                            font: {
                                family: "'JetBrains Mono', monospace",
                                size: 10,
                            },
                            callback: (value) => {
                                const sign = value >= 0 ? '+' : '';
                                return `${sign}${value.toFixed(2)}`;
                            }
                        }
                    }
                },
                animation: {
                    duration: 1000,
                    easing: 'easeOutQuart',
                }
            }
        });
    },

    /**
     * Generate demo data for the chart
     * @returns {object} Demo data with labels and values
     */
    generateDemoData() {
        const days = 14;
        const labels = [];
        const values = [];
        let cumulative = 0;

        const today = new Date();

        for (let i = days - 1; i >= 0; i--) {
            const date = new Date(today);
            date.setDate(date.getDate() - i);
            labels.push(date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }));

            // Generate realistic-looking PNL fluctuations
            // Starting flat, then showing some trading activity
            if (i > 10) {
                // First few days - no trading yet
                cumulative = 0;
            } else {
                // Random daily PNL between -0.3 and +0.5 SOL
                const dailyPnl = (Math.random() - 0.35) * 0.8;
                cumulative += dailyPnl;
            }

            values.push(parseFloat(cumulative.toFixed(4)));
        }

        return { labels, values };
    },

    /**
     * Update chart with new trade data
     * @param {Array} trades - Array of trade objects with pnl and timestamp
     */
    updateWithTrades(trades) {
        if (!this.chart || !trades || trades.length === 0) return;

        // Group trades by day and calculate cumulative PNL
        const dailyPnl = {};
        let cumulative = 0;

        // Sort trades by timestamp
        const sortedTrades = [...trades].sort((a, b) =>
            new Date(a.timestamp) - new Date(b.timestamp)
        );

        sortedTrades.forEach(trade => {
            const date = new Date(trade.timestamp).toLocaleDateString('en-US', {
                month: 'short',
                day: 'numeric'
            });

            cumulative += trade.pnl_sol || 0;
            dailyPnl[date] = cumulative;
        });

        const labels = Object.keys(dailyPnl);
        const values = Object.values(dailyPnl);

        // Update chart data
        this.chart.data.labels = labels;
        this.chart.data.datasets[0].data = values;
        this.chart.update('none'); // No animation for data updates
    },

    /**
     * Add a new data point to the chart
     * @param {string} label - Date label
     * @param {number} value - Cumulative PNL value
     */
    addDataPoint(label, value) {
        if (!this.chart) return;

        this.chart.data.labels.push(label);
        this.chart.data.datasets[0].data.push(value);

        // Keep only last 30 data points
        if (this.chart.data.labels.length > 30) {
            this.chart.data.labels.shift();
            this.chart.data.datasets[0].data.shift();
        }

        this.chart.update();
    },

    /**
     * Reset chart to empty state
     */
    reset() {
        if (!this.chart) return;

        this.chart.data.labels = [];
        this.chart.data.datasets[0].data = [];
        this.chart.update();
    },

    /**
     * Refresh with demo data
     */
    loadDemoData() {
        if (!this.chart) return;

        const demoData = this.generateDemoData();
        this.chart.data.labels = demoData.labels;
        this.chart.data.datasets[0].data = demoData.values;
        this.chart.update();
    }
};

// Initialize chart when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    // Small delay to ensure Chart.js is loaded
    setTimeout(() => {
        PNLChart.init();
    }, 100);
});
