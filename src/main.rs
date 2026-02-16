use std::sync::{Arc, LazyLock};

use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use clap::Parser;
use figment2::{
  Figment,
  providers::{Env, Format as _, Toml},
};
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::{config::Config, delegate::AppDelegate, providers::debug::DebugProvider, watcher::watch_config};

mod components;
mod config;
mod constants;
mod delegate;
mod providers;
mod utils;
mod views;
mod watcher;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Cycle tray values from 0% to 100% and switch between tiers.
  #[arg(long)]
  debug_values: bool,

  /// Open the configuration file in the default text editor.
  #[arg(long)]
  open_config: bool,

  /// Open the logs directory in the default file manager.
  #[arg(long)]
  open_logs: bool,
}

static CONFIG_PATH: LazyLock<Utf8PathBuf> = LazyLock::new(|| {
  let config_dir = dirs::config_dir()
    .and_then(|p| Utf8PathBuf::try_from(p).ok())
    .unwrap_or_else(|| Utf8PathBuf::from("~/.config"));

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

  if args.open_logs {
    open::that(&*utils::log::LOG_DIR)?;

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

  log::info!("Selected provider: {:?}", config.provider);

  let provider = config.provider.into_provider(&config.settings)?;
  let provider = match args.debug_values {
    true => Arc::new(DebugProvider::new(&*provider)),
    false => provider,
  };

  let delegate = AppDelegate::new(mtm, provider, args, config);

  // Watch config file for changes.
  let watcher = watch_config(&delegate, mtm).inspect_err(|e| log::warn!("{e:#}")).ok();

  // Run application.
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));
  app.run();

  drop(watcher);

  return Ok(());
}
