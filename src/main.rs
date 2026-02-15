use std::{path::PathBuf, sync::LazyLock};

use anyhow::{Context as _, Result};
use clap::Parser;
use figment2::{
  Figment,
  providers::{Env, Format as _, Toml},
};
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::{config::Config, delegate::AppDelegate};

mod components;
mod config;
mod delegate;
mod providers;
mod utils;
mod views;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Debug: cycle tray values from 0% to 100%.
  #[arg(long)]
  cycle_values: bool,

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
  let args = CliArgs::parse();

  Config::ensure_exists()?;

  if args.open_config {
    edit::edit_file(&*CONFIG_PATH)?;

    return Ok(());
  }

  let config = Figment::new()
    .merge(Toml::file(&*CONFIG_PATH))
    .merge(Env::prefixed("LIMENT_CONFIG_").split("_"))
    .extract::<Config>()?;

  let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let provider = config.provider.into_provider()?;
  let delegate = AppDelegate::new(mtm, provider, args, config);
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));

  app.run();

  return Ok(());
}
