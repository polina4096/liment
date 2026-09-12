use std::collections::HashMap;

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

use super::CliproxyClient;
use crate::providers::{
  DataProvider, ProviderKind, TierInfo, UsageData,
  codex::{USAGE_URL, USER_AGENT, UsageResponse},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CliproxyCodexSettings {
  /// CLIProxy base URL (e.g. "http://localhost:8317").
  pub base_url: String,

  /// CLIProxy management API secret key.
  pub management_token: String,

  /// Auth index identifying which CLIProxy account to use.
  pub auth_index: String,
}

pub struct CliproxyCodexProvider {
  client: CliproxyClient,
  auth_index: String,
}

#[derive(Debug, Deserialize)]
struct AuthFilesResponse {
  files: Vec<AuthFile>,
}

#[derive(Debug, Deserialize)]
struct AuthFile {
  auth_index: String,
  id_token: Option<AuthIdToken>,
}

#[derive(Debug, Deserialize)]
struct AuthIdToken {
  chatgpt_account_id: Option<String>,
}

impl CliproxyCodexProvider {
  pub fn new(settings: &CliproxyCodexSettings) -> Result<Self> {
    log::info!("Initializing CLIProxy Codex provider");

    return Ok(Self {
      client: CliproxyClient::new(&settings.base_url, &settings.management_token),
      auth_index: settings.auth_index.clone(),
    });
  }

  fn fetch_chatgpt_account_id(&self) -> Option<String> {
    log::debug!("Fetching auth file metadata");

    let body = self.client.management_get("/v0/management/auth-files")?;
    let response: AuthFilesResponse = serde_json::from_str(&body)
      .inspect_err(|e| log::error!("Failed to parse auth-files response: {}", e))
      .ok()?;

    let auth_file = response.files.into_iter().find(|file| file.auth_index == self.auth_index).or_else(|| {
      log::error!("No auth file found for auth index {}", self.auth_index);
      None
    })?;

    return auth_file.id_token.and_then(|t| t.chatgpt_account_id).or_else(|| {
      log::error!("Auth file {} missing id_token.chatgpt_account_id", self.auth_index);
      None
    });
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    let chatgpt_account_id = self
      .fetch_chatgpt_account_id()
      .inspect(|_| log::debug!("Resolved ChatGPT account id for auth index {}", self.auth_index))
      .or_else(|| {
        log::error!("No ChatGPT account id found for auth index {}", self.auth_index);
        None
      })?;

    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer $TOKEN$".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
    headers.insert("Chatgpt-Account-Id".to_string(), chatgpt_account_id);

    let body = self.client.api_get(&self.auth_index, USAGE_URL, headers)?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed codex usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse codex usage response: {}", e))
      .ok();
  }
}

impl DataProvider for CliproxyCodexProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::CliproxyCodex;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return Some(self.fetch_usage()?.into());
  }

  fn fetch_profile(&self) -> Option<TierInfo> {
    return self.fetch_usage().and_then(|u| u.plan_type.map(|t| t.tier_info()));
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../../resources/codex.svg");
  }
}
