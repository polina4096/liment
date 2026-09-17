use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::eyre::{ContextCompat as _, Result};
use jiff::Timestamp;
use rgb::Rgb;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::{
  providers::{DataProvider, ProviderKind, TierInfo, UsageData, UsageDetail, UsageWindow},
  utils::http::{Client, HttpError},
};

pub const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const USER_AGENT: &str = "codex_cli_rs/0.76.0 (Debian 13.0.0; x86_64) WindowsTerminal";

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CodexSettings {
  /// Access token override. If not set, reads from `~/.codex/auth.json`.
  pub token: Option<String>,

  /// ChatGPT account id override. If not set, reads from `~/.codex/auth.json`.
  pub account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
  pub plan_type: Option<SubscriptionTier>,
  pub rate_limit: Option<RateLimit>,
  pub code_review_rate_limit: Option<RateLimit>,
  #[serde(default)]
  pub additional_rate_limits: Vec<AdditionalRateLimit>,
  pub rate_limit_reset_credits: Option<ResetCredits>,
}

/// Credits that can be spent to reset a rate limit window early.
#[derive(Debug, Deserialize)]
pub struct ResetCredits {
  pub available_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimit {
  pub primary_window: Option<UsageBucket>,
  pub secondary_window: Option<UsageBucket>,
}

#[derive(Debug, Deserialize)]
pub struct AdditionalRateLimit {
  pub limit_name: Option<String>,
  pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize)]
pub struct UsageBucket {
  pub used_percent: f64,
  pub limit_window_seconds: i64,
  pub reset_at: i64,
}

impl UsageBucket {
  /// Short human-readable window duration, e.g. "5h" or "7d".
  fn format_duration(&self) -> String {
    let seconds = self.limit_window_seconds;

    if seconds % 86400 == 0 {
      return format!("{}d", seconds / 86400);
    }

    if seconds % 3600 == 0 {
      return format!("{}h", seconds / 3600);
    }

    return format!("{}m", seconds.div_euclid(60));
  }

  fn to_window(&self, title: String, short_title: Option<String>) -> UsageWindow {
    return UsageWindow {
      title,
      short_title,
      utilization: self.used_percent,
      resets_at: Timestamp::from_second(self.reset_at).ok(),
      period_seconds: Some(self.limit_window_seconds),
    };
  }
}

impl RateLimit {
  fn windows(&self) -> impl Iterator<Item = &UsageBucket> {
    return self.primary_window.iter().chain(self.secondary_window.iter());
  }
}

impl From<UsageResponse> for UsageData {
  fn from(usage: UsageResponse) -> Self {
    let mut windows = Vec::new();

    if let Some(rate_limit) = &usage.rate_limit {
      for bucket in rate_limit.windows() {
        let duration = bucket.format_duration();
        windows.push(bucket.to_window(format!("{} Limit", duration), Some(duration)));
      }
    }

    if let Some(code_review) = &usage.code_review_rate_limit {
      for bucket in code_review.windows() {
        windows.push(bucket.to_window(format!("Review {}", bucket.format_duration()), None));
      }
    }

    for additional in &usage.additional_rate_limits {
      let Some(rate_limit) = &additional.rate_limit
      else {
        continue;
      };

      let name = additional.limit_name.as_deref().unwrap_or("Extra");
      for bucket in rate_limit.windows() {
        windows.push(bucket.to_window(format!("{} {}", name, bucket.format_duration()), None));
      }
    }

    let details = usage
      .rate_limit_reset_credits
      .and_then(|credits| credits.available_count)
      .map(|count| {
        return UsageDetail {
          label: "Reset credits".to_string(),
          value: count.to_string(),
        };
      })
      .into_iter()
      .collect();

    return UsageData {
      api_usage: None,
      peak_hours: None,
      windows,
      details,
    };
  }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "String")]
pub enum SubscriptionTier {
  Free,
  Plus,
  Pro,
  ProLite,
  Team,
  Enterprise,
  Unknown(String),
}

impl From<String> for SubscriptionTier {
  fn from(value: String) -> Self {
    return match value.as_str() {
      "free" => SubscriptionTier::Free,
      "plus" => SubscriptionTier::Plus,
      "pro" => SubscriptionTier::Pro,
      "prolite" => SubscriptionTier::ProLite,
      "team" | "business" => SubscriptionTier::Team,
      "enterprise" => SubscriptionTier::Enterprise,
      _ => SubscriptionTier::Unknown(value),
    };
  }
}

impl SubscriptionTier {
  pub fn tier_info(&self) -> TierInfo {
    return TierInfo {
      name: self.to_string(),
      color: match self {
        SubscriptionTier::Free => Rgb::new(140, 140, 155),
        SubscriptionTier::Plus => Rgb::new(90, 145, 210),
        SubscriptionTier::Pro => Rgb::new(75, 175, 155),
        SubscriptionTier::ProLite => Rgb::new(95, 160, 145),
        SubscriptionTier::Team => Rgb::new(185, 135, 90),
        SubscriptionTier::Enterprise => Rgb::new(130, 115, 180),
        SubscriptionTier::Unknown(_) => Rgb::new(140, 140, 155),
      },
    };
  }
}

impl std::fmt::Display for SubscriptionTier {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      SubscriptionTier::Free => write!(f, "Free"),
      SubscriptionTier::Plus => write!(f, "Plus"),
      SubscriptionTier::Pro => write!(f, "Pro"),
      SubscriptionTier::ProLite => write!(f, "Pro Lite"),
      SubscriptionTier::Team => write!(f, "Team"),
      SubscriptionTier::Enterprise => write!(f, "Enterprise"),
      SubscriptionTier::Unknown(name) => write!(f, "{}", name),
    };
  }
}

fn get_auth_path() -> Result<Utf8PathBuf> {
  let home = etcetera::home_dir()?;
  let home = Utf8Path::from_path(&home).context("Home directory path is not valid UTF-8")?;

  return Ok(home.join(".codex").join("auth.json"));
}

/// Shape of `~/.codex/auth.json`, written and refreshed by the Codex CLI.
#[derive(Debug, Deserialize)]
struct CodexAuthFile {
  tokens: Option<CodexAuthTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexAuthTokens {
  access_token: String,
  account_id: Option<String>,
}

struct Credentials {
  token: SecretString,
  account_id: Option<String>,
}

pub struct CodexProvider {
  settings: CodexSettings,
  client: Client,
}

impl CodexProvider {
  pub fn new(settings: &CodexSettings) -> Result<Self> {
    log::info!("Initializing Codex provider");

    let provider = Self {
      settings: settings.clone(),
      client: Client::new(),
    };

    provider.read_credentials()?;

    return Ok(provider);
  }

  /// Reads the Codex credentials fresh on every request: the CLI refreshes the tokens in place,
  /// and re-reading a local file is cheaper than tracking expiry ourselves.
  fn read_credentials(&self) -> Result<Credentials> {
    if let Some(token) = &self.settings.token {
      log::debug!("Using token from provider settings");

      return Ok(Credentials {
        token: SecretString::from(token.clone()),
        account_id: self.settings.account_id.clone(),
      });
    }

    let path = get_auth_path()?;
    log::debug!("Reading Codex credentials from {}", path);

    let contents = fs_err::read_to_string(&path)?;
    let auth: CodexAuthFile = serde_json::from_str(&contents)?;
    let tokens = auth.tokens.context("No `tokens` in Codex auth file, log in with `codex login`")?;

    return Ok(Credentials {
      token: SecretString::from(tokens.access_token),
      account_id: self.settings.account_id.clone().or(tokens.account_id),
    });
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.get(USAGE_URL)?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed codex usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse codex usage response: {}", e))
      .ok();
  }

  fn get(&self, url: &str) -> Option<String> {
    let credentials = self
      .read_credentials()
      .inspect_err(|e| log::error!("Failed to read Codex credentials: {:#}", e))
      .ok()?;

    let auth_header = format!("Bearer {}", credentials.token.expose_secret());
    let mut headers = vec![
      ("Authorization", auth_header.as_str()),
      ("Content-Type", "application/json"),
      ("User-Agent", USER_AGENT),
    ];

    if let Some(account_id) = &credentials.account_id {
      headers.push(("Chatgpt-Account-Id", account_id));
    }
    else {
      log::warn!("No ChatGPT account id available, the request will likely be rejected");
    }

    return match self.client.get(url, &headers) {
      Ok(body) => Some(body),
      // Backoff outcomes are already logged by the client.
      Err(HttpError::Skipped | HttpError::RateLimited) => None,
      Err(HttpError::Status(401)) => {
        log::error!("Codex token rejected (401), log in again with `codex login`");
        None
      }
      Err(e) => {
        log::error!("Request failed for {}: {}", url, e);
        None
      }
    };
  }
}

impl DataProvider for CodexProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::Codex;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return Some(self.fetch_usage()?.into());
  }

  fn fetch_profile(&self) -> Option<TierInfo> {
    return self.fetch_usage().and_then(|u| u.plan_type.map(|t| t.tier_info()));
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../resources/codex.svg");
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = include_str!("../../resources/tests/codex_usage.json");

  #[test]
  fn parses_sample_usage() {
    let usage: UsageResponse = serde_json::from_str(SAMPLE).unwrap();

    assert!(matches!(usage.plan_type, Some(SubscriptionTier::ProLite)));

    let data: UsageData = usage.into();
    let titles: Vec<_> = data.windows.iter().map(|w| (w.title.as_str(), w.short_title.as_deref())).collect();

    assert_eq!(titles, vec![
      ("7d Limit", Some("7d")),
      ("GPT-5.3-Codex-Spark 5h", None),
      ("GPT-5.3-Codex-Spark 7d", None),
    ]);

    let details: Vec<_> = data.details.iter().map(|d| (d.label.as_str(), d.value.as_str())).collect();
    assert_eq!(details, vec![("Reset credits", "2")]);
  }
}
