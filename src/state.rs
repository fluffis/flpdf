use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Default)]
pub struct AppState {
    pub last_dir: Option<String>,
    pub recent_files: Vec<String>,
}

impl AppState {
    fn state_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("flpdf").join("state.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::state_path() else {
            return Self::default();
        };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::state_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn add_recent(&mut self, file_path: &str) {
        // Resolve to absolute path
        let abs = std::fs::canonicalize(file_path)
            .unwrap_or_else(|_| PathBuf::from(file_path));
        let abs_str = abs.to_string_lossy().to_string();

        // Remove if already present, then push to front
        self.recent_files.retain(|f| f != &abs_str);
        self.recent_files.insert(0, abs_str);
        self.recent_files.truncate(MAX_RECENT);

        // Update last_dir
        if let Some(parent) = abs.parent() {
            self.last_dir = Some(parent.to_string_lossy().to_string());
        }

        self.save();
    }

    pub fn last_dir_path(&self) -> Option<PathBuf> {
        self.last_dir.as_ref().map(PathBuf::from).filter(|p| p.is_dir())
    }
}
