#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::sync::Arc;

#[cfg(target_os = "macos")]
use anyhow::Context as _;
use anyhow::Result;
use clap::Parser;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, runtime::ProtocolObject};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use secrecy::ExposeSecret as _;

use crate::api::ApiClient;

mod api;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod icon;
mod platform;
mod util;

#[derive(Parser)]
#[command()]
struct CliArgs {
  /// Print the token read from the keychain to stdout.
  #[arg(long)]
  print_token: bool,

  /// Debug: cycle tray colors from 0% to 100% over ~10 seconds.
  #[cfg(target_os = "macos")]
  #[arg(long)]
  cycle_colors: bool,
}

fn main() -> Result<()> {
  let args = CliArgs::parse();

  if args.print_token {
    #[cfg(target_os = "macos")]
    {
      let token = platform::macos::util::fetch_keychain_token()?;
      println!("Claude API Token: {}", token.expose_secret());
    }

    #[cfg(target_os = "linux")]
    {
      let token = util::get_claude_token()?;
      println!("Claude API Token: {}", token.expose_secret());
    }

    #[cfg(target_os = "windows")]
    {
      unsafe { windows::Win32::System::Console::AllocConsole().ok() };
      let token = util::get_claude_token()?;
      println!("Claude API Token: {}", token.expose_secret());
    }

    return Ok(());
  }

  #[cfg(target_os = "macos")]
  {
    let mtm = MainThreadMarker::new().context("Failed to create main thread marker")?;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let token = platform::macos::util::get_claude_token(mtm)?;
    let api = Arc::new(ApiClient::new(token));

    let delegate = platform::macos::delegate::AppDelegate::new(mtm, api, args);
    let delegate = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(delegate));

    app.run();
  }

  #[cfg(target_os = "linux")]
  {
    let token = util::get_claude_token()?;
    let api = Arc::new(ApiClient::new(token));

    platform::linux::run(api);
  }

  #[cfg(target_os = "windows")]
  {
    let token = util::get_claude_token()?;
    let api = Arc::new(ApiClient::new(token));

    platform::windows::run(api);
  }

  return Ok(());
}
