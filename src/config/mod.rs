mod loader;
mod schema;
pub mod workspace;

pub use loader::load_config;
pub use loader::save_config;
pub use schema::*;

use anyhow::Result;
use std::path::Path;

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        loader::load_config(path)
    }

    pub fn save(&self) -> Result<()> {
        loader::save_config(self)
    }

    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".rustyclaw")
            .join("config.yaml")
    }
}
