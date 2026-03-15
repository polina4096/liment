use std::sync::Arc;

use color_eyre::eyre::ContextCompat as _;
use jiff::Timestamp;
use rgb::Rgb;
use serde::{Deserialize, Serialize};

use crate::{
  providers::{
    claude_code::{ClaudeCodeProvider, ClaudeCodeSettings},
    cliproxy::{CliproxyClaudeProvider, CliproxyClaudeSettings, CliproxyCodexProvider, CliproxyCodexSettings},
  },
  utils::notification,
};

pub mod claude_code;
pub mod cliproxy;
pub mod debug;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Hash, PartialEq, Eq, strum::Display, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
  #[strum(to_string = "Claude Code")]
  ClaudeCode,
  #[strum(to_string = "Cliproxy Claude")]
  CliproxyClaude,
  #[strum(to_string = "Cliproxy Codex")]
  CliproxyCodex,
  #[serde(other)]
  Unknown,
}

#[derive(Deserialize, Serialize, Default)]
pub struct ProviderSettings {
  pub claude_code: Option<ClaudeCodeSettings>,
  pub cliproxy_claude: Option<CliproxyClaudeSettings>,
  pub cliproxy_codex: Option<CliproxyCodexSettings>,
}

pub struct TierInfo {
  pub name: String,
  pub color: Rgb<u8>,
}

pub struct UsageData {
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
  pub resets_at: Option<Timestamp>,

  /// Total period duration in seconds (e.g. 18000 for 5h, 604800 for 7d).
  pub period_seconds: Option<i64>,
}

pub trait DataProvider: Send + Sync {
  /// Returns the kind of this provider.
  fn kind(&self) -> ProviderKind;

  /// Fetches usage data for the provider.
  fn fetch_data(&self) -> Option<UsageData>;

  /// Fetches the account tier info. Returns `None` if the provider doesn't support it.
  fn fetch_profile(&self) -> Option<TierInfo> {
    return None;
  }

  /// Returns SVG bytes for tray icon.
  fn tray_icon_svg(&self) -> &'static [u8];
}

/// No-op provider used when the configured provider is unknown or unavailable.
pub struct NullProvider;

impl DataProvider for NullProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::Unknown;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return None;
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1 1'/>";
  }
}

impl ProviderKind {
  pub fn into_provider(self, settings: &ProviderSettings) -> color_eyre::eyre::Result<Arc<dyn DataProvider>> {
    match self {
      ProviderKind::ClaudeCode => {
        let settings = settings.claude_code.clone().unwrap_or_default();

        return Ok(Arc::new(ClaudeCodeProvider::new(&settings)?));
      }

      ProviderKind::CliproxyClaude => {
        let settings = settings
          .cliproxy_claude
          .as_ref()
          .context("cliproxy_claude provider requires [settings.cliproxy_claude] in config")?;

        return Ok(Arc::new(CliproxyClaudeProvider::new(settings)?));
      }

      ProviderKind::CliproxyCodex => {
        let settings = settings
          .cliproxy_codex
          .as_ref()
          .context("cliproxy_codex provider requires [settings.cliproxy_codex] in config")?;

        return Ok(Arc::new(CliproxyCodexProvider::new(settings)?));
      }

      ProviderKind::Unknown => {
        let msg = "Unknown provider in config, falling back to null provider";
        log::error!("{msg}");
        notification::send_error(msg);

        return Ok(Arc::new(NullProvider));
      }
    };
  }
}
