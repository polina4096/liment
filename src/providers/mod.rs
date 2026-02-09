use std::sync::Arc;

use anyhow::{Result, bail};
use jiff::Timestamp;

use crate::config::ProviderDef;

pub mod claude_code;
pub mod cliproxy_claude;

pub struct UsageData {
  /// Account tier label (e.g. "Pro", "Max 5x").
  pub account_tier: Option<String>,

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

pub trait UsageProvider: Send + Sync {
  fn fetch_data(&self) -> Option<UsageData>;

  /// Two placeholder labels for the menubar while loading (e.g. ["7d ..", "5h .."]).
  fn placeholder_lines(&self) -> [&str; 2];
}

pub fn create_provider(def: &ProviderDef) -> Result<Arc<dyn UsageProvider>> {
  match def.provider_type.as_str() {
    "claude_code" => Ok(Arc::new(claude_code::ClaudeCodeProvider::new()?)),
    "cliproxy_claude" => Ok(Arc::new(cliproxy_claude::CliproxyClaudeProvider::new(def)?)),
    other => bail!("Unknown provider type: {}", other),
  }
}
