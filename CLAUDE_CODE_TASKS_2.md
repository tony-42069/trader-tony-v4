# TraderTony V4 - Claude Code Implementation Guide
## Project Path: D:\AI Projects\trader-tony-v4
## GitHub Repo: https://github.com/tony-42069/trader-tony-v4

---

## ⚠️ IMPORTANT: WORKFLOW RULES

**Claude Code MUST submit a Pull Request after each meaningful change/modification.**

```
Workflow:
1. Create feature branch from main (e.g., feat/phase-1-api-server)
2. Make changes
3. Commit with clear message
4. Push branch
5. Create PR with description of changes
6. Wait for review/merge before starting next task
```

---

## CONTEXT FOR CLAUDE CODE

TraderTony V4 is an autonomous Solana memecoin trading bot written in Rust. The bot:
- Discovers new tokens via Helius DAS API
- Analyzes risk (honeypot detection, liquidity, authorities, etc.)
- Executes trades via Jupiter aggregator
- Manages positions with SL/TP/Trailing Stop

**CURRENT STATE**: Core trading logic is ~90% complete. Telegram interface exists but needs to be REMOVED and replaced with a REST API + Web Dashboard.

**GOAL**: Fully autonomous bot with:
1. Public webapp showing bot's trades and performance
2. Manual copy trade signals (users see and copy manually)
3. Auto-copy trade feature (users connect wallet, trades execute automatically)
4. 10% profit fee on auto-copy trade sells

**DEPLOYMENT TARGET**:
- Backend: Railway (Rust binary)
- Frontend: Vercel (Static HTML/JS)

---

## ARCHITECTURE OVERVIEW

```
┌─────────────────────────────────────────────────────────────────┐
│                    VERCEL (FRONTEND)                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Web Dashboard (HTML/JS)                     │   │
│  │  • Bot Status & Stats                                    │   │
│  │  • Active Positions                                      │   │
│  │  • Trade History                                         │   │
│  │  • Copy Trade Signals                                    │   │
│  │  • Wallet Connect (Phantom/Solflare)                     │   │
│  │  • Auto-Copy Toggle                                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────┬────────────────────────────────────┘
                             │ HTTPS / WSS
┌────────────────────────────▼────────────────────────────────────┐
│                    RAILWAY (BACKEND)                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Rust API Server (Axum)                      │   │
│  │  • REST API Endpoints                                    │   │
│  │  • WebSocket for Real-time Updates                       │   │
│  │  • Copy Trade Transaction Builder                        │   │
│  │  • User Wallet Tracking                                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              AutoTrader Engine                           │   │
│  │  • Token Discovery (Helius)                              │   │
│  │  • Risk Analysis                                         │   │
│  │  • Trade Execution (Jupiter)                             │   │
│  │  • Position Management                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## PHASE 1: REMOVE TELEGRAM & ADD REST API
**Priority: CRITICAL**
**Branch: feat/phase-1-remove-telegram-add-api**

### Task 1.1: Update Cargo.toml Dependencies
### Task 1.2: Delete Telegram Bot Module  
### Task 1.3: Create Web API Module Structure
### Task 1.4: Implement AppState and Server
### Task 1.5: Implement Core REST Endpoints
### Task 1.6: Implement WebSocket Handler
### Task 1.7: Update main.rs
### Task 1.8: Update Config

---

## PHASE 2: BUILD WEB DASHBOARD (FRONTEND)
**Priority: HIGH**
**Branch: feat/phase-2-web-dashboard**

### Task 2.1-2.8: Dashboard implementation with wallet connect

---

## PHASE 3: COPY TRADE SYSTEM  
**Priority: HIGH**
**Branch: feat/phase-3-copy-trade**

### Task 3.1-3.6: Copy trade with 10% profit fee

---

## PHASE 4: DEPLOYMENT
**Priority: HIGH**
**Branch: feat/phase-4-deployment**

### Task 4.1-4.4: Railway + Vercel deployment

---

## PHASE 5: TESTING & LAUNCH

### Task 5.1-5.4: Testing and production launch

---

*See full document at D:\AI Projects\trader-tony-v4\CLAUDE_CODE_TASKS.md*
*Document Version: 2.0 - January 7, 2026*
