use std::{process::Command, sync::LazyLock};

use camino::Utf8PathBuf;
use clap::Parser;
use color_eyre::eyre::{Context as _, ContextCompat as _, Result};
use figment2::{
  Figment,
  providers::{Env, Format as _, Toml},
};
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::{config::Config, delegate::AppDelegate, watcher::watch_config};

mod config;
mod constants;
mod delegate;
mod profile_cache;
mod providers;
mod ui;
mod updater;
mod utils;
mod watcher;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Open the configuration file in the default text editor.
  #[arg(long)]
  open_config: bool,

  /// Open the logs directory in the default file manager.
  #[arg(long)]
  open_logs: bool,

  /// Ad-hoc codesign the current executable and restart.
  #[arg(long)]
  self_sign: bool,
}

static CONFIG_PATH: LazyLock<Utf8PathBuf> = LazyLock::new(|| {
  let config_dir = etcetera::base_strategy::Xdg::new()
    .ok()
    .and_then(|s| Utf8PathBuf::try_from(etcetera::BaseStrategy::config_dir(&s)).ok())
    .unwrap_or_else(|| Utf8PathBuf::from("~/.config"));

  return config_dir.join("liment").join("config.toml");
});

fn main() -> Result<()> {
  utils::log::init_logger();
  color_eyre::install()?;

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

  if args.self_sign {
    self_sign()?;

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

  let delegate = AppDelegate::new(mtm, config);

  // Watch config file for changes.
  let watcher = watch_config(&delegate, mtm).inspect_err(|e| log::warn!("{e:#}")).ok();

  // Run application.
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));
  app.run();

  drop(watcher);

  return Ok(());
}

/// Ad-hoc codesigns the current executable (or .app bundle) and relaunches it.
fn self_sign() -> Result<()> {
  let exe = Utf8PathBuf::try_from(std::env::current_exe().context("Failed to get current exe")?)
    .context("Exe path is not valid UTF-8")?;

  // If running from a .app bundle, sign the bundle root instead of the bare binary.
  let sign_target = exe
    .ancestors()
    .find(|p| p.as_str().ends_with(".app"))
    .map(|p| p.to_owned())
    .unwrap_or_else(|| exe.clone());

  log::info!("Ad-hoc signing: {sign_target}");

  let status = Command::new("codesign")
    .args(["--force", "--sign", "-", sign_target.as_str()])
    .status()
    .context("Failed to run codesign")?;

  if !status.success() {
    color_eyre::eyre::bail!("codesign exited with {status}");
  }

  log::info!("Relaunching: {exe}");

  Command::new(&*exe).spawn().context("Failed to relaunch")?;

  return Ok(());
}
