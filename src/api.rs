use jiff::Timestamp;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct UsageResponse {
  /// Rolling 5-hour usage bucket.
  pub five_hour: Option<UsageBucket>,

  /// Rolling 7-day overall usage bucket.
  pub seven_day: Option<UsageBucket>,

  /// Rolling 7-day Sonnet-specific usage bucket.
  pub seven_day_sonnet: Option<UsageBucket>,

  /// Rolling 7-day Opus-specific usage bucket.
  pub seven_day_opus: Option<UsageBucket>,

  /// Extra usage credit information.
  pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageBucket {
  /// How much of the bucket has been used (as a percentage from 0 to 100).
  pub utilization: f64,

  /// Bucket reset timestamp.
  pub resets_at: Timestamp,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtraUsage {
  /// Whether extra usage credits are enabled for this account.
  pub is_enabled: bool,

  /// Monthly spending limit in cents.
  pub monthly_limit: f64,

  /// Credits consumed this month in cents.
  pub used_credits: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileResponse {
  /// The user's organization details.
  pub organization: ProfileOrganization,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileOrganization {
  /// The user's subscription tier.
  pub rate_limit_tier: SubscriptionTier,
}

#[derive(Debug, Deserialize, Clone, Copy)]
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

pub struct ApiClient {
  token: SecretString,
}

impl ApiClient {
  pub fn new(token: SecretString) -> Self {
    return Self { token };
  }

  pub fn fetch_usage(&self) -> Option<UsageResponse> {
    let mut response = ureq::get("https://api.anthropic.com/api/oauth/usage")
      .header("Authorization", &format!("Bearer {}", self.token.expose_secret()))
      .header("anthropic-beta", "oauth-2025-04-20")
      .header("Content-Type", "application/json")
      .call()
      .ok()?;

    let body = response.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
  }

  pub fn fetch_profile(&self) -> Option<ProfileResponse> {
    let mut response = ureq::get("https://api.anthropic.com/api/oauth/profile")
      .header("Authorization", &format!("Bearer {}", self.token.expose_secret()))
      .header("anthropic-beta", "oauth-2025-04-20")
      .header("Content-Type", "application/json")
      .call()
      .ok()?;

    let body = response.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
  }
}
