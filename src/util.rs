use std::path::PathBuf;

use anyhow::{Context as _, Result};
use jiff::Timestamp;
use secrecy::SecretString;
use serde::Deserialize;

pub fn format_reset_time(resets_at: &Timestamp) -> String {
  let now = Timestamp::now();
  let diff = resets_at.as_second() - now.as_second();

  if diff <= 0 {
    return "now".to_string();
  }

  let days = diff / 86400;
  let hours = (diff % 86400) / 3600;
  let mins = (diff % 3600) / 60;

  if days > 0 {
    return format!("{}d {}h", days, hours);
  }

  if hours > 0 {
    return format!("{}h {}m", hours, mins);
  }

  return format!("{}m", mins);
}

/// Re-reads the token from the platform credential store.
/// Called by the API client on 401 to handle token rotation.
#[cfg(target_os = "linux")]
pub fn refresh_token() -> Result<SecretString> {
  return fetch_credentials_file();
}

#[cfg(target_os = "macos")]
pub fn refresh_token() -> Result<SecretString> {
  return crate::platform::macos::util::fetch_keychain_token();
}

#[cfg(target_os = "linux")]
pub fn get_claude_token() -> Result<SecretString> {
  if let Ok(token) = std::env::var("LIMENT_TOKEN") {
    return Ok(SecretString::from(token));
  }

  return fetch_credentials_file()
    .context("Failed to read token. Set LIMENT_TOKEN or sign in to Claude Code.");
}

#[cfg(target_os = "linux")]
fn fetch_credentials_file() -> Result<SecretString> {
  let home = std::env::var("HOME").context("HOME environment variable not set")?;
  let path = PathBuf::from(home).join(".claude").join(".credentials.json");
  let content =
    std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;

  #[derive(Deserialize)]
  struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
  }

  #[derive(Deserialize)]
  struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    claude_oauth: ClaudeOAuth,
  }

  let creds: ClaudeCredentials = serde_json::from_str(&content)?;
  return Ok(SecretString::from(creds.claude_oauth.access_token));
}
