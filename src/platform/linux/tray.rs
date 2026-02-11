use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use gtk4::{self as gtk, glib, prelude::*};
use ksni::blocking::{Handle, TrayMethods as _};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip, Tray};

use crate::api::{ApiClient, ProfileResponse, UsageBucket, UsageResponse};
use crate::icon;
use crate::util::format_reset_time;

use super::popup;

enum UiEvent {
  Toggle(i32, i32),
  DataUpdate(Option<UsageResponse>, Option<ProfileResponse>),
  Refresh,
}

pub struct LinuxTray {
  api: Arc<ApiClient>,
  handle: Option<Handle<LinuxTray>>,
  usage: Option<UsageResponse>,
  profile: Option<ProfileResponse>,
  ui_sender: mpsc::Sender<UiEvent>,
}

impl Tray for LinuxTray {
  fn id(&self) -> String {
    return "liment".into();
  }

  fn title(&self) -> String {
    if let Some(ref usage) = self.usage {
      let seven_d = usage.seven_day.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
      let five_h = usage.five_hour.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
      return format!("Claude Usage — 7d {}% | 5h {}%", seven_d, five_h);
    }
    return "Claude Usage".into();
  }

  fn icon_pixmap(&self) -> Vec<Icon> {
    return [128, 64, 48, 32, 22]
      .iter()
      .map(|&size| {
        let d = icon::render_tray_icon(size);
        Icon { width: d.width, height: d.height, data: d.data }
      })
      .collect();
  }

  fn tool_tip(&self) -> ToolTip {
    let description = match &self.usage {
      Some(u) => {
        let seven_d = u.seven_day.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
        let five_h = u.five_hour.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
        format!("7d {}% | 5h {}%", seven_d, five_h)
      }
      None => "Loading...".into(),
    };

    return ToolTip {
      icon_name: String::new(),
      icon_pixmap: Vec::new(),
      title: "Claude Usage".into(),
      description,
    };
  }

  fn activate(&mut self, x: i32, y: i32) {
    let _ = self.ui_sender.send(UiEvent::Toggle(x, y));
  }

  fn menu(&self) -> Vec<MenuItem<Self>> {
    let mut items = Vec::new();

    let tier_str = self
      .profile
      .as_ref()
      .map(|p| format!("  [{}]", p.organization.rate_limit_tier))
      .unwrap_or_default();
    items.push(
      StandardItem { label: format!("Claude Usage{}", tier_str), enabled: false, ..Default::default() }.into(),
    );

    items.push(MenuItem::Separator);

    if let Some(usage) = &self.usage {
      push_bucket(&mut items, "5h Limit", &usage.five_hour);
      push_bucket(&mut items, "7d Limit", &usage.seven_day);
      push_bucket(&mut items, "7d Sonnet", &usage.seven_day_sonnet);
      push_bucket(&mut items, "7d Opus", &usage.seven_day_opus);

      if let Some(extra) = &usage.extra_usage {
        if extra.is_enabled {
          items.push(MenuItem::Separator);

          items.push(
            StandardItem { label: "Extra Usage".into(), enabled: false, ..Default::default() }.into(),
          );

          let limit = extra.monthly_limit / 100.0;
          let used = extra.used_credits / 100.0;
          items.push(
            StandardItem {
              label: format!("Spent  ${:.2} / ${:.2}", used, limit),
              enabled: false,
              ..Default::default()
            }
            .into(),
          );
        }
      }
    } else {
      items.push(StandardItem { label: "Loading...".into(), enabled: false, ..Default::default() }.into());
    }

    items.push(MenuItem::Separator);

    items.push(
      StandardItem {
        label: "Refresh".into(),
        activate: Box::new(|tray: &mut Self| {
          if let Some(handle) = tray.handle.clone() {
            let api = Arc::clone(&tray.api);
            std::thread::spawn(move || {
              let usage = api.fetch_usage();
              let profile = api.fetch_profile();
              handle.update(|tray| {
                tray.usage = usage;
                tray.profile = profile;
              });
            });
          }
        }),
        ..Default::default()
      }
      .into(),
    );

    items.push(
      StandardItem {
        label: "Quit".into(),
        activate: Box::new(|_| std::process::exit(0)),
        ..Default::default()
      }
      .into(),
    );

    items
  }
}

fn progress_bar(pct: f64, width: usize) -> String {
  let filled = ((pct / 100.0) * width as f64).round() as usize;
  let empty = width.saturating_sub(filled);
  format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn push_bucket(items: &mut Vec<MenuItem<LinuxTray>>, label: &str, bucket: &Option<UsageBucket>) {
  let Some(bucket) = bucket else { return };
  let reset = format_reset_time(&bucket.resets_at);
  let pct = bucket.utilization as u32;

  items.push(
    StandardItem {
      label: format!("{}  {}%          resets in {}", label, pct, reset),
      enabled: false,
      ..Default::default()
    }
    .into(),
  );
  items.push(
    StandardItem { label: progress_bar(bucket.utilization, 20), enabled: false, ..Default::default() }.into(),
  );
}

pub fn run(api: Arc<ApiClient>) {
  let app = gtk::Application::builder().application_id("dev.liment").build();

  let api_clone = Arc::clone(&api);
  let first_run = Cell::new(true);

  app.connect_activate(move |app| {
    if !first_run.replace(false) {
      return;
    }

    let _hold = app.hold();

    let (tx, rx) = mpsc::channel::<UiEvent>();

    let widgets = Rc::new(popup::build_popup(app));

    // Spawn ksni tray
    let tray = LinuxTray {
      api: Arc::clone(&api_clone),
      handle: None,
      usage: None,
      profile: None,
      ui_sender: tx.clone(),
    };
    let handle: Handle<LinuxTray> = tray.spawn().expect("failed to spawn tray");
    // Store the handle inside the tray so menu callbacks can use it.
    handle.update(|tray| {
      tray.handle = Some(handle.clone());
    });

    // Single refresh thread: updates both ksni icon and GTK popup
    let api_refresh = Arc::clone(&api_clone);
    let tx_refresh = tx.clone();
    let handle_refresh = handle.clone();
    std::thread::spawn(move || {
      let usage = api_refresh.fetch_usage();
      let profile = api_refresh.fetch_profile();
      handle_refresh.update(|t| {
        t.usage = usage.clone();
        t.profile = profile.clone();
      });
      let _ = tx_refresh.send(UiEvent::DataUpdate(usage, profile));

      loop {
        std::thread::sleep(Duration::from_secs(60));
        let usage = api_refresh.fetch_usage();
        let profile = api_refresh.fetch_profile();
        handle_refresh.update(|t| {
          t.usage = usage.clone();
          t.profile = profile.clone();
        });
        let _ = tx_refresh.send(UiEvent::DataUpdate(usage, profile));
      }
    });

    // Wire refresh button
    let tx_btn = tx.clone();
    widgets.refresh_btn.connect_clicked(move |_| {
      let _ = tx_btn.send(UiEvent::Refresh);
    });

    // Poll for events from background threads
    let api_manual = Arc::clone(&api_clone);
    let tx_manual = tx.clone();
    let handle_manual = handle.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
      while let Ok(event) = rx.try_recv() {
        match event {
          UiEvent::Toggle(x, y) => popup::toggle_popup(&widgets, x, y),
          UiEvent::DataUpdate(Some(usage), profile) => {
            popup::update_popup(&widgets, &usage, &profile);
          }
          UiEvent::DataUpdate(None, _) => {}
          UiEvent::Refresh => {
            let api = Arc::clone(&api_manual);
            let s = tx_manual.clone();
            let h = handle_manual.clone();
            std::thread::spawn(move || {
              let usage = api.fetch_usage();
              let profile = api.fetch_profile();
              h.update(|t| {
                t.usage = usage.clone();
                t.profile = profile.clone();
              });
              let _ = s.send(UiEvent::DataUpdate(usage, profile));
            });
          }
        }
      }
      return glib::ControlFlow::Continue;
    });
  });

  app.run_with_args::<String>(&[]);
}
