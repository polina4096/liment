use anyhow::{Context as _, Result};
use clap::Parser;
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::{
  config::{create_providers, ensure_config_file},
  delegate::AppDelegate,
};

mod components;
mod config;
mod delegate;
mod providers;
mod utils;
mod views;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Debug: cycle tray colors from 0% to 100% over ~10 seconds.
  #[arg(long)]
  cycle_colors: bool,
}

fn main() -> Result<()> {
  let args = CliArgs::parse();

  ensure_config_file();

  let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let (menubar_provider, _all_providers) = create_providers()?;

  let delegate = AppDelegate::new(mtm, menubar_provider, args);
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));

  app.run();

  return Ok(());
}
