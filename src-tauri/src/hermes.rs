//! Optional source: Hermes Agent (github.com/NousResearch/hermes-agent).
//!
//! Hermes calls models directly and records usage in a SQLite database,
//! `$HERMES_HOME/state.db` (default `~/.hermes`, plus one per profile). It
//! stores token totals per session × model × task (`session_model_usage`) and a
//! timestamp for every reply (`messages`). Headroom spreads each usage row
//! evenly over that row's replies, so days and hours line up with when you
//! actually worked. Opened read-only; message text is never read.

use crate::analytics::Acc;
use crate::pricing::Tokens;
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Every Hermes state database on this computer (main + profiles).
pub fn databases() -> Vec<PathBuf> {
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".hermes")));
    let Some(home) = home else { return vec![] };
    let mut dbs = vec![home.join("state.db")];
    if let Ok(profiles) = std::fs::read_dir(home.join("profiles")) {
        dbs.extend(profiles.flatten().map(|p| p.path().join("state.db")));
    }
    dbs.into_iter().filter(|p| p.is_file()).collect()
}

struct UsageRow {
    session: String,
    model: String,
    calls: u64,
    t: Tokens,
    est_cost: Option<f64>,
    first: f64,
    last: f64,
}

/// Splits `total` into `n` near-equal integer parts that add back up exactly.
fn split(total: u64, n: usize, i: usize) -> u64 {
    let n = n as u64;
    total / n + u64::from((i as u64) < total % n)
}

/// Assistant-reply timestamps (unix seconds) per session id.
type ReplyTimes = HashMap<String, Vec<f64>>;

fn read(db: &Path) -> rusqlite::Result<(Vec<UsageRow>, ReplyTimes)> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(2))?;

    let has_usage_table: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='session_model_usage'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)?;
    // Older Hermes versions only keep per-session totals.
    let sql = if has_usage_table {
        "SELECT session_id, model, api_call_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                estimated_cost_usd, first_seen, last_seen FROM session_model_usage"
    } else {
        "SELECT id, model, api_call_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                estimated_cost_usd, started_at, COALESCE(last_activity_at, ended_at, started_at) FROM sessions"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| {
            let n = |i| {
                r.get::<_, Option<i64>>(i)
                    .map(|v| v.unwrap_or(0).max(0) as u64)
            };
            Ok(UsageRow {
                session: r.get(0)?,
                model: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                calls: n(2)?,
                // Reasoning tokens are left out: providers already bill them as output.
                t: Tokens {
                    input: n(3)?,
                    output: n(4)?,
                    cache_read: n(5)?,
                    cache_write_5m: n(6)?,
                    cache_write_1h: 0,
                },
                est_cost: r.get(7)?,
                first: r.get::<_, Option<f64>>(8)?.unwrap_or(0.0),
                last: r.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut replies: ReplyTimes = HashMap::new();
    let mut stmt = conn.prepare("SELECT session_id, timestamp FROM messages WHERE role = 'assistant' AND timestamp IS NOT NULL ORDER BY timestamp")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))? {
        let (s, ts) = row?;
        replies.entry(s).or_default().push(ts);
    }
    Ok((rows, replies))
}

/// Adds every Hermes database's usage to `acc` under the "hermes" source.
pub fn scan(dbs: &[PathBuf], acc: &mut Acc) {
    let mut found = Vec::new();
    for db in dbs {
        let Ok((rows, replies)) = read(db) else {
            continue;
        };
        found.push(db.display().to_string());
        for row in rows
            .into_iter()
            .filter(|r| !r.model.is_empty() && r.last > 0.0)
        {
            acc.session("hermes", &row.session);
            // The replies this row's model produced: those inside its first/last-seen window.
            let mut times: Vec<f64> = replies
                .get(&row.session)
                .map(|ts| {
                    ts.iter()
                        .copied()
                        .filter(|t| *t >= row.first - 60.0 && *t <= row.last + 60.0)
                        .collect()
                })
                .unwrap_or_default();
            if times.is_empty() {
                times.push(row.last);
            }
            let n = times.len();
            for (i, ts) in times.into_iter().enumerate() {
                let Some(when) = Utc.timestamp_millis_opt((ts * 1000.0) as i64).single() else {
                    continue;
                };
                let t = Tokens {
                    input: split(row.t.input, n, i),
                    output: split(row.t.output, n, i),
                    cache_read: split(row.t.cache_read, n, i),
                    cache_write_5m: split(row.t.cache_write_5m, n, i),
                    cache_write_1h: 0,
                };
                let est = row.est_cost.filter(|c| *c > 0.0).map(|c| c / n as f64);
                acc.add("hermes", when, &row.model, split(row.calls, n, i), t, est);
            }
        }
    }
    if !found.is_empty() {
        acc.source("hermes", "Hermes Agent", found.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_adds_back_up() {
        let parts: Vec<u64> = (0..3).map(|i| split(10, 3, i)).collect();
        assert_eq!(parts, vec![4, 3, 3]);
        assert_eq!((0..7).map(|i| split(5, 7, i)).sum::<u64>(), 5);
    }

    #[test]
    fn spreads_session_usage_over_its_replies() {
        let path = std::env::temp_dir().join(format!("headroom-hermes-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE sessions (id TEXT, model TEXT);
             CREATE TABLE session_model_usage (session_id TEXT, model TEXT, task TEXT, api_call_count INTEGER,
               input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER,
               reasoning_tokens INTEGER, estimated_cost_usd REAL, first_seen REAL, last_seen REAL);
             CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, timestamp REAL);
             -- 2026-10-07 15:00Z and 2026-10-08 15:00Z
             INSERT INTO session_model_usage VALUES ('s1','claude-opus-5-5',NULL,4,100,1000,0,0,50,0.5,1791385200,1791471600);
             INSERT INTO session_model_usage VALUES ('s1','z-ai/glm-5.2','approval',1,10,10,0,0,0,0.002,1791471500,1791471500);
             INSERT INTO session_model_usage VALUES ('s2','',NULL,1,1,1,0,0,0,0,0,0);
             INSERT INTO messages (session_id, role, content, timestamp) VALUES
               ('s1','user','secret',1791385100), ('s1','assistant','a',1791385200), ('s1','assistant','b',1791471600),
               ('s1','tool','t',1791471000);",
        )
        .unwrap();
        drop(c);

        let mut acc = Acc::default();
        scan(std::slice::from_ref(&path), &mut acc);
        let a = acc.finish(false);
        std::fs::remove_file(&path).unwrap();

        assert_eq!(a.sources.len(), 1);
        assert_eq!(a.sources[0].id, "hermes");
        assert_eq!(a.sources[0].sessions, 1); // s2 has no model and is skipped
        let opus: Vec<_> = a
            .days
            .iter()
            .filter(|d| d.model == "claude-opus-5-5")
            .collect();
        assert_eq!(opus.len(), 2, "spread over the two days it replied on");
        assert_eq!(opus.iter().map(|d| d.replies).sum::<u64>(), 4);
        assert_eq!(opus.iter().map(|d| d.output_tokens).sum::<u64>(), 1000);
        // Priced from our table (Opus 5.5), not Hermes' estimate: 100 in + 1000 out.
        let value: f64 = opus.iter().map(|d| d.api_value).sum();
        assert!((value - (100.0 * 4.0 + 1000.0 * 20.0) / 1e6).abs() < 1e-9);
        // Unknown models fall back to Hermes' own estimate; router prefix dropped.
        let glm = a.days.iter().find(|d| d.model == "glm-5.2").unwrap();
        assert!((glm.api_value - 0.002).abs() < 1e-12);
        assert_eq!(glm.unpriced_replies, 0);
        assert!(a.hours.iter().all(|h| h.source == "hermes"));
    }
}
