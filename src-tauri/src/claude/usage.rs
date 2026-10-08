//! Fetches plan limits from Anthropic's usage endpoint: the same (undocumented)
//! endpoint behind Claude's Settings → Usage page and Claude Code's `/usage`.
//!
//! The response lists every limit generically in `limits[]`, so new limits
//! (e.g. a per-model weekly cap) show up in Headroom without an update.
//! Team / Enterprise accounts (including SSO sign-ins) use the same endpoint;
//! usage-based Enterprise seats report a monthly spend instead of windows.

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
    spend: Option<RawSpend>,
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
    limit_dollars: Option<f64>,
    used_dollars: Option<f64>,
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

/// Monthly spend: usage credits on Pro/Max/Team, or the whole bill on
/// usage-based Enterprise seats.
#[derive(Deserialize)]
struct RawSpend {
    used: Option<Money>,
    limit: Option<Money>,
    percent: Option<f64>,
    severity: Option<String>,
    #[serde(default)]
    enabled: bool,
}

#[derive(Deserialize)]
struct Money {
    amount_minor: i64,
    currency: Option<String>,
    #[serde(default = "two")]
    exponent: u32,
}

fn two() -> u32 {
    2
}

impl Money {
    fn value(&self) -> f64 {
        self.amount_minor as f64 / 10f64.powi(self.exponent as i32)
    }
    fn show(&self) -> String {
        money(self.value(), self.currency.as_deref().unwrap_or("USD"))
    }
}

fn money(v: f64, currency: &str) -> String {
    match currency {
        "USD" => format!("${v:.2}"),
        "EUR" => format!("€{v:.2}"),
        "GBP" => format!("£{v:.2}"),
        c => format!("{v:.2} {c}"),
    }
}

const HOUR: i64 = 3600;

/// Human label + window length for each known limit kind.
fn describe(kind: &str, scope: Option<&serde_json::Value>) -> (String, Option<i64>) {
    let scoped = scope
        .and_then(|s| {
            s.as_str()
                .map(str::to_string)
                .or_else(|| s.get("model").and_then(|m| m.as_str()).map(str::to_string))
        })
        .map(|s| title_case(&s));
    match kind {
        "session" => ("Current session".into(), Some(5 * HOUR)),
        "weekly_all" => ("Weekly · all models".into(), Some(7 * 24 * HOUR)),
        k if k.starts_with("weekly_") => {
            let who = scoped.unwrap_or_else(|| title_case(&k["weekly_".len()..]));
            (format!("Weekly · {who}"), Some(7 * 24 * HOUR))
        }
        k => (
            scoped
                .map(|s| format!("{} · {s}", title_case(k)))
                .unwrap_or_else(|| title_case(k)),
            None,
        ),
    }
}

fn title_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Raw {
    /// The named window object that matches a `limits[]` kind (it carries dollar figures).
    fn window(&self, kind: &str) -> Option<&RawWindow> {
        match kind {
            "session" => self.five_hour.as_ref(),
            "weekly_all" => self.seven_day.as_ref(),
            "weekly_opus" => self.seven_day_opus.as_ref(),
            "weekly_sonnet" => self.seven_day_sonnet.as_ref(),
            _ => None,
        }
    }
}

/// "$3.10 of $10.00" when a window is metered in dollars (some Team/Enterprise seats).
fn dollar_detail(w: Option<&RawWindow>) -> Option<String> {
    let w = w?;
    Some(format!(
        "{} of {}",
        money(w.used_dollars?, "USD"),
        money(w.limit_dollars?, "USD")
    ))
}

fn spend_limit(s: &RawSpend) -> Option<Limit> {
    let used = s.used.as_ref().map(Money::value).unwrap_or(0.0);
    // Hide it entirely for plans that don't use credits and haven't spent anything.
    if !s.enabled && s.limit.is_none() && used == 0.0 {
        return None;
    }
    let used_text = s
        .used
        .as_ref()
        .map(Money::show)
        .unwrap_or_else(|| money(0.0, "USD"));
    let (percent, detail, capped) = match &s.limit {
        Some(limit) if limit.value() > 0.0 => (
            s.percent.unwrap_or(used / limit.value() * 100.0),
            format!("{used_text} of {}", limit.show()),
            true,
        ),
        _ => (0.0, format!("{used_text} · no limit"), false),
    };
    Some(Limit {
        id: "spend".into(),
        label: "Spend this month".into(),
        group: "spend".into(),
        percent,
        resets_at: None,
        window_secs: None,
        severity: s.severity.clone().unwrap_or_else(|| "normal".into()),
        active: false,
        detail: Some(detail),
        capped,
    })
}

/// Turns the raw response into Headroom's provider-neutral snapshot.
pub fn parse(
    json: &str,
    plan: Option<String>,
    tier: Option<String>,
    now: DateTime<Utc>,
) -> Result<Snapshot, FetchError> {
    let raw: Raw = serde_json::from_str(json)
        .map_err(|e| FetchError::Other(format!("unexpected response: {e}")))?;

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
                detail: dollar_detail(raw.window(&l.kind)),
                capped: true,
            }
        })
        .collect();

    // Older responses only had the named windows; keep working with those.
    if limits.is_empty() {
        for (kind, group) in [
            ("session", "session"),
            ("weekly_all", "weekly"),
            ("weekly_opus", "weekly"),
            ("weekly_sonnet", "weekly"),
        ] {
            let Some(w) = raw.window(kind) else { continue };
            let Some(p) = w.utilization else { continue };
            let (label, window_secs) = describe(kind, None);
            limits.push(Limit {
                id: kind.into(),
                label,
                group: group.into(),
                percent: p,
                resets_at: w.resets_at,
                window_secs,
                severity: "normal".into(),
                active: false,
                detail: dollar_detail(Some(w)),
                capped: true,
            });
        }
    }

    if let Some(spend) = raw.spend.as_ref().and_then(spend_limit) {
        limits.push(spend);
    }

    // Session first, then weekly limits, then spend, then anything new.
    let rank = |g: &str| match g {
        "session" => 0,
        "weekly" => 1,
        "spend" => 2,
        _ => 3,
    };
    limits.sort_by_key(|l| rank(&l.group));

    let (breakdown, breakdown_since) = raw
        .seven_day_breakdown
        .map(|b| {
            let rows = b
                .rows
                .into_iter()
                .map(|r| Breakdown {
                    label: r.display_name.unwrap_or_else(|| title_case(&r.key)),
                    key: r.key,
                    percent: r.percent,
                })
                .collect();
            (rows, b.window_started_at)
        })
        .unwrap_or_default();

    Ok(Snapshot {
        provider: "claude".into(),
        plan,
        tier,
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
        .map_err(|e| {
            FetchError::Other(if e.is_connect() || e.is_timeout() {
                "Can't reach Anthropic (offline?)".into()
            } else {
                e.to_string()
            })
        })?;
    match res.status().as_u16() {
        200 => {}
        401 | 403 => return Err(FetchError::Unauthorized),
        429 => return Err(FetchError::RateLimited),
        s => return Err(FetchError::Other(format!("Anthropic returned HTTP {s}"))),
    }
    let body = res
        .text()
        .await
        .map_err(|e| FetchError::Other(e.to_string()))?;
    parse(&body, token.plan.clone(), token.tier.clone(), Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/usage_2026-10.json");

    fn p(json: &str) -> Snapshot {
        parse(json, None, None, Utc::now()).unwrap()
    }

    #[test]
    fn parses_real_response() {
        let s = parse(FIXTURE, Some("pro".into()), None, Utc::now()).unwrap();
        // Spend is off and $0 for this Pro account, so it stays hidden.
        assert_eq!(s.limits.len(), 2);
        assert_eq!(s.limits[0].label, "Current session");
        assert_eq!(s.limits[0].percent, 7.0);
        assert_eq!(s.limits[0].window_secs, Some(5 * 3600));
        assert_eq!(s.limits[0].detail, None);
        assert_eq!(s.limits[1].label, "Weekly · all models");
        assert_eq!(s.limits[1].percent, 30.0);
        assert!(s.limits[1].resets_at.is_some());
        assert_eq!(s.breakdown[0].key, "claude_code");
        assert_eq!(s.breakdown[0].percent, 100.0);
        assert!(s.breakdown_since.is_some());
    }

    #[test]
    fn falls_back_to_named_windows() {
        let s = p(
            r#"{"five_hour":{"utilization":42.0,"resets_at":"2026-10-08T21:49:59Z"},"seven_day":{"utilization":7.5,"resets_at":null},"seven_day_opus":null}"#,
        );
        let got: Vec<_> = s
            .limits
            .iter()
            .map(|l| (l.id.as_str(), l.percent))
            .collect();
        assert_eq!(got, vec![("session", 42.0), ("weekly_all", 7.5)]);
    }

    #[test]
    fn unknown_limits_still_show_up_sorted_after_known_ones() {
        let s = p(r#"{"limits":[
            {"kind":"mystery_meter","group":"other","percent":3,"resets_at":null},
            {"kind":"weekly_opus","group":"weekly","percent":60,"resets_at":null},
            {"kind":"session","group":"session","percent":10,"resets_at":null}]}"#);
        let labels: Vec<_> = s.limits.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Current session", "Weekly · Opus", "Mystery Meter"]
        );
    }

    #[test]
    fn usage_based_enterprise_shows_monthly_spend_against_its_cap() {
        let s = p(
            r#"{"limits":[],"spend":{"used":{"amount_minor":1240,"currency":"USD","exponent":2},
            "limit":{"amount_minor":10000,"currency":"USD","exponent":2},"percent":12.4,"severity":"normal","enabled":true}}"#,
        );
        assert_eq!(s.limits.len(), 1);
        let l = &s.limits[0];
        assert_eq!(
            (l.label.as_str(), l.percent, l.capped),
            ("Spend this month", 12.4, true)
        );
        assert_eq!(l.detail.as_deref(), Some("$12.40 of $100.00"));
    }

    #[test]
    fn spend_without_a_cap_shows_the_amount_only() {
        let s = p(
            r#"{"spend":{"used":{"amount_minor":530,"currency":"USD","exponent":2},"limit":null,"enabled":true}}"#,
        );
        assert_eq!(s.limits[0].detail.as_deref(), Some("$5.30 · no limit"));
        assert!(!s.limits[0].capped);
    }

    #[test]
    fn dollar_metered_windows_get_a_detail_line() {
        let s = p(
            r#"{"limits":[{"kind":"session","group":"session","percent":31,"resets_at":null}],
            "five_hour":{"utilization":31,"resets_at":null,"limit_dollars":10.0,"used_dollars":3.1}}"#,
        );
        assert_eq!(s.limits[0].detail.as_deref(), Some("$3.10 of $10.00"));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(parse("<html>", None, None, Utc::now()).is_err());
    }
}
