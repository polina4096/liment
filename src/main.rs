use std::{
  path::PathBuf,
  sync::{Arc, LazyLock},
};

use anyhow::{Context as _, Result};
use clap::Parser;
use figment2::{
  Figment,
  providers::{Env, Format as _, Toml},
};
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::{config::Config, delegate::AppDelegate, providers::debug::DebugProvider};

mod components;
mod config;
mod constants;
mod delegate;
mod providers;
mod utils;
mod views;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Cycle tray values from 0% to 100% and switch between tiers.
  #[arg(long)]
  debug_values: bool,

  /// Open the configuration file in the default text editor.
  #[arg(long)]
  open_config: bool,
}

static CONFIG_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
  let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
  let config_path = config_dir.join("liment").join("config.toml");

  return config_path;
});

fn main() -> Result<()> {
  utils::log::init_logger();

  Config::ensure_exists()?;

  // Process CLI arguments.
  let args = CliArgs::parse();

  if args.open_config {
    edit::edit_file(&*CONFIG_PATH)?;

    return Ok(());
  }

  // Load configuration.
  let config = Figment::new()
    .merge(Toml::file(&*CONFIG_PATH))
    .merge(Env::prefixed("LIMENT_CONFIG_").split("_"))
    .extract::<Config>()?;

  // Initialize application.
  let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let provider = config.provider.into_provider()?;
  let provider = match args.debug_values {
    true => Arc::new(DebugProvider::new(&*provider)),
    false => provider,
  };

  let delegate = AppDelegate::new(mtm, provider, args, config);
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));

  app.run();

  return Ok(());
}
