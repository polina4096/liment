use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::Parser;
use objc2::{MainThreadMarker, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use secrecy::ExposeSecret as _;

use crate::{
  api::ApiClient,
  delegate::AppDelegate,
  util::{fetch_keychain_token, get_claude_token},
};

mod api;
mod components;
mod delegate;
mod util;
mod views;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Print the token read from the keychain to stdout.
  #[arg(long)]
  print_token: bool,

  /// Debug: cycle tray colors from 0% to 100% over ~10 seconds.
  #[arg(long)]
  cycle_colors: bool,
}

fn main() -> Result<()> {
  // Handle CLI.
  let args = CliArgs::parse();

  if args.print_token {
    println!("Claude API Token: {}", fetch_keychain_token()?.expose_secret());

    return Ok(());
  }

  // Run app.
  let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let token = get_claude_token(mtm)?;
  let api = Arc::new(ApiClient::new(token));

  let delegate = AppDelegate::new(mtm, api, args);
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));

  app.run();

  return Ok(());
}
