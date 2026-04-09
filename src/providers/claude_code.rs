use std::{sync::Mutex, time::Instant};

use color_eyre::eyre::{ContextCompat as _, Result};
use jiff::Timestamp;
use rgb::Rgb;
use secrecy::{ExposeSecret, SecretString};
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use serde::{Deserialize, Serialize};

use super::{DataProvider, PeakHoursInfo, ProviderKind, UsageData};
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

/// Weekdays 13:00–19:00 GMT are peak hours for Claude.
pub fn compute_claude_peak_hours() -> PeakHoursInfo {
  let now = Timestamp::now().to_zoned(jiff::tz::TimeZone::get("GMT").unwrap());
  let weekday = now.weekday();
  let hour = now.hour();

  let is_weekday = weekday != jiff::civil::Weekday::Saturday && weekday != jiff::civil::Weekday::Sunday;
  let is_peak = is_weekday && (13 .. 19).contains(&hour);

  let ends_at = if is_peak {
    // Peak ends at 19:00 today
    now.with().hour(19).minute(0).second(0).build().unwrap().timestamp()
  }
  else if is_weekday && hour < 13 {
    // Off-peak ends at 13:00 today
    now.with().hour(13).minute(0).second(0).build().unwrap().timestamp()
  }
  else {
    // Weekend or weekday after 19:00 — next peak is Monday 13:00 (or tomorrow if weekday)
    let days_until = match weekday {
      jiff::civil::Weekday::Friday if hour >= 19 => 3,
      jiff::civil::Weekday::Saturday => 2,
      jiff::civil::Weekday::Sunday => 1,
      _ => 1, // weekday after 19:00
    };
    let next_day = now.checked_add(jiff::SignedDuration::from_hours(days_until * 24)).unwrap();
    next_day.with().hour(13).minute(0).second(0).build().unwrap().timestamp()
  };

  return PeakHoursInfo { is_peak, ends_at };
}

impl From<UsageResponse> for UsageData {
  fn from(usage: UsageResponse) -> Self {
    let api_usage = usage.extra_usage.as_ref().and_then(|extra| {
      if !extra.is_enabled {
        return None;
      }

      return Some(ApiUsage {
        usage_usd: extra.used_credits.unwrap_or(0.0) / 100.0,
        limit_usd: extra.monthly_limit.map(|l| l / 100.0),
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
        windows.push(UsageWindow {
          title: title.to_string(),
          short_title: short_title.map(|s| s.to_string()),
          utilization: b.utilization.unwrap_or(0.0),
          resets_at: b.resets_at,
          period_seconds: Some(*period_secs),
        });
      }
    }

    return UsageData {
      api_usage,
      peak_hours: Some(compute_claude_peak_hours()),
      windows,
    };
  }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageBucket {
  #[serde(default)]
  pub utilization: Option<f64>,
  pub resets_at: Option<Timestamp>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtraUsage {
  pub is_enabled: bool,
  #[serde(default)]
  pub monthly_limit: Option<f64>,
  #[serde(default)]
  pub used_credits: Option<f64>,
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

pub struct ClaudeCodeProvider {
  token: Mutex<TokenState>,
  backoff: Mutex<BackoffState>,
}

struct TokenState {
  secret: SecretString,
  /// Known expiry from the keychain. `None` when the source doesn't provide one (e.g. config override).
  expires_at: Option<Timestamp>,
}

struct BackoffState {
  retry_after: Option<Instant>,
  consecutive_failures: u32,
}

impl ClaudeCodeProvider {
  pub fn new(settings: &ClaudeCodeSettings) -> Result<Self> {
    log::info!("Initializing Claude Code provider");

    let token = Self::fetch_token(settings)?;

    let backoff = Mutex::new(BackoffState {
      retry_after: None,
      consecutive_failures: 0,
    });

    return Ok(Self { token: Mutex::new(token), backoff });
  }

  fn fetch_token(settings: &ClaudeCodeSettings) -> Result<TokenState> {
    if let Some(token) = &settings.token {
      log::info!("Using token from provider settings");

      return Ok(TokenState {
        secret: SecretString::from(token.clone()),
        expires_at: None,
      });
    }

    log::debug!("Token not set in config, fetching from keychain");

    return Self::fetch_keychain_token();
  }

  fn fetch_keychain_token() -> Result<TokenState> {
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
      #[serde(rename = "expiresAt")]
      expires_at: Option<i64>,
    }

    #[derive(Deserialize)]
    struct ClaudeKeychain {
      #[serde(rename = "claudeAiOauth")]
      claude_oauth: ClaudeOAuth,
    }

    let json_str = String::from_utf8(data)?;
    let value: ClaudeKeychain = serde_json::from_str(&json_str)?;
    let expires_at = value.claude_oauth.expires_at.and_then(|ms| Timestamp::from_millisecond(ms).ok());
    return Ok(TokenState {
      secret: SecretString::from(value.claude_oauth.access_token),
      expires_at,
    });
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.get("https://api.anthropic.com/api/oauth/usage")?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse usage response: {}", e))
      .ok();
  }

  fn fetch_profile_response(&self) -> Option<ProfileResponse> {
    log::debug!("Fetching profile data");

    let body = self.get("https://api.anthropic.com/api/oauth/profile")?;

    return serde_json::from_str(&body)
      .inspect(|p: &ProfileResponse| log::debug!("Parsed profile: {:?}", p))
      .inspect_err(|e| log::warn!("Failed to parse profile response: {}", e))
      .ok();
  }

  fn get(&self, url: &str) -> Option<String> {
    // Check if we're in a backoff period
    {
      let backoff = self.backoff.lock().unwrap();
      if let Some(retry_after) = backoff.retry_after
        && Instant::now() < retry_after
      {
        log::debug!(
          "Skipping request to {} (rate limit backoff, {}s remaining)",
          url,
          (retry_after - Instant::now()).as_secs()
        );
        return None;
      }
    }

    // Proactive expiry check: if the current token is known to have expired,
    // re-read the keychain before making the request.
    let needs_refresh = {
      let token_guard = self.token.lock().unwrap();
      token_guard.expires_at.is_some_and(|ts| Timestamp::now() >= ts)
    };
    if needs_refresh {
      log::debug!("Access token expired, re-reading keychain before request");
      match Self::fetch_keychain_token() {
        Ok(new_state) => {
          let mut token_guard = self.token.lock().unwrap();
          if new_state.secret.expose_secret() == token_guard.secret.expose_secret() {
            log::warn!("Keychain still has the same expired token, skipping request");
            return None;
          }
          *token_guard = new_state;
          log::info!("Loaded fresh token from keychain (proactive refresh)");
        }
        Err(e) => {
          log::error!("Failed to re-read keychain for expired token: {}", e);
          return None;
        }
      }
    }

    let mut result = self.get_inner(url);

    if let Err(ureq::Error::StatusCode(401)) = &result {
      log::warn!("Got 401 for {}, refreshing token from keychain", url);

      if let Ok(new_state) = Self::fetch_keychain_token() {
        {
          let mut token_guard = self.token.lock().unwrap();
          if new_state.secret.expose_secret() == token_guard.secret.expose_secret() {
            log::warn!("Keychain returned the same token, skipping retry (token likely expired)");
            return None;
          }
          *token_guard = new_state;
        }

        log::info!("Token refreshed, retrying request");

        result = self.get_inner(url).inspect_err(|e| log::error!("Retry failed for {}: {}", url, e));
      }
      else {
        log::error!("Failed to refresh token from keychain");
      }
    }

    if let Err(ureq::Error::StatusCode(429)) = &result {
      let mut backoff = self.backoff.lock().unwrap();
      backoff.consecutive_failures += 1;
      let delay_secs = 60u64 * (1 << backoff.consecutive_failures.min(4));
      backoff.retry_after = Some(Instant::now() + std::time::Duration::from_secs(delay_secs));
      log::warn!("Rate limited (429), backing off for {}s", delay_secs);
      return None;
    }

    if let Err(ref e) = result {
      log::error!("Request failed for {}: {}", url, e);
    }

    // Reset backoff on success
    if result.is_ok() {
      let mut backoff = self.backoff.lock().unwrap();
      if backoff.consecutive_failures > 0 {
        log::info!("Request succeeded, resetting backoff");
        backoff.consecutive_failures = 0;
        backoff.retry_after = None;
      }
    }

    return result.ok();
  }

  fn get_inner(&self, url: &str) -> Result<String, ureq::Error> {
    log::debug!("GET {}", url);

    let token = self.token.lock().unwrap();
    let mut response = ureq::get(url)
      .header("Authorization", &format!("Bearer {}", token.secret.expose_secret()))
      .header("anthropic-beta", "oauth-2025-04-20")
      .header("User-Agent", "claude-code/2.1.71")
      .call()?;

    return response.body_mut().read_to_string();
  }
}

impl DataProvider for ClaudeCodeProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::ClaudeCode;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return Some(self.fetch_usage()?.into());
  }

  fn fetch_profile(&self) -> Option<TierInfo> {
    return self.fetch_profile_response().map(|p| p.organization.rate_limit_tier.tier_info());
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../resources/claude.svg");
  }
}
