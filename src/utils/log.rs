use std::{fs::File, sync::LazyLock};

use camino::Utf8PathBuf;
use color_eyre::{Result, eyre::Context as _};
use jiff::{Zoned, fmt::strtime};
use log::LevelFilter;
use tap::Pipe as _;

use crate::constants::{LIMENT_NO_DISK_LOGS, LIMENT_NO_LOGS, LIMENT_OVERRIDE_LOG_DIR};

pub static LOG_DIR: LazyLock<Utf8PathBuf> = LazyLock::new(|| {
  return std::env::var(LIMENT_OVERRIDE_LOG_DIR).map(Utf8PathBuf::from).unwrap_or_else(|_| {
    let data_dir = etcetera::base_strategy::Xdg::new()
      .ok()
      .and_then(|x| Utf8PathBuf::try_from(etcetera::BaseStrategy::data_dir(&x)).ok())
      .unwrap_or_else(|| Utf8PathBuf::from("~/.local/share"));

    return data_dir.join("liment").join("logs");
  });
});

fn open_log_file() -> Result<Option<File>> {
  if std::env::var(LIMENT_NO_DISK_LOGS).is_ok() {
    return Ok(None);
  }

  let log_dir = &*LOG_DIR;

  if !fs_err::exists(log_dir).unwrap_or(false) {
    fs_err::create_dir_all(log_dir).context("Failed to create log directory")?;
  }

  let now = strtime::format("%Y_%m_%dT%H_%M_%S", &Zoned::now()).context("Failed to format time")?;
  let file = fs_err::File::create(log_dir.join(now).with_extension("log")).context("Failed to create a log file")?;

  return Ok(Some(file.into_parts().0));
}

pub fn init_logger() {
  if std::env::var(LIMENT_NO_LOGS).is_ok() {
    return;
  }

  let pkg = env!("CARGO_PKG_NAME");
  let result = fern::Dispatch::new()
    .filter(move |metadata| metadata.target().starts_with(pkg))
    .format(|out, message, record| {
      out.finish(format_args!("[{}][{}] {}", record.level(), record.target(), message));
    })
    .level(LevelFilter::Debug)
    .chain(std::io::stderr())
    .pipe(|d| {
      match open_log_file() {
        Ok(Some(file)) => d.chain(file),
        Ok(None) => d,
        Err(e) => {
          eprintln!("Failed to initialize disk logger: {e}");
          d
        }
      }
    })
    .apply();

  if let Err(e) = result {
    eprintln!("Failed to initialize logger: {e}");
  }
}
