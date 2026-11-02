//! Session persistence and crash recovery for Echo Browser.
//!
//! Tracks open tabs, tab order, pinned states, and active tab groups, saving state
//! atomically to disk (`~/.config/echo-browser/session.json`). If the browser terminates
//! abnormally (e.g. system crash, SIGKILL, power loss), the next launch detects
//! `clean_shutdown == false` and offers one-click session restoration.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTab {
    pub title: String,
    pub url: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub tabs: Vec<SavedTab>,
    pub selected_index: Option<usize>,
    pub active_group: Option<String>,
    pub clean_shutdown: bool,
    pub saved_at: u64,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            selected_index: None,
            active_group: None,
            clean_shutdown: true,
            saved_at: 0,
        }
    }
}

pub struct SessionManager;

impl SessionManager {
    /// Returns default path: `~/.config/echo-browser/session.json`
    pub fn default_session_path() -> PathBuf {
        glib::user_config_dir()
            .join("echo-browser")
            .join("session.json")
    }

    /// Loads session state from the given JSON file.
    /// Returns `Ok(None)` if the file does not exist or cannot be parsed.
    pub fn load_session(path: &Path) -> io::Result<Option<SessionState>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)?;
        match serde_json::from_str::<SessionState>(&content) {
            Ok(session) => Ok(Some(session)),
            Err(err) => {
                log::warn!("Failed to parse session at {}: {err}", path.display());
                Ok(None)
            }
        }
    }

    /// Atomically writes session state using a `.tmp` file and rename.
    pub fn save_session(path: &Path, session: &SessionState) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Deletes the session file if present.
    pub fn clear_session(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_serialization_roundtrip() {
        let state = SessionState {
            tabs: vec![
                SavedTab {
                    title: "Echo Browser".into(),
                    url: "echo://start".into(),
                    pinned: true,
                },
                SavedTab {
                    title: "DuckDuckGo".into(),
                    url: "https://duckduckgo.com".into(),
                    pinned: false,
                },
            ],
            selected_index: Some(1),
            active_group: Some("Work".into()),
            clean_shutdown: false,
            saved_at: 1700000000,
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: SessionState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, deserialized);
    }

    #[test]
    fn test_atomic_save_and_load() {
        let temp_dir =
            std::env::temp_dir().join(format!("echo_session_test_{}", std::process::id()));
        let session_file = temp_dir.join("session.json");

        let state = SessionState {
            tabs: vec![SavedTab {
                title: "Test".into(),
                url: "https://example.com".into(),
                pinned: false,
            }],
            selected_index: Some(0),
            active_group: None,
            clean_shutdown: false,
            saved_at: 12345678,
        };

        SessionManager::save_session(&session_file, &state).expect("save session");
        assert!(session_file.exists());

        let loaded = SessionManager::load_session(&session_file)
            .expect("load session")
            .expect("session present");
        assert_eq!(loaded, state);

        SessionManager::clear_session(&session_file).expect("clear session");
        assert!(!session_file.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_corrupted_file_handling() {
        let temp_dir =
            std::env::temp_dir().join(format!("echo_session_corrupt_{}", std::process::id()));
        let session_file = temp_dir.join("corrupt.json");
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        fs::write(&session_file, "{ invalid json content").expect("write invalid");

        let loaded = SessionManager::load_session(&session_file).expect("load session");
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_missing_file_returns_none() {
        let missing = PathBuf::from("/non/existent/path/session.json");
        let loaded = SessionManager::load_session(&missing).expect("load missing");
        assert!(loaded.is_none());
    }
}
