use std::sync::Arc;

use jiff::Timestamp;
use rgb::Rgb;
use serde::{Deserialize, Serialize};

pub mod claude_code;
pub mod debug;

#[derive(Deserialize, Serialize)]
pub enum ProviderKind {
  ClaudeCode,
}

pub struct TierInfo {
  pub name: String,
  pub color: Rgb<u8>,
}

pub struct UsageData {
  /// Account tier label (e.g. "Pro", "Max 5x").
  pub account_tier: Option<TierInfo>,

  /// API/extra usage credit info.
  pub api_usage: Option<ApiUsage>,

  /// Usage windows (e.g. 5h limit, 7d limit).
  pub windows: Vec<UsageWindow>,
}

pub struct ApiUsage {
  /// Credits consumed (USD).
  pub usage_usd: f64,

  /// Monthly spending limit (USD), if any.
  pub limit_usd: Option<f64>,
}

pub struct UsageWindow {
  /// Human-readable window title (e.g. "5h Limit", "7d Sonnet").
  pub title: String,

  /// Short label for the menubar tray (e.g. "5h", "7d"). None = not shown in menubar.
  pub short_title: Option<String>,

  /// How much of the bucket has been used (0–100).
  pub utilization: f64,

  /// Bucket reset timestamp.
  pub resets_at: Timestamp,

  /// Total period duration in seconds (e.g. 18000 for 5h, 604800 for 7d).
  pub period_seconds: Option<i64>,
}

pub trait DataProvider: Send + Sync {
  /// Fetches usage data for the provider.
  fn fetch_data(&self) -> Option<UsageData>;

  /// Returns all possible tiers for this provider.
  fn all_tiers(&self) -> Vec<TierInfo>;
}

impl ProviderKind {
  pub fn into_provider(&self) -> anyhow::Result<Arc<dyn DataProvider>> {
    return Ok(match self {
      ProviderKind::ClaudeCode => Arc::new(claude_code::ClaudeCodeProvider::new()?),
    });
  }
}
