use anyhow::{Context as _, Result};
use jiff::Timestamp;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertStyle, NSView};
use objc2_foundation::NSString;
use secrecy::SecretString;
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use serde::Deserialize;

pub trait NSViewExt {
  #[expect(non_snake_case)]
  fn noAutoresize(&self);
}

impl NSViewExt for NSView {
  fn noAutoresize(&self) {
    return self.setTranslatesAutoresizingMaskIntoConstraints(false);
  }
}

pub trait SearchResultExt {
  fn into_data(self) -> Option<Vec<u8>>;
}

impl SearchResultExt for SearchResult {
  fn into_data(self) -> Option<Vec<u8>> {
    match self {
      SearchResult::Data(d) => Some(d),
      _ => None,
    }
  }
}

pub fn get_claude_token(mtm: MainThreadMarker) -> Result<SecretString> {
  // Use environment variable set string if available.
  if let Ok(token) = std::env::var("ANTHROPIC_LIMITS_TRAY_TOKEN") {
    return Ok(SecretString::from(token));
  }

  // Otherwise just attempt to fetch from keychain.
  return fetch_keychain_token().map_err(|e| {
    let error = "Failed to retrieve access token.";
    let hint = "You've haven't signed in to Claude Code yet.";

    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Missing Access Token"));
    alert.setInformativeText(&NSString::from_str(&format!("{} {}", error, hint)));
    alert.setAlertStyle(NSAlertStyle::Critical);
    alert.runModal();

    return e.context(error);
  });
}

pub fn fetch_keychain_token() -> Result<SecretString> {
  let results = ItemSearchOptions::new()
    .class(ItemClass::generic_password())
    .service("Claude Code-credentials")
    .load_data(true)
    .search()?;

  let data = results
    .into_iter()
    .find_map(|result| result.into_data())
    .context("Failed to find required data in keychain.")?;

  #[derive(Deserialize)]
  struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
  }

  #[derive(Deserialize)]
  struct ClaudeKeychain {
    #[serde(rename = "claudeAiOauth")]
    claude_oauth: ClaudeOAuth,
  }

  let json_str = String::from_utf8(data)?;
  let value: ClaudeKeychain = serde_json::from_str(&json_str)?;
  return Ok(SecretString::from(value.claude_oauth.access_token));
}

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
