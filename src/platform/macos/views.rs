use objc2::{MainThreadMarker, rc::Retained, sel};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSString;
use tap::Tap as _;

use crate::api::{ProfileResponse, UsageResponse};
use super::components;
use super::delegate::AppDelegate;

pub fn loading_menu(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSMenu> {
  return NSMenu::new(mtm).tap(|menu| {
    let loading_item = NSMenuItem::new(mtm);
    loading_item.setTitle(&NSString::from_str("Loading..."));
    loading_item.setEnabled(false);
    menu.addItem(&loading_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&refresh_item(mtm, app));
    menu.addItem(&quit_item(mtm, app));
  });
}

pub fn populate_menu(
  menu: &NSMenu,
  mtm: MainThreadMarker,
  app: &AppDelegate,
  profile: &Option<ProfileResponse>,
  usage: &UsageResponse,
) {
  menu.removeAllItems();

  // Header with tier badge.
  let tier = profile.as_ref().map(|p| p.organization.rate_limit_tier);
  let header_item = NSMenuItem::new(mtm);
  let header_view = components::header_row(mtm, "Claude Usage", tier);
  header_item.setView(Some(&header_view));
  menu.addItem(&header_item);

  // Usage limits.
  if let Some(bucket) = &usage.five_hour {
    menu.addItem(&components::bucket_row(mtm, "5h Limit", bucket));
  }

  // 7-day overall usage.
  if let Some(bucket) = &usage.seven_day {
    menu.addItem(&components::bucket_row(mtm, "7d Limit", bucket));
  }

  // 7-day sonnet usage.
  if let Some(bucket) = &usage.seven_day_sonnet {
    menu.addItem(&components::bucket_row(mtm, "7d Sonnet", bucket));
  }

  // 7-day opus usage.
  if let Some(bucket) = &usage.seven_day_opus {
    menu.addItem(&components::bucket_row(mtm, "7d Opus", bucket));
  }

  // Extra usage / credits.
  if let Some(extra) = &usage.extra_usage
    && extra.is_enabled
  {
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let limit = extra.monthly_limit / 100.0;
    let used = extra.used_credits / 100.0;

    let header_view = components::label_row(mtm, "Extra Usage", true);
    let header_item = NSMenuItem::new(mtm);
    header_item.setView(Some(&header_view));
    menu.addItem(&header_item);

    let value_text = format!("${:.2} / ${:.2}", used, limit);
    let used_view = components::key_value_row(mtm, "Spent", &value_text);
    let used_item = NSMenuItem::new(mtm);
    used_item.setView(Some(&used_view));
    menu.addItem(&used_item);
  }

  // Separator + Refresh + Quit.
  menu.addItem(&NSMenuItem::separatorItem(mtm));
  menu.addItem(&refresh_item(mtm, app));
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
