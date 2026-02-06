use std::cell::RefCell;

use objc2::{
  DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
  rc::Retained,
  runtime::{AnyObject, NSObject},
  sel,
};
use objc2_app_kit::{
  NSApplication, NSApplicationDelegate, NSFont, NSFontWeightSemibold, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
  NSVariableStatusItemLength,
};
use objc2_foundation::{NSDefaultRunLoopMode, NSNotification, NSObjectProtocol, NSRunLoop, NSString, NSTimer};

use crate::{
  api::{ApiClient, ProfileResponse, UsageResponse},
  util::format_reset_time,
  views,
};

pub struct AppDelegateIvars {
  api: ApiClient,
  status_item: RefCell<Option<Retained<NSStatusItem>>>,
  timer: RefCell<Option<Retained<NSTimer>>>,
}

define_class!(
  #[unsafe(super(NSObject))]
  #[thread_kind = MainThreadOnly]
  #[name = "AppDelegate"]
  #[ivars = AppDelegateIvars]
  pub struct AppDelegate;

  impl AppDelegate {
    #[unsafe(method(onTimer:))]
    fn on_timer(&self, _timer: &NSTimer) {
      self.refresh();
    }

    #[unsafe(method(onQuit:))]
    fn on_quit(&self, _sender: &AnyObject) {
      let app = NSApplication::sharedApplication(MainThreadMarker::from(self));
      app.terminate(None);
    }
  }

  unsafe impl NSObjectProtocol for AppDelegate {}

  unsafe impl NSApplicationDelegate for AppDelegate {
    #[unsafe(method(applicationDidFinishLaunching:))]
    fn did_finish_launching(&self, _notification: &NSNotification) {
      let mtm = MainThreadMarker::from(self);

      // Create status bar item
      let status_bar = NSStatusBar::systemStatusBar();
      let status_item =
        status_bar.statusItemWithLength(NSVariableStatusItemLength);

      // Set initial title
      if let Some(button) = status_item.button(mtm) {
        button.setTitle(&NSString::from_str("L: ..."));
        let font = NSFont::monospacedSystemFontOfSize_weight(12.0, unsafe { NSFontWeightSemibold });
        button.setFont(Some(&font));
      }

      *self.ivars().status_item.borrow_mut() = Some(status_item);

      // First refresh + schedule timer
      self.refresh();

      let timer = unsafe {
        NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(60.0, self, sel!(onTimer:), None, true)
      };
      unsafe { NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSDefaultRunLoopMode) };
      *self.ivars().timer.borrow_mut() = Some(timer);
    }
  }
);

impl AppDelegate {
  pub fn new(mtm: MainThreadMarker, api: ApiClient) -> Retained<Self> {
    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars {
      api,
      status_item: RefCell::new(None),
      timer: RefCell::new(None),
    });

    unsafe { msg_send![super(this), init] }
  }

  fn refresh(&self) {
    let api = &self.ivars().api;
    let usage = api.fetch_usage();
    let profile = api.fetch_profile();

    self.update_ui(usage, profile);
  }

  fn update_ui(&self, usage: Option<UsageResponse>, profile: Option<ProfileResponse>) {
    let mtm = MainThreadMarker::from(self);
    let status_item_ref = self.ivars().status_item.borrow();
    let Some(status_item) = status_item_ref.as_ref()
    else {
      return;
    };

    let Some(usage) = usage
    else {
      if let Some(button) = status_item.button(mtm) {
        button.setTitle(&NSString::from_str("L: ??"));
      }
      return;
    };

    // Update tray title with the most relevant utilization
    let five_h = usage.five_hour.as_ref().map(|b| b.utilization).unwrap_or(0.0);
    let seven_d = usage.seven_day.as_ref().map(|b| b.utilization).unwrap_or(0.0);
    let display_pct = five_h.max(seven_d);

    let title = format!("L: {}%", display_pct as u32);
    if let Some(button) = status_item.button(mtm) {
      button.setTitle(&NSString::from_str(&title));
    }

    // Build menu
    let menu = NSMenu::new(mtm);

    // Header with tier badge
    let tier = profile.as_ref().map(|p| p.organization.rate_limit_tier);
    let header_item = NSMenuItem::new(mtm);
    let header_view = views::make_header_row(mtm, "Claude Usage", tier);
    header_item.setView(Some(&header_view));
    menu.addItem(&header_item);

    // 5-hour usage
    if let Some(ref bucket) = usage.five_hour {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = views::make_progress_row(mtm, "5h Limit", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day overall usage
    if let Some(ref bucket) = usage.seven_day {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = views::make_progress_row(mtm, "7d Limit", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day sonnet usage
    if let Some(ref bucket) = usage.seven_day_sonnet {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = views::make_progress_row(mtm, "7d Sonnet", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day opus usage
    if let Some(ref bucket) = usage.seven_day_opus {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = views::make_progress_row(mtm, "7d Opus", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // Extra usage / credits
    if let Some(ref extra) = usage.extra_usage
      && extra.is_enabled
    {
      menu.addItem(&NSMenuItem::separatorItem(mtm));

      let limit = extra.monthly_limit as f64 / 100.0;
      let used = extra.used_credits / 100.0;

      let header_view = views::make_label_row(mtm, "Extra Usage", true);
      let header_item = NSMenuItem::new(mtm);
      header_item.setView(Some(&header_view));
      menu.addItem(&header_item);

      let value_text = format!("${:.2} / ${:.2}", used, limit);
      let used_view = views::make_key_value_row(mtm, "Spent", &value_text);
      let used_item = NSMenuItem::new(mtm);
      used_item.setView(Some(&used_view));
      menu.addItem(&used_item);
    }

    // Separator + Quit
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit_item = unsafe {
      NSMenuItem::initWithTitle_action_keyEquivalent(
        mtm.alloc::<NSMenuItem>(),
        &NSString::from_str("Quit"),
        Some(sel!(onQuit:)),
        &NSString::from_str("q"),
      )
    };
    unsafe { quit_item.setTarget(Some(self)) };
    menu.addItem(&quit_item);

    status_item.setMenu(Some(&menu));
  }
}
