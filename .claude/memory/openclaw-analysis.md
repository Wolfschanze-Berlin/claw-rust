# OpenClaw Architecture Analysis

Deep analysis of github.com/openclaw/openclaw (TypeScript, Node.js 22, pnpm monorepo).

## Core Types to Port

### ChannelPlugin (master interface, ~20 adapters)
- `id`, `meta`, `capabilities` (required)
- Optional adapters: config, gateway, outbound, security, groups, mentions, status, auth, elevated, commands, streaming, threading, messaging, agentPrompt, directory, resolver, actions, heartbeat, onboarding, pairing, setup

### MsgContext (~60+ optional fields)
- PascalCase JSON field names (Body, From, To, SessionKey, etc.)
- Central inbound message envelope

### Gateway Protocol (JSON-over-WebSocket RPC)
- 3 frame types: `req`, `res`, `event` (discriminated by `type` field)
- Handshake: `ConnectParams` -> `HelloOk`
- Default port: 18789
- Error codes: NOT_LINKED, NOT_PAIRED, AGENT_TIMEOUT, INVALID_REQUEST, UNAVAILABLE

### Session Keys
- Format: `agent:<agentId>:<channel>:<scope>`
- DM scopes: main, per-peer, per-channel-peer, per-account-channel-peer

### Config System
- JSON5 with `$include` (recursive) + `${ENV}` substitution
- Validated via Zod schemas
- Default path: `~/.openclaw/config.json5`

### Routing (8-level priority)
1. binding.peer → 2. binding.peer.parent → 3. binding.guild+roles → 4. binding.guild → 5. binding.team → 6. binding.account → 7. binding.channel → 8. default

### Hook Pipeline (20+ events)
before_model_resolve, before_prompt_build, before_agent_start, llm_input, before_tool_call, after_tool_call, llm_output, agent_end, message_received, message_sending, message_sent, session_start, session_end, gateway_start, gateway_stop, before_compaction, after_compaction, before_reset, tool_result_persist, before_message_write

### Channel IDs (built-in)
telegram, whatsapp, discord, irc, googlechat, slack, signal, imessage

### Key Dependencies (TS -> Rust mapping)
- grammy -> teloxide (Telegram)
- ws -> actix-ws (WebSocket)
- express -> actix-web (HTTP)
- @sinclair/typebox -> serde + serde_json (schemas)
- zod -> custom validation (config)
- croner -> tokio-cron-scheduler (cron)
