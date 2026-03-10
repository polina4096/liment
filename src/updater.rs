use std::{cell::RefCell, process::Command};

use anyhow::Context as _;
use camino::{Utf8Path, Utf8PathBuf};
use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use semver::Version;
use serde::Deserialize;

use crate::constants::LIMENT_OVERRIDE_VERSION;

const GITHUB_RELEASES_URL: &str = "https://api.github.com/repos/polina4096/liment/releases/latest";
const ASSET_NAME: &str = "liment.app.zip";

#[derive(Debug, Clone)]
pub enum UpdateState {
  Unchecked,
  UpToDate,
  Available { version: Version, download_url: String },
  Downloading,
  Failed { error: String },
}

pub struct Updater {
  state: RefCell<UpdateState>,
}

impl Updater {
  pub fn new() -> Self {
    return Self {
      state: RefCell::new(UpdateState::Unchecked),
    };
  }

  pub fn state(&self) -> std::cell::Ref<'_, UpdateState> {
    return self.state.borrow();
  }

  pub fn set_state(&self, state: UpdateState) {
    *self.state.borrow_mut() = state;
  }
}

/// Checks for a newer release and returns the new state.
/// Performs a blocking HTTP request — call from a background thread.
pub fn check_for_update() -> UpdateState {
  match fetch_latest_release() {
    Err(e) => {
      log::error!("Update check failed: {e:#}");

      return UpdateState::Failed { error: e.to_string() };
    }

    Ok(VersionInfo { current, latest, .. }) if latest <= current => {
      log::info!("Already up to date (v{current})");

      return UpdateState::UpToDate;
    }

    Ok(VersionInfo { current, latest, latest_url }) => {
      log::info!("Update available: v{current} -> v{latest}");

      return UpdateState::Available {
        version: latest,
        download_url: latest_url,
      };
    }
  }
}

/// Downloads and installs the update, then relaunches.
/// Performs blocking I/O — call from a background thread.
pub fn download_and_install(url: &str) -> anyhow::Result<()> {
  let new_app = download_and_extract(url)?;

  return install_and_relaunch(&new_app);
}

#[derive(Debug)]
struct VersionInfo {
  current: Version,
  latest: Version,
  latest_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
  tag_name: String,
  assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
  name: String,
  browser_download_url: String,
}

/// Fetches the latest release from GitHub.
fn fetch_latest_release() -> anyhow::Result<VersionInfo> {
  log::debug!("Checking for updates...");

  let mut response = ureq::get(GITHUB_RELEASES_URL)
    .header("User-Agent", "liment-updater")
    .call()
    .context("Failed to fetch latest release")?;

  let body = response.body_mut().read_to_string().context("Failed to read response body")?;
  let release: GitHubRelease = serde_json::from_str(&body).context("Failed to parse release JSON")?;

  let tag = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
  let latest = Version::parse(tag).context("Failed to parse latest version")?;
  let current_str =
    std::env::var(LIMENT_OVERRIDE_VERSION).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
  let current = Version::parse(&current_str).context("Failed to parse current version")?;

  log::info!("Current version: {current}, latest: {latest}");

  let asset = release
    .assets
    .iter()
    .find(|a| a.name == ASSET_NAME)
    .context("Release has no liment.app.zip asset")?;

  return Ok(VersionInfo {
    current,
    latest,
    latest_url: asset.browser_download_url.clone(),
  });
}

/// Downloads and extracts the update zip from the given URL.
fn download_and_extract(url: &str) -> anyhow::Result<Utf8PathBuf> {
  let tmp = tempfile::tempdir().context("Failed to create temp directory")?;
  let update_dir = Utf8PathBuf::try_from(tmp.keep()).context("Temp dir is not valid UTF-8")?;
  let zip_path = update_dir.join(ASSET_NAME);

  log::info!("Downloading update from {url}");

  // Download the update zip.
  let mut response = ureq::get(url) //
    .header("User-Agent", "liment-updater")
    .call()
    .context("Failed to download update")?;

  let body = response.body_mut().read_to_vec().context("Failed to read update body")?;
  fs_err::write(&zip_path, &body).context("Failed to write update zip")?;

  log::info!("Extracting update to {update_dir}");

  // Extract the update zip.
  let status = Command::new("unzip")
    .args(["-o", zip_path.as_str(), "-d", update_dir.as_str()])
    .output()
    .context("Failed to run unzip")?;

  if !status.status.success() {
    let e = String::from_utf8_lossy(&status.stderr);
    anyhow::bail!("unzip failed: {e}");
  }

  // Verify the extracted app bundle contains the binary.
  let new_app = update_dir.join("liment.app");
  let binary = new_app.join("Contents/MacOS/liment");

  if !binary.exists() {
    anyhow::bail!("Extracted app bundle is missing the binary at {binary}");
  }

  return Ok(new_app);
}

/// Returns the path to the app bundle.
fn app_bundle_path() -> anyhow::Result<Utf8PathBuf> {
  let exe = Utf8PathBuf::try_from(std::env::current_exe().context("Failed to get current exe path")?)
    .context("Executable path is not valid UTF-8")?;

  // Expected: `/path/to/liment.app/Contents/MacOS/liment`.
  let app_path = exe
    .parent() // Contents/MacOS`
    .and_then(|p| p.parent()) // Contents
    .and_then(|p| p.parent()) // liment.app
    .context("Cannot determine .app bundle path")?;

  if !app_path.as_str().ends_with(".app") {
    anyhow::bail!("Not running from a .app bundle (path: {app_path})");
  }

  return Ok(app_path.to_owned());
}

/// Installs the new app and relaunches the current process.
fn install_and_relaunch(new_app: &Utf8Path) -> anyhow::Result<()> {
  let current_app = app_bundle_path()?;
  let backup_app = Utf8PathBuf::from(format!("{current_app}.old"));

  log::info!("Installing update: {new_app} -> {current_app}");

  // Move current `.app` to `.app.old` backup.
  fs_err::rename(&current_app, &backup_app).context("Failed to back up current app")?;

  // Move new `.app` into place.
  if let Err(e) = fs_err::rename(new_app, &current_app) {
    // Restore backup on failure (best-effort).
    if let Err(e) = fs_err::rename(&backup_app, &current_app) {
      log::warn!("Failed to restore backup: {e}");
    };

    return Err(e).context("Failed to install new app");
  }

  // Clean up backup (best-effort).
  if let Err(e) = fs_err::remove_dir_all(&backup_app) {
    log::warn!("Failed to clean up backup: {e}");
  };

  log::info!("Relaunching from {current_app}");

  Command::new("open").arg("-n").arg(current_app.as_str()).spawn().context("Failed to relaunch app")?;

  // Terminate the current instance on the main thread.
  DispatchQueue::main().exec_async(move || {
    let mtm = MainThreadMarker::new().expect("Must be on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.terminate(None);
  });

  return Ok(());
}
