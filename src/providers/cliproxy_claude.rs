use std::collections::HashMap;

use anyhow::Result;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator as _;

use super::{DataProvider, UsageData};
use crate::providers::{
  TierInfo,
  claude_code::{ProfileResponse, SubscriptionTier, UsageResponse, into_usage_data},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CliproxyClaudeSettings {
  /// CLIProxy base URL (e.g. "http://localhost:8317").
  pub base_url: String,

  /// CLIProxy management API secret key.
  pub management_token: String,

  /// Auth index identifying which CLIProxy account to use.
  pub auth_index: String,
}

pub struct CliproxyClaudeProvider {
  base_url: String,
  management_token: SecretString,
  auth_index: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiCallRequest {
  auth_index: String,
  method: String,
  url: String,
  header: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ApiCallResponse {
  status_code: u16,
  body: String,
}

impl CliproxyClaudeProvider {
  pub fn new(settings: &CliproxyClaudeSettings) -> Result<Self> {
    log::info!("Initializing CLIProxy Claude provider");

    return Ok(Self {
      base_url: settings.base_url.trim_end_matches('/').to_string(),
      management_token: SecretString::from(settings.management_token.clone()),
      auth_index: settings.auth_index.clone(),
    });
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.api_get("https://api.anthropic.com/api/oauth/usage")?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse usage response: {}", e))
      .ok();
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    log::debug!("Fetching profile data");

    let body = self.api_get("https://api.anthropic.com/api/oauth/profile")?;

    return serde_json::from_str(&body)
      .inspect(|p: &ProfileResponse| log::debug!("Parsed profile: {:?}", p))
      .inspect_err(|e| log::warn!("Failed to parse profile response: {}", e))
      .ok();
  }

  fn api_get(&self, url: &str) -> Option<String> {
    log::debug!("Proxied GET {} via cliproxy", url);

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer $TOKEN$".to_string());
    headers.insert("Anthropic-Beta".to_string(), "oauth-2025-04-20".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let request = ApiCallRequest {
      auth_index: self.auth_index.clone(),
      method: "GET".to_string(),
      url: url.to_string(),
      header: headers,
    };

    let endpoint = format!("{}/v0/management/api-call", self.base_url);
    let json_body = serde_json::to_string(&request)
      .inspect_err(|e| log::error!("Failed to serialize api-call request: {}", e))
      .ok()?;

    let mut response = ureq::post(&endpoint)
      .header("Authorization", &format!("Bearer {}", self.management_token.expose_secret()))
      .header("Content-Type", "application/json")
      .send(&json_body)
      .inspect_err(|e| log::error!("Cliproxy request failed for {}: {}", url, e))
      .ok()?;

    let response_text = response
      .body_mut()
      .read_to_string()
      .inspect_err(|e| log::error!("Failed to read cliproxy response body: {}", e))
      .ok()?;

    let parsed: ApiCallResponse = serde_json::from_str(&response_text)
      .inspect_err(|e| log::error!("Failed to parse cliproxy response: {}", e))
      .ok()?;

    if parsed.status_code != 200 {
      log::error!("Cliproxy API returned status {}: {}", parsed.status_code, parsed.body);
      return None;
    }

    return Some(parsed.body);
  }
}

impl DataProvider for CliproxyClaudeProvider {
  fn fetch_data(&self) -> Option<UsageData> {
    let usage = self.fetch_usage()?;
    let profile = self.fetch_profile();

    return Some(into_usage_data(usage, profile));
  }

  fn all_tiers(&self) -> Vec<TierInfo> {
    return SubscriptionTier::iter().map(|t| t.tier_info()).collect();
  }
}
