//! Workspace management for dynamic system prompts
//!
//! The workspace contains markdown files that define the agent's personality,
//! instructions, and other configuration that can be edited remotely.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Types of workspace files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceFile {
    /// SOUL.md - Persona, tone, and boundaries
    Soul,
    /// IDENTITY.md - Agent name, vibe, emoji
    Identity,
    /// AGENTS.md - Operating instructions
    Agents,
    /// USER.md - User preferences and info
    User,
    /// TOOLS.md - Tool usage conventions and creation guide
    Tools,
}

impl WorkspaceFile {
    /// Get the filename for this workspace file type
    pub fn filename(&self) -> &'static str {
        match self {
            WorkspaceFile::Soul => "SOUL.md",
            WorkspaceFile::Identity => "IDENTITY.md",
            WorkspaceFile::Agents => "AGENTS.md",
            WorkspaceFile::User => "USER.md",
            WorkspaceFile::Tools => "TOOLS.md",
        }
    }

    /// Get all workspace file types
    pub fn all() -> &'static [WorkspaceFile] {
        &[
            WorkspaceFile::Soul,
            WorkspaceFile::Identity,
            WorkspaceFile::Agents,
            WorkspaceFile::User,
            WorkspaceFile::Tools,
        ]
    }

    /// Parse from filename
    pub fn from_filename(name: &str) -> Option<WorkspaceFile> {
        match name.to_uppercase().as_str() {
            "SOUL.MD" | "SOUL" => Some(WorkspaceFile::Soul),
            "IDENTITY.MD" | "IDENTITY" => Some(WorkspaceFile::Identity),
            "AGENTS.MD" | "AGENTS" => Some(WorkspaceFile::Agents),
            "USER.MD" | "USER" => Some(WorkspaceFile::User),
            "TOOLS.MD" | "TOOLS" => Some(WorkspaceFile::Tools),
            _ => None,
        }
    }
}

/// Workspace manager for loading and saving workspace files
#[derive(Debug, Clone)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    /// Create a new workspace manager
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Get the workspace path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if the workspace exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Initialize the workspace with default files if they don't exist
    pub fn init_default(&self) -> Result<()> {
        // Create workspace directory
        fs::create_dir_all(&self.path)
            .with_context(|| format!("Failed to create workspace directory: {:?}", self.path))?;

        info!("Initializing workspace at {:?}", self.path);

        // Create each file with default content if it doesn't exist
        for file_type in WorkspaceFile::all() {
            let file_path = self.file_path(*file_type);
            if !file_path.exists() {
                let content = self.default_content(*file_type);
                fs::write(&file_path, content)
                    .with_context(|| format!("Failed to create {:?}", file_path))?;
                info!("Created {}", file_type.filename());
            }
        }

        Ok(())
    }

    /// Load a workspace file
    pub fn load_file(&self, file_type: WorkspaceFile) -> Option<String> {
        let file_path = self.file_path(file_type);
        match fs::read_to_string(&file_path) {
            Ok(content) => {
                debug!("Loaded {} ({} bytes)", file_type.filename(), content.len());
                Some(content)
            }
            Err(e) => {
                warn!("Failed to load {}: {}", file_type.filename(), e);
                None
            }
        }
    }

    /// Save a workspace file
    pub fn save_file(&self, file_type: WorkspaceFile, content: &str) -> Result<()> {
        let file_path = self.file_path(file_type);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&file_path, content)
            .with_context(|| format!("Failed to write {:?}", file_path))?;

        info!("Saved {} ({} bytes)", file_type.filename(), content.len());
        Ok(())
    }

    /// List all existing workspace files
    pub fn list_files(&self) -> Vec<WorkspaceFile> {
        WorkspaceFile::all()
            .iter()
            .filter(|&&ft| self.file_path(ft).exists())
            .copied()
            .collect()
    }

    /// Get the full path to a workspace file
    fn file_path(&self, file_type: WorkspaceFile) -> PathBuf {
        self.path.join(file_type.filename())
    }

    /// Get default content for a workspace file
    fn default_content(&self, file_type: WorkspaceFile) -> &'static str {
        match file_type {
            WorkspaceFile::Soul => include_str!("../../templates/SOUL.md"),
            WorkspaceFile::Identity => include_str!("../../templates/IDENTITY.md"),
            WorkspaceFile::Agents => include_str!("../../templates/AGENTS.md"),
            WorkspaceFile::User => include_str!("../../templates/USER.md"),
            WorkspaceFile::Tools => include_str!("../../templates/TOOLS.md"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_workspace_file_filename() {
        assert_eq!(WorkspaceFile::Soul.filename(), "SOUL.md");
        assert_eq!(WorkspaceFile::Identity.filename(), "IDENTITY.md");
        assert_eq!(WorkspaceFile::Agents.filename(), "AGENTS.md");
        assert_eq!(WorkspaceFile::User.filename(), "USER.md");
        assert_eq!(WorkspaceFile::Tools.filename(), "TOOLS.md");
    }

    #[test]
    fn test_workspace_file_from_filename() {
        assert_eq!(
            WorkspaceFile::from_filename("SOUL.md"),
            Some(WorkspaceFile::Soul)
        );
        assert_eq!(
            WorkspaceFile::from_filename("soul"),
            Some(WorkspaceFile::Soul)
        );
        assert_eq!(WorkspaceFile::from_filename("unknown"), None);
    }

    #[test]
    fn test_workspace_init_and_load() {
        let dir = tempdir().unwrap();
        let workspace = Workspace::new(dir.path().join("workspace"));

        // Initially doesn't exist
        assert!(!workspace.exists());

        // After init, should exist with files
        workspace.init_default().unwrap();
        assert!(workspace.exists());

        // Should be able to list and load files
        let files = workspace.list_files();
        assert_eq!(files.len(), 5);

        let soul = workspace.load_file(WorkspaceFile::Soul);
        assert!(soul.is_some());
        assert!(!soul.unwrap().is_empty());
    }

    #[test]
    fn test_workspace_save_file() {
        let dir = tempdir().unwrap();
        let workspace = Workspace::new(dir.path().join("workspace"));
        workspace.init_default().unwrap();

        let new_content = "# Custom Soul\n\nCustom personality.";
        workspace
            .save_file(WorkspaceFile::Soul, new_content)
            .unwrap();

        let loaded = workspace.load_file(WorkspaceFile::Soul).unwrap();
        assert_eq!(loaded, new_content);
    }
}
