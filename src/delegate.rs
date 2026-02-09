use std::ffi::c_void;
use std::sync::Arc;

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
  AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
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
use tap::Tap;

use crate::{
  CliArgs,
  config::{self, AppConfig},
  providers::{UsageData, UsageProvider},
  utils::macos::schedule_timer,
  views,
};

pub struct AppDelegateIvars {
  /// Provider to fetch usage data.
  provider: Arc<dyn UsageProvider>,

  /// Status bar item for displaying the current usage.
  status_item: Retained<NSStatusItem>,

  /// Configuration options from the command line arguments.
  args: CliArgs,

  /// Whether to render the tray icon in monochrome.
  monochrome_icon: bool,

  /// Display mode: "usage" or "remaining".
  pub display_mode: String,

  /// Whether to show period percentage next to "resets in".
  pub show_period_percentage: bool,

  /// Reset time format: "relative" or "absolute".
  pub reset_time_format: String,

  /// Refetch interval in seconds.
  pub refetch_interval: f64,
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
      let app = NSApplication::sharedApplication(self.mtm());

      app.terminate(None);
    }

    #[unsafe(method(onRefresh:))]
    fn on_refresh(&self, _sender: &AnyObject) {
      self.refresh();

      // Reopen the menu so the user sees the update in-place.
      let mtm = self.mtm();
      if let Some(button) = self.ivars().status_item.button(mtm) {
        unsafe { button.performClick(None) };
      }
    }

    #[unsafe(method(onOpenConfig:))]
    fn on_open_config(&self, _sender: &AnyObject) {
      let path = config::get_config_path();
      let url = objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
      let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
      workspace.activateFileViewerSelectingURLs(&objc2_foundation::NSArray::from_retained_slice(&[url]));
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
          self.ivars().monochrome_icon,
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

      // Refresh UI periodically.
      schedule_timer!(self.ivars().refetch_interval, self, onTimer);

      // Debug: cycle colors every 0.5s (20 steps over ~10s).
      if self.ivars().args.cycle_colors {
        schedule_timer!(0.5, self, onDebugTimer);
      }
    }
  }
);

impl AppDelegate {
  pub fn new(mtm: MainThreadMarker, args: CliArgs, config: &AppConfig) -> Retained<Self> {
    let status_bar = NSStatusBar::systemStatusBar();
    let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    // Setup the app tray button with a loading placeholder.
    if let Some(button) = status_item.button(mtm) {
      let ph = config.menubar_provider.placeholder_lines();
      let img =
        Self::build_tray_image(&format!("{} ..", ph[0]), 0.0, &format!("{} ..", ph[1]), 0.0, config.monochrome_icon);

      button.setImage(Some(&img));
      button.setTitle(&NSString::new());
    }

    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars {
      provider: Arc::clone(&config.menubar_provider),
      status_item,
      args,
      monochrome_icon: config.monochrome_icon,
      display_mode: config.display_mode.clone(),
      show_period_percentage: config.show_period_percentage,
      reset_time_format: config.reset_time_format.clone(),
      refetch_interval: config.refetch_interval,
    });
    let this: Retained<Self> = unsafe { msg_send![super(this), init] };

    // Set initial menu so the tray is interactive while loading.
    let loading_menu = views::loading_menu(mtm, &this);
    this.ivars().status_item.setMenu(Some(&loading_menu));

    return this;
  }

  /// Refetches latest data from the API and updates the UI.
  fn refresh(&self) {
    let provider = Arc::clone(&self.ivars().provider);
    let mtm = self.mtm();
    let this = MainThreadBound::new(self.retain(), mtm);

    std::thread::spawn(move || {
      let data = provider.fetch_data();

      DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("Must be on main thread.");
        this.get(mtm).rebuild_ui(data);
      });
    });
  }

  fn rebuild_ui(&self, data: Option<UsageData>) {
    let mtm = MainThreadMarker::from(self);
    let status_item = &self.ivars().status_item;

    let Some(data) = data else {
      if let Some(tray_button) = status_item.button(mtm) {
        let ph = self.ivars().provider.placeholder_lines();
        let img = Self::build_tray_image(
          &format!("{} --", ph[0]),
          0.0,
          &format!("{} --", ph[1]),
          0.0,
          self.ivars().monochrome_icon,
        );
        tray_button.setImage(Some(&img));
      }
      return;
    };

    if let Some(tray_button) = status_item.button(mtm) {
      // Use first two windows that have a short_title for tray display.
      let mut tray_windows = data.windows.iter().filter(|w| w.short_title.is_some());
      let w0 = tray_windows.next();
      let w1 = tray_windows.next();
      let is_remaining = self.ivars().display_mode == "remaining";
      let p0 = w0.map(|w| if is_remaining { 100.0 - w.utilization } else { w.utilization }).unwrap_or(0.0);
      let p1 = w1.map(|w| if is_remaining { 100.0 - w.utilization } else { w.utilization }).unwrap_or(0.0);

      let v0 = p0 as u32;
      let v1 = p1 as u32;
      let w = (v0.max(1).ilog10() as usize + 1).max(v1.max(1).ilog10() as usize + 1);

      let label0 = w0.and_then(|w| w.short_title.as_deref()).unwrap_or("--");
      let label1 = w1.and_then(|w| w.short_title.as_deref()).unwrap_or("--");
      let line1 = format!("{} {:>w$}%", label0, v0);
      let line2 = format!("{} {:>w$}%", label1, v1);

      let img = Self::build_tray_image(&line1, p0 / 100.0, &line2, p1 / 100.0, self.ivars().monochrome_icon);
      tray_button.setImage(Some(&img));
    }

    let menu = status_item.menu(mtm).unwrap_or_else(|| {
      return objc2_app_kit::NSMenu::new(mtm).tap(|menu| {
        status_item.setMenu(Some(&menu));
      });
    });

    views::populate_menu(&menu, mtm, self, &data);
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
  fn build_tray_image(line1: &str, p1: f64, line2: &str, p2: f64, monochrome: bool) -> Retained<NSImage> {
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
    if monochrome {
      logo_img.setTemplate(true);
    }

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

    let img = NSImage::imageWithSize_flipped_drawingHandler(image_size, false, &block);
    if monochrome {
      img.setTemplate(true);
    }
    return img;
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
