//! Model provider trait — the contract every LLM backend implements.

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;

use crate::error::ModelError;
use crate::types::{ChatRequest, ChatResponse, StreamChunk};

/// A boxed stream of [`StreamChunk`]s returned by streaming calls.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send>>;

/// Trait implemented by every LLM provider (Anthropic, OpenAI, Ollama, etc.).
///
/// The agent runtime calls these methods without knowing which concrete
/// provider is behind the trait object.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Human-readable provider name (e.g. "anthropic", "openai").
    fn provider_name(&self) -> &str;

    /// Whether the provider supports tool/function calling.
    fn supports_tools(&self) -> bool;

    /// Maximum context window size in tokens.
    fn max_context_window(&self) -> u64;

    /// Send a chat completion request and receive the full response.
    async fn chat_completion(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatResponse, ModelError>;

    /// Send a chat completion request and receive a streaming response.
    async fn chat_completion_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatStream, ModelError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify the trait is object-safe (can be used as `dyn ModelProvider`).
    fn _assert_object_safe(_: &dyn ModelProvider) {}

    #[test]
    fn chat_stream_is_send() {
        fn assert_send<T: Send>() {}
        // ChatStream must be Send (not Sync — streams are inherently !Sync).
        assert_send::<ChatStream>();
    }
}
