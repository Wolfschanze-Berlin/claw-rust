# Project: claw-rust

Rust port of [openclaw/openclaw](https://github.com/openclaw/openclaw) — a multi-channel AI chatbot gateway (TypeScript).

## Key Facts
- **Repo**: github.com/Wolfschanze-Berlin/claw-rust
- **Epic**: #1 (28 sub-issues, #2-#28, across 13 phases)
- **Constraint**: 1:1 API match — WebSocket RPC protocol, config schema, channel plugin interface must be identical
- **Tech stack**: actix-web, tokio, tracing, serde, SQLite (rusqlite), egui, teloxide (Telegram)
- **Config format**: JSON5 with `$include` + `${ENV}` substitution
- **Architecture**: Cargo workspace with crates: claw-config, claw-core, claw-gateway, claw-routing, claw-channels, claw-plugins, claw-dispatch, claw-db, claw-app

## Detailed Analysis
See [openclaw-analysis.md](openclaw-analysis.md) for full OpenClaw architecture breakdown.

## Phase Order
1. Foundation (config, errors, logging) → 2. Core Traits → 3. Registry → 4. Routing → 5. Protocol → 6. Gateway → 7. Command Queue → 8. Dispatch → 9. Plugins → 10. SQLite → 11. Telegram → 12. Installer → 13. egui
