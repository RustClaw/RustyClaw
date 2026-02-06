use crate::core::Router;
use crate::config::Config;
use crate::storage::Storage;
use crate::api::ApiError;
use axum::{extract::State, Json};
use std::sync::Arc;
use serde_json::Value;

/// Get current configuration
pub async fn get_config<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
) -> Json<Config> {
    Json(router.config().read().await.clone())
}

#[derive(serde::Serialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub channels: Vec<String>,
}

/// List all configured agents
pub async fn get_agents<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
) -> Json<Vec<AgentSummary>> {
    let config_arc = router.config();
    let config = config_arc.read().await;
    let agents = config.agents.iter()
        .map(|(id, cfg)| AgentSummary {
            id: id.clone(),
            name: cfg.name.clone(),
            channels: cfg.channels.clone(),
        })
        .collect();
    Json(agents)
}

/// Update configuration with partial patch
pub async fn patch_config<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Json(patch): Json<Value>,
) -> Result<Json<Config>, ApiError> {
    let config_arc = router.config();
    let mut config_guard = config_arc.write().await;
    
    // Convert current config to Value
    let mut config_value = serde_json::to_value(&*config_guard)
        .map_err(|e| ApiError::InternalError(format!("Failed to serialize current config: {}", e)))?;
        
    // Merge patch
    json_merge(&mut config_value, patch);
    
    // Deserialize back to Config
    let mut new_config: Config = serde_json::from_value(config_value)
        .map_err(|e| ApiError::BadRequest(format!("Invalid configuration: {}", e)))?;
        
    // Restore config_path (skipped during serialization)
    new_config.config_path = config_guard.config_path.clone();
    
    // Update guard
    *config_guard = new_config;
    
    // Save to disk
    config_guard.save()
        .map_err(|e| ApiError::InternalError(format!("Failed to save config: {}", e)))?;
        
    Ok(Json(config_guard.clone()))
}

/// Recursive JSON merge
fn json_merge(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                json_merge(target_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch,
    }
}
