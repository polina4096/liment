use std::cell::OnceCell;

use objc2::{
  DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
  rc::Retained,
  runtime::{AnyObject, NSObject},
  sel,
};
use objc2_app_kit::{
  NSApplication, NSApplicationDelegate, NSFont, NSFontWeightSemibold, NSStatusBar, NSStatusItem,
  NSVariableStatusItemLength,
};
use objc2_foundation::{NSDefaultRunLoopMode, NSNotification, NSObjectProtocol, NSRunLoop, NSString, NSTimer};

use crate::{
  api::{ApiClient, ProfileResponse, UsageResponse},
  views,
};

pub struct AppDelegateIvars {
  /// A client to fetch information from the API.
  api: ApiClient,

  /// Status bar item for displaying the current usage.
  status_item: Retained<NSStatusItem>,

  /// Timer for refreshing the status bar item.
  refresh_timer: OnceCell<Retained<NSTimer>>,
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
      // First refresh + schedule timer.
      self.refresh();

      let timer = unsafe {
        NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(60.0, self, sel!(onTimer:), None, true)
      };

      unsafe { NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSDefaultRunLoopMode) };

      self.ivars().refresh_timer.set(timer).expect("Failed to set refresh timer.");
    }
  }
);

impl AppDelegate {
  pub fn new(mtm: MainThreadMarker, api: ApiClient) -> Retained<Self> {
    let status_bar = NSStatusBar::systemStatusBar();
    let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    // Setup the app tray button.
    if let Some(button) = status_item.button(mtm) {
      // Initialize the tray button with a loading placeholder.
      button.setTitle(&NSString::from_str("L: ..."));

      let font = NSFont::monospacedSystemFontOfSize_weight(12.0, unsafe { NSFontWeightSemibold });

      button.setFont(Some(&font));
    }

    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars {
      api,
      status_item,
      refresh_timer: OnceCell::new(),
    });

    return unsafe { msg_send![super(this), init] };
  }

  /// Refetches latest data from the API and updates the UI.
  fn refresh(&self) {
    let api = &self.ivars().api;
    let usage = api.fetch_usage();
    let profile = api.fetch_profile();

    self.rebuild_ui(usage, profile);
  }

  /// Rebuilds the UI.
  fn rebuild_ui(&self, usage: Option<UsageResponse>, profile: Option<ProfileResponse>) {
    let mtm = MainThreadMarker::from(self);
    let status_item = &self.ivars().status_item;

    let Some(usage) = usage
    else {
      // Handle failure to fetch usage data and reflect it in the tray button.
      if let Some(tray_button) = status_item.button(mtm) {
        tray_button.setTitle(&NSString::from_str("L: ??"));
      }

      return;
    };

    // Update tray title with the most relevant utilization.
    if let Some(tray_button) = status_item.button(mtm) {
      let five_h = usage.five_hour.as_ref().map(|b| b.utilization).unwrap_or(0.0);
      let seven_d = usage.seven_day.as_ref().map(|b| b.utilization).unwrap_or(0.0);
      let display_pct = five_h.max(seven_d);

      let title = format!("L: {}%", display_pct as u32);
      tray_button.setTitle(&NSString::from_str(&title));
    }

    // Build and update the tray menu.
    let menu_view = views::menu(mtm, self, &profile, &usage);
    status_item.setMenu(Some(&menu_view));
  }
}
