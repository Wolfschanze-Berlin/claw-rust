# Project Guidelines

## Code Style

- Rust 2024 workspace; use `rustfmt` defaults. Prefer LSP diagnostics over ad-hoc `cargo check` for quick feedback.
- Match JSON/TypeScript shapes with `#[serde(rename = "FieldName")]`; mark optional fields with `Option<T>` plus `#[serde(skip_serializing_if = "Option::is_none")]`.
- Discriminated unions use `#[serde(tag = "type")]` enums. Use `async_trait` for async traits and `tokio_util::sync::CancellationToken` for abort semantics.
- UI/egui: never block the render thread—bridge async ↔ sync with `mpsc` channels.

## Architecture

- Multi-crate workspace (see `APP_WORKFLOW.md` for diagrams):
  - `claw-core` (errors, runtime), `claw-config` (JSON5 loader with `$include` / `${ENV}`), `claw-db` (SQLite WAL)
  - Channel adapters: `claw-channels`, `claw-telegram`, `claw-discord`, `claw-whatsapp`
  - Pipeline: `claw-dispatch` (command lanes), `claw-routing` (binding cascade, session keys), `claw-plugins` (hook pipeline)
  - Server/frontdoor: `claw-gateway` (actix-web/ws); entry binary: `claw-app`

## Build and Test

- Prefer LSP for quick diagnostics; formal checks:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets`
  - `cargo test --workspace`
- Runtime prereqs: copy `.env.example` → `.env`, and provide `config/config.json` (JSON5 with `$include` and `${ENV}` support) before running.

## Conventions

- Follow OpenClaw porting rules: serde renames for PascalCase fields, optional fields stay optional, tagged enums for protocol frames.
- Plugin hook order matters; early-exit allowed. Channel integrations implement `ChannelPlugin` + inbound/outbound/command/mention adapters.
- Config is resolved at runtime (JSON5 → `$include` expansion → `${ENV}` substitution); do not hardcode secrets.

## Workflow

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

### Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:

   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```

5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**

- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
