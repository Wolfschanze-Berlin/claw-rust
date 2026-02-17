# claw-rust Application Workflow

> **Auto-maintained**: Keep this document in sync with code changes.
> Last updated: 2026-02-17

## Crate Dependency Graph

Shows how the 9 workspace crates depend on each other. `claw-core` is the
foundation with zero internal dependencies; `claw-app` is the top-level binary
that wires everything together.

```mermaid
graph TD
    APP["claw-app<br/><i>binary entry point</i>"]
    GW["claw-gateway<br/><i>HTTP + WebSocket server</i>"]
    DISP["claw-dispatch<br/><i>message pipeline</i>"]
    PLUG["claw-plugins<br/><i>plugin lifecycle & hooks</i>"]
    ROUTE["claw-routing<br/><i>agent binding & session keys</i>"]
    CHAN["claw-channels<br/><i>channel traits & registry</i>"]
    DB["claw-db<br/><i>SQLite persistence</i>"]
    CFG["claw-config<br/><i>JSON5 config loader</i>"]
    CORE["claw-core<br/><i>errors, runtime, backoff</i>"]

    APP --> GW
    APP --> DISP
    APP --> PLUG
    APP --> ROUTE
    APP --> CHAN
    APP --> DB
    APP --> CFG
    APP --> CORE

    GW --> ROUTE
    GW --> CHAN
    GW --> CFG
    GW --> CORE

    DISP --> ROUTE
    DISP --> CHAN
    DISP --> CFG
    DISP --> CORE

    PLUG --> GW
    PLUG --> CHAN
    PLUG --> CFG
    PLUG --> CORE

    ROUTE --> CHAN
    ROUTE --> CFG
    ROUTE --> CORE

    CHAN --> CFG
    CHAN --> CORE

    DB --> CHAN
    DB --> ROUTE
    DB --> CORE

    CFG --> CORE

    style CORE fill:#4CAF50,color:#fff
    style CFG fill:#8BC34A,color:#fff
    style CHAN fill:#03A9F4,color:#fff
    style ROUTE fill:#FF9800,color:#fff
    style DB fill:#9C27B0,color:#fff
    style DISP fill:#F44336,color:#fff
    style PLUG fill:#FF5722,color:#fff
    style GW fill:#2196F3,color:#fff
    style APP fill:#607D8B,color:#fff
```

## Application Startup Flow

The `claw-app` binary boots the system in this sequence:

```mermaid
sequenceDiagram
    participant Main as claw-app (main)
    participant Core as claw-core
    participant Cfg as claw-config
    participant Chan as claw-channels
    participant Route as claw-routing
    participant Plug as claw-plugins
    participant DB as claw-db
    participant Disp as claw-dispatch
    participant GW as claw-gateway

    Main->>Core: init_tracing()
    Main->>Cfg: load config (JSON5 + $include + ${ENV})
    Main->>Core: RuntimeEnv::new("claw")
    Main->>DB: open SQLite (WAL mode)
    Main->>Chan: register channel plugins
    Main->>Route: build routing table (8-level priority)
    Main->>Plug: register plugins + hook pipeline
    Main->>Disp: init dispatch pipeline + command queue
    Main->>GW: start actix-web server (HTTP + WS)
    GW-->>Main: listening on port 18789
```

## Inbound Message Flow

When a message arrives from an external channel (e.g. Telegram), it flows
through the system like this:

```mermaid
flowchart LR
    EXT["External Channel<br/>(Telegram, Discord, etc.)"]
    CHAN["claw-channels<br/>ChannelPlugin.receive()"]
    CTX["Build MsgContext<br/>(~60 fields)"]
    HOOKS1["Plugin Hooks<br/>message_received"]
    ROUTE["claw-routing<br/>resolve agent binding<br/>(8-level cascade)"]
    SESS["Session Key<br/>agent:id:channel:scope"]
    DB_R["claw-db<br/>load session state"]
    DISP["claw-dispatch<br/>command lane queue"]
    HOOKS2["Plugin Hooks<br/>before_model_resolve<br/>before_prompt_build<br/>before_agent_start"]
    AGENT["AI Agent<br/>(LLM call)"]
    HOOKS3["Plugin Hooks<br/>llm_output<br/>agent_end"]
    OUT["Outbound Delivery"]
    DB_W["claw-db<br/>persist transcript"]

    EXT --> CHAN --> CTX --> HOOKS1 --> ROUTE
    ROUTE --> SESS --> DB_R --> DISP
    DISP --> HOOKS2 --> AGENT
    AGENT --> HOOKS3 --> OUT
    AGENT --> DB_W

    style EXT fill:#E91E63,color:#fff
    style AGENT fill:#9C27B0,color:#fff
    style OUT fill:#4CAF50,color:#fff
```

## Outbound Message Flow

Responses flow back to the originating channel:

```mermaid
flowchart LR
    AGENT["AI Agent Response"]
    HOOKS["Plugin Hooks<br/>message_sending"]
    DISP["claw-dispatch<br/>outbound delivery"]
    CHAN["claw-channels<br/>ChannelPlugin.send()"]
    HOOKS2["Plugin Hooks<br/>message_sent"]
    EXT["External Channel"]

    AGENT --> HOOKS --> DISP --> CHAN --> HOOKS2 --> EXT

    style AGENT fill:#9C27B0,color:#fff
    style EXT fill:#E91E63,color:#fff
```

## Gateway WebSocket Protocol

The gateway implements a JSON-RPC protocol over WebSocket with three frame
types: `req`, `res`, and `event`.

```mermaid
sequenceDiagram
    participant Client
    participant GW as claw-gateway

    Client->>GW: WS Connect
    GW-->>Client: HTTP 101 Upgrade

    Client->>GW: req { type: "req", method: "connect", params: ConnectParams }
    GW-->>Client: res { type: "res", id, result: HelloOk }

    Note over Client,GW: Session established

    Client->>GW: req { type: "req", method: "send", params: { ... } }
    GW-->>Client: res { type: "res", id, result: { ... } }

    GW-->>Client: event { type: "event", event: "message", data: { ... } }

    Note over Client,GW: Error codes: NOT_LINKED, NOT_PAIRED,<br/>AGENT_TIMEOUT, INVALID_REQUEST, UNAVAILABLE
```

## Plugin Hook Pipeline

Plugins can intercept 20+ lifecycle events. Hooks fire in registration order.

```mermaid
flowchart TD
    subgraph "Message Lifecycle Hooks"
        MR["message_received"]
        BMR["before_model_resolve"]
        BPB["before_prompt_build"]
        BAS["before_agent_start"]
        LI["llm_input"]
        BTC["before_tool_call"]
        ATC["after_tool_call"]
        LO["llm_output"]
        AE["agent_end"]
        MS["message_sending"]
        MSent["message_sent"]
    end

    subgraph "Session Hooks"
        SS["session_start"]
        SE["session_end"]
        BR["before_reset"]
    end

    subgraph "Gateway Hooks"
        GS["gateway_start"]
        GSt["gateway_stop"]
    end

    subgraph "Storage Hooks"
        BC["before_compaction"]
        AC["after_compaction"]
        TRP["tool_result_persist"]
        BMW["before_message_write"]
    end

    MR --> BMR --> BPB --> BAS --> LI
    LI --> BTC --> ATC --> LO --> AE
    AE --> MS --> MSent

    style MR fill:#03A9F4,color:#fff
    style LI fill:#FF9800,color:#fff
    style LO fill:#FF9800,color:#fff
    style AE fill:#4CAF50,color:#fff
```

## Routing: 8-Level Agent Binding Cascade

When resolving which AI agent handles a message, routing checks bindings in
priority order (first match wins):

```mermaid
flowchart TD
    MSG["Incoming Message"]
    P1["1. binding.peer"]
    P2["2. binding.peer.parent"]
    P3["3. binding.guild + roles"]
    P4["4. binding.guild"]
    P5["5. binding.team"]
    P6["6. binding.account"]
    P7["7. binding.channel"]
    P8["8. default agent"]
    AGENT["Resolved Agent"]

    MSG --> P1
    P1 -->|miss| P2
    P2 -->|miss| P3
    P3 -->|miss| P4
    P4 -->|miss| P5
    P5 -->|miss| P6
    P6 -->|miss| P7
    P7 -->|miss| P8
    P1 -->|hit| AGENT
    P2 -->|hit| AGENT
    P3 -->|hit| AGENT
    P4 -->|hit| AGENT
    P5 -->|hit| AGENT
    P6 -->|hit| AGENT
    P7 -->|hit| AGENT
    P8 --> AGENT

    style MSG fill:#E91E63,color:#fff
    style AGENT fill:#4CAF50,color:#fff
    style P1 fill:#F44336,color:#fff
    style P8 fill:#9E9E9E,color:#fff
```

## Session Key Format

Session keys scope conversations to a specific agent-channel-peer combination:

```
agent:<agentId>:<channel>:<scope>
```

```mermaid
flowchart LR
    subgraph "Session Key Components"
        A["agent"] --- ID["agentId"]
        ID --- CH["channel<br/>(telegram, discord, ...)"]
        CH --- SC["scope"]
    end

    subgraph "Scope Types"
        S1["main"]
        S2["per-peer"]
        S3["per-channel-peer"]
        S4["per-account-channel-peer"]
    end

    SC --> S1
    SC --> S2
    SC --> S3
    SC --> S4
```

## Config Loading Pipeline

Configuration is loaded from JSON5 files with support for recursive includes
and environment variable substitution:

```mermaid
flowchart TD
    FILE["config.json5<br/>(~/.openclaw/config.json5)"]
    PARSE["Parse JSON5"]
    INC["Resolve $include directives<br/>(recursive)"]
    ENV["Substitute ${ENV} variables"]
    VALIDATE["Schema validation"]
    CFG["Validated AppConfig"]

    FILE --> PARSE --> INC --> ENV --> VALIDATE --> CFG

    INC -->|"$include: ./agents.json5"| INC

    style FILE fill:#FF9800,color:#fff
    style CFG fill:#4CAF50,color:#fff
```

## Crate Module Map

Detailed module breakdown for each crate:

```mermaid
flowchart TD
    subgraph claw-core
        C1["error.rs<br/>ErrorShape, ErrorCode, token redaction"]
        C2["runtime.rs<br/>RuntimeEnv, init_tracing"]
        C3["backoff.rs<br/>BackoffPolicy, compute_backoff, sleep_with_abort"]
    end

    subgraph claw-config
        CF1["lib.rs<br/>JSON5 parsing, $include, ${ENV} substitution"]
    end

    subgraph claw-channels
        CH1["lib.rs<br/>ChannelPlugin trait, adapters, MsgContext, registry"]
    end

    subgraph claw-routing
        R1["lib.rs<br/>8-level binding cascade, session key gen/parse"]
    end

    subgraph claw-dispatch
        D1["lib.rs<br/>inbound pipeline, outbound delivery, command lane queue"]
    end

    subgraph claw-gateway
        G1["lib.rs<br/>actix-web HTTP, WS handshake, RPC dispatch, events"]
    end

    subgraph claw-plugins
        P1["lib.rs<br/>plugin registration, lifecycle, hook pipeline, PluginApi"]
    end

    subgraph claw-db
        DB1["lib.rs<br/>SQLite WAL, sessions, transcripts, message history"]
    end

    subgraph claw-app
        A1["main.rs<br/>binary entry point, wires all crates"]
    end

    style claw-core fill:#4CAF50,color:#fff
    style claw-config fill:#8BC34A,color:#fff
    style claw-channels fill:#03A9F4,color:#fff
    style claw-routing fill:#FF9800,color:#fff
    style claw-dispatch fill:#F44336,color:#fff
    style claw-gateway fill:#2196F3,color:#fff
    style claw-plugins fill:#FF5722,color:#fff
    style claw-db fill:#9C27B0,color:#fff
    style claw-app fill:#607D8B,color:#fff
```

## Supported Channels

Built-in channel IDs matching the OpenClaw protocol:

```mermaid
flowchart LR
    subgraph "Channel Plugins"
        TG["Telegram<br/>(teloxide)"]
        WA["WhatsApp"]
        DC["Discord"]
        IRC["IRC"]
        GC["Google Chat"]
        SL["Slack"]
        SIG["Signal"]
        IM["iMessage"]
    end

    REG["Channel Registry"]
    TG --> REG
    WA --> REG
    DC --> REG
    IRC --> REG
    GC --> REG
    SL --> REG
    SIG --> REG
    IM --> REG

    style TG fill:#0088cc,color:#fff
    style DC fill:#5865F2,color:#fff
    style SL fill:#4A154B,color:#fff
    style REG fill:#607D8B,color:#fff
```

## Tech Stack Summary

| Layer | Rust Crate/Lib | OpenClaw Equivalent |
|-------|---------------|---------------------|
| Runtime | `tokio` | Node.js event loop |
| HTTP/WS | `actix-web` + `actix-ws` | Express + ws |
| Serialization | `serde` + `serde_json` + `json5` | @sinclair/typebox |
| Database | `rusqlite` (SQLite WAL) | better-sqlite3 |
| Telegram | `teloxide` | grammy |
| Logging | `tracing` + `tracing-subscriber` | pino |
| Errors | `thiserror` + `anyhow` | custom Error classes |
| Cancellation | `tokio_util::CancellationToken` | AbortSignal |
| Config validation | Custom (serde) | Zod |
