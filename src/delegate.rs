use std::ffi::c_void;

use block2::RcBlock;
use objc2::{
  AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
  rc::Retained,
  runtime::{AnyObject, Bool, NSObject},
};
use objc2_app_kit::{
  NSApplication, NSApplicationDelegate, NSAttributedStringNSStringDrawing, NSColor, NSFont, NSFontAttributeName,
  NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{
  NSAttributedString, NSData, NSMutableAttributedString, NSNotification, NSObjectProtocol, NSRange, NSRect, NSSize,
  NSString, NSTimer,
};

use crate::{
  CliArgs,
  api::{ApiClient, ProfileResponse, UsageResponse},
  util::schedule_timer,
  views,
};

pub struct AppDelegateIvars {
  /// A client to fetch information from the API.
  api: ApiClient,

  /// Status bar item for displaying the current usage.
  status_item: Retained<NSStatusItem>,

  /// Configuration options from the command line arguments.
  args: CliArgs,
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

    #[unsafe(method(onDebugTimer:))]
    fn on_debug_timer(&self, _timer: &NSTimer) {
      let mtm = self.mtm();

      if let Some(button) = self.ivars().status_item.button(mtm) {
        let secs = std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .unwrap_or_default()
          .as_secs_f64();

        let p1 = ((secs + 3.7) % 10.0) / 10.0;
        let p2 = (secs % 10.0) / 10.0;
        let v1 = (p1 * 100.0) as u32;
        let v2 = (p2 * 100.0) as u32;
        let w = (v1.max(1).ilog10() as usize + 1).max(v2.max(1).ilog10() as usize + 1);
        let img = Self::build_tray_image(
          &format!("7d {:>w$}%", v1), p1,
          &format!("5h {:>w$}%", v2), p2,
        );

        button.setImage(Some(&img));
      }
    }

  }

  unsafe impl NSObjectProtocol for AppDelegate {}

  unsafe impl NSApplicationDelegate for AppDelegate {
    #[unsafe(method(applicationDidFinishLaunching:))]
    fn did_finish_launching(&self, _notification: &NSNotification) {
      // First refresh.
      self.refresh();

      // Refresh UI every 60 seconds.
      schedule_timer!(60.0, self, onTimer);

      // Debug: cycle colors every 0.5s (20 steps over ~10s).
      if self.ivars().args.cycle_colors {
        schedule_timer!(0.5, self, onDebugTimer);
      }
    }
  }
);

impl AppDelegate {
  pub fn new(mtm: MainThreadMarker, api: ApiClient, args: CliArgs) -> Retained<Self> {
    let status_bar = NSStatusBar::systemStatusBar();
    let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    // Setup the app tray button with a loading placeholder.
    if let Some(button) = status_item.button(mtm) {
      let img = Self::build_tray_image("7d ..", 0.0, "5h ..", 0.0);

      button.setImage(Some(&img));
      button.setTitle(&NSString::new());
    }

    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars { api, status_item, args });

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
        let img = Self::build_tray_image("7d --", 0.0, "5h --", 0.0);

        tray_button.setImage(Some(&img));
      }

      return;
    };

    // Update tray title with per-bucket utilization, colored green-to-red.
    if let Some(tray_button) = status_item.button(mtm) {
      let five_h = usage.five_hour.as_ref().map(|b| b.utilization).unwrap_or(0.0);
      let seven_d = usage.seven_day.as_ref().map(|b| b.utilization).unwrap_or(0.0);

      let v1 = seven_d as u32;
      let v2 = five_h as u32;
      let w = (v1.max(1).ilog10() as usize + 1).max(v2.max(1).ilog10() as usize + 1);
      let line1 = format!("7d {:>w$}%", v1);
      let line2 = format!("5h {:>w$}%", v2);

      let five_p = five_h / 100.0;
      let seven_p = seven_d / 100.0;

      let img = Self::build_tray_image(&line1, five_p, &line2, seven_p);

      tray_button.setImage(Some(&img));
    }

    // Build and update the tray menu.
    let menu_view = views::menu(mtm, self, &profile, &usage);

    status_item.setMenu(Some(&menu_view));
  }

  /// Builds a two-line attributed string with per-line colors.
  fn build_attributed_line(text: &str, p: f64) -> Retained<NSAttributedString> {
    let font = NSFont::monospacedSystemFontOfSize_weight(9.0, unsafe { NSFontWeightSemibold });
    let str = NSString::from_str(text);

    let attr = unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &str, None) };

    // Wrap in mutable to add attributes.
    let result = NSMutableAttributedString::initWithAttributedString(NSMutableAttributedString::alloc(), &attr);
    let range = NSRange::new(0, str.len());

    unsafe {
      result.addAttribute_value_range(NSFontAttributeName, &font, range);
      result.addAttribute_value_range(NSForegroundColorAttributeName, &Self::utilization_color(p), range);
    }

    // Upcast to immutable.
    return Retained::into_super(result);
  }

  /// Renders the Claude logo and two colored lines into an NSImage for the tray button.
  /// Using an image instead of an attributed title allows macOS to properly
  /// dim the content on inactive displays via menu bar compositing.
  fn build_tray_image(line1: &str, p1: f64, line2: &str, p2: f64) -> Retained<NSImage> {
    let attr1 = Self::build_attributed_line(line1, p1);
    let attr2 = Self::build_attributed_line(line2, p2);

    let size1 = attr1.size();
    let size2 = attr2.size();

    // Find longest string width & text height.
    let text_width = size1.width.max(size2.width).ceil();
    let line_height = 10.0_f64;
    let text_height = line_height * 2.0;

    // Logo size and padding.
    let logo_size = 14.0_f64;
    let logo_padding = 8.0_f64;

    // Offset for text: logo width + x padding.
    let text_x = logo_size + logo_padding;

    // Total button image size.
    let (width, height) = (text_x + text_width, text_height);
    let image_size = NSSize::new(width, height);

    // Load the Claude logo from the embedded SVG.
    let svg_bytes = include_bytes!("../resources/claude.svg");
    let logo_data = unsafe { NSData::dataWithBytes_length(svg_bytes.as_ptr() as *const c_void, svg_bytes.len()) };
    let logo_img = NSImage::initWithData(NSImage::alloc(), &logo_data).expect("failed to load Claude logo");
    logo_img.setSize(NSSize::new(logo_size, logo_size));

    let block = RcBlock::new(move |_rect: NSRect| -> Bool {
      // Draw logo on the left, vertically centered.
      let logo_y = (height - logo_size) / 2.0;
      let logo_rect = NSRect::new(CGPoint::new(0.0, logo_y), NSSize::new(logo_size, logo_size));
      logo_img.drawInRect(logo_rect);

      // Draw text lines to the right of the logo.
      attr1.drawAtPoint(CGPoint::new(text_x, line_height));
      attr2.drawAtPoint(CGPoint::new(text_x, 0.0));

      return Bool::YES;
    });

    return NSImage::imageWithSize_flipped_drawingHandler(image_size, true, &block);
  }

  /// Returns a system catalog color based on utilization level.
  /// Uses catalog colors so macOS vibrancy compositing properly dims them on inactive displays.
  fn utilization_color(pct: f64) -> Retained<NSColor> {
    match pct {
      p if p < 0.5 => NSColor::controlTextColor(),
      p if p < 0.75 => NSColor::yellowColor(),
      p if p < 0.90 => NSColor::orangeColor(),
      _ => NSColor::redColor(),
    }
  }
}
