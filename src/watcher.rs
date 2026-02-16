use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use dispatch2::{DispatchQueue, MainThreadBound};
use notify::{RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind};
use objc2::{MainThreadMarker, rc::Retained};

use crate::{CONFIG_PATH, config::Config, delegate::AppDelegate};

/// Returns true for events that indicate the file content may have changed.
/// This includes direct writes and renames (used by editors for atomic saves).
fn is_content_change(kind: &notify::EventKind) -> bool {
  return matches!(
    kind,
    notify::EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_)) | notify::EventKind::Create(_)
  );
}

pub fn watch_config(delegate: &Retained<AppDelegate>, mtm: MainThreadMarker) -> anyhow::Result<RecommendedWatcher> {
  use anyhow::Context as _;

  let delegate = Arc::new(MainThreadBound::new(delegate.clone(), mtm));

  let (tx, rx) = std::sync::mpsc::channel();
  let mut watcher = notify::RecommendedWatcher::new(tx, notify::Config::default()) //
    .context("Failed to create config watcher")?;

  watcher
    .watch(CONFIG_PATH.as_std_path(), RecursiveMode::NonRecursive)
    .context("Failed to watch config file")?;

  std::thread::spawn(move || {
    let mut last_reload = Instant::now();

    for event in rx {
      let result: anyhow::Result<()> = (|| {
        let event = event?;

        // Only react to content modifications and renames (atomic saves).
        if !is_content_change(&event.kind) {
          return Ok(());
        }

        // Debounce: skip if we reloaded less than 200ms ago.
        if last_reload.elapsed() < Duration::from_millis(200) {
          return Ok(());
        }

        let toml_str = fs_err::read_to_string(&*CONFIG_PATH)?;
        let new_config: Config = toml_edit::de::from_str(&toml_str)?;

        log::info!("Config file changed, reloading");
        last_reload = Instant::now();

        let delegate = Arc::clone(&delegate);
        DispatchQueue::main().exec_async(move || {
          let mtm = MainThreadMarker::new().expect("Must be on main thread");
          delegate.get(mtm).reload_config(new_config);
        });

        return Ok(());
      })();

      if let Err(e) = result {
        log::warn!("Config watcher error: {e}");
      }
    }
  });

  return Ok(watcher);
}
