//! The background loop: fetch usage, update the menu bar, alert on thresholds,
//! and push the new state to every open window.

use crate::claude::{credentials, usage};
use crate::model::{Snapshot, UsageState};
use crate::settings::Settings;
use chrono::Utc;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

pub struct AppState {
    pub usage: Mutex<UsageState>,
    pub settings: Mutex<Settings>,
    pub settings_path: std::path::PathBuf,
    /// Poked by "Refresh now" / settings changes to skip the wait.
    pub wake: Notify,
    /// (limit id, threshold, reset time) already alerted, so each fires once per window.
    pub alerted: Mutex<HashSet<String>>,
    pub client: reqwest::Client,
}

const SIGNED_OUT: &str = "Sign in to Claude Code on this computer with your Claude account: run `claude auth login` (add `--sso` for company single sign-on). Headroom connects automatically.";

/// Why there's no Claude sign-in to borrow: `(status, message)`.
/// API-key and cloud-provider setups are fine, they just have no plan limits.
pub fn explain_missing(auth: Option<credentials::AuthStatus>) -> (&'static str, String) {
    let Some(a) = auth else {
        return ("signed_out", SIGNED_OUT.into());
    };
    if a.logged_in && !a.api_provider.is_empty() && a.api_provider != "firstParty" {
        return (
            "no_plan",
            format!("Claude Code here runs through {}, so there are no Claude plan limits to show. The Models tab still works.", a.api_provider),
        );
    }
    if a.logged_in && a.auth_method != "claude.ai" {
        return (
            "no_plan",
            "Claude Code is signed in with an API key, which is billed per use and has no plan limits. The Models tab still works. To track a Claude plan instead, run `claude auth login` (add `--sso` for company sign-in).".into(),
        );
    }
    ("signed_out", SIGNED_OUT.into())
}
const EXPIRED: &str =
    "Your Claude sign-in expired. Open Claude Code once and Headroom will reconnect.";

async fn load_token(force_refresh: bool) -> Result<credentials::Token, (&'static str, String)> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut token = credentials::load().map_err(|e| match e {
            credentials::CredError::NotFound => explain_missing(credentials::auth_status()),
            e => ("signed_out", e.to_string()),
        })?;
        if force_refresh || token.is_expired(Utc::now()) {
            // Let Claude Code refresh its own token, then read it again.
            if credentials::nudge_refresh() {
                token = credentials::load().map_err(|e| ("signed_out", e.to_string()))?;
            }
            if token.is_expired(Utc::now()) {
                return Err(("signed_out", EXPIRED.to_string()));
            }
        }
        Ok(token)
    })
    .await
    .map_err(|e| ("error", e.to_string()))?
}

async fn check(state: &AppState) -> UsageState {
    let previous = state.usage.lock().unwrap().snapshot.clone();
    let now = Some(Utc::now());
    let signed_out = |(status, msg): (&str, String)| UsageState {
        snapshot: None,
        status: status.into(),
        message: Some(msg),
        checked_at: now,
    };
    let stale = |msg: String| UsageState {
        snapshot: previous.clone(),
        status: "error".into(),
        message: Some(msg),
        checked_at: now,
    };

    let token = match load_token(false).await {
        Ok(t) => t,
        Err(msg) => return signed_out(msg),
    };
    let mut result = usage::fetch(&state.client, &token).await;
    if matches!(result, Err(usage::FetchError::Unauthorized)) {
        // Token revoked or rotated under us: refresh once and retry.
        match load_token(true).await {
            Ok(t) => result = usage::fetch(&state.client, &t).await,
            Err(msg) => return signed_out(msg),
        }
    }
    match result {
        Ok(snapshot) => UsageState {
            snapshot: Some(snapshot),
            status: "ok".into(),
            message: None,
            checked_at: now,
        },
        Err(usage::FetchError::Unauthorized) => signed_out(("signed_out", EXPIRED.into())),
        Err(usage::FetchError::RateLimited) => {
            stale("Anthropic asked us to slow down. Showing the last numbers.".into())
        }
        Err(usage::FetchError::Other(msg)) => stale(msg),
    }
}

pub fn tray_title(snapshot: Option<&Snapshot>, mode: &str) -> Option<String> {
    let s = snapshot?;
    let pct = |group: &str| {
        s.limits
            .iter()
            .find(|l| l.group == group)
            .map(|l| format!("{:.0}%", l.percent))
    };
    match mode {
        "session" => pct("session"),
        "weekly" => pct("weekly"),
        "both" => match (pct("session"), pct("weekly")) {
            (Some(a), Some(b)) => Some(format!("{a} · {b}")),
            (a, b) => a.or(b),
        },
        _ => None,
    }
}

fn update_tray(app: &AppHandle, state: &UsageState, settings: &Settings) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let title = tray_title(state.snapshot.as_ref(), &settings.tray_text);
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(title.as_deref());
    let tip = match (&state.snapshot, state.status.as_str()) {
        (_, "signed_out") => "Headroom: not signed in".to_string(),
        (Some(s), _) => s
            .limits
            .iter()
            .map(|l| format!("{} {:.0}%", l.label, l.percent))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "Headroom".to_string(),
    };
    let _ = tray.set_tooltip(Some(tip));
}

fn alert_thresholds(app: &AppHandle, state: &AppState, snapshot: &Snapshot, settings: &Settings) {
    use tauri_plugin_notification::NotificationExt;
    if !settings.notify {
        return;
    }
    let mut alerted = state.alerted.lock().unwrap();
    for l in &snapshot.limits {
        // Highest threshold crossed only, so a jump from 70% to 99% sends one alert.
        let Some(&t) = settings
            .notify_at
            .iter()
            .rev()
            .find(|&&t| l.percent >= t as f64)
        else {
            continue;
        };
        let key = format!(
            "{}:{}:{:?}",
            l.id,
            t,
            l.resets_at.map(|r| r.timestamp() / 60)
        );
        if alerted.insert(key) {
            let when = l
                .resets_at
                .map(|r| {
                    format!(
                        " Resets {}.",
                        r.with_timezone(&chrono::Local).format("%a %-I:%M %p")
                    )
                })
                .unwrap_or_default();
            let _ = app
                .notification()
                .builder()
                .title(format!("{}: {:.0}% used", l.label, l.percent))
                .body(format!("Claude {}.{when}", l.label.to_lowercase()))
                .show();
        }
    }
}

pub async fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let next = check(&state).await;
    let settings = state.settings.lock().unwrap().clone();
    if let Some(s) = &next.snapshot {
        if next.status == "ok" {
            alert_thresholds(app, &state, s, &settings);
            crate::history::record(app, s);
        }
    }
    update_tray(app, &next, &settings);
    if std::env::var_os("HEADROOM_DEBUG").is_some() {
        let summary = next.snapshot.as_ref().map(|s| {
            s.limits
                .iter()
                .map(|l| format!("{} {:.0}%", l.id, l.percent))
                .collect::<Vec<_>>()
                .join(", ")
        });
        eprintln!(
            "[headroom] status={} limits={:?} message={:?}",
            next.status, summary, next.message
        );
    }
    *state.usage.lock().unwrap() = next.clone();
    let _ = app.emit("usage", next);
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app).await;
            let secs = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .refresh_secs;
            let state = app.state::<AppState>();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(secs)) => {}
                _ = state.wake.notified() => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Limit;

    fn snap(session: f64, weekly: f64) -> Snapshot {
        let l = |id: &str, group: &str, p| Limit {
            id: id.into(),
            label: id.into(),
            group: group.into(),
            percent: p,
            resets_at: None,
            window_secs: None,
            severity: "normal".into(),
            active: false,
            detail: None,
            capped: true,
        };
        Snapshot {
            provider: "claude".into(),
            plan: None,
            tier: None,
            limits: vec![
                l("session", "session", session),
                l("weekly_all", "weekly", weekly),
            ],
            breakdown: vec![],
            breakdown_since: None,
            extra_usage: false,
            fetched_at: Utc::now(),
        }
    }

    #[test]
    fn explains_why_there_are_no_plan_limits() {
        use credentials::AuthStatus;
        let st = |method: &str, provider: &str| {
            Some(AuthStatus {
                logged_in: true,
                auth_method: method.into(),
                api_provider: provider.into(),
            })
        };
        assert_eq!(explain_missing(None).0, "signed_out");
        assert_eq!(
            explain_missing(st("claude.ai", "firstParty")).0,
            "signed_out"
        );
        let (status, msg) = explain_missing(st("api_key", "firstParty"));
        assert_eq!(status, "no_plan");
        assert!(msg.contains("API key"));
        let (status, msg) = explain_missing(st("api_key", "bedrock"));
        assert_eq!(status, "no_plan");
        assert!(msg.contains("bedrock"));
        assert!(SIGNED_OUT.contains("--sso"));
    }

    #[test]
    fn tray_title_modes() {
        let s = snap(5.0, 30.4);
        assert_eq!(tray_title(Some(&s), "both").as_deref(), Some("5% · 30%"));
        assert_eq!(tray_title(Some(&s), "weekly").as_deref(), Some("30%"));
        assert_eq!(tray_title(Some(&s), "none"), None);
        assert_eq!(tray_title(None, "both"), None);
    }
}
