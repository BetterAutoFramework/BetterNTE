---
date: 2026-05-09
type: feature
scope: betternte-relay, web
---

# Add relay server and web flow editor MVP

## Summary

Added two new modules to enable browser-based flow editing with real-time execution on the local client:

1. **betternte-relay** — WebSocket relay server for browser↔client communication
2. **web/** — React Flow based visual flow editor

## Architecture

```
Browser (React Flow) ←WS→ Relay Server ←WS→ BetterNTE Client
```

- Relay server runs on port 9280, supports session-based pairing
- Clients register and get a session_id (UUID)
- Browsers join a session by session_id
- All flow execution messages are relayed bidirectionally

## betternte-relay

- axum WebSocket server with CORS support
- Session management: one client per session, multiple browsers can watch
- Message types: register, join, flow:run, flow:stop, flow:log, flow:step, flow:done
- Health check at GET /health
- Static file serving from web/dist (production)

## web/ frontend

- Vite + React + TypeScript + Tailwind CSS
- @xyflow/react for node-based editor
- Zustand for state management
- 8 step types with custom node components:
  - script (purple), click (green), swipe (blue), key_press (yellow)
  - wait (gray), set_variable (orange), flow (cyan), group (teal)
- Drag-to-add step palette
- Node config panel with type-specific forms
- Real-time log panel with level filtering
- WebSocket relay hook with auto-reconnect
- Flow JSON import/export (compatible with BetterNTE runtime format)
- Run/Stop controls

## How to run

```bash
# Terminal 1: Relay server
cargo run -p betternte-relay

# Terminal 2: Web frontend
cd web && npm run dev

# Open http://localhost:5173
# Enter session ID from client to connect
```

## Files added

- `crates/betternte-relay/Cargo.toml`
- `crates/betternte-relay/src/main.rs`
- `web/` — entire frontend (12 source files)
