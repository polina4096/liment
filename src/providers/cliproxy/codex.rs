use std::collections::HashMap;

use anyhow::Result;
use jiff::Timestamp;
use rgb::Rgb;
use serde::{Deserialize, Serialize};

use super::CliproxyClient;
use crate::providers::{DataProvider, ProviderKind, TierInfo, UsageData, UsageWindow};

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

#[derive(Debug, Deserialize)]
struct UsageResponse {
  plan_type: Option<String>,
  rate_limit: Option<RateLimit>,
  code_review_rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
  primary_window: Option<UsageBucket>,
  secondary_window: Option<UsageBucket>,
}

#[derive(Debug, Deserialize)]
struct UsageBucket {
  used_percent: f64,
  limit_window_seconds: i64,
  reset_at: i64,
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
    headers.insert("User-Agent".to_string(), "codex_cli_rs/0.76.0 (Debian 13.0.0; x86_64) WindowsTerminal".to_string());
    headers.insert("Chatgpt-Account-Id".to_string(), chatgpt_account_id);

    let body = self.client.api_get(&self.auth_index, "https://chatgpt.com/backend-api/wham/usage", headers)?;

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
    let usage = self.fetch_usage()?;
    let mut windows = Vec::new();

    if let Some(rate_limit) = &usage.rate_limit {
      if let Some(primary) = &rate_limit.primary_window {
        windows.push(UsageWindow {
          title: "5h Limit".to_string(),
          short_title: Some("5h".to_string()),
          utilization: primary.used_percent,
          resets_at: Timestamp::from_second(primary.reset_at).ok(),
          period_seconds: Some(primary.limit_window_seconds),
        });
      }

      if let Some(secondary) = &rate_limit.secondary_window {
        windows.push(UsageWindow {
          title: "7d Limit".to_string(),
          short_title: Some("7d".to_string()),
          utilization: secondary.used_percent,
          resets_at: Timestamp::from_second(secondary.reset_at).ok(),
          period_seconds: Some(secondary.limit_window_seconds),
        });
      }
    }

    if let Some(code_review) = &usage.code_review_rate_limit {
      if let Some(primary) = &code_review.primary_window {
        windows.push(UsageWindow {
          title: "Code Review 7d".to_string(),
          short_title: None,
          utilization: primary.used_percent,
          resets_at: Timestamp::from_second(primary.reset_at).ok(),
          period_seconds: Some(primary.limit_window_seconds),
        });
      }

      if let Some(secondary) = &code_review.secondary_window {
        windows.push(UsageWindow {
          title: "Code Review Secondary".to_string(),
          short_title: None,
          utilization: secondary.used_percent,
          resets_at: Timestamp::from_second(secondary.reset_at).ok(),
          period_seconds: Some(secondary.limit_window_seconds),
        });
      }
    }

    let account_tier = usage.plan_type.map(|plan| tier_info_for_plan(&plan));

    return Some(UsageData { account_tier, api_usage: None, windows });
  }

  fn all_tiers(&self) -> Vec<TierInfo> {
    return ["free", "plus", "pro", "team", "enterprise"].iter().map(|plan| tier_info_for_plan(plan)).collect();
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../../resources/codex.svg");
  }
}

fn tier_info_for_plan(plan: &str) -> TierInfo {
  let normalized = plan.to_ascii_lowercase();
  let (name, color) = match normalized.as_str() {
    "free" => ("Free", Rgb::new(140, 140, 155)),
    "plus" => ("Plus", Rgb::new(90, 145, 210)),
    "pro" => ("Pro", Rgb::new(75, 175, 155)),
    "team" => ("Team", Rgb::new(185, 135, 90)),
    "enterprise" => ("Enterprise", Rgb::new(130, 115, 180)),
    _ => (plan, Rgb::new(130, 130, 130)),
  };

  return TierInfo { name: name.to_string(), color };
}
