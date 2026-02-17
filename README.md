# claw-rust

Rust-based runtime for the OpenClaw multi-agent platform.

## Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- A `.env` file with required environment variables (see below)

## Getting Started

```bash
# Clone and enter the project
git clone <repo-url>
cd claw-rust

# Copy the example env and fill in your values
cp .env.example .env

# Build
cargo build

# Run
cargo run
```

## Project Structure

```
claw-rust/
  src/
    main.rs          # Application entry point
  config/
    config.json      # Runtime agent/channel config (git-ignored)
  .claude/
    rules/rust.md    # Rust coding conventions
  Cargo.toml         # Package manifest
  .env               # Environment variables (git-ignored)
```

## Configuration

`config/config.json` holds runtime configuration for agents, channels (Telegram), and model providers. It references secrets via `${VAR}` syntax resolved from environment variables at runtime.

Create your own from the template or ask a team member for a working copy.

### Required Environment Variables

| Variable | Purpose |
|---|---|
| `TELEGRAM_BOT_TOKEN` | Main Telegram bot |
| `EINSTEIN_TELEGRAM_BOT_TOKEN` | Einstein agent bot |
| `GAUSS_TELEGRAM_BOT_TOKEN` | Gauss agent bot |
| `VON_BRAUN_TELEGRAM_BOT_TOKEN` | Von Braun agent bot |
| `GEMINI_API_KEY` | Gemini API access |
| `OPENCLAW_GATEWAY_TOKEN` | Gateway auth token |

## Development

This project uses Claude Code with project-level rules in `CLAUDE.md` and `.claude/rules/`.

```bash
# Check for errors
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

## License

TBD
