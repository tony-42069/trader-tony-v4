# TraderTony V4 Deployment Guide

This guide outlines how to deploy TraderTony V4.

**IMPORTANT:** Running a trading bot carries significant risk. Ensure you understand the code and risks involved before deploying, especially with real funds. Always use a dedicated burner wallet.

## Local Deployment (Development & Testing)

This is suitable for development and testing purposes.

### Prerequisites

- Rust (latest stable recommended)
- Telegram Bot Token
- Helius API Key
- Solana Wallet Private Key (Base58 encoded)

### Steps

1.  **Clone Repository:**
    ```bash
    git clone https://github.com/yourusername/trader-tony-v4.git # Replace with actual URL
    cd trader-tony-v4
    ```
2.  **Configure `.env`:** Create a `.env` file in the project root (refer to `README.md` for the format and required variables). Ensure `DEMO_MODE` is set appropriately (`true` for testing, `false` for live trading - **USE CAUTION**).
3.  **Build:**
    ```bash
    cargo build --release
    ```
4.  **Create Data Directory:** (Required for position persistence)
    ```bash
    mkdir data
    ```
5.  **Run:**
    ```bash
    ./target/release/trader-tony-v4
    ```
    The bot will start, connect to Telegram, and begin logging to the console. Press `Ctrl+C` to stop.

## Server Deployment (Linux using systemd)

This provides a more robust way to run the bot continuously on a Linux server (e.g., a VPS).

### Prerequisites

- A Linux server (Ubuntu, Debian, etc.)
- Rust installed on the server
- Git installed on the server
- Bot prerequisites (API keys, etc.)

### Steps

1.  **SSH into your server.**
2.  **Clone Repository & Build:** Follow steps 1 and 3 from the Local Deployment section on the server.
3.  **Configure `.env`:** Create and configure the `.env` file on the server in the project directory. **Ensure `DEMO_MODE=false` if running live.**
4.  **Create Data Directory:**
    ```bash
    mkdir /path/to/trader-tony-v4/data # Use the actual path
    ```
5.  **Create systemd Service File:**
    ```bash
    sudo nano /etc/systemd/system/tradertony.service
    ```
6.  **Paste and Edit Service Configuration:**
    Replace `your_server_user` and `/path/to/trader-tony-v4` with your actual server username and the full path to the cloned project directory.

    ```ini
    [Unit]
    Description=TraderTony V4 Solana Trading Bot
    After=network.target # Ensure network is up before starting

    [Service]
    User=your_server_user # The user the bot should run as
    Group=your_server_user # Optional: Specify group
    WorkingDirectory=/path/to/trader-tony-v4 # IMPORTANT: Set correct path
    EnvironmentFile=/path/to/trader-tony-v4/.env # Load environment variables from .env
    ExecStart=/path/to/trader-tony-v4/target/release/trader-tony-v4 # Path to the compiled binary
    Restart=on-failure # Restart if the bot crashes
    RestartSec=10s # Wait 10 seconds before restarting
    StandardOutput=journal # Redirect stdout to journald
    StandardError=journal # Redirect stderr to journald

    [Install]
    WantedBy=multi-user.target # Start on system boot
    ```
7.  **Reload systemd, Enable, and Start:**
    ```bash
    sudo systemctl daemon-reload
    sudo systemctl enable tradertony.service # Enable to start on boot
    sudo systemctl start tradertony.service
    ```
8.  **Check Status:**
    ```bash
    sudo systemctl status tradertony.service
    ```
    (Press `q` to exit status view)
9.  **View Logs:**
    ```bash
    sudo journalctl -u tradertony.service -f # Follow logs in real-time
    ```
    (Press `Ctrl+C` to stop following)

## Docker Deployment (Alternative)

Docker provides containerization for easier dependency management and deployment.

### Prerequisites

- Docker installed

### Steps

1.  **Create a `Dockerfile`** in the project root:

    ```dockerfile
    # Stage 1: Build the application
    FROM rust:latest as builder
    WORKDIR /app
    # Copy manifests first for layer caching
    COPY Cargo.toml Cargo.lock ./
    # Build dummy project to cache dependencies
    RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src target
    # Copy full source code
    COPY . .
    # Build the actual application
    RUN cargo build --release

    # Stage 2: Create the final minimal image
    FROM debian:bullseye-slim
    # Install necessary runtime dependencies (e.g., SSL certificates)
    RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
    # Copy the built binary from the builder stage
    COPY --from=builder /app/target/release/trader-tony-v4 /usr/local/bin/trader-tony-v4
    # Create data directory within the container
    RUN mkdir /data
    WORKDIR /app # Set working directory (optional, but good practice)

    # Set user (optional, for better security)
    # RUN useradd -ms /bin/bash trader
    # USER trader

    # Command to run the application
    # The .env file will be mounted as a volume
    CMD ["/usr/local/bin/trader-tony-v4"]
    ```
2.  **Configure `.env`:** Create your `.env` file locally.
3.  **Build the Docker Image:**
    ```bash
    docker build -t trader-tony-v4 .
    ```
4.  **Run the Docker Container:**
    Mount the local `.env` file and the `data` directory into the container.

    ```bash
    # Create data directory locally if it doesn't exist
    mkdir -p data

    docker run -d --name tradertony \
      -v $(pwd)/.env:/app/.env \
      -v $(pwd)/data:/data \
      --restart unless-stopped \
      trader-tony-v4
    ```
    *   `-d`: Run in detached mode (background).
    *   `--name tradertony`: Assign a name to the container.
    *   `-v $(pwd)/.env:/app/.env`: Mount your local `.env` file into the container at `/app/.env`. Adjust `/app/.env` if your `WORKDIR` or `.env` loading logic differs.
    *   `-v $(pwd)/data:/data`: Mount the local `data` directory for position persistence.
    *   `--restart unless-stopped`: Automatically restart the container unless manually stopped.
5.  **View Logs:**
    ```bash
    docker logs -f tradertony
    ```
6.  **Stop Container:**
    ```bash
    docker stop tradertony
    ```
7.  **Remove Container:**
    ```bash
    docker rm tradertony
    ```

## Updating the Bot

### Local / systemd

1.  Navigate to the project directory: `cd /path/to/trader-tony-v4`
2.  Stop the bot (`Ctrl+C` locally, or `sudo systemctl stop tradertony` on server).
3.  Pull the latest code: `git pull origin main` (or your branch).
4.  Rebuild: `cargo build --release`.
5.  Restart the bot (run command locally, or `sudo systemctl start tradertony` on server).

### Docker

1.  Navigate to the project directory locally.
2.  Pull the latest code: `git pull origin main`.
3.  Stop and remove the old container:
    ```bash
    docker stop tradertony
    docker rm tradertony
    ```
4.  Rebuild the Docker image: `docker build -t trader-tony-v4 .`
5.  Run the new container using the `docker run` command from step 4 of Docker deployment. Your `.env` and `data` volume will be reused.
