//! Local analytics from Claude Code's conversation logs (`~/.claude/projects/**/*.jsonl`).
//!
//! Every assistant reply in those logs records the model and token counts.
//! Nothing leaves the computer. Claude chats on claude.ai/desktop aren't in
//! these logs; the usage endpoint's weekly breakdown covers that split instead.

use crate::pricing::{self, Tokens};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// One row per (local day, model): the UI aggregates any date range from these.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayModel {
    pub date: NaiveDate,
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

/// Replies per local (date, hour), for the "busiest hours" heatmap.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HourCount {
    pub date: NaiveDate,
    /// 0 = Monday … 6 = Sunday
    pub weekday: u32,
    pub hour: u32,
    pub replies: u64,
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
    pub sources: Vec<String>,
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

#[derive(Default)]
struct Acc {
    rows: HashMap<(NaiveDate, String), DayModel>,
    hours: HashMap<(NaiveDate, u32), u64>,
    seen: HashSet<String>,
    sessions: HashSet<String>,
    first: Option<DateTime<Utc>>,
}

impl Acc {
    fn add_line(&mut self, raw: &str) {
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            return;
        };
        if line.kind.as_deref() != Some("assistant") {
            return;
        }
        let (Some(msg), Some(ts)) = (line.message, line.timestamp) else {
            return;
        };
        let Some(model) = msg.model.filter(|m| !m.starts_with('<')) else {
            return;
        }; // skip "<synthetic>"
           // Claude Code writes one line per content block of the same reply: count it once.
        if let Some(id) = &msg.id {
            let key = format!("{id}:{}", line.request_id.as_deref().unwrap_or(""));
            if !self.seen.insert(key) {
                return;
            }
        }
        if let Some(s) = line.session_id {
            self.sessions.insert(s);
        }
        self.first = Some(self.first.map_or(ts, |f| f.min(ts)));
        let local = ts.with_timezone(&Local);
        let date = local.date_naive();
        *self.hours.entry((date, local.hour())).or_default() += 1;
        let u = msg.usage.unwrap_or_default();
        let value = pricing::cost(&model, u.tokens());
        let row = self
            .rows
            .entry((date, model.clone()))
            .or_insert_with(|| DayModel {
                date,
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
            None => row.unpriced_replies += 1,
        }
        row.replies += 1;
        row.input_tokens += u.input_tokens;
        row.output_tokens += u.output_tokens;
        row.cache_read_tokens += u.cache_read_input_tokens;
        let t = u.tokens();
        row.cache_write_tokens += t.cache_write_5m + t.cache_write_1h;
    }

    fn finish(self, files: usize, sources: Vec<String>) -> Analytics {
        let mut days: Vec<DayModel> = self.rows.into_values().collect();
        days.sort_by(|a, b| a.date.cmp(&b.date).then(a.model.cmp(&b.model)));
        let mut hours: Vec<HourCount> = self
            .hours
            .into_iter()
            .map(|((date, hour), replies)| HourCount {
                date,
                weekday: date.weekday().num_days_from_monday(),
                hour,
                replies,
            })
            .collect();
        hours.sort_by_key(|h| (h.date, h.hour));
        Analytics {
            days,
            hours,
            prices_as_of: pricing::PRICES_AS_OF.to_string(),
            sessions: self.sessions.len(),
            files_scanned: files,
            first_seen: self.first,
            sources,
        }
    }
}

pub fn scan_dirs(dirs: &[PathBuf]) -> Analytics {
    let mut files = Vec::new();
    for d in dirs {
        jsonl_files(d, &mut files);
    }
    let mut acc = Acc::default();
    for f in &files {
        let Ok(file) = std::fs::File::open(f) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            acc.add_line(&line);
        }
    }
    acc.finish(
        files.len(),
        dirs.iter().map(|d| d.display().to_string()).collect(),
    )
}

pub fn scan() -> Analytics {
    scan_dirs(&log_dirs())
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

        let a = scan_dirs(std::slice::from_ref(&dir));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(a.files_scanned, 2);
        assert_eq!(a.sessions, 1);
        let opus: Vec<_> = a
            .days
            .iter()
            .filter(|d| d.model == "claude-opus-5-5")
            .collect();
        assert_eq!(opus.iter().map(|d| d.replies).sum::<u64>(), 2);
        assert_eq!(opus.iter().map(|d| d.output_tokens).sum::<u64>(), 55);
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
}
