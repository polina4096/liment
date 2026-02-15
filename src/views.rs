use objc2::{DefinedClass, MainThreadMarker, rc::Retained, sel};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSString;
use tap::Tap as _;

use crate::{components, delegate::AppDelegate, providers::UsageData};

pub fn loading_menu(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenu> {
  return NSMenu::new(mtm).tap(|menu| {
    let loading_item = NSMenuItem::new(mtm);
    loading_item.setTitle(&NSString::from_str("Loading..."));
    loading_item.setEnabled(false);
    menu.addItem(&loading_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&refresh_item(mtm, app));
    menu.addItem(&open_config_item(mtm, app));
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
  for window in &data.windows {
    menu.addItem(&components::bucket_row(
      mtm,
      &window.title,
      window.utilization,
      &window.resets_at,
      if app.ivars().config().show_period_percentage { window.period_seconds } else { None },
      app.ivars().config().reset_time_format,
      app.ivars().config().display_mode,
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

  // Separator + Refresh + Quit.
  menu.addItem(&NSMenuItem::separatorItem(mtm));
  menu.addItem(&refresh_item(mtm, app));
  menu.addItem(&open_config_item(mtm, app));
  menu.addItem(&quit_item(mtm, app));
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
