use objc2::{DefinedClass, MainThreadMarker, rc::Retained, sel};
use objc2_app_kit::{NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::NSString;
use tap::Tap as _;

use strum::IntoEnumIterator as _;

use crate::{
  components,
  delegate::AppDelegate,
  providers::{ProviderKind, UsageData},
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
    menu.addItem(&update_item(mtm, app, &UpdateState::Unchecked));
    menu.addItem(&provider_item(mtm, app, current_provider));
    menu.addItem(&open_config_item(mtm, app));
    menu.addItem(&open_logs_item(mtm, app));
    menu.addItem(&quit_item(mtm, app));
  });
}

pub fn populate_menu(menu: &NSMenu, mtm: MainThreadMarker, app: &AppDelegate, data: &UsageData) {
  menu.removeAllItems();

  // Header with tier badge.
  let header_item = NSMenuItem::new(mtm);
  let header_view = components::header_row(mtm, "Usage", &data.account_tier);
  header_item.setView(Some(&header_view));
  menu.addItem(&header_item);

  // Usage windows.
  let config = app.ivars().config();

  for window in &data.windows {
    menu.addItem(&components::bucket_row(
      mtm,
      &window.title,
      window.utilization,
      window.resets_at.as_ref(),
      if config.show_period_percentage { window.period_seconds } else { None },
      config.reset_time_format,
      config.display_mode,
    ));
  }

  // API / extra usage.
  if let Some(api_usage) = &data.api_usage {
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let header_view = components::label_row(mtm, "Extra Usage", true);
    let header_item = NSMenuItem::new(mtm);
    header_item.setView(Some(&header_view));
    menu.addItem(&header_item);

    let value_text = if let Some(limit) = api_usage.limit_usd {
      format!("${:.2} / ${:.2}", api_usage.usage_usd, limit)
    }
    else {
      format!("${:.2}", api_usage.usage_usd)
    };
    let used_view = components::key_value_row(mtm, "Spent", &value_text);
    let used_item = NSMenuItem::new(mtm);
    used_item.setView(Some(&used_view));
    menu.addItem(&used_item);
  }

  // Separator + Update + Refresh + Quit.
  let update_state = app.ivars().update_state();
  let current_provider = app.ivars().provider().kind();
  menu.addItem(&NSMenuItem::separatorItem(mtm));
  menu.addItem(&update_item(mtm, app, &update_state));
  menu.addItem(&refresh_item(mtm, app));
  menu.addItem(&provider_item(mtm, app, current_provider));
  menu.addItem(&open_config_item(mtm, app));
  menu.addItem(&open_logs_item(mtm, app));
  menu.addItem(&quit_item(mtm, app));
}

const UPDATE_ITEM_TAG: isize = 9001;

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

    if kind == current {
      sub_item.setState(NSControlStateValueOn);
    }

    submenu.addItem(&sub_item);
  }

  item.setSubmenu(Some(&submenu));

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
