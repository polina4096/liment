use std::path::PathBuf;

use anyhow::Context as _;
use jiff::{Zoned, fmt::strtime};
use log::LevelFilter;
use simplelog::{ColorChoice, CombinedLogger, SharedLogger, TermLogger, TerminalMode, WriteLogger};

use crate::constants::{LIMENT_NO_DISK_LOGS, LIMENT_NO_LOGS, LIMENT_OVERRIDE_LOG_DIR};

fn term_logger(config: simplelog::Config, loggers: &mut Vec<Box<dyn SharedLogger>>) {
  loggers.push(TermLogger::new(LevelFilter::Debug, config, TerminalMode::Mixed, ColorChoice::Auto));
}

fn disk_logger(config: simplelog::Config, loggers: &mut Vec<Box<dyn SharedLogger>>) -> anyhow::Result<()> {
  if std::env::var(LIMENT_NO_DISK_LOGS).is_err() {
    let log_dir = std::env::var(LIMENT_OVERRIDE_LOG_DIR).map(PathBuf::from).unwrap_or_else(|_| {
      let data_dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share"));
      let log_dir = data_dir.join("liment");

      return log_dir;
    });

    if !fs_err::exists(&log_dir).unwrap_or(false) {
      fs_err::create_dir_all(&log_dir).context("Failed to create log directory")?;
    }

    let now = strtime::format("%Y_%m_%dT%H_%M_%S", &Zoned::now()).context("Failed to format time")?;
    let file = fs_err::File::create(log_dir.join(now)).context("Failed to create a log file")?;
    loggers.push(WriteLogger::new(LevelFilter::Debug, config, file));
  }

  return Ok(());
}

pub fn init_logger() {
  if std::env::var(LIMENT_NO_LOGS).is_ok() {
    return;
  }

  let config = simplelog::ConfigBuilder::new() //
    .add_filter_allow_str(env!("CARGO_PKG_NAME"))
    .build();

  let mut errors = Vec::new();
  let mut loggers = Vec::new();
  term_logger(config.clone(), &mut loggers);
  disk_logger(config.clone(), &mut loggers).unwrap_or_else(|e| errors.push(("Failed to initialize disk logger: ", e)));

  match CombinedLogger::init(loggers) {
    Ok(()) => {
      for (msg, err) in errors {
        log::error!("{}: {}", msg, err);
      }
    }

    Err(e) => {
      println!("Failed to initialize logger: {}", e);
    }
  }
}
