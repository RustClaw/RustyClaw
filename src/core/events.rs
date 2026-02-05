use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::broadcast;

/// System-wide events
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// A tool/skill was added or updated
    ToolUpdated(String),
    /// A tool/skill was removed
    ToolRemoved(String),
    /// A session was created
    SessionCreated(String),
}

/// Global event bus
static EVENT_BUS: OnceCell<Arc<broadcast::Sender<SystemEvent>>> = OnceCell::new();

/// Initialize the global event bus
pub fn init_event_bus() -> Arc<broadcast::Sender<SystemEvent>> {
    EVENT_BUS
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(100);
            Arc::new(tx)
        })
        .clone()
}

/// Get the global event bus
pub fn get_event_bus() -> Arc<broadcast::Sender<SystemEvent>> {
    init_event_bus()
}

/// Publish a system event
pub fn publish_event(event: SystemEvent) {
    let bus = get_event_bus();
    // We ignore errors if there are no receivers
    let _ = bus.send(event);
}

/// Subscribe to system events
pub fn subscribe() -> broadcast::Receiver<SystemEvent> {
    get_event_bus().subscribe()
}
