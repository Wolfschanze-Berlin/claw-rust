# Rust Context

Rules for Rust development in this project.

## Tooling

- Use the LSP tool for diagnostics instead of running `cargo check` via shell. LSP provides faster, integrated feedback without spawning a separate process.

## OpenClaw Porting Conventions

- When porting TS types: use `#[serde(rename = "FieldName")]` for exact JSON field names
- Use `async_trait` for async trait methods
- Use `tokio_util::sync::CancellationToken` for TS `AbortSignal` equivalents
- Use `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]` for all optional fields
- TS interfaces with optional methods map to Rust structs with `Option<Box<dyn Adapter>>`
- TS discriminated unions (`type: "req" | "res"`) map to `#[serde(tag = "type")]` enums
