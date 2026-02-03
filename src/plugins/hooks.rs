use crate::plugins::traits::{HookModification, HookType, PluginHook, ToolContext};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

/// Hook metadata for priority and ordering
#[derive(Clone)]
struct HookEntry {
    name: String,
    priority: i32,
    hook: PluginHook,
}

/// Manages all registered hooks and their execution
pub struct HookRunner {
    hooks: Arc<RwLock<HashMap<HookType, Vec<HookEntry>>>>,
}

impl HookRunner {
    /// Create a new hook runner
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a hook
    pub async fn register_hook(
        &self,
        hook_type: HookType,
        name: String,
        priority: i32,
        hook: PluginHook,
    ) -> Result<()> {
        let mut hooks = self.hooks.write().await;
        let entries = hooks.entry(hook_type).or_insert_with(Vec::new);

        entries.push(HookEntry {
            name,
            priority,
            hook,
        });

        // Sort by priority (higher first)
        entries.sort_by_key(|h| std::cmp::Reverse(h.priority));

        debug!(
            "Registered hook: {} for {:?}",
            entries.last().unwrap().name,
            hook_type
        );

        Ok(())
    }

    /// Execute all hooks of a type (void hooks - parallel execution)
    #[instrument(skip(self, ctx), fields(hook_type = ?hook_type))]
    pub async fn run_void_hooks(&self, hook_type: HookType, ctx: ToolContext) -> Result<()> {
        let hooks = self.hooks.read().await;

        if let Some(entries) = hooks.get(&hook_type) {
            let futures: Vec<_> = entries
                .iter()
                .map(|entry| {
                    let hook = entry.hook.clone();
                    let ctx = ctx.clone();
                    async move {
                        if let Err(e) = (hook)(hook_type, ctx).await {
                            tracing::warn!("Hook error: {}", e);
                        }
                    }
                })
                .collect();

            futures::future::join_all(futures).await;
        }

        Ok(())
    }

    /// Execute hooks that can modify behavior (sequential)
    #[instrument(skip(self, ctx), fields(hook_type = ?hook_type))]
    pub async fn run_modifying_hooks(
        &self,
        hook_type: HookType,
        ctx: ToolContext,
    ) -> Result<Option<HookModification>> {
        let hooks = self.hooks.read().await;

        if let Some(entries) = hooks.get(&hook_type) {
            let mut combined_modification = HookModification {
                system_prompt_override: None,
                prepend_context: None,
                block_tool: None,
                block_reason: None,
                modified_parameters: None,
            };

            for entry in entries {
                if let Ok(Some(modification)) = (entry.hook)(hook_type, ctx.clone()).await {
                    // Apply modifications (first one wins for overrides)
                    if modification.system_prompt_override.is_some() {
                        combined_modification.system_prompt_override =
                            modification.system_prompt_override;
                    }
                    if modification.prepend_context.is_some() {
                        combined_modification.prepend_context = modification.prepend_context;
                    }
                    if modification.block_tool.is_some() {
                        combined_modification.block_tool = modification.block_tool;
                        combined_modification.block_reason = modification.block_reason;
                    }
                    if modification.modified_parameters.is_some() {
                        combined_modification.modified_parameters =
                            modification.modified_parameters;
                    }
                }
            }

            return Ok(Some(combined_modification));
        }

        Ok(None)
    }

    /// Run before_agent_start hooks
    pub async fn run_before_agent_start(
        &self,
        ctx: ToolContext,
    ) -> Result<Option<HookModification>> {
        self.run_modifying_hooks(HookType::BeforeAgentStart, ctx)
            .await
    }

    /// Run agent_end hooks
    pub async fn run_agent_end(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::AgentEnd, ctx).await
    }

    /// Run before_compaction hooks
    pub async fn run_before_compaction(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::BeforeCompaction, ctx).await
    }

    /// Run after_compaction hooks
    pub async fn run_after_compaction(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::AfterCompaction, ctx).await
    }

    /// Run message_received hooks
    pub async fn run_message_received(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::MessageReceived, ctx).await
    }

    /// Run message_sending hooks
    pub async fn run_message_sending(&self, ctx: ToolContext) -> Result<Option<HookModification>> {
        self.run_modifying_hooks(HookType::MessageSending, ctx)
            .await
    }

    /// Run message_sent hooks
    pub async fn run_message_sent(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::MessageSent, ctx).await
    }

    /// Run before_tool_call hooks
    pub async fn run_before_tool_call(&self, ctx: ToolContext) -> Result<Option<HookModification>> {
        self.run_modifying_hooks(HookType::BeforeToolCall, ctx)
            .await
    }

    /// Run after_tool_call hooks
    pub async fn run_after_tool_call(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::AfterToolCall, ctx).await
    }

    /// Run tool_result_persist hooks
    pub async fn run_tool_result_persist(
        &self,
        ctx: ToolContext,
    ) -> Result<Option<HookModification>> {
        self.run_modifying_hooks(HookType::ToolResultPersist, ctx)
            .await
    }

    /// Run session_start hooks
    pub async fn run_session_start(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::SessionStart, ctx).await
    }

    /// Run session_end hooks
    pub async fn run_session_end(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::SessionEnd, ctx).await
    }

    /// Run gateway_start hooks
    pub async fn run_gateway_start(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::GatewayStart, ctx).await
    }

    /// Run gateway_stop hooks
    pub async fn run_gateway_stop(&self, ctx: ToolContext) -> Result<()> {
        self.run_void_hooks(HookType::GatewayStop, ctx).await
    }

    /// Clear all hooks (mostly for testing)
    pub async fn clear_all_hooks(&self) {
        let mut hooks = self.hooks.write().await;
        hooks.clear();
    }

    /// Get hook count for a type
    pub async fn hook_count(&self, hook_type: HookType) -> usize {
        let hooks = self.hooks.read().await;
        hooks.get(&hook_type).map(|h| h.len()).unwrap_or(0)
    }
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_hook_runner_creation() {
        let runner = HookRunner::new();
        assert_eq!(runner.hook_count(HookType::BeforeAgentStart).await, 0);
    }

    #[tokio::test]
    async fn test_register_and_run_void_hooks() {
        let runner = HookRunner::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let hook: PluginHook = Arc::new(move |_, _| {
            let counter = counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            })
        });

        runner
            .register_hook(HookType::SessionStart, "test_hook".to_string(), 1, hook)
            .await
            .unwrap();

        let ctx = ToolContext {
            session_id: "test".to_string(),
            workspace_dir: None,
            agent_id: None,
            message_channel: None,
            sandboxed: false,
            metadata: std::collections::HashMap::new(),
        };

        runner.run_session_start(ctx).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_hook_priority_ordering() {
        let runner = HookRunner::new();
        let execution_order = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        for (priority, order) in &[(10, "high"), (5, "medium"), (1, "low")] {
            let order_vec = execution_order.clone();
            let order_str = order.to_string();

            let hook: PluginHook = Arc::new(move |_, _| {
                let order_vec = order_vec.clone();
                let order_str = order_str.clone();
                Box::pin(async move {
                    let mut orders = order_vec.lock().await;
                    orders.push(order_str);
                    Ok(None)
                })
            });

            runner
                .register_hook(
                    HookType::BeforeAgentStart,
                    format!("hook_{}", order),
                    *priority,
                    hook,
                )
                .await
                .unwrap();
        }

        let ctx = ToolContext {
            session_id: "test".to_string(),
            workspace_dir: None,
            agent_id: None,
            message_channel: None,
            sandboxed: false,
            metadata: std::collections::HashMap::new(),
        };

        runner.run_before_agent_start(ctx).await.unwrap();
        let orders = execution_order.lock().await;

        // Higher priority executes first
        assert_eq!(orders[0], "high");
        assert_eq!(orders[1], "medium");
        assert_eq!(orders[2], "low");
    }
}
