use std::sync::Mutex;

use anyhow::{Context as _, Result};
use jiff::Timestamp;
use rgb::Rgb;
use secrecy::{ExposeSecret, SecretString};
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator as _;

use super::{DataProvider, UsageData};
use crate::providers::{ApiUsage, TierInfo, UsageWindow};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ClaudeCodeSettings {
  /// OAuth token override. If not set, reads from keychain.
  pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageResponse {
  pub five_hour: Option<UsageBucket>,
  pub seven_day: Option<UsageBucket>,
  pub seven_day_sonnet: Option<UsageBucket>,
  pub seven_day_opus: Option<UsageBucket>,
  pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageBucket {
  pub utilization: f64,
  pub resets_at: Option<Timestamp>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtraUsage {
  pub is_enabled: bool,
  pub monthly_limit: f64,
  pub used_credits: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileResponse {
  pub organization: ProfileOrganization,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileOrganization {
  pub rate_limit_tier: SubscriptionTier,
}

#[derive(Debug, Deserialize, Clone, Copy, strum::EnumIter)]
pub enum SubscriptionTier {
  #[serde(rename = "default_claude_free")]
  Free,
  #[serde(rename = "default_claude_pro")]
  Pro,
  #[serde(rename = "default_claude_max_5x")]
  Max5x,
  #[serde(rename = "default_claude_max_20x")]
  Max20x,
}

impl SubscriptionTier {
  pub fn tier_info(&self) -> TierInfo {
    return TierInfo {
      name: self.to_string(),
      color: match self {
        SubscriptionTier::Free => Rgb::new(140, 140, 155),
        SubscriptionTier::Pro => Rgb::new(90, 145, 210),
        SubscriptionTier::Max5x => Rgb::new(145, 110, 200),
        SubscriptionTier::Max20x => Rgb::new(205, 130, 95),
      },
    };
  }
}

impl std::fmt::Display for SubscriptionTier {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      SubscriptionTier::Free => write!(f, "Free"),
      SubscriptionTier::Pro => write!(f, "Pro"),
      SubscriptionTier::Max5x => write!(f, "Max 5x"),
      SubscriptionTier::Max20x => write!(f, "Max 20x"),
    };
  }
}

pub fn into_usage_data(usage: UsageResponse, profile: Option<ProfileResponse>) -> UsageData {
  let account_tier = profile.map(|p| p.organization.rate_limit_tier.tier_info());

  let api_usage = usage.extra_usage.as_ref().and_then(|extra| {
    if !extra.is_enabled {
      return None;
    }

    return Some(ApiUsage {
      usage_usd: extra.used_credits / 100.0,
      limit_usd: Some(extra.monthly_limit / 100.0),
    });
  });

  let mut windows = Vec::new();
  let buckets: &[(&str, Option<&str>, &Option<UsageBucket>, i64)] = &[
    ("5h Limit", Some("5h"), &usage.five_hour, 5 * 3600),
    ("7d Limit", Some("7d"), &usage.seven_day, 7 * 86400),
    ("7d Sonnet", None, &usage.seven_day_sonnet, 7 * 86400),
    ("7d Opus", None, &usage.seven_day_opus, 7 * 86400),
  ];

  for (title, short_title, bucket, period_secs) in buckets {
    if let Some(b) = bucket {
      if let Some(resets_at) = b.resets_at {
        windows.push(UsageWindow {
          title: title.to_string(),
          short_title: short_title.map(|s| s.to_string()),
          utilization: b.utilization,
          resets_at,
          period_seconds: Some(*period_secs),
        });
      }
    }
  }

  return UsageData { account_tier, api_usage, windows };
}

pub struct ClaudeCodeProvider {
  token: Mutex<SecretString>,
}

impl ClaudeCodeProvider {
  pub fn new(settings: &ClaudeCodeSettings) -> Result<Self> {
    log::info!("Initializing Claude Code provider");

    let token = Self::fetch_token(settings)?;

    return Ok(Self { token: Mutex::new(token) });
  }

  fn fetch_token(settings: &ClaudeCodeSettings) -> Result<SecretString> {
    if let Some(token) = &settings.token {
      log::info!("Using token from provider settings");

      return Ok(SecretString::from(token.clone()));
    }

    log::debug!("Token not set in config, fetching from keychain");

    return Self::fetch_keychain_token();
  }

  fn fetch_keychain_token() -> Result<SecretString> {
    let results = ItemSearchOptions::new()
      .class(ItemClass::generic_password())
      .service("Claude Code-credentials")
      .load_data(true)
      .search()?;

    let data = results
      .into_iter()
      .find_map(|r| {
        match r {
          SearchResult::Data(d) => Some(d),
          _ => None,
        }
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
    log::debug!("Fetching usage data");

    let body = self.get("https://api.anthropic.com/api/oauth/usage")?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse usage response: {}", e))
      .ok();
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    log::debug!("Fetching profile data");

    let body = self.get("https://api.anthropic.com/api/oauth/profile")?;

    return serde_json::from_str(&body)
      .inspect(|p: &ProfileResponse| log::debug!("Parsed profile: {:?}", p))
      .inspect_err(|e| log::warn!("Failed to parse profile response: {}", e))
      .ok();
  }

  fn get(&self, url: &str) -> Option<String> {
    let result = self.get_inner(url);

    if let Err(ureq::Error::StatusCode(401)) = &result {
      log::warn!("Got 401 for {}, refreshing token from keychain", url);

      if let Ok(new_token) = Self::fetch_keychain_token() {
        *self.token.lock().unwrap() = new_token;

        log::info!("Token refreshed, retrying request");

        return self.get_inner(url).inspect_err(|e| log::error!("Retry failed for {}: {}", url, e)).ok();
      }
      else {
        log::error!("Failed to refresh token from keychain");
      }
    }

    if let Err(ref e) = result {
      log::error!("Request failed for {}: {}", url, e);
    }

    return result.ok();
  }

  fn get_inner(&self, url: &str) -> Result<String, ureq::Error> {
    log::debug!("GET {}", url);

    let token = self.token.lock().unwrap();
    let mut response = ureq::get(url)
      .header("Authorization", &format!("Bearer {}", token.expose_secret()))
      .header("anthropic-beta", "oauth-2025-04-20")
      .header("Content-Type", "application/json")
      .call()?;

    return response.body_mut().read_to_string();
  }
}

impl DataProvider for ClaudeCodeProvider {
  fn fetch_data(&self) -> Option<UsageData> {
    let usage = self.fetch_usage()?;
    let profile = self.fetch_profile();

    return Some(into_usage_data(usage, profile));
  }

  fn all_tiers(&self) -> Vec<TierInfo> {
    return SubscriptionTier::iter().map(|t| t.tier_info()).collect();
  }
}
