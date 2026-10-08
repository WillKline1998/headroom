//! Local analytics from Claude Code's conversation logs (`~/.claude/projects/**/*.jsonl`).
//!
//! Every assistant reply in those logs records the model and token counts.
//! Nothing leaves the computer. Claude chats on claude.ai/desktop aren't in
//! these logs; the usage endpoint's weekly breakdown covers that split instead.

use chrono::{DateTime, Local, NaiveDate, Utc};
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
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub days: Vec<DayModel>,
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
    dirs.into_iter().filter(|d| d.is_dir() && seen.insert(d.clone())).collect()
}

fn jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
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
    seen: HashSet<String>,
    sessions: HashSet<String>,
    first: Option<DateTime<Utc>>,
}

impl Acc {
    fn add_line(&mut self, raw: &str) {
        let Ok(line) = serde_json::from_str::<Line>(raw) else { return };
        if line.kind.as_deref() != Some("assistant") {
            return;
        }
        let (Some(msg), Some(ts)) = (line.message, line.timestamp) else { return };
        let Some(model) = msg.model.filter(|m| !m.starts_with('<')) else { return }; // skip "<synthetic>"
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
        let date = ts.with_timezone(&Local).date_naive();
        let u = msg.usage.unwrap_or_default();
        let row = self.rows.entry((date, model.clone())).or_insert_with(|| DayModel {
            date,
            model,
            replies: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        });
        row.replies += 1;
        row.input_tokens += u.input_tokens;
        row.output_tokens += u.output_tokens;
        row.cache_read_tokens += u.cache_read_input_tokens;
        row.cache_write_tokens += u.cache_creation_input_tokens;
    }

    fn finish(self, files: usize, sources: Vec<String>) -> Analytics {
        let mut days: Vec<DayModel> = self.rows.into_values().collect();
        days.sort_by(|a, b| a.date.cmp(&b.date).then(a.model.cmp(&b.model)));
        Analytics { days, sessions: self.sessions.len(), files_scanned: files, first_seen: self.first, sources }
    }
}

pub fn scan_dirs(dirs: &[PathBuf]) -> Analytics {
    let mut files = Vec::new();
    for d in dirs {
        jsonl_files(d, &mut files);
    }
    let mut acc = Acc::default();
    for f in &files {
        let Ok(file) = std::fs::File::open(f) else { continue };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            acc.add_line(&line);
        }
    }
    acc.finish(files.len(), dirs.iter().map(|d| d.display().to_string()).collect())
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
            format!(r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r-{id}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":10,"output_tokens":{out},"cache_read_input_tokens":100,"cache_creation_input_tokens":5}}}}}}"#)
        };
        let lines = [
            reply("m1", "claude-opus-5-5", "2026-10-07T15:00:00Z", 50),
            reply("m1", "claude-opus-5-5", "2026-10-07T15:00:00Z", 50), // same reply, 2nd content block
            reply("m2", "claude-sonnet-5-5", "2026-10-07T16:00:00Z", 20),
            reply("m3", "<synthetic>", "2026-10-07T16:00:00Z", 0),
            r#"{"type":"user","timestamp":"2026-10-07T15:00:00Z","message":{"role":"user"}}"#.to_string(),
            "not json".to_string(),
        ];
        std::fs::write(dir.join("proj/a.jsonl"), lines.join("\n")).unwrap();
        std::fs::write(dir.join("proj/sub/agent.jsonl"), reply("m4", "claude-opus-5-5", "2026-10-07T17:00:00Z", 5)).unwrap();

        let a = scan_dirs(&[dir.clone()]);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(a.files_scanned, 2);
        assert_eq!(a.sessions, 1);
        let opus: Vec<_> = a.days.iter().filter(|d| d.model == "claude-opus-5-5").collect();
        assert_eq!(opus.iter().map(|d| d.replies).sum::<u64>(), 2);
        assert_eq!(opus.iter().map(|d| d.output_tokens).sum::<u64>(), 55);
        assert_eq!(a.days.iter().filter(|d| d.model == "claude-sonnet-5-5").count(), 1);
        assert!(a.days.iter().all(|d| !d.model.starts_with('<')));
    }
}
