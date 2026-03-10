use std::process::Command;

use camino::Utf8PathBuf;

/// Resolves the signing target: the .app bundle if running from one, otherwise the bare executable.
fn sign_target() -> Option<Utf8PathBuf> {
  let exe = Utf8PathBuf::try_from(std::env::current_exe().ok()?).ok()?;

  let target = exe
    .ancestors()
    .find(|p| p.as_str().ends_with(".app"))
    .map(|p| p.to_owned())
    .unwrap_or(exe);

  return Some(target);
}

/// Returns true if the current executable is already code-signed.
fn is_signed(target: &Utf8PathBuf) -> bool {
  Command::new("codesign")
    .args(["--verify", "--quiet"])
    .arg(target.as_str())
    .status()
    .is_ok_and(|s| s.success())
}

/// Ad-hoc signs the current executable (or .app bundle) if not already signed.
/// Returns true if the app was signed and needs a restart.
pub fn ensure_signed() -> bool {
  let Some(target) = sign_target() else {
    return false;
  };

  if is_signed(&target) {
    return false;
  }

  log::info!("Ad-hoc signing: {target}");

  let signed = Command::new("codesign")
    .args(["--force", "--sign", "-", target.as_str()])
    .status()
    .is_ok_and(|s| s.success());

  if signed {
    log::info!("Relaunching after codesign");

    let exe = match std::env::current_exe() {
      Ok(p) => p,
      Err(e) => {
        log::error!("Failed to get exe path: {e}");
        return false;
      }
    };

    if let Err(e) = Command::new(exe).spawn() {
      log::error!("Failed to relaunch: {e}");
      return false;
    }

    return true;
  }

  log::error!("Ad-hoc codesign failed");

  return false;
}
