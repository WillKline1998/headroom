//! Local usage analytics. Nothing leaves the computer.
//!
//! Sources:
//! - **Claude Code**: `~/.claude/projects/**/*.jsonl`. Every assistant reply
//!   records its model and token counts.
//! - **Hermes Agent** (optional, see `hermes.rs`): `~/.hermes/state.db`.
//!
//! Claude chats on claude.ai / desktop aren't logged locally; the usage
//! endpoint's weekly breakdown covers that split instead.

use crate::pricing::{self, Tokens};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// One row per (local day, source, model): the UI aggregates any range from these.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayModel {
    pub date: NaiveDate,
    /// "claude_code" | "hermes"
    pub source: String,
    pub model: String,
    pub replies: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// What these replies would have cost at API list prices (USD).
    pub api_value: f64,
    /// Replies from models with no known price (left out of `api_value`).
    pub unpriced_replies: u64,
}

/// Replies per local (date, hour, source), for the "busiest hours" heatmap.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HourCount {
    pub date: NaiveDate,
    pub source: String,
    /// 0 = Monday … 6 = Sunday
    pub weekday: u32,
    pub hour: u32,
    pub replies: u64,
}

/// A source that was found on this computer.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub label: String,
    /// Where it was read from, e.g. a folder or database path.
    pub location: String,
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub days: Vec<DayModel>,
    pub hours: Vec<HourCount>,
    pub prices_as_of: String,
    pub sessions: usize,
    pub files_scanned: usize,
    pub first_seen: Option<DateTime<Utc>>,
    pub sources: Vec<SourceInfo>,
    /// Hermes was found but turned off in Settings.
    pub hermes_available: bool,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    cache_creation: Option<CacheCreation>,
}

/// Newer logs split cache writes by lifetime; 1-hour writes cost more.
#[derive(Deserialize, Default)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

impl Usage {
    fn tokens(&self) -> Tokens {
        let (w5, w1h) = match &self.cache_creation {
            Some(c) if c.ephemeral_5m_input_tokens + c.ephemeral_1h_input_tokens > 0 => {
                (c.ephemeral_5m_input_tokens, c.ephemeral_1h_input_tokens)
            }
            _ => (self.cache_creation_input_tokens, 0), // older logs: assume the default 5-minute cache
        };
        Tokens {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_write_5m: w5,
            cache_write_1h: w1h,
        }
    }
}

/// Where Claude Code keeps logs (newer versions use ~/.config/claude).
pub fn log_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = crate::claude::credentials::claude_dir() {
        dirs.push(d.join("projects"));
    }
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join(".claude/projects"));
        dirs.push(h.join(".config/claude/projects"));
    }
    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|d| d.is_dir() && seen.insert(d.clone()))
        .collect()
}

fn jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            jsonl_files(&p, out); // subagent transcripts live in nested folders
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

/// Collects usage from any source into day/hour rows.
#[derive(Default)]
pub struct Acc {
    rows: HashMap<(NaiveDate, String, String), DayModel>,
    hours: HashMap<(NaiveDate, u32, String), u64>,
    seen: HashSet<String>,
    sessions: HashMap<String, HashSet<String>>,
    first: Option<DateTime<Utc>>,
    files: usize,
    sources: Vec<SourceInfo>,
}

/// Model ids from routers look like "anthropic/claude-opus-5-5"; keep the last part.
pub fn normalize_model(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_string()
}

impl Acc {
    pub fn session(&mut self, source: &str, id: &str) {
        self.sessions
            .entry(source.to_string())
            .or_default()
            .insert(id.to_string());
    }

    /// Records `replies` replies at `ts`. `fallback_value` is used when our
    /// price table doesn't know the model (e.g. Hermes' own cost estimate).
    pub fn add(
        &mut self,
        source: &str,
        ts: DateTime<Utc>,
        model: &str,
        replies: u64,
        t: Tokens,
        fallback_value: Option<f64>,
    ) {
        self.first = Some(self.first.map_or(ts, |f| f.min(ts)));
        let local = ts.with_timezone(&Local);
        let date = local.date_naive();
        if replies > 0 {
            *self
                .hours
                .entry((date, local.hour(), source.to_string()))
                .or_default() += replies;
        }
        let model = normalize_model(model);
        let value = pricing::cost(&model, t).or(fallback_value);
        let row = self
            .rows
            .entry((date, source.to_string(), model.clone()))
            .or_insert_with(|| DayModel {
                date,
                source: source.to_string(),
                model,
                replies: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                api_value: 0.0,
                unpriced_replies: 0,
            });
        match value {
            Some(v) => row.api_value += v,
            None => row.unpriced_replies += replies,
        }
        row.replies += replies;
        row.input_tokens += t.input;
        row.output_tokens += t.output;
        row.cache_read_tokens += t.cache_read;
        row.cache_write_tokens += t.cache_write_5m + t.cache_write_1h;
    }

    pub fn source(&mut self, id: &str, label: &str, location: String) {
        let sessions = self.sessions.get(id).map_or(0, HashSet::len);
        self.sources.push(SourceInfo {
            id: id.into(),
            label: label.into(),
            location,
            sessions,
        });
    }

    fn add_claude_code_line(&mut self, raw: &str) {
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            return;
        };
        if line.kind.as_deref() != Some("assistant") {
            return;
        }
        let (Some(msg), Some(ts)) = (line.message, line.timestamp) else {
            return;
        };
        // Skip placeholder entries such as "<synthetic>".
        let Some(model) = msg.model.filter(|m| !m.starts_with('<')) else {
            return;
        };
        // Claude Code writes one line per content block of the same reply: count it once.
        if let Some(id) = &msg.id {
            let key = format!("{id}:{}", line.request_id.as_deref().unwrap_or(""));
            if !self.seen.insert(key) {
                return;
            }
        }
        if let Some(s) = &line.session_id {
            self.session("claude_code", s);
        }
        let t = msg.usage.unwrap_or_default().tokens();
        self.add("claude_code", ts, &model, 1, t, None);
    }

    pub fn scan_claude_code(&mut self, dirs: &[PathBuf]) {
        let mut files = Vec::new();
        for d in dirs {
            jsonl_files(d, &mut files);
        }
        for f in &files {
            let Ok(file) = std::fs::File::open(f) else {
                continue;
            };
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                self.add_claude_code_line(&line);
            }
        }
        self.files += files.len();
        if !files.is_empty() {
            let location = dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            self.source("claude_code", "Claude Code", location);
        }
    }

    pub fn finish(self, hermes_available: bool) -> Analytics {
        let mut days: Vec<DayModel> = self.rows.into_values().collect();
        days.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then(a.source.cmp(&b.source))
                .then(a.model.cmp(&b.model))
        });
        let mut hours: Vec<HourCount> = self
            .hours
            .into_iter()
            .map(|((date, hour, source), replies)| HourCount {
                date,
                source,
                weekday: date.weekday().num_days_from_monday(),
                hour,
                replies,
            })
            .collect();
        hours.sort_by(|a, b| (a.date, a.hour, &a.source).cmp(&(b.date, b.hour, &b.source)));
        Analytics {
            days,
            hours,
            prices_as_of: pricing::PRICES_AS_OF.to_string(),
            sessions: self.sessions.values().map(HashSet::len).sum(),
            files_scanned: self.files,
            first_seen: self.first,
            sources: self.sources,
            hermes_available,
        }
    }
}

/// Everything the Models tab needs. Hermes is read only when `include_hermes` is on.
pub fn scan(include_hermes: bool) -> Analytics {
    let mut acc = Acc::default();
    acc.scan_claude_code(&log_dirs());
    let dbs = crate::hermes::databases();
    if include_hermes {
        crate::hermes::scan(&dbs, &mut acc);
    }
    acc.finish(!include_hermes && !dbs.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_replies_once_per_model_and_skips_noise() {
        let dir = std::env::temp_dir().join(format!("headroom-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("proj/sub")).unwrap();
        let reply = |id: &str, model: &str, ts: &str, out: u64| {
            format!(
                r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r-{id}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":10,"output_tokens":{out},"cache_read_input_tokens":100,"cache_creation_input_tokens":5}}}}}}"#
            )
        };
        let lines = [
            reply("m1", "claude-opus-5-5", "2026-10-07T15:00:00Z", 50),
            reply("m1", "claude-opus-5-5", "2026-10-07T15:00:00Z", 50), // same reply, 2nd content block
            reply("m2", "claude-sonnet-5-5", "2026-10-07T16:00:00Z", 20),
            reply("m3", "<synthetic>", "2026-10-07T16:00:00Z", 0),
            r#"{"type":"user","timestamp":"2026-10-07T15:00:00Z","message":{"role":"user"}}"#
                .to_string(),
            "not json".to_string(),
        ];
        std::fs::write(dir.join("proj/a.jsonl"), lines.join("\n")).unwrap();
        std::fs::write(
            dir.join("proj/sub/agent.jsonl"),
            reply("m4", "claude-opus-5-5", "2026-10-07T17:00:00Z", 5),
        )
        .unwrap();

        let mut acc = Acc::default();
        acc.scan_claude_code(std::slice::from_ref(&dir));
        let a = acc.finish(false);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(a.files_scanned, 2);
        assert_eq!(a.sessions, 1);
        assert_eq!(a.sources.len(), 1);
        assert_eq!(a.sources[0].id, "claude_code");
        let opus: Vec<_> = a
            .days
            .iter()
            .filter(|d| d.model == "claude-opus-5-5")
            .collect();
        assert_eq!(opus.iter().map(|d| d.replies).sum::<u64>(), 2);
        assert_eq!(opus.iter().map(|d| d.output_tokens).sum::<u64>(), 55);
        assert!(opus.iter().all(|d| d.source == "claude_code"));
        assert_eq!(
            a.days
                .iter()
                .filter(|d| d.model == "claude-sonnet-5-5")
                .count(),
            1
        );
        assert!(a.days.iter().all(|d| !d.model.starts_with('<')));
        // m1 + m4 on Opus 5.5: 2×(10 in, 100 cache read, 5 cache write) + 55 out
        let expected = (20.0 * 4.0 + 55.0 * 20.0 + 200.0 * 0.20 + 10.0 * 5.0) / 1e6;
        assert!((opus.iter().map(|d| d.api_value).sum::<f64>() - expected).abs() < 1e-12);
        assert_eq!(a.hours.iter().map(|h| h.replies).sum::<u64>(), 3);
        assert!(a.hours.iter().all(|h| h.weekday < 7 && h.hour < 24));
    }

    #[test]
    fn router_prefixes_are_dropped() {
        assert_eq!(
            normalize_model("anthropic/claude-opus-5-5"),
            "claude-opus-5-5"
        );
        assert_eq!(normalize_model("z-ai/glm-5.2"), "glm-5.2");
        assert_eq!(normalize_model("claude-sonnet-5-5"), "claude-sonnet-5-5");
    }
}
