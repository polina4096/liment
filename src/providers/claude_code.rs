use std::sync::Mutex;

use anyhow::{Context as _, Result};
use secrecy::{ExposeSecret, SecretString};
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use serde::Deserialize;

use super::{UsageData, UsageProvider};
use crate::utils::claude_api::{ProfileResponse, UsageResponse};

pub struct ClaudeCodeProvider {
  token: Mutex<SecretString>,
}

impl ClaudeCodeProvider {
  pub fn new() -> Result<Self> {
    let token = Self::fetch_token()?;
    return Ok(Self { token: Mutex::new(token) });
  }

  fn fetch_token() -> Result<SecretString> {
    if let Ok(token) = std::env::var("LIMENT_CLAUDE_CODE_TOKEN") {
      return Ok(SecretString::from(token));
    }
    Self::fetch_keychain_token()
  }

  fn fetch_keychain_token() -> Result<SecretString> {
    let results = ItemSearchOptions::new()
      .class(ItemClass::generic_password())
      .service("Claude Code-credentials")
      .load_data(true)
      .search()?;

    let data = results
      .into_iter()
      .find_map(|r| match r {
        SearchResult::Data(d) => Some(d),
        _ => None,
      })
      .context("Failed to find Claude Code credentials in keychain")?;

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

  fn fetch_usage(&self) -> Option<UsageResponse> {
    let body = self.get("https://api.anthropic.com/api/oauth/usage")?;
    serde_json::from_str(&body).ok()
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    let body = self.get("https://api.anthropic.com/api/oauth/profile")?;
    serde_json::from_str(&body).ok()
  }

  fn get(&self, url: &str) -> Option<String> {
    let result = self.get_inner(url);

    if let Err(ureq::Error::StatusCode(401)) = &result {
      if let Ok(new_token) = Self::fetch_keychain_token() {
        *self.token.lock().unwrap() = new_token;
        return self.get_inner(url).inspect_err(|e| eprintln!("Error: {}", e)).ok();
      }
    }

    return result.ok();
  }

  fn get_inner(&self, url: &str) -> Result<String, ureq::Error> {
    let token = self.token.lock().unwrap();
    let mut response = ureq::get(url)
      .header("Authorization", &format!("Bearer {}", token.expose_secret()))
      .header("anthropic-beta", "oauth-2025-04-20")
      .header("Content-Type", "application/json")
      .call()?;

    return response.body_mut().read_to_string();
  }
}

impl UsageProvider for ClaudeCodeProvider {
  fn fetch_data(&self) -> Option<UsageData> {
    let usage = self.fetch_usage()?;
    let profile = self.fetch_profile();
    return Some(crate::utils::claude_api::into_usage_data(usage, profile));
  }

  fn placeholder_lines(&self) -> [&str; 2] {
    ["5h", "7d"]
  }
}
