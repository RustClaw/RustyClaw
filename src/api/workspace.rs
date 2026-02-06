use crate::config::workspace::WorkspaceFile;
use crate::core::Router;
use crate::storage::Storage;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct WorkspaceFileResponse {
    pub content: String,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceFileRequest {
    pub content: String,
}

#[derive(Serialize)]
pub struct WorkspaceListResponse {
    pub files: Vec<String>,
}

/// List available workspace files
pub async fn list_workspace_files<S: Storage + 'static>(
    State(_router): State<Arc<Router<S>>>,
) -> Json<WorkspaceListResponse> {
    // Return list of known workspace files
    Json(WorkspaceListResponse {
        files: vec![
            "soul".to_string(),
            "identity".to_string(),
            "agents".to_string(),
            "user".to_string(),
            "tools".to_string(),
        ],
    })
}

/// Get workspace file content
pub async fn get_workspace_file<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Path(file_type): Path<String>,
) -> Result<Json<WorkspaceFileResponse>, StatusCode> {
    let workspace_file = match file_type.to_lowercase().as_str() {
        "soul" => WorkspaceFile::Soul,
        "identity" => WorkspaceFile::Identity,
        "agents" => WorkspaceFile::Agents,
        "user" => WorkspaceFile::User,
        "tools" => WorkspaceFile::Tools,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let content = router
        .workspace()
        .load_file(workspace_file)
        .unwrap_or_default();

    Ok(Json(WorkspaceFileResponse { content }))
}

/// Update workspace file content
pub async fn update_workspace_file<S: Storage + 'static>(
    State(router): State<Arc<Router<S>>>,
    Path(file_type): Path<String>,
    Json(request): Json<UpdateWorkspaceFileRequest>,
) -> Result<StatusCode, StatusCode> {
    let workspace_file = match file_type.to_lowercase().as_str() {
        "soul" => WorkspaceFile::Soul,
        "identity" => WorkspaceFile::Identity,
        "agents" => WorkspaceFile::Agents,
        "user" => WorkspaceFile::User,
        "tools" => WorkspaceFile::Tools,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    // Note: Workspace currently only supports load_file, need to add save_file
    // For now, assuming save_file exists or will be added.
    // I need to add save_file to Workspace struct first!
    if let Err(e) = router
        .workspace()
        .save_file(workspace_file, &request.content)
    {
        tracing::error!("Failed to save workspace file: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(StatusCode::OK)
}
