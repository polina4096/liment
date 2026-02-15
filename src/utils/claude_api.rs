use jiff::Timestamp;
use rgb::Rgb;
use serde::Deserialize;

use crate::providers::{ApiUsage, TierInfo, UsageData, UsageWindow};

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
  pub resets_at: Timestamp,
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
    TierInfo {
      name: self.to_string(),
      color: match self {
        SubscriptionTier::Free => Rgb::new(140, 140, 155),
        SubscriptionTier::Pro => Rgb::new(90, 145, 210),
        SubscriptionTier::Max5x => Rgb::new(145, 110, 200),
        SubscriptionTier::Max20x => Rgb::new(205, 130, 95),
      },
    }
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
    Some(ApiUsage {
      usage_usd: extra.used_credits / 100.0,
      limit_usd: Some(extra.monthly_limit / 100.0),
    })
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
        utilization: b.utilization,
        resets_at: b.resets_at,
        period_seconds: Some(*period_secs),
      });
    }
  }

  return UsageData { account_tier, api_usage, windows };
}
