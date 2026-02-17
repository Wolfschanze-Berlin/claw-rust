---
name: claw-rust Project Patterns
description: This skill provides guidance on Rust patterns and conventions for the claw-rust OpenClaw port. Use when implementing any claw-rust component.
version: 1.0.0
---

# claw-rust Project Patterns

This document describes the core Rust patterns, conventions, and architectural decisions for the claw-rust project—a native Rust port of OpenClaw, a multi-channel AI chatbot gateway.

## Overview

claw-rust is porting the TypeScript OpenClaw architecture to Rust, focusing on:
- Type safety and memory safety guarantees
- High-performance async runtime using Tokio
- Zero-cost abstractions via trait-based plugin architecture
- Modular workspace organization

## 1. Serde Serialization Patterns

### Field Renaming (PascalCase)

Use `#[serde(rename_all = "PascalCase")]` at the struct level to match the original TypeScript/JSON naming conventions:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChannelConfig {
    pub channel_id: String,      // Serializes as "ChannelId"
    pub provider_name: String,   // Serializes as "ProviderName"
    pub is_enabled: bool,        // Serializes as "IsEnabled"
}
```

### Discriminated Unions with tag

Use `#[serde(tag = "type")]` for enum variants that represent different message types or command variants. This embeds the discriminator directly in the JSON object:

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum CommandMessage {
    #[serde(rename = "SendMessage")]
    SendMessage {
        channel_id: String,
        content: String,
    },
    #[serde(rename = "UpdateStatus")]
    UpdateStatus {
        status: String,
    },
}

// Serializes to: {"type": "SendMessage", "channelId": "...", "content": "..."}
```

### Conditional Serialization with skip_serializing_if

Use `#[serde(skip_serializing_if = "Option::is_none")]` to omit optional fields from JSON when they are None:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Message {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
}
```

## 2. Async Patterns

### async_trait for Trait Methods

Use `#[async_trait]` from the `async_trait` crate for traits with async methods. This macro generates the necessary future types:

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ChannelPlugin: Send + Sync {
    async fn initialize(&mut self) -> anyhow::Result<()>;
    async fn send_message(&self, msg: Message) -> anyhow::Result<String>;
    async fn shutdown(&self) -> anyhow::Result<()>;
}

// Implementation
pub struct SlackPlugin;

#[async_trait]
impl ChannelPlugin for SlackPlugin {
    async fn initialize(&mut self) -> anyhow::Result<()> {
        // Initialization logic
        Ok(())
    }

    async fn send_message(&self, msg: Message) -> anyhow::Result<String> {
        // Send via Slack API
        Ok("sent".to_string())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
```

### Tokio Runtime

Use `tokio::runtime::Runtime` for the main runtime context. For most applications, use the `#[tokio::main]` macro:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Application initialization
    let gateway = Gateway::new(config)?;
    gateway.run().await?;
    Ok(())
}
```

For custom runtime configuration:

```rust
let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(8)
    .thread_name("claw-worker")
    .enable_all()
    .build()?;

runtime.block_on(async { /* async code */ })
```

### CancellationToken for Abort Signals

Use `tokio_util::sync::CancellationToken` for graceful shutdown coordination across async tasks:

```rust
use tokio_util::sync::CancellationToken;

async fn run_with_cancellation() -> anyhow::Result<()> {
    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    // Spawn worker task
    let worker_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    println!("Shutdown signal received");
                    break;
                }
                // Main work loop
                _ = do_work() => {}
            }
        }
    });

    // Wait for signal or trigger cancellation
    cancel_token.cancel();
    worker_handle.await?;
    Ok(())
}
```

## 3. Error Handling

### thiserror for Error Derives

Use `thiserror::Error` for domain-specific errors with automatic Display impl:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Plugin initialization failed: {0}")]
    PluginError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

### anyhow for Application Errors

Use `anyhow::Result<T>` and `anyhow::Error` for high-level application code and error propagation:

```rust
use anyhow::{anyhow, Result, Context};

pub async fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .context("Failed to read config file")?;

    let config: Config = serde_json::from_str(&content)
        .context("Failed to parse config JSON")?;

    Ok(config)
}

// Error conversion with context
pub fn validate_channel(channel: &Channel) -> Result<()> {
    if channel.id.is_empty() {
        return Err(anyhow!("Channel ID cannot be empty"));
    }
    Ok(())
}
```

### ErrorShape for Protocol Compliance

Define an `ErrorShape` struct that matches the protocol specification for consistent error responses:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct ErrorShape {
    pub error_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorShape {
    pub fn from_error(error: anyhow::Error, request_id: Option<String>) -> Self {
        Self {
            error_type: "ApplicationError".to_string(),
            message: error.to_string(),
            details: None,
            request_id,
        }
    }
}
```

## 4. Configuration Loading Patterns

### JSON5 Parsing

Use `json5` crate for flexible JSON5 configuration files that support comments and trailing commas:

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct GatewayConfig {
    pub port: u16,
    pub channels: Vec<ChannelConfig>,
    // ... other fields
}

pub fn load_config_json5(path: &str) -> anyhow::Result<GatewayConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: GatewayConfig = json5::from_str(&content)?;
    Ok(config)
}
```

### $include Directives

Implement config inclusion support with a custom deserializer or post-processing step:

```rust
pub fn resolve_includes(content: &str, base_path: &str) -> anyhow::Result<String> {
    // Find all $include directives and resolve them
    let re = regex::Regex::new(r#"\$include\s*:\s*"([^"]+)""#)?;

    let mut result = content.to_string();
    for cap in re.captures_iter(content) {
        let include_path = &cap[1];
        let full_path = std::path::Path::new(base_path)
            .parent()
            .unwrap()
            .join(include_path);

        let included_content = std::fs::read_to_string(full_path)?;
        result = result.replace(&cap[0], &included_content);
    }

    Ok(result)
}
```

### ${ENV_VAR} Substitution

Implement environment variable substitution in configuration:

```rust
pub fn substitute_env_vars(content: &str) -> anyhow::Result<String> {
    let re = regex::Regex::new(r#"\$\{([A-Za-z_][A-Za-z0-9_]*)\}"#)?;

    let result = re.replace_all(content, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_else(|_| {
            format!("${{{}}}", var_name)  // Leave unresolved if not found
        })
    });

    Ok(result.to_string())
}
```

## 5. Trait Design Patterns

### ChannelPlugin with Optional Adapter Sub-traits

Define a base `ChannelPlugin` trait and optional sub-traits for capabilities:

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ChannelPlugin: Send + Sync {
    async fn initialize(&mut self) -> anyhow::Result<()>;
    async fn send_message(&self, msg: Message) -> anyhow::Result<String>;
    async fn shutdown(&self) -> anyhow::Result<()>;
}

// Optional adapter traits for specific capabilities
#[async_trait]
pub trait MessageReceiver: Send + Sync {
    async fn receive_message(&self) -> anyhow::Result<Option<Message>>;
}

#[async_trait]
pub trait UserProfileAdapter: Send + Sync {
    async fn get_user_profile(&self, user_id: &str) -> anyhow::Result<UserProfile>;
    async fn update_user_profile(&self, user_id: &str, profile: UserProfile) -> anyhow::Result<()>;
}

#[async_trait]
pub trait MessageHistoryAdapter: Send + Sync {
    async fn save_message(&self, msg: Message) -> anyhow::Result<()>;
    async fn get_history(&self, user_id: &str, limit: usize) -> anyhow::Result<Vec<Message>>;
}

// Store as Option<Box<dyn Trait>>
pub struct ChannelContext {
    pub plugin: Box<dyn ChannelPlugin>,
    pub message_receiver: Option<Box<dyn MessageReceiver>>,
    pub user_profile: Option<Box<dyn UserProfileAdapter>>,
    pub message_history: Option<Box<dyn MessageHistoryAdapter>>,
    // ... ~20 optional adapters
}

impl ChannelContext {
    pub async fn send_with_history(&self, msg: Message) -> anyhow::Result<String> {
        let result = self.plugin.send_message(msg.clone()).await?;

        // Use history adapter if available
        if let Some(history) = &self.message_history {
            history.save_message(msg).await?;
        }

        Ok(result)
    }
}
```

## 6. Concurrency Patterns

### Tokio Channels (mpsc) for Message Passing

Use `tokio::sync::mpsc` for async message passing between tasks:

```rust
use tokio::sync::mpsc;

pub async fn create_command_queue(capacity: usize) -> (mpsc::Sender<Command>, mpsc::Receiver<Command>) {
    let (tx, rx) = mpsc::channel(capacity);
    (tx, rx)
}

pub async fn process_commands(mut rx: mpsc::Receiver<Command>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::SendMessage(msg) => { /* handle */ },
            Command::Shutdown => break,
        }
    }
}
```

### Semaphores for Lane-based Command Queue

Use `tokio::sync::Semaphore` to limit concurrent operations per "lane" (channel, user, or priority level):

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct CommandQueue {
    // Map of lane IDs to semaphores limiting concurrent operations
    lanes: Arc<std::sync::RwLock<std::collections::HashMap<String, Arc<Semaphore>>>>,
    max_concurrent_per_lane: usize,
}

impl CommandQueue {
    pub fn new(max_concurrent_per_lane: usize) -> Self {
        Self {
            lanes: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            max_concurrent_per_lane,
        }
    }

    pub async fn execute_in_lane<F, T>(&self, lane_id: &str, future: F) -> anyhow::Result<T>
    where
        F: std::future::Future<Output = anyhow::Result<T>>,
    {
        // Get or create semaphore for this lane
        let semaphore = {
            let mut lanes = self.lanes.write().unwrap();
            lanes.entry(lane_id.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(self.max_concurrent_per_lane)))
                .clone()
        };

        // Acquire permit and execute
        let _permit = semaphore.acquire().await?;
        future.await
    }
}
```

## 7. Registry Pattern (Thread-safe)

### Arc + RwLock<HashMap> for Thread-safe Registries

Use `Arc<RwLock<HashMap>>` for thread-safe, cloneable registries:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Box<dyn ChannelPlugin>>>>,
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        Self {
            plugins: Arc::clone(&self.plugins),
        }
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, name: String, plugin: Box<dyn ChannelPlugin>) -> anyhow::Result<()> {
        let mut plugins = self.plugins.write().await;
        plugins.insert(name, plugin);
        Ok(())
    }

    pub async fn get(&self, name: &str) -> anyhow::Result<Option<Arc<Box<dyn ChannelPlugin>>>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.get(name).map(|p| Arc::new(p.clone())))
    }

    pub async fn list(&self) -> anyhow::Result<Vec<String>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.keys().cloned().collect())
    }
}
```

For immutable reads with many readers, prefer `RwLock` over `Mutex`. For write-heavy workloads, consider `DashMap` from `dashmap` crate for lock-free operations.

## 8. Module Organization and Workspace Plan

### Planned Workspace Structure

The claw-rust project uses a Cargo workspace with modular crates:

```
claw-rust/
├── Cargo.toml                    # Workspace root
├── claw-config/                  # Configuration parsing and loading
│   ├── src/
│   │   ├── lib.rs
│   │   ├── parser.rs            # JSON5, $include, ${ENV_VAR}
│   │   └── schema.rs            # Config data structures
│   └── Cargo.toml
├── claw-core/                    # Core types and traits
│   ├── src/
│   │   ├── lib.rs
│   │   ├── plugin.rs            # ChannelPlugin trait definitions
│   │   ├── adapters.rs          # Optional adapter traits
│   │   ├── message.rs           # Message types
│   │   └── errors.rs            # Error types
│   └── Cargo.toml
├── claw-gateway/                 # Main gateway orchestration
│   ├── src/
│   │   ├── lib.rs
│   │   ├── gateway.rs           # Gateway initialization and management
│   │   ├── router.rs            # Message routing logic
│   │   └── registry.rs          # Plugin/adapter registries
│   └── Cargo.toml
├── claw-plugins/                 # Built-in channel implementations
│   ├── src/
│   │   ├── lib.rs
│   │   ├── slack.rs
│   │   ├── discord.rs
│   │   ├── telegram.rs
│   │   └── http.rs
│   └── Cargo.toml
└── claw-cli/                     # CLI tool for management
    ├── src/
    │   ├── main.rs
    │   └── commands/
    └── Cargo.toml
```

### Internal Crate Dependencies

- `claw-gateway` depends on `claw-core` and `claw-config`
- `claw-plugins` depends on `claw-core`
- `claw-cli` depends on `claw-gateway`, `claw-config`, `claw-core`

## Best Practices

### 1. Prefer Owned Data in Public APIs

Use `String` and `Vec<T>` rather than `&str` and `&[T]` in trait methods and struct fields for flexibility:

```rust
// Good
pub async fn send_message(&self, content: String) -> anyhow::Result<()>;

// Less flexible
pub async fn send_message(&self, content: &str) -> anyhow::Result<()>;
```

### 2. Use Builder Pattern for Complex Configs

For structs with many optional fields, implement a builder:

```rust
pub struct GatewayBuilder {
    port: Option<u16>,
    max_connections: Option<usize>,
    // ...
}

impl GatewayBuilder {
    pub fn new() -> Self { /* ... */ }
    pub fn port(mut self, port: u16) -> Self { self.port = Some(port); self }
    pub fn build(self) -> anyhow::Result<Gateway> { /* ... */ }
}
```

### 3. Use Type-level Builder for Plugin Options

Leverage Rust's type system to make invalid states unrepresentable:

```rust
pub struct PluginOptions<T: PluginState> {
    config: String,
    _state: std::marker::PhantomData<T>,
}

// Type states
pub struct Uninitialized;
pub struct Initialized;

impl PluginOptions<Uninitialized> {
    pub fn initialize(self) -> PluginOptions<Initialized> { /* ... */ }
}
```

### 4. Minimize Allocations in Hot Paths

Use `&[u8]` for message bodies and buffer reuse where possible. Use `SmallVec` from `smallvec` crate for small collections:

```rust
use smallvec::SmallVec;

// Preallocates 4 items on the stack; heap allocation if needed
pub fn collect_handlers() -> SmallVec<[Box<dyn Handler>; 4]> {
    SmallVec::new()
}
```

## Testing Patterns

### Mock Traits and Adapters

Use conditional compilation and trait objects for testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin;

    #[async_trait]
    impl ChannelPlugin for MockPlugin {
        async fn initialize(&mut self) -> anyhow::Result<()> { Ok(()) }
        async fn send_message(&self, _msg: Message) -> anyhow::Result<String> {
            Ok("mocked".to_string())
        }
        async fn shutdown(&self) -> anyhow::Result<()> { Ok(()) }
    }

    #[tokio::test]
    async fn test_gateway_initialization() -> anyhow::Result<()> {
        let plugin = Box::new(MockPlugin);
        // Test code
        Ok(())
    }
}
```

## References

- Tokio documentation: https://tokio.rs/
- async_trait: https://docs.rs/async-trait/
- thiserror: https://docs.rs/thiserror/
- anyhow: https://docs.rs/anyhow/
- serde: https://serde.rs/
- json5: https://docs.rs/json5/
