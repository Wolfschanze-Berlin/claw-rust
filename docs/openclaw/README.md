# OpenClaw Reference Documentation

Deep analysis of the [OpenClaw](https://github.com/openclaw/openclaw) personal AI assistant platform. These docs serve as the definitive reference for the **claw-rust** port — covering every subsystem, data flow, and integration point without any code.

## Documentation Map

```mermaid
graph LR
    ROOT["OpenClaw Docs"] --> OV["overview/"]
    ROOT --> GW["gateway/"]
    ROOT --> AG["agent-runtime/"]
    ROOT --> CH["channels/"]
    ROOT --> SP["skills-and-plugins/"]
    ROOT --> TA["tools-and-api/"]
    ROOT --> RC["routing-config-memory/"]
    ROOT --> AR["auto-reply-and-security/"]

    OV -.- OVD["Architecture Overview<br/>High-level system design"]
    GW -.- GWD["Gateway Server<br/>WS control plane, RPC, auth"]
    AG -.- AGD["Agent/Pi Runtime<br/>AI execution engine"]
    CH -.- CHD["Channel System<br/>Platform adapters"]
    SP -.- SPD["Skills & Plugins<br/>Extensibility layer"]
    TA -.- TAD["Tools & API<br/>Agent capabilities, JSON-RPC"]
    RC -.- RCD["Routing, Config, Memory<br/>Session keys, schemas, embeddings"]
    AR -.- ARD["Auto-Reply & Security<br/>Message pipeline, access control"]

    style ROOT fill:#1a1a2e,color:#fff
    style OV fill:#16213e,color:#fff
    style GW fill:#0f3460,color:#fff
    style AG fill:#533483,color:#fff
    style CH fill:#e94560,color:#fff
    style SP fill:#0f3460,color:#fff
    style TA fill:#16213e,color:#fff
    style RC fill:#533483,color:#fff
    style AR fill:#e94560,color:#fff
```

## Subsystem Index

| # | Directory | Document | Description |
|---|-----------|----------|-------------|
| 0 | [`overview/`](overview/) | Architecture Overview | High-level system design, subsystem map, data flow diagrams |
| 1 | [`gateway/`](gateway/) | Gateway Server | WebSocket control plane, JSON-RPC protocol, authorization, config reload |
| 2 | [`agent-runtime/`](agent-runtime/) | Agent/Pi Runtime | AI model execution, tool orchestration, session management, compaction |
| 3 | [`channels/`](channels/) | Channel System | Platform adapters (WhatsApp, Telegram, Discord, Slack, Signal, etc.) |
| 4 | [`skills-and-plugins/`](skills-and-plugins/) | Skills & Plugins | Skill injection, plugin API, hooks (20 lifecycle events), providers |
| 5 | [`tools-and-api/`](tools-and-api/) | Tools & API | Agent tools, tool policies, sandbox, Gateway JSON-RPC API |
| 6 | [`routing-config-memory/`](routing-config-memory/) | Routing, Config, Memory | Session keys, config schemas, hot-reload, vector embeddings |
| 7 | [`auto-reply-and-security/`](auto-reply-and-security/) | Auto-Reply & Security | Message processing pipeline, DM pairing, exec approvals |

## End-to-End Message Flow

```mermaid
sequenceDiagram
    participant User
    participant Channel as Channel Adapter
    participant Router as Routing Engine
    participant Pipeline as Auto-Reply Pipeline
    participant Agent as Pi Agent Runtime
    participant Tools as Tool System
    participant Memory as Memory Store

    User->>Channel: Send message
    Channel->>Channel: Normalize to ChatEnvelope
    Channel->>Router: Route message
    Router->>Router: Resolve session key
    Router->>Pipeline: Dispatch to auto-reply
    Pipeline->>Pipeline: Extract directives
    Pipeline->>Pipeline: Check commands
    Pipeline->>Agent: Execute agent turn
    Agent->>Agent: Build system prompt
    Agent->>Agent: Load history
    Agent->>Agent: Call LLM API
    Agent->>Tools: Execute tool calls
    Tools->>Memory: Search memory (if needed)
    Tools-->>Agent: Tool results
    Agent-->>Pipeline: Response chunks
    Pipeline->>Pipeline: Format & chunk reply
    Pipeline->>Channel: Deliver reply
    Channel->>User: Platform-native message
```

## OpenClaw Source Layout (TypeScript)

| Source Directory | Subsystem | claw-rust Crate |
|------------------|-----------|-----------------|
| `src/gateway/` | Gateway control plane | `claw-gateway` |
| `src/agents/` | Agent/Pi runtime | `claw-core` |
| `src/channels/`, `src/discord/`, `src/telegram/`, etc. | Channel adapters | `claw-channels` |
| `src/auto-reply/` | Message pipeline | `claw-dispatch` |
| `src/plugins/`, `src/plugin-sdk/` | Plugin system | `claw-plugins` |
| `src/routing/` | Session routing | `claw-routing` |
| `src/config/` | Configuration | `claw-config` |
| `src/memory/` | Semantic search | `claw-db` |
| `src/browser/` | Browser control | (future crate) |
| `src/cron/` | Scheduled tasks | (future crate) |
| `src/cli/` | CLI interface | `claw-app` |
| `src/security/` | Security & audit | `claw-core` |
