use anyhow::Context as _;
use documented::DocumentedFields;
use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use toml_edit::DocumentMut;

use crate::{CONFIG_PATH, providers::ProviderKind};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
  Usage,
  Remaining,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DateTimeFormat {
  Relative,
  Absolute,
}

#[serde_inline_default]
#[derive(Deserialize, Serialize, DocumentedFields)]
pub struct Config {
  /// Whether to render the tray icon in monochrome.
  #[serde_inline_default(true)]
  pub monochrome_icon: bool,

  /// Display mode: "usage" or "remaining".
  #[serde_inline_default(DisplayMode::Usage)]
  pub display_mode: DisplayMode,

  /// Whether to show period percentage next to "resets in".
  #[serde_inline_default(false)]
  pub show_period_percentage: bool,

  /// Reset time format: "relative" (resets in 3h) or "absolute" (resets on 13 Feb, 14:00).
  #[serde_inline_default(DateTimeFormat::Relative)]
  pub reset_time_format: DateTimeFormat,

  /// How often to refetch usage data, in seconds.
  #[serde_inline_default(60)]
  pub refetch_interval: u32,

  /// Default data provider, the LLM subscription you use.
  #[serde_inline_default(ProviderKind::ClaudeCode)]
  pub provider: ProviderKind,
}

impl Default for Config {
  fn default() -> Self {
    return Self {
      monochrome_icon: true,
      display_mode: DisplayMode::Usage,
      show_period_percentage: false,
      reset_time_format: DateTimeFormat::Relative,
      refetch_interval: 60,
      provider: ProviderKind::ClaudeCode,
    };
  }
}

impl Config {
  pub fn default_toml() -> anyhow::Result<String> {
    let toml_str = toml_edit::ser::to_string_pretty(&Self::default())?;
    let mut doc: DocumentMut = toml_str.parse()?;

    for (i, (key, comment)) in doc.clone().iter().zip(Self::FIELD_DOCS.iter()).enumerate() {
      let prefix = if i == 0 { format!("# {comment}\n") } else { format!("\n# {comment}\n") };
      doc.key_mut(key.0).context("missing key in serialized config")?.leaf_decor_mut().set_prefix(prefix);
    }

    return Ok(doc.to_string());
  }

  pub fn ensure_exists() -> anyhow::Result<()> {
    let config_path = &*CONFIG_PATH;

    if !config_path.exists() {
      if let Some(parent) = config_path.parent() {
        fs_err::create_dir_all(parent)?;
      }

      fs_err::write(config_path, Config::default_toml()?)?;
    }

    return Ok(());
  }
}
