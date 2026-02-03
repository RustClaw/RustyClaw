use anyhow::{Context, Result};
use notify_debouncer_mini::new_debouncer;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

/// File system watcher for skill files
pub struct SkillWatcher {
    skills_dir: PathBuf,
}

impl SkillWatcher {
    /// Create a new skill watcher
    pub fn new(skills_dir: &str) -> Self {
        Self {
            skills_dir: PathBuf::from(skills_dir),
        }
    }

    /// Run the skill watcher
    pub async fn run(&self) -> Result<()> {
        // Ensure skills directory exists
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir).context(format!(
                "Failed to create skills directory: {}",
                self.skills_dir.display()
            ))?;
            info!("Created skills directory: {}", self.skills_dir.display());
        }

        // Do initial scan
        self.initial_scan().await?;

        // Set up file watcher with debouncing
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            move |res: notify_debouncer_mini::DebounceEventResult| {
                let _ = tx.send(res);
            },
        )
        .context("Failed to create debouncer")?;

        debouncer
            .watcher()
            .watch(&self.skills_dir, notify::RecursiveMode::NonRecursive)
            .context("Failed to watch skills directory")?;

        info!(
            "Skill watcher started (watching: {})",
            self.skills_dir.display()
        );

        // Main watch loop
        while let Some(event_result) = rx.recv().await {
            match event_result {
                Ok(events) => {
                    for event in events {
                        let path_buf = PathBuf::from(&event.path);
                        if path_buf
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s == "md")
                            .unwrap_or(false)
                        {
                            // The debouncer gives us an underlying EventKind through event.kind
                            // For skills, we treat both Create and Modify as reload, Remove as unload
                            if matches!(&event.kind, notify_debouncer_mini::DebouncedEventKind::Any) {
                                // Check if file still exists to determine if it's a create/modify or remove
                                if path_buf.exists() {
                                    self.handle_skill_change(&path_buf).await;
                                } else {
                                    self.handle_skill_remove(&path_buf).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Watcher error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Initial scan of skills directory
    async fn initial_scan(&self) -> Result<()> {
        info!("Scanning skills directory: {}", self.skills_dir.display());

        match std::fs::read_dir(&self.skills_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        match super::skills::parse_skill_file(&path) {
                            Ok(entry) => match super::skills::load_skill(entry).await {
                                Ok(_) => {
                                    info!("Loaded skill from: {}", path.display());
                                }
                                Err(e) => {
                                    warn!("Failed to load skill from {}: {}", path.display(), e);
                                }
                            },
                            Err(e) => {
                                warn!("Failed to parse skill file {}: {}", path.display(), e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read skills directory: {}", e);
            }
        }

        Ok(())
    }

    /// Handle a skill file change
    async fn handle_skill_change(&self, path: &std::path::Path) {
        match super::skills::parse_skill_file(path) {
            Ok(entry) => {
                let skill_name = entry.manifest.name.clone();
                match super::skills::load_skill(entry).await {
                    Ok(_) => {
                        info!(
                            "Skill '{}' loaded/updated from: {}",
                            skill_name,
                            path.display()
                        );
                    }
                    Err(e) => {
                        warn!("Failed to load skill '{}': {}", skill_name, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse skill file {}: {}", path.display(), e);
            }
        }
    }

    /// Handle a skill file removal
    async fn handle_skill_remove(&self, path: &std::path::Path) {
        // Try to find which skill this file corresponds to
        let entries = super::skills::list_skills().await;
        if !entries.is_empty() {
            for entry in entries {
                if entry.source_path == *path {
                    match super::skills::unload_skill(&entry.manifest.name).await {
                        Ok(_) => {
                            info!(
                                "Skill '{}' unloaded (file removed: {})",
                                entry.manifest.name,
                                path.display()
                            );
                        }
                        Err(e) => {
                            warn!("Failed to unload skill '{}': {}", entry.manifest.name, e);
                        }
                    }
                    return;
                }
            }
        }

        info!("Skill file removed: {}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_initial_scan() {
        let temp_dir = TempDir::new().unwrap();
        let skills_path = temp_dir.path().join("skills");
        fs::create_dir(&skills_path).unwrap();

        // Create test skill files
        let skill1_content = r#"---
name: test_skill_1
description: "Test skill 1"
parameters: {}
runtime: bash
---
echo "test 1"
"#;

        let skill2_content = r#"---
name: test_skill_2
description: "Test skill 2"
parameters: {}
runtime: bash
---
echo "test 2"
"#;

        fs::write(skills_path.join("skill1.md"), skill1_content).unwrap();
        fs::write(skills_path.join("skill2.md"), skill2_content).unwrap();

        // Create watcher and run initial scan
        let watcher = SkillWatcher::new(skills_path.to_str().unwrap());
        watcher.initial_scan().await.unwrap();

        // Verify both skills are loaded
        let loaded_skills = super::super::skills::list_skills().await;
        assert!(loaded_skills
            .iter()
            .any(|s| s.manifest.name == "test_skill_1"));
        assert!(loaded_skills
            .iter()
            .any(|s| s.manifest.name == "test_skill_2"));

        // Clean up
        super::super::skills::unload_skill("test_skill_1")
            .await
            .ok();
        super::super::skills::unload_skill("test_skill_2")
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_initial_scan_handles_missing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let skills_path = temp_dir.path().join("nonexistent_skills");

        let watcher = SkillWatcher::new(skills_path.to_str().unwrap());
        // initial_scan should succeed even if directory doesn't exist
        // (it logs a warning but doesn't fail)
        let result = watcher.initial_scan().await;
        assert!(result.is_ok());
    }
}
