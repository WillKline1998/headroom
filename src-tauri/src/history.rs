//! A local log of every usage check, so the Limits tab can chart how each
//! window filled. One JSON object per line in `<app data dir>/history.jsonl`.

use crate::model::Snapshot;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// The weekly window is 7 days; keeping 35 leaves room for a few past weeks.
const KEEP_DAYS: i64 = 35;
/// An unchanged reading is only worth recording again after this long, so
/// 3-minute polling doesn't write a flat line of identical points.
const MIN_GAP_MINUTES: i64 = 15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitPoint {
    pub id: String,
    pub percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub t: DateTime<Utc>,
    pub limits: Vec<LimitPoint>,
}

impl Point {
    fn from_snapshot(s: &Snapshot, t: DateTime<Utc>) -> Self {
        let limits = s
            .limits
            .iter()
            .map(|l| LimitPoint {
                id: l.id.clone(),
                percent: l.percent,
                resets_at: l.resets_at,
            })
            .collect();
        Self { t, limits }
    }
}

/// Skip a reading identical to the last one unless that one is getting old.
pub fn should_write(last: Option<&Point>, next: &Point) -> bool {
    match last {
        Some(l) => l.limits != next.limits || next.t - l.t >= Duration::minutes(MIN_GAP_MINUTES),
        None => true,
    }
}

fn is_expired(p: &Point, now: DateTime<Utc>) -> bool {
    now - p.t > Duration::days(KEEP_DAYS)
}

/// Parse the log, skipping lines a crash or hand-edit left unreadable.
pub fn parse(text: &str) -> Vec<Point> {
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub fn load(path: &Path) -> Vec<Point> {
    std::fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

/// Add a reading, pruning old points. The file is only rewritten when it
/// holds something to drop; otherwise it's a cheap append.
pub fn record_at(path: &Path, snapshot: &Snapshot, now: DateTime<Utc>) -> std::io::Result<()> {
    let next = Point::from_snapshot(snapshot, now);
    let mut points = load(path);
    if !should_write(points.last(), &next) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if points.first().is_some_and(|p| is_expired(p, now)) {
        points.retain(|p| !is_expired(p, now));
        points.push(next);
        let body: String = points
            .iter()
            .map(|p| serde_json::to_string(p).expect("point serializes") + "\n")
            .collect();
        return std::fs::write(path, body);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&next).expect("point serializes")
    )
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("history.jsonl"))
}

/// Called after each successful poll. History is a nicety, so failures are ignored.
pub fn record(app: &AppHandle, snapshot: &Snapshot) {
    if let Some(p) = path(app) {
        let _ = record_at(&p, snapshot, Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, h, m, 0).unwrap()
    }

    fn point(t: DateTime<Utc>, percent: f64) -> Point {
        Point {
            t,
            limits: vec![LimitPoint {
                id: "session".into(),
                percent,
                resets_at: Some(at(20, 0)),
            }],
        }
    }

    #[test]
    fn writes_first_point() {
        assert!(should_write(None, &point(at(9, 0), 5.0)));
    }

    #[test]
    fn skips_unchanged_recent_reading() {
        let last = point(at(9, 0), 5.0);
        assert!(!should_write(Some(&last), &point(at(9, 3), 5.0)));
    }

    #[test]
    fn writes_when_percent_changes() {
        let last = point(at(9, 0), 5.0);
        assert!(should_write(Some(&last), &point(at(9, 3), 6.0)));
    }

    #[test]
    fn writes_unchanged_reading_once_old_enough() {
        let last = point(at(9, 0), 5.0);
        assert!(should_write(Some(&last), &point(at(9, 15), 5.0)));
    }

    #[test]
    fn writes_when_window_resets() {
        let last = point(at(9, 0), 5.0);
        let mut next = point(at(9, 3), 5.0);
        next.limits[0].resets_at = Some(at(21, 0));
        assert!(should_write(Some(&last), &next));
    }

    #[test]
    fn parse_skips_corrupt_lines() {
        let good = serde_json::to_string(&point(at(9, 0), 5.0)).unwrap();
        let text = format!("{good}\nnot json\n\n{{\"t\":1}}\n{good}\n");
        assert_eq!(parse(&text).len(), 2);
    }

    #[test]
    fn serializes_camel_case() {
        let json = serde_json::to_string(&point(at(9, 0), 5.0)).unwrap();
        assert!(json.contains("\"resetsAt\""));
        assert!(json.contains("\"t\":\"2026-10-08T09:00:00Z\""));
    }

    fn snapshot(percent: f64) -> Snapshot {
        use crate::model::Limit;
        Snapshot {
            provider: "claude".into(),
            plan: None,
            tier: None,
            limits: vec![Limit {
                id: "session".into(),
                label: "Session".into(),
                group: "session".into(),
                percent,
                resets_at: Some(at(20, 0)),
                window_secs: Some(5 * 3600),
                severity: "normal".into(),
                active: false,
                detail: None,
                capped: true,
            }],
            breakdown: vec![],
            breakdown_since: None,
            extra_usage: false,
            fetched_at: at(9, 0),
        }
    }

    #[test]
    fn record_appends_dedupes_and_prunes() {
        let dir = std::env::temp_dir().join(format!("headroom-history-{}", std::process::id()));
        let file = dir.join("history.jsonl");
        let _ = std::fs::remove_dir_all(&dir);

        record_at(&file, &snapshot(5.0), at(9, 0)).unwrap();
        record_at(&file, &snapshot(5.0), at(9, 3)).unwrap(); // duplicate: skipped
        record_at(&file, &snapshot(8.0), at(9, 6)).unwrap();
        assert_eq!(load(&file).len(), 2);

        // 40 days later both old points fall out and only the new one stays.
        record_at(&file, &snapshot(1.0), at(9, 6) + Duration::days(40)).unwrap();
        let points = load(&file);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].limits[0].percent, 1.0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_loads_empty() {
        assert!(load(Path::new("/nonexistent/history.jsonl")).is_empty());
    }
}
