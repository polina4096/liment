use objc2::{DefinedClass, MainThreadMarker, rc::Retained, runtime::AnyObject, sel};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::{NSAttributedString, NSString};
use strum::IntoEnumIterator as _;
use tap::Tap as _;

use crate::{
  delegate::AppDelegate,
  providers::{ApiUsage, ProviderKind, TierInfo, UsageData},
  ui::components,
  updater::UpdateState,
};

pub fn loading_menu(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenu> {
  return NSMenu::new(mtm).tap(|menu| {
    let loading_item = NSMenuItem::new(mtm);
    loading_item.setTitle(&NSString::from_str("Loading..."));
    loading_item.setEnabled(false);
    menu.addItem(&loading_item);

    let current_provider = app.ivars().provider().kind();

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&refresh_item(mtm, app));
    menu.addItem(&provider_item(mtm, app, current_provider));
    menu.addItem(&update_item(mtm, app, &UpdateState::Unchecked));
    menu.addItem(&about_item(mtm, app));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&open_config_item(mtm, app));
    menu.addItem(&open_logs_item(mtm, app));
    menu.addItem(&quit_item(mtm, app));
  });
}

pub fn populate_menu(
  menu: &NSMenu,
  mtm: MainThreadMarker,
  app: &AppDelegate,
  data: &UsageData,
  profile: Option<&TierInfo>,
) {
  menu.removeAllItems();

  // Header with tier badge.
  let config = app.ivars().config();
  let version = if config.show_version { Some(concat!("v", env!("CARGO_PKG_VERSION"))) } else { None };
  let header_item = NSMenuItem::new(mtm);
  let header_view = components::header_row(mtm, "Usage", &profile, version);
  header_item.setView(Some(&header_view));
  menu.addItem(&header_item);

  for window in &data.windows {
    menu.addItem(&components::bucket_row(mtm, &components::BucketRowParams {
      label: &window.title,
      utilization: window.utilization,
      resets_at: window.resets_at.as_ref(),
      period_seconds: window.period_seconds,
      show_period_percentage: config.show_period_percentage,
      show_pacing_warning: config.show_pacing_warning,
      reset_time_format: config.reset_time_format,
      display_mode: config.display_mode,
    }));
  }

  // Peak hours indicator (under all usages, above the separator).
  if let Some(peak) = &data.peak_hours {
    let peak_item = NSMenuItem::new(mtm);
    let peak_view = components::peak_hours_row(mtm, peak);
    peak_item.setView(Some(&peak_view));
    menu.addItem(&peak_item);
  }

  // API / extra usage.
  if let Some(api_usage) = &data.api_usage {
    extra_usage_section(menu, mtm, api_usage);
  }

  // Separator + actions + utilities.
  let update_state = app.ivars().update_state();
  let current_provider = app.ivars().provider().kind();
  menu.addItem(&NSMenuItem::separatorItem(mtm));
  menu.addItem(&refresh_item(mtm, app));
  menu.addItem(&provider_item(mtm, app, current_provider));
  menu.addItem(&update_item(mtm, app, &update_state));
  menu.addItem(&about_item(mtm, app));
  menu.addItem(&NSMenuItem::separatorItem(mtm));
  menu.addItem(&open_config_item(mtm, app));
  menu.addItem(&open_logs_item(mtm, app));
  menu.addItem(&quit_item(mtm, app));
}

/// Renders the "Extra Usage" section.
///
/// When there's no free overage grant, shows a single row:
///   `Spent — $usage / $max_paid`
///
/// When a grant is present, splits into two parallel rows so each subsidy stream is visible:
///   `Free — $consumed / $grant_total`
///   `Paid — $out_of_pocket / $effective_cap`
///
/// where:
///   - `consumed = min(usage, grant)` is how much of the grant has been used
///   - `out_of_pocket = max(usage − grant, 0)` is what the user has actually been billed
///   - `effective_cap = max(max_paid − grant, 0)` is the paid cap with the grant subtracted,
///     so it represents the user's true out-of-pocket budget
fn extra_usage_section(menu: &NSMenu, mtm: MainThreadMarker, api_usage: &ApiUsage) {
  menu.addItem(&NSMenuItem::separatorItem(mtm));

  let header_view = components::label_row(mtm, "Extra Usage", true);
  let header_item = NSMenuItem::new(mtm);
  header_item.setView(Some(&header_view));
  menu.addItem(&header_item);

  let max_paid_text = |amount: f64| -> String {
    if !api_usage.is_enabled {
      return "$0.00".to_string();
    }
    if api_usage.max_paid_usd.is_none() {
      return "unlimited".to_string();
    }
    return format!("${:.2}", amount);
  };

  match api_usage.free_credits_usd {
    None => {
      // No grant — single "Spent" row, same shape as before.
      let cap = api_usage.max_paid_usd.unwrap_or(0.0);
      let value = format!("${:.2} / {}", api_usage.usage_usd, max_paid_text(cap));
      add_kv_row(menu, mtm, "Spent", &value);
    }
    Some(free) => {
      // Free row: how much of the grant has been consumed.
      let consumed = api_usage.usage_usd.min(free);
      let free_value = format!("${:.2} / ${:.2}", consumed, free);
      add_kv_row(menu, mtm, "Free", &free_value);

      // Paid row: out-of-pocket against the cap minus the grant (the user's true budget).
      let out_of_pocket = (api_usage.usage_usd - free).max(0.0);
      let effective_cap = api_usage.max_paid_usd.map(|cap| (cap - free).max(0.0)).unwrap_or(0.0);
      let paid_value = format!("${:.2} / {}", out_of_pocket, max_paid_text(effective_cap));
      add_kv_row(menu, mtm, "Paid", &paid_value);
    }
  }
}

fn add_kv_row(menu: &NSMenu, mtm: MainThreadMarker, key: &str, value: &str) {
  let view = components::key_value_row(mtm, key, value);
  let item = NSMenuItem::new(mtm);
  item.setView(Some(&view));
  menu.addItem(&item);
}

const UPDATE_ITEM_TAG: isize = 9001;
const PROVIDER_ITEM_TAG: isize = 9002;

fn update_item(mtm: MainThreadMarker, app: &AppDelegate, state: &UpdateState) -> Retained<NSMenuItem> {
  let (title, action, enabled) = match state {
    UpdateState::Unchecked | UpdateState::UpToDate => {
      ("Check for Updates".to_string(), Some(sel!(onCheckForUpdates:)), true)
    }

    UpdateState::Failed { error } => {
      log::debug!("Previous update check failed: {error}");
      ("Check for Updates".to_string(), Some(sel!(onCheckForUpdates:)), true)
    }

    UpdateState::Available { version, .. } => (format!("Update to v{version}…"), Some(sel!(onInstallUpdate:)), true),

    UpdateState::Downloading => ("Downloading Update…".to_string(), None, false),
  };

  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str(&title),
      action,
      &NSString::from_str("u"),
    )
  };

  unsafe { item.setTarget(Some(app)) };

  item.setEnabled(enabled);
  item.setTag(UPDATE_ITEM_TAG);

  return item;
}

/// Replaces the update menu item in-place without rebuilding the entire menu.
pub fn update_update_item(menu: &NSMenu, mtm: MainThreadMarker, app: &AppDelegate, state: &UpdateState) {
  if let Some(old_item) = menu.itemWithTag(UPDATE_ITEM_TAG) {
    let index = menu.indexOfItem(&old_item);
    menu.removeItem(&old_item);
    let new_item = update_item(mtm, app, state);
    menu.insertItem_atIndex(&new_item, index);
  }
}

fn refresh_item(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenuItem> {
  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str("Refresh"),
      Some(sel!(onRefresh:)),
      &NSString::from_str("r"),
    )
  };
  unsafe { item.setTarget(Some(app)) };
  return item;
}

fn open_config_item(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenuItem> {
  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str("Open Config…"),
      Some(sel!(onOpenConfig:)),
      &NSString::from_str(","),
    )
  };
  unsafe { item.setTarget(Some(app)) };
  return item;
}

fn open_logs_item(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenuItem> {
  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str("Open Logs…"),
      Some(sel!(onOpenLogs:)),
      &NSString::from_str("l"),
    )
  };
  unsafe { item.setTarget(Some(app)) };
  return item;
}

fn provider_item(mtm: MainThreadMarker, app: &AppDelegate, current: ProviderKind) -> Retained<NSMenuItem> {
  let item = NSMenuItem::new(mtm);
  item.setTitle(&NSString::from_str("Change Provider"));
  item.setTag(PROVIDER_ITEM_TAG);

  let submenu = NSMenu::new(mtm);
  for (i, kind) in ProviderKind::iter().filter(|k| *k != ProviderKind::Unknown).enumerate() {
    let sub_item = unsafe {
      NSMenuItem::initWithTitle_action_keyEquivalent(
        mtm.alloc::<NSMenuItem>(),
        &NSString::from_str(&kind.to_string()),
        Some(sel!(onChangeProvider:)),
        &NSString::new(),
      )
    };

    unsafe { sub_item.setTarget(Some(app)) };
    sub_item.setTag(i as isize);

    let state = if kind == current { NSControlStateValueOn } else { NSControlStateValueOff };
    sub_item.setState(state);

    submenu.addItem(&sub_item);
  }

  item.setSubmenu(Some(&submenu));

  return item;
}

/// Replaces the provider menu item in-place without rebuilding the entire menu.
pub fn update_provider_item(menu: &NSMenu, mtm: MainThreadMarker, app: &AppDelegate, current: ProviderKind) {
  if let Some(old_item) = menu.itemWithTag(PROVIDER_ITEM_TAG) {
    let index = menu.indexOfItem(&old_item);
    menu.removeItem(&old_item);
    let new_item = provider_item(mtm, app, current);
    menu.insertItem_atIndex(&new_item, index);
  }
}

fn about_item(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenuItem> {
  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str("About liment"),
      Some(sel!(onAbout:)),
      &NSString::new(),
    )
  };
  unsafe { item.setTarget(Some(app)) };
  return item;
}

fn quit_item(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenuItem> {
  let item = unsafe {
    NSMenuItem::initWithTitle_action_keyEquivalent(
      mtm.alloc::<NSMenuItem>(),
      &NSString::from_str("Quit"),
      Some(sel!(onQuit:)),
      &NSString::from_str("q"),
    )
  };
  unsafe { item.setTarget(Some(app)) };
  return item;
}

pub fn build_credits() -> Retained<NSAttributedString> {
  let mtm = MainThreadMarker::new().expect("Must be on main thread");

  let html = concat!(
    r#"<div style="text-align: center; font-family: -apple-system; font-size: 11px; color: #888;">"#,
    "Simple LLM usage limits in your menu bar.<br/>",
    r#"<a href="https://github.com/polina4096/liment/issues">Issues</a>"#,
    " &bull; ",
    r#"<a href="https://github.com/polina4096/liment">Source Code</a>"#,
    "</div>"
  );

  let ns_html = NSString::from_str(html);
  let ns_data: Retained<AnyObject> = unsafe { objc2::msg_send![&ns_html, dataUsingEncoding: 4_usize] };

  let doc_type_key = NSString::from_str("DocumentType");
  let doc_type_val = NSString::from_str("NSHTML");
  let opts: Retained<AnyObject> = unsafe {
    objc2::msg_send![
      objc2::class!(NSDictionary),
      dictionaryWithObject: &*doc_type_val,
      forKey: &*doc_type_key
    ]
  };

  let mut doc_attrs: *mut AnyObject = std::ptr::null_mut();
  let result: Retained<NSAttributedString> = unsafe {
    objc2::msg_send![
      mtm.alloc::<NSAttributedString>(),
      initWithData: &*ns_data,
      options: &*opts,
      documentAttributes: &mut doc_attrs,
      error: std::ptr::null_mut::<*mut AnyObject>()
    ]
  };

  return result;
}
