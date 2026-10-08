//! Provider-neutral data the UI renders. Another provider (e.g. Codex) only
//! needs to produce a `Snapshot` to appear in the widget.

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub id: String,
    pub label: String,
    /// "session" | "weekly" | anything new
    pub group: String,
    /// 0–100
    pub percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
    /// Window length, used to show how much of the window has elapsed.
    pub window_secs: Option<i64>,
    /// Anthropic's own "normal" | "warning" | … rating.
    pub severity: String,
    /// The limit currently constraining the account.
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown {
    pub key: String,
    pub label: String,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub provider: String,
    pub plan: Option<String>,
    pub limits: Vec<Limit>,
    /// Where this week's usage went (Claude Code, chat, Cowork, …).
    pub breakdown: Vec<Breakdown>,
    pub breakdown_since: Option<DateTime<Utc>>,
    pub extra_usage: bool,
    pub fetched_at: DateTime<Utc>,
}

/// What the widget shows: the latest numbers plus whether they're current.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageState {
    pub snapshot: Option<Snapshot>,
    /// "loading" | "ok" | "signed_out" | "error"
    pub status: String,
    pub message: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
}
