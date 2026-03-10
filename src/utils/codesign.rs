use std::process::Command;

use camino::Utf8PathBuf;

/// Returns true if the current executable is already code-signed.
fn is_signed() -> bool {
  let exe = match std::env::current_exe() {
    Ok(p) => p,
    Err(_) => return false,
  };

  Command::new("codesign")
    .args(["--verify", "--quiet"])
    .arg(&exe)
    .status()
    .is_ok_and(|s| s.success())
}

/// Ad-hoc signs the current executable (or .app bundle) if not already signed.
/// Returns true if the app was signed and needs a restart.
pub fn ensure_signed() -> bool {
  if is_signed() {
    return false;
  }

  let exe = match Utf8PathBuf::try_from(std::env::current_exe().unwrap_or_default()) {
    Ok(p) => p,
    Err(_) => return false,
  };

  let sign_target = exe
    .ancestors()
    .find(|p| p.as_str().ends_with(".app"))
    .map(|p| p.to_owned())
    .unwrap_or_else(|| exe.clone());

  log::info!("Ad-hoc signing: {sign_target}");

  let signed = Command::new("codesign")
    .args(["--force", "--sign", "-", sign_target.as_str()])
    .status()
    .is_ok_and(|s| s.success());

  if signed {
    log::info!("Relaunching after codesign");

    if let Err(e) = Command::new(&*exe).spawn() {
      log::error!("Failed to relaunch: {e}");
      return false;
    }

    return true;
  }

  log::error!("Ad-hoc codesign failed");

  return false;
}
