//! Memory management system
//!
//! Handles daily logs (short-term) and curated memory (long-term).
//! Memories are stored in markdown files within the workspace.

use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::Local;

#[derive(Debug, Clone)]
pub struct MemoryManager {
    workspace_path: PathBuf,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new<P: AsRef<Path>>(workspace_path: P) -> Self {
        Self {
            workspace_path: workspace_path.as_ref().to_path_buf(),
        }
    }

    /// Get the path to the memory directory
    fn memory_dir(&self) -> PathBuf {
        self.workspace_path.join("memory")
    }

    /// Ensure memory directory exists
    fn ensure_memory_dir(&self) -> Result<()> {
        let dir = self.memory_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir).context("Failed to create memory directory")?;
        }
        Ok(())
    }

    /// Get the path for today's memory log
    fn today_log_path(&self) -> PathBuf {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.memory_dir().join(format!("{}.md", today))
    }

    /// Get path for curated memory (MEMORY.md)
    fn curated_memory_path(&self) -> PathBuf {
        self.workspace_path.join("MEMORY.md")
    }

    /// Read today's memory log
    pub fn get_today_log(&self) -> Result<String> {
        let path = self.today_log_path();
        if path.exists() {
            fs::read_to_string(path).context("Failed to read daily memory log")
        } else {
            Ok(String::new())
        }
    }

    /// Append content to today's memory log
    pub fn append_memory(&self, content: &str) -> Result<()> {
        self.ensure_memory_dir()?;
        
        let path = self.today_log_path();
        let timestamp = Local::now().format("%H:%M:%S");
        
        // Append or create
        let entry = format!("\n[{}] {}\n", timestamp, content.trim());
        
        // Usage of OpenOptions to append
        use std::fs::OpenOptions;
        use std::io::Write;
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .context("Failed to open daily memory log")?;
            
        file.write_all(entry.as_bytes())?;
        
        tracing::info!("Appended to memory log: {}", path.display());
        Ok(())
    }

    /// Read curated memory (MEMORY.md)
    pub fn get_curated_memory(&self) -> Option<String> {
        let path = self.curated_memory_path();
        if path.exists() {
            fs::read_to_string(path).ok()
        } else {
            None
        }
    }
}
