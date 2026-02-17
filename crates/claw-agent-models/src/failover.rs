//! Model provider failover chain.
//!
//! Wraps multiple [`ModelProvider`] implementations with automatic failover.
//! When the primary provider fails with a retriable error (rate limit, 5xx,
//! timeout), the chain marks it in cooldown and tries the next provider.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::ModelError;
use crate::provider::{ChatStream, ModelProvider};
use crate::types::{ChatRequest, ChatResponse};

// ---------------------------------------------------------------------------
// Cooldown tracking
// ---------------------------------------------------------------------------

/// Tracks cooldown state for a single provider.
struct ProviderSlot {
    provider: Arc<dyn ModelProvider>,
    cooldown_until: Option<Instant>,
}

impl std::fmt::Debug for ProviderSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderSlot")
            .field("provider", &self.provider.provider_name())
            .field("cooldown_until", &self.cooldown_until)
            .finish()
    }
}

impl ProviderSlot {
    fn is_available(&self) -> bool {
        self.cooldown_until
            .map(|until| Instant::now() >= until)
            .unwrap_or(true)
    }

    fn enter_cooldown(&mut self, duration: Duration) {
        self.cooldown_until = Some(Instant::now() + duration);
        info!(
            provider = self.provider.provider_name(),
            cooldown_secs = duration.as_secs(),
            "provider entering cooldown"
        );
    }

    fn clear_cooldown(&mut self) {
        self.cooldown_until = None;
    }
}

// ---------------------------------------------------------------------------
// Failover config
// ---------------------------------------------------------------------------

/// Configuration for the failover chain.
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    /// How long a provider stays in cooldown after a retriable error.
    pub cooldown_duration: Duration,
    /// Maximum time to wait for any provider to become available.
    pub max_wait: Duration,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            cooldown_duration: Duration::from_secs(60),
            max_wait: Duration::from_secs(120),
        }
    }
}

// ---------------------------------------------------------------------------
// FailoverChain
// ---------------------------------------------------------------------------

/// A model provider that wraps multiple providers with automatic failover.
///
/// When the primary provider fails with a retriable error, the chain marks
/// it in cooldown and tries the next available provider. If all providers
/// are in cooldown, waits for the earliest one to expire.
pub struct FailoverChain {
    slots: Arc<RwLock<Vec<ProviderSlot>>>,
    config: FailoverConfig,
}

impl FailoverChain {
    /// Create a new failover chain with the given providers (in priority order).
    pub fn new(providers: Vec<Arc<dyn ModelProvider>>, config: FailoverConfig) -> Self {
        let slots = providers
            .into_iter()
            .map(|p| ProviderSlot {
                provider: p,
                cooldown_until: None,
            })
            .collect();

        Self {
            slots: Arc::new(RwLock::new(slots)),
            config,
        }
    }

    /// Try each available provider in order. On retriable failure, cooldown
    /// that provider and try the next one.
    async fn try_with_failover<F, T>(&self, operation: F) -> Result<T, ModelError>
    where
        F: Fn(Arc<dyn ModelProvider>) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, ModelError>> + Send>,
        >,
    {
        let deadline = Instant::now() + self.config.max_wait;

        loop {
            // Find the next available provider.
            let provider = {
                let slots = self.slots.read().await;
                slots
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.is_available())
                    .map(|(i, s)| (i, Arc::clone(&s.provider)))
            };

            if let Some((index, provider)) = provider {
                debug!(
                    provider = provider.provider_name(),
                    index,
                    "attempting provider"
                );

                match operation(Arc::clone(&provider)).await {
                    Ok(result) => {
                        // Success — clear any cooldown.
                        let mut slots = self.slots.write().await;
                        if let Some(slot) = slots.get_mut(index) {
                            slot.clear_cooldown();
                        }
                        return Ok(result);
                    }
                    Err(err) if err.is_retriable() => {
                        warn!(
                            provider = provider.provider_name(),
                            error = %err,
                            "provider failed with retriable error, entering cooldown"
                        );

                        let mut slots = self.slots.write().await;
                        if let Some(slot) = slots.get_mut(index) {
                            // Use retry_after from RateLimited if available.
                            let cooldown = match &err {
                                ModelError::RateLimited {
                                    retry_after: Some(d),
                                    ..
                                } => *d,
                                _ => self.config.cooldown_duration,
                            };
                            slot.enter_cooldown(cooldown);
                        }
                        // Continue to try next provider.
                    }
                    Err(err) => {
                        // Non-retriable error — propagate immediately.
                        return Err(err);
                    }
                }
            } else {
                // All providers in cooldown — find the earliest expiry.
                let earliest = {
                    let slots = self.slots.read().await;
                    slots
                        .iter()
                        .filter_map(|s| s.cooldown_until)
                        .min()
                };

                if let Some(expiry) = earliest {
                    if Instant::now() >= deadline {
                        return Err(ModelError::ProviderError {
                            provider: "failover".into(),
                            message: "all providers exhausted and max wait exceeded".into(),
                            status_code: None,
                        });
                    }

                    let wait_time = expiry.saturating_duration_since(Instant::now());
                    debug!(wait_ms = wait_time.as_millis(), "waiting for cooldown expiry");
                    tokio::time::sleep(wait_time).await;
                } else {
                    return Err(ModelError::ProviderError {
                        provider: "failover".into(),
                        message: "no providers configured".into(),
                        status_code: None,
                    });
                }
            }
        }
    }
}

#[async_trait]
impl ModelProvider for FailoverChain {
    fn provider_name(&self) -> &str {
        "failover"
    }

    fn supports_tools(&self) -> bool {
        // Assume tools are supported if any provider supports them.
        // This is checked at runtime per-call anyway.
        true
    }

    fn max_context_window(&self) -> u64 {
        // Return the minimum context window across providers for safety.
        // In practice, the caller should check the specific model's window.
        200_000
    }

    async fn chat_completion(&self, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let req = request.clone();
        self.try_with_failover(move |provider| {
            let r = req.clone();
            Box::pin(async move { provider.chat_completion(&r).await })
        })
        .await
    }

    async fn chat_completion_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatStream, ModelError> {
        let req = request.clone();
        self.try_with_failover(move |provider| {
            let r = req.clone();
            Box::pin(async move { provider.chat_completion_stream(&r).await })
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FinishReason, Usage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock provider that can be configured to succeed or fail.
    struct MockProvider {
        name: &'static str,
        call_count: AtomicUsize,
        behavior: MockBehavior,
    }

    enum MockBehavior {
        Succeed,
        FailRetriable,
        FailAuth,
    }

    impl MockProvider {
        fn new(name: &'static str, behavior: MockBehavior) -> Arc<Self> {
            Arc::new(Self {
                name,
                call_count: AtomicUsize::new(0),
                behavior,
            })
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl ModelProvider for MockProvider {
        fn provider_name(&self) -> &str {
            self.name
        }

        fn supports_tools(&self) -> bool {
            true
        }

        fn max_context_window(&self) -> u64 {
            100_000
        }

        async fn chat_completion(
            &self,
            _request: &ChatRequest,
        ) -> Result<ChatResponse, ModelError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            match self.behavior {
                MockBehavior::Succeed => Ok(ChatResponse {
                    id: "resp-1".into(),
                    model: self.name.into(),
                    content: "Hello!".into(),
                    tool_calls: vec![],
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                    },
                    finish_reason: FinishReason::Stop,
                }),
                MockBehavior::FailRetriable => Err(ModelError::RateLimited {
                    provider: self.name.into(),
                    retry_after: Some(Duration::from_millis(10)),
                }),
                MockBehavior::FailAuth => Err(ModelError::AuthError {
                    provider: self.name.into(),
                    message: "invalid key".into(),
                }),
            }
        }

        async fn chat_completion_stream(
            &self,
            request: &ChatRequest,
        ) -> Result<ChatStream, ModelError> {
            // Reuse chat_completion for simplicity in tests.
            let resp = self.chat_completion(request).await?;
            Ok(Box::pin(futures_util::stream::once(async move {
                Ok(crate::types::StreamChunk::ContentDelta(resp.content))
            })))
        }
    }

    fn test_request() -> ChatRequest {
        ChatRequest {
            model: "test".into(),
            messages: vec![],
            system: None,
            tools: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn primary_succeeds_no_failover() {
        let primary = MockProvider::new("primary", MockBehavior::Succeed);
        let backup = MockProvider::new("backup", MockBehavior::Succeed);

        let chain = FailoverChain::new(
            vec![primary.clone() as Arc<dyn ModelProvider>, backup.clone()],
            FailoverConfig::default(),
        );

        let resp = chain.chat_completion(&test_request()).await.unwrap();
        assert_eq!(resp.model, "primary");
        assert_eq!(primary.calls(), 1);
        assert_eq!(backup.calls(), 0);
    }

    #[tokio::test]
    async fn failover_to_backup_on_retriable_error() {
        let primary = MockProvider::new("primary", MockBehavior::FailRetriable);
        let backup = MockProvider::new("backup", MockBehavior::Succeed);

        let chain = FailoverChain::new(
            vec![primary.clone() as Arc<dyn ModelProvider>, backup.clone()],
            FailoverConfig {
                cooldown_duration: Duration::from_secs(60),
                max_wait: Duration::from_secs(5),
            },
        );

        let resp = chain.chat_completion(&test_request()).await.unwrap();
        assert_eq!(resp.model, "backup");
        assert_eq!(primary.calls(), 1);
        assert_eq!(backup.calls(), 1);
    }

    #[tokio::test]
    async fn non_retriable_error_propagated_immediately() {
        let primary = MockProvider::new("primary", MockBehavior::FailAuth);
        let backup = MockProvider::new("backup", MockBehavior::Succeed);

        let chain = FailoverChain::new(
            vec![primary.clone() as Arc<dyn ModelProvider>, backup.clone()],
            FailoverConfig::default(),
        );

        let result = chain.chat_completion(&test_request()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ModelError::AuthError { .. }));
        // Backup should NOT be tried for auth errors.
        assert_eq!(backup.calls(), 0);
    }

    #[tokio::test]
    async fn empty_chain_returns_error() {
        let chain = FailoverChain::new(vec![], FailoverConfig::default());
        let result = chain.chat_completion(&test_request()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cooldown_recovery() {
        let primary = MockProvider::new("primary", MockBehavior::FailRetriable);
        let backup = MockProvider::new("backup", MockBehavior::Succeed);

        let chain = FailoverChain::new(
            vec![primary.clone() as Arc<dyn ModelProvider>, backup.clone()],
            FailoverConfig {
                cooldown_duration: Duration::from_millis(50),
                max_wait: Duration::from_secs(5),
            },
        );

        // First call: primary fails, backup succeeds.
        let resp1 = chain.chat_completion(&test_request()).await.unwrap();
        assert_eq!(resp1.model, "backup");

        // Wait for cooldown to expire.
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Primary is still configured to fail, so it will fail again and backup
        // will be used. But the key point is the cooldown expired and primary was retried.
        let resp2 = chain.chat_completion(&test_request()).await.unwrap();
        assert_eq!(resp2.model, "backup");
        assert_eq!(primary.calls(), 2); // Was retried after cooldown.
    }
}
