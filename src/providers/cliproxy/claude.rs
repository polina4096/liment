use std::collections::HashMap;

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

use super::CliproxyClient;
use crate::providers::{
  DataProvider, ProviderKind, Tier, UsageData,
  claude::{ANTHROPIC_BETA, PROFILE_URL, ProfileResponse, USAGE_URL, USER_AGENT, UsageResponse},
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
  client: CliproxyClient,
  auth_index: String,
}

impl CliproxyClaudeProvider {
  pub fn new(settings: &CliproxyClaudeSettings) -> Result<Self> {
    log::info!("Initializing CLIProxy Claude provider");

    return Ok(Self {
      client: CliproxyClient::new(&settings.base_url, &settings.management_token),
      auth_index: settings.auth_index.clone(),
    });
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.api_get(USAGE_URL)?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse usage response: {}", e))
      .ok();
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    log::debug!("Fetching profile data");

    let body = self.api_get(PROFILE_URL)?;

    return serde_json::from_str(&body)
      .inspect(|p: &ProfileResponse| log::debug!("Parsed profile: {:?}", p))
      .inspect_err(|e| log::warn!("Failed to parse profile response: {}", e))
      .ok();
  }

  fn api_get(&self, url: &str) -> Option<String> {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer $TOKEN$".to_string());
    headers.insert("Anthropic-Beta".to_string(), ANTHROPIC_BETA.to_string());
    headers.insert("User-Agent".to_string(), USER_AGENT.to_string());

    return self.client.api_get(&self.auth_index, url, headers);
  }
}

impl DataProvider for CliproxyClaudeProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::CliproxyClaude;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return Some(self.fetch_usage()?.into());
  }

  fn fetch_tier(&self) -> Option<Tier> {
    return self.fetch_profile().map(|p| p.organization.rate_limit_tier.tier());
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../../resources/claude.svg");
  }
}
