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
mod delegate;
mod util;
mod views;

#[derive(Parser)]
#[command()]
struct Args {
  /// Read the token from the keychain and print it to stdout.
  #[arg(long)]
  read_token: bool,
}

fn main() -> Result<()> {
  // Handle CLI.
  let args = Args::parse();

  if args.read_token {
    print!("{}", fetch_keychain_token()?.expose_secret());

    return Ok(());
  }

  // Run app.
  let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let token = get_claude_token(mtm)?;
  let api = ApiClient::new(token);

  let delegate = AppDelegate::new(mtm, api);
  let delegate = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate));

  app.run();

  return Ok(());
}
