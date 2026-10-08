//! Fetches plan limits from Anthropic's usage endpoint: the same (undocumented)
//! endpoint behind Claude's Settings → Usage page and Claude Code's `/usage`.
//!
//! The response lists every limit generically in `limits[]`, so new limits
//! (e.g. a per-model weekly cap) show up in Headroom without an update.

use super::credentials::Token;
use crate::model::{Breakdown, Limit, Snapshot};
use chrono::{DateTime, Utc};
use serde::Deserialize;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("signed out")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("{0}")]
    Other(String),
}

#[derive(Deserialize, Default)]
struct Raw {
    #[serde(default)]
    limits: Vec<RawLimit>,
    five_hour: Option<RawWindow>,
    seven_day: Option<RawWindow>,
    seven_day_opus: Option<RawWindow>,
    seven_day_sonnet: Option<RawWindow>,
    seven_day_breakdown: Option<RawBreakdown>,
    extra_usage: Option<RawExtra>,
}

#[derive(Deserialize)]
struct RawLimit {
    kind: String,
    group: Option<String>,
    percent: f64,
    severity: Option<String>,
    resets_at: Option<DateTime<Utc>>,
    scope: Option<serde_json::Value>,
    #[serde(default)]
    is_active: bool,
}

#[derive(Deserialize)]
struct RawWindow {
    utilization: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct RawBreakdown {
    window_started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    rows: Vec<RawRow>,
}

#[derive(Deserialize)]
struct RawRow {
    key: String,
    display_name: Option<String>,
    percent: f64,
}

#[derive(Deserialize)]
struct RawExtra {
    #[serde(default)]
    is_enabled: bool,
}

const HOUR: i64 = 3600;

/// Human label + window length for each known limit kind.
fn describe(kind: &str, scope: Option<&serde_json::Value>) -> (String, Option<i64>) {
    let scoped = scope
        .and_then(|s| s.as_str().map(str::to_string).or_else(|| s.get("model").and_then(|m| m.as_str()).map(str::to_string)))
        .map(|s| title_case(&s));
    match kind {
        "session" => ("Current session".into(), Some(5 * HOUR)),
        "weekly_all" => ("Weekly · all models".into(), Some(7 * 24 * HOUR)),
        k if k.starts_with("weekly_") => {
            let who = scoped.unwrap_or_else(|| title_case(&k["weekly_".len()..]));
            (format!("Weekly · {who}"), Some(7 * 24 * HOUR))
        }
        k => (scoped.map(|s| format!("{} · {s}", title_case(k))).unwrap_or_else(|| title_case(k)), None),
    }
}

fn title_case(s: &str) -> String {
    s.split(|c| c == '_' || c == '-' || c == ' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turns the raw response into Headroom's provider-neutral snapshot.
pub fn parse(json: &str, plan: Option<String>, now: DateTime<Utc>) -> Result<Snapshot, FetchError> {
    let raw: Raw = serde_json::from_str(json).map_err(|e| FetchError::Other(format!("unexpected response: {e}")))?;

    let mut limits: Vec<Limit> = raw
        .limits
        .iter()
        .map(|l| {
            let (label, window_secs) = describe(&l.kind, l.scope.as_ref());
            Limit {
                id: l.kind.clone(),
                label,
                group: l.group.clone().unwrap_or_else(|| "other".into()),
                percent: l.percent,
                resets_at: l.resets_at,
                window_secs,
                severity: l.severity.clone().unwrap_or_else(|| "normal".into()),
                active: l.is_active,
            }
        })
        .collect();

    // Older responses only had the named windows; keep working with those.
    if limits.is_empty() {
        let named = [
            ("session", "session", &raw.five_hour),
            ("weekly_all", "weekly", &raw.seven_day),
            ("weekly_opus", "weekly", &raw.seven_day_opus),
            ("weekly_sonnet", "weekly", &raw.seven_day_sonnet),
        ];
        for (kind, group, w) in named {
            if let Some(RawWindow { utilization: Some(p), resets_at }) = w {
                let (label, window_secs) = describe(kind, None);
                limits.push(Limit { id: kind.into(), label, group: group.into(), percent: *p, resets_at: *resets_at, window_secs, severity: "normal".into(), active: false });
            }
        }
    }

    // Session first, then weekly limits, then anything new.
    let rank = |g: &str| match g { "session" => 0, "weekly" => 1, _ => 2 };
    limits.sort_by_key(|l| rank(&l.group));

    let (breakdown, breakdown_since) = raw
        .seven_day_breakdown
        .map(|b| {
            let rows = b
                .rows
                .into_iter()
                .map(|r| Breakdown { label: r.display_name.unwrap_or_else(|| title_case(&r.key)), key: r.key, percent: r.percent })
                .collect();
            (rows, b.window_started_at)
        })
        .unwrap_or_default();

    Ok(Snapshot {
        provider: "claude".into(),
        plan,
        limits,
        breakdown,
        breakdown_since,
        extra_usage: raw.extra_usage.map(|e| e.is_enabled).unwrap_or(false),
        fetched_at: now,
    })
}

pub async fn fetch(client: &reqwest::Client, token: &Token) -> Result<Snapshot, FetchError> {
    let res = client
        .get(USAGE_URL)
        .bearer_auth(&token.access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| FetchError::Other(if e.is_connect() || e.is_timeout() { "Can't reach Anthropic (offline?)".into() } else { e.to_string() }))?;
    match res.status().as_u16() {
        200 => {}
        401 | 403 => return Err(FetchError::Unauthorized),
        429 => return Err(FetchError::RateLimited),
        s => return Err(FetchError::Other(format!("Anthropic returned HTTP {s}"))),
    }
    let body = res.text().await.map_err(|e| FetchError::Other(e.to_string()))?;
    parse(&body, token.plan.clone(), Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/usage_2026-10.json");

    #[test]
    fn parses_real_response() {
        let s = parse(FIXTURE, Some("pro".into()), Utc::now()).unwrap();
        assert_eq!(s.limits.len(), 2);
        assert_eq!(s.limits[0].label, "Current session");
        assert_eq!(s.limits[0].percent, 7.0);
        assert_eq!(s.limits[0].window_secs, Some(5 * 3600));
        assert_eq!(s.limits[1].label, "Weekly · all models");
        assert_eq!(s.limits[1].percent, 30.0);
        assert!(s.limits[1].resets_at.is_some());
        assert_eq!(s.breakdown[0].key, "claude_code");
        assert_eq!(s.breakdown[0].percent, 100.0);
        assert!(s.breakdown_since.is_some());
    }

    #[test]
    fn falls_back_to_named_windows() {
        let json = r#"{"five_hour":{"utilization":42.0,"resets_at":"2026-10-08T21:49:59Z"},"seven_day":{"utilization":7.5,"resets_at":null},"seven_day_opus":null}"#;
        let s = parse(json, None, Utc::now()).unwrap();
        let got: Vec<_> = s.limits.iter().map(|l| (l.id.as_str(), l.percent)).collect();
        assert_eq!(got, vec![("session", 42.0), ("weekly_all", 7.5)]);
    }

    #[test]
    fn unknown_limits_still_show_up_sorted_after_known_ones() {
        let json = r#"{"limits":[
            {"kind":"mystery_meter","group":"other","percent":3,"resets_at":null},
            {"kind":"weekly_opus","group":"weekly","percent":60,"resets_at":null},
            {"kind":"session","group":"session","percent":10,"resets_at":null}]}"#;
        let s = parse(json, None, Utc::now()).unwrap();
        let labels: Vec<_> = s.limits.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(labels, vec!["Current session", "Weekly · Opus", "Mystery Meter"]);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(parse("<html>", None, Utc::now()).is_err());
    }
}
