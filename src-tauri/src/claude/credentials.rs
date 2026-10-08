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
    pub access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub plan: Option<String>,
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
}

/// Parses the JSON blob Claude Code stores (same shape in the keychain and on disk).
pub fn parse(json: &str) -> Result<Token, CredError> {
    let stored: Stored =
        serde_json::from_str(json).map_err(|e| CredError::Unreadable(e.to_string()))?;
    let o = stored.oauth.ok_or(CredError::NotFound)?;
    Ok(Token {
        access_token: o.access_token,
        expires_at: o.expires_at.and_then(|ms| Utc.timestamp_millis_opt(ms).single()),
        plan: o.subscription_type,
    })
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
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
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
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
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

/// Asks the Claude Code CLI to check its sign-in, which refreshes an expired
/// token as a side effect. Costs no usage. Returns true if the CLI ran.
pub fn nudge_refresh() -> bool {
    let Some(cli) = find_claude_cli() else { return false };
    let mut cmd = Command::new(cli);
    cmd.args(["auth", "status"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash
    }
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
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
    fn missing_oauth_is_not_found() {
        assert!(matches!(parse(r#"{"other":1}"#), Err(CredError::NotFound)));
    }

    #[test]
    fn expiry_has_a_minute_of_margin() {
        let now = Utc::now();
        let t = |secs| Token { access_token: String::new(), expires_at: Some(now + chrono::Duration::seconds(secs)), plan: None };
        assert!(t(30).is_expired(now));
        assert!(!t(600).is_expired(now));
    }
}
