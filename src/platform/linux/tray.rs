use std::sync::Arc;
use std::time::Duration;

use ksni::blocking::{Handle, TrayMethods as _};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, Tray};

use crate::api::{ApiClient, ProfileResponse, UsageBucket, UsageResponse};
use crate::icon;
use crate::util::format_reset_time;

pub struct LinuxTray {
  api: Arc<ApiClient>,
  handle: Option<Handle<LinuxTray>>,
  usage: Option<UsageResponse>,
  profile: Option<ProfileResponse>,
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
    let (line1, p1, line2, p2) = match &self.usage {
      Some(u) => {
        let five_h = u.five_hour.as_ref().map(|b| b.utilization).unwrap_or(0.0);
        let seven_d = u.seven_day.as_ref().map(|b| b.utilization).unwrap_or(0.0);

        let v1 = seven_d as u32;
        let v2 = five_h as u32;
        let w = (v1.max(1).ilog10() as usize + 1).max(v2.max(1).ilog10() as usize + 1);

        (format!("7d {:>w$}%", v1), seven_d / 100.0, format!("5h {:>w$}%", v2), five_h / 100.0)
      }
      None => ("7d ..".into(), 0.0, "5h ..".into(), 0.0),
    };

    let icon_data = icon::render_tray_icon(&line1, p1, &line2, p2);
    return vec![Icon { width: icon_data.width, height: icon_data.height, data: icon_data.data }];
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
  let tray = LinuxTray { api: Arc::clone(&api), handle: None, usage: None, profile: None };

  let handle: Handle<LinuxTray> = tray.spawn().expect("failed to spawn tray");

  // Store the handle inside the tray so menu callbacks can use it.
  handle.update(|tray| {
    tray.handle = Some(handle.clone());
  });

  {
    let usage = api.fetch_usage();
    let profile = api.fetch_profile();
    handle.update(|tray| {
      tray.usage = usage;
      tray.profile = profile;
    });
  }

  let api_clone = Arc::clone(&api);
  std::thread::spawn(move || loop {
    std::thread::sleep(Duration::from_secs(60));
    let usage = api_clone.fetch_usage();
    let profile = api_clone.fetch_profile();
    handle.update(|tray| {
      tray.usage = usage;
      tray.profile = profile;
    });
  });

  loop {
    std::thread::park();
  }
}
