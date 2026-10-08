//! Finds the OAuth sign-in that Claude Code already stored on this computer.
//!
//! Headroom never asks for a password and never writes credentials: it borrows
//! Claude Code's access token read-only. Claude Code refreshes that token itself
//! whenever it runs; if it has expired, we nudge the `claude` CLI to refresh it.

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Token {
    /// Whose sign-in this is: "claude_code" or "hermes".
    pub source: &'static str,
    pub access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub plan: Option<String>,
    pub tier: Option<String>,
}

impl Token {
    /// Treat tokens that expire within a minute as already expired.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .map(|t| t <= now + chrono::Duration::seconds(60))
            .unwrap_or(false)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CredError {
    #[error("Claude Code isn't signed in on this computer")]
    NotFound,
    #[error("Couldn't read Claude Code's sign-in: {0}")]
    Unreadable(String),
}

#[derive(Deserialize)]
struct Stored {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<StoredOauth>,
}

#[derive(Deserialize)]
struct StoredOauth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
}

/// Parses the JSON blob Claude Code stores (same shape in the keychain and on disk).
pub fn parse(json: &str) -> Result<Token, CredError> {
    let stored: Stored =
        serde_json::from_str(json).map_err(|e| CredError::Unreadable(e.to_string()))?;
    let o = stored.oauth.ok_or(CredError::NotFound)?;
    Ok(Token {
        source: "claude_code",
        access_token: o.access_token,
        expires_at: o
            .expires_at
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single()),
        plan: o.subscription_type,
        tier: o.rate_limit_tier,
    })
}

/// Hermes Agent's Claude sign-in, for people who use Claude through Hermes
/// without Claude Code. Read-only from `$HERMES_HOME/auth.json`; Hermes renews
/// it itself whenever it calls Claude. Picks the OAuth credential that stays
/// valid longest.
pub fn load_hermes() -> Result<Token, CredError> {
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".hermes")))
        .ok_or(CredError::NotFound)?;
    let json = std::fs::read_to_string(home.join("auth.json")).map_err(|_| CredError::NotFound)?;
    parse_hermes(&json)
}

#[derive(Deserialize)]
struct HermesAuth {
    #[serde(default)]
    credential_pool: std::collections::HashMap<String, Vec<HermesCred>>,
}

#[derive(Deserialize)]
struct HermesCred {
    auth_type: Option<String>,
    access_token: Option<String>,
    expires_at_ms: Option<i64>,
}

pub fn parse_hermes(json: &str) -> Result<Token, CredError> {
    let auth: HermesAuth =
        serde_json::from_str(json).map_err(|e| CredError::Unreadable(e.to_string()))?;
    auth.credential_pool
        .get("anthropic")
        .into_iter()
        .flatten()
        .filter(|c| c.auth_type.as_deref() == Some("oauth"))
        .filter_map(|c| Some((c.access_token.clone()?, c.expires_at_ms)))
        .filter(|(t, _)| !t.is_empty())
        .max_by_key(|(_, exp)| exp.unwrap_or(0))
        .map(|(access_token, exp)| Token {
            source: "hermes",
            access_token,
            expires_at: exp.and_then(|ms| Utc.timestamp_millis_opt(ms).single()),
            plan: None, // Hermes doesn't record the plan; the widget just says "Claude"
            tier: None,
        })
        .ok_or(CredError::NotFound)
}

/// Claude Code's config dir: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}

pub fn load() -> Result<Token, CredError> {
    // macOS keeps it in the login keychain; everything else uses a JSON file.
    #[cfg(target_os = "macos")]
    if let Some(json) = read_keychain() {
        return parse(&json);
    }
    let path = claude_dir()
        .map(|d| d.join(".credentials.json"))
        .ok_or(CredError::NotFound)?;
    let json = std::fs::read_to_string(&path).map_err(|_| CredError::NotFound)?;
    parse(&json)
}

#[cfg(target_os = "macos")]
fn read_keychain() -> Option<String> {
    let out = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Desktop apps don't inherit the shell's PATH, so look in the usual install spots too.
fn find_claude_cli() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let exe = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).collect())
        .unwrap_or_default();
    candidates.extend([
        home.join(".local/bin").join(exe),
        home.join(".claude/local").join(exe),
        PathBuf::from("/opt/homebrew/bin").join(exe),
        PathBuf::from("/usr/local/bin").join(exe),
        home.join("AppData/Roaming/npm/claude.cmd"),
    ]);
    candidates.into_iter().find(|p| p.is_file())
}

/// How Claude Code on this computer is signed in, from `claude auth status --json`.
#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthStatus {
    pub logged_in: bool,
    /// "claude.ai" (subscription, including SSO) or an API-key / console method
    pub auth_method: String,
    /// "firstParty", or a cloud provider such as Bedrock / Vertex
    pub api_provider: String,
}

pub fn auth_status() -> Option<AuthStatus> {
    let out = claude_command(&["auth", "status", "--json"])?;
    serde_json::from_slice(&out).ok()
}

fn claude_command(args: &[&str]) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::process::Stdio;
    let mut cmd = Command::new(find_claude_cli()?);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash
    }
    let mut child = cmd.spawn().ok()?;
    // Never let a stuck CLI stall the refresh loop.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = Vec::new();
                child.stdout.take()?.read_to_end(&mut out).ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(200))
            }
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// Gets Claude Code to refresh an expired sign-in. `claude -p /usage` runs
/// Claude Code's own usage lookup (no AI request, so it costs nothing), which
/// refreshes and saves the token as a side effect. (`claude auth status` does
/// not refresh; checked 2026-10-08.) Returns true if the CLI ran.
pub fn nudge_refresh() -> bool {
    claude_command(&["-p", "/usage"]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_code_blob() {
        let t = parse(r#"{"claudeAiOauth":{"accessToken":"abc","refreshToken":"r","expiresAt":1791499298688,"scopes":["user:profile"],"subscriptionType":"pro"}}"#).unwrap();
        assert_eq!(t.access_token, "abc");
        assert_eq!(t.plan.as_deref(), Some("pro"));
        assert_eq!(t.expires_at.unwrap().timestamp(), 1791499298);
    }

    #[test]
    fn reads_auth_status_json() {
        let s: AuthStatus = serde_json::from_str(
            r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"x"}"#,
        )
        .unwrap();
        assert_eq!(
            s,
            AuthStatus {
                logged_in: true,
                auth_method: "claude.ai".into(),
                api_provider: "firstParty".into()
            }
        );
    }

    #[test]
    fn reads_hermes_oauth_picking_the_longest_lived() {
        let json = r#"{"credential_pool":{
            "nous":[{"auth_type":"oauth","access_token":"nope","expires_at_ms":9999999999999}],
            "anthropic":[
              {"auth_type":"oauth","access_token":"old","expires_at_ms":1791400000000},
              {"auth_type":"api_key","access_token":"key"},
              {"auth_type":"oauth","access_token":"new","expires_at_ms":1791500000000},
              {"auth_type":"oauth","source":"claude_code"}]}}"#;
        let t = parse_hermes(json).unwrap();
        assert_eq!((t.source, t.access_token.as_str()), ("hermes", "new"));
        assert!(matches!(
            parse_hermes(r#"{"credential_pool":{}}"#),
            Err(CredError::NotFound)
        ));
    }

    #[test]
    fn missing_oauth_is_not_found() {
        assert!(matches!(parse(r#"{"other":1}"#), Err(CredError::NotFound)));
    }

    #[test]
    fn expiry_has_a_minute_of_margin() {
        let now = Utc::now();
        let t = |secs| Token {
            source: "claude_code",
            access_token: String::new(),
            expires_at: Some(now + chrono::Duration::seconds(secs)),
            plan: None,
            tier: None,
        };
        assert!(t(30).is_expired(now));
        assert!(!t(600).is_expired(now));
    }
}
