//! User preferences, stored as JSON in the OS's app-config folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// How often to ask Anthropic for fresh numbers. The endpoint is rate-limited,
    /// and countdowns tick locally between checks, so minutes are plenty.
    pub refresh_secs: u64,
    pub always_on_top: bool,
    pub notify: bool,
    /// Alert once per window when a limit crosses each of these percentages.
    pub notify_at: Vec<u8>,
    /// What the macOS menu bar shows next to the icon: "both" | "session" | "weekly" | "none".
    pub tray_text: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_secs: 180,
            always_on_top: true,
            notify: true,
            notify_at: vec![80, 95],
            tray_text: "both".into(),
        }
    }
}

impl Settings {
    pub const MIN_REFRESH_SECS: u64 = 60;

    /// Clamp anything a hand-edited file could get wrong.
    pub fn sanitized(mut self) -> Self {
        self.refresh_secs = self.refresh_secs.clamp(Self::MIN_REFRESH_SECS, 3600);
        self.notify_at.retain(|p| (1..=100).contains(p));
        self.notify_at.sort_unstable();
        self.notify_at.dedup();
        if !["both", "session", "weekly", "none"].contains(&self.tray_text.as_str()) {
            self.tray_text = "both".into();
        }
        self
    }

    pub fn load(path: &PathBuf) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self, path: &PathBuf) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            path,
            serde_json::to_string_pretty(self).expect("settings serialize"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bad_values() {
        let s = Settings {
            refresh_secs: 5,
            notify_at: vec![95, 0, 80, 80, 150],
            tray_text: "???".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.refresh_secs, 60);
        assert_eq!(s.notify_at, vec![80, 95]);
        assert_eq!(s.tray_text, "both");
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"alwaysOnTop":false}"#).unwrap();
        assert!(!s.always_on_top);
        assert_eq!(s.refresh_secs, 180);
    }
}
