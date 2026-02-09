use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::providers::{UsageProvider, create_provider};

#[derive(Deserialize, Clone)]
pub struct ProviderDef {
  /// Provider type identifier (e.g. "claude_code").
  #[serde(rename = "type")]
  pub provider_type: String,

  /// Provider-specific configuration.
  #[serde(flatten)]
  pub config: toml::Table,
}

#[derive(Deserialize)]
struct Config {
  /// Which provider to show in the menubar (by index into `providers`).
  #[serde(default)]
  menubar_provider: usize,

  /// List of provider definitions.
  #[serde(default = "default_providers")]
  providers: Vec<ProviderDef>,
}

fn default_providers() -> Vec<ProviderDef> {
  vec![ProviderDef {
    provider_type: "claude_code".to_string(),
    config: toml::Table::new(),
  }]
}

impl Default for Config {
  fn default() -> Self {
    Self {
      menubar_provider: 0,
      providers: default_providers(),
    }
  }
}

fn config_path() -> PathBuf {
  let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
  return base.join("liment").join("providers.toml");
}

const DEFAULT_CONFIG: &str = "\
# Which provider to show in the menubar (index into [[providers]]).
menubar_provider = 0

[[providers]]
type = \"claude_code\"
";

/// Creates the config file with defaults if it doesn't exist. Returns the path.
pub fn ensure_config_file() -> PathBuf {
  let path = config_path();
  if !path.exists() {
    if let Some(parent) = path.parent() {
      let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, DEFAULT_CONFIG);
  }
  return path;
}

/// Returns the config file path (for opening in Finder).
pub fn get_config_path() -> PathBuf {
  return config_path();
}

fn load_config() -> Config {
  let path = config_path();
  match std::fs::read_to_string(&path) {
    Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
      eprintln!("Warning: failed to parse {}: {}", path.display(), e);
      Config::default()
    }),
    Err(_) => Config::default(),
  }
}

pub fn create_providers() -> Result<(Arc<dyn UsageProvider>, Vec<Arc<dyn UsageProvider>>)> {
  let config = load_config();

  if config.providers.is_empty() {
    bail!("No providers configured");
  }

  if config.menubar_provider >= config.providers.len() {
    bail!(
      "menubar_provider index {} out of range (have {} providers)",
      config.menubar_provider,
      config.providers.len()
    );
  }

  let providers: Vec<Arc<dyn UsageProvider>> =
    config.providers.iter().map(|def| create_provider(def)).collect::<Result<_>>()?;

  let menubar = Arc::clone(&providers[config.menubar_provider]);

  return Ok((menubar, providers));
}
