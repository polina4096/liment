fn main() {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .stderr(std::process::Stdio::null())
    .output()
    .ok()
    .filter(|o| o.status.success())
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    .unwrap_or_else(|| "unknown".to_string());

  println!("cargo:rustc-env=GIT_COMMIT_SHORT={output}");
}
