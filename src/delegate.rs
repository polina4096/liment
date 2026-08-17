use std::{cell::RefCell, ffi::c_void, sync::Arc};

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
  AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
  rc::Retained,
  runtime::{AnyObject, Bool, NSObject},
};
use objc2_app_kit::{
  NSApplication, NSApplicationDelegate, NSAttributedStringNSStringDrawing, NSColor, NSCompositingOperation, NSFont,
  NSFontAttributeName, NSFontWeightSemibold, NSForegroundColorAttributeName, NSImage, NSRectFillUsingOperation,
  NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSWindow,
};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{
  NSAttributedString, NSData, NSMutableAttributedString, NSNotification, NSObjectProtocol, NSRange, NSRect, NSSize,
  NSString, NSTimer,
};
use strum::IntoEnumIterator as _;
use tap::Tap;

use crate::{
  CONFIG_PATH,
  config::{Config, DisplayMode},
  constants::LIMENT_DEBUG_REFETCH_INTERVAL,
  profile_cache::ProfileCache,
  providers::{DataProvider, NullProvider, ProviderKind, TierInfo, UsageData, UsageWindow, debug::DebugProvider},
  ui::views,
  updater::{self, UpdateState, Updater},
  utils::{codesign, log::LOG_DIR, macos::schedule_timer, notification, toml::serialize_to_item},
};

struct TrayBucket<'a> {
  text: &'a str,
  utilization: f64,
  warn: bool,
}

/// How many usage lines fit next to the tray logo.
const TRAY_MAX_LINES: usize = 2;

pub struct AppDelegateIvars {
  /// Provider to fetch usage data.
  provider: RefCell<Arc<dyn DataProvider>>,

  /// Cached profile tier info per provider, shared with background threads.
  profile_cache: Arc<ProfileCache>,

  /// Status bar item for displaying the current usage.
  status_item: Retained<NSStatusItem>,

  /// Hot-reloadable configuration.
  config: RefCell<Config>,

  /// Auto-updater.
  updater: Updater,

  /// Retained about window (kept alive so it doesn't get deallocated).
  about_window: RefCell<Option<Retained<NSWindow>>>,
}

impl AppDelegateIvars {
  pub fn provider(&self) -> std::cell::Ref<'_, Arc<dyn DataProvider>> {
    return self.provider.borrow();
  }

  pub fn config(&self) -> std::cell::Ref<'_, Config> {
    return self.config.borrow();
  }

  pub fn update_state(&self) -> std::cell::Ref<'_, UpdateState> {
    return self.updater.state();
  }
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
      if let Err(e) = open::that(&*CONFIG_PATH) {
        log::error!("Failed to open config file: {}", e);
      }
    }

    #[unsafe(method(onOpenLogs:))]
    fn on_open_logs(&self, _sender: &AnyObject) {
      if let Err(e) = open::that(&*LOG_DIR) {
        log::error!("Failed to open logs directory: {}", e);
      }
    }

    #[unsafe(method(onCheckForUpdates:))]
    fn on_check_for_updates(&self, _sender: &AnyObject) {
      self.attempt_update(true);
    }

    #[unsafe(method(onInstallUpdate:))]
    fn on_install_update(&self, _sender: &AnyObject) {
      self.install_update();
    }

    #[unsafe(method(onAbout:))]
    fn on_about(&self, _sender: &AnyObject) {
      let mtm = self.mtm();

      let mut window_ref = self.ivars().about_window.borrow_mut();
      if let Some(window) = window_ref.as_ref() && window.isVisible() {
          window.makeKeyAndOrderFront(None);
          #[allow(deprecated)]
          NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
          return;
      }

      let window = crate::ui::about::build_about_window(mtm, self);

      #[allow(deprecated)]
      NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);

      window.makeKeyAndOrderFront(None);
      *window_ref = Some(window);
    }

    #[unsafe(method(onOpenIssues:))]
    fn on_open_issues(&self, _sender: &AnyObject) {
      let _ = open::that("https://github.com/polina4096/liment/issues");
    }

    #[unsafe(method(onOpenSource:))]
    fn on_open_source(&self, _sender: &AnyObject) {
      let _ = open::that("https://github.com/polina4096/liment");
    }

    #[unsafe(method(onChangeProvider:))]
    fn on_change_provider(&self, sender: &AnyObject) {
      let tag: isize = unsafe { msg_send![sender, tag] };
      let Some(kind) = ProviderKind::iter()
        .filter(|k| *k != ProviderKind::Unknown)
        .nth(tag as usize)
      else {
        return;
      };

      self.change_provider(kind);
    }

  }

  unsafe impl NSObjectProtocol for AppDelegate {}

  unsafe impl NSApplicationDelegate for AppDelegate {
    #[unsafe(method(applicationDidFinishLaunching:))]
    fn did_finish_launching(&self, _notification: &NSNotification) {
      // Request notification permissions.
      notification::request_authorization();

      // Auto-codesign if enabled and not already signed.
      if self.ivars().config().auto_codesign && codesign::ensure_signed() {
        NSApplication::sharedApplication(self.mtm()).terminate(None);
        return;
      }

      // First refresh.
      self.refresh();

      // Check for updates on startup if enabled.
      if self.ivars().config().check_updates {
        self.attempt_update(false);
      }

      let refetch_interval = std::env::var(LIMENT_DEBUG_REFETCH_INTERVAL)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(self.ivars().config().refetch_interval as f64);

      // Refresh UI periodically.
      schedule_timer!(refetch_interval, self, onTimer);
    }
  }
);

impl AppDelegate {
  pub fn new(mtm: MainThreadMarker, config: Config) -> Retained<Self> {
    let status_bar = NSStatusBar::systemStatusBar();
    let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    let provider = Self::provider_from_config(&config);

    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars {
      provider: RefCell::new(provider),
      profile_cache: Arc::new(ProfileCache::default()),
      status_item,
      config: RefCell::new(config),
      updater: Updater::new(),
      about_window: RefCell::new(None),
    });
    let this: Retained<Self> = unsafe { msg_send![super(this), init] };

    // Set initial menu so the tray is interactive while loading.
    let loading_menu = views::loading_menu(mtm, &this);
    this.ivars().status_item.setMenu(Some(&loading_menu));

    return this;
  }

  fn change_provider(&self, kind: ProviderKind) {
    if kind == self.ivars().provider().kind() {
      return;
    }

    // Update the config file on disk.
    let config_str = match fs_err::read_to_string(&*CONFIG_PATH) {
      Ok(s) => s,
      Err(e) => {
        log::error!("Failed to read config: {e}");
        return;
      }
    };

    let mut doc: toml_edit::DocumentMut = match config_str.parse() {
      Ok(d) => d,
      Err(e) => {
        log::error!("Failed to parse config: {e}");
        return;
      }
    };

    doc["provider"] = serialize_to_item(kind);

    if let Err(e) = fs_err::write(&*CONFIG_PATH, doc.to_string()) {
      log::error!("Failed to write config: {e}");
    }

    // The file watcher will pick up the change and call reload_config.
  }

  pub fn reload_config(&self, new_config: Config) {
    // Auto-codesign if enabled and not already signed.
    if new_config.auto_codesign && codesign::ensure_signed() {
      NSApplication::sharedApplication(self.mtm()).terminate(None);
      return;
    }

    let provider = Self::provider_from_config(&new_config);
    *self.ivars().provider.borrow_mut() = provider;
    *self.ivars().config.borrow_mut() = new_config;

    // Update the provider checkmark immediately so it reflects the actual provider,
    // even if the fetch hasn't completed yet (or returns None for NullProvider).
    let mtm = self.mtm();
    if let Some(menu) = self.ivars().status_item.menu(mtm) {
      let current = self.ivars().provider().kind();
      views::update_provider_item(&menu, mtm, self, current);
    }

    self.refresh();
  }

  fn provider_from_config(config: &Config) -> Arc<dyn DataProvider> {
    let provider = match config.provider.into_provider(&config.settings, config.cli_keychain) {
      Ok(provider) => provider,
      Err(e) => {
        let msg = format!("Failed to create provider: {e:#}");
        log::error!("{msg}");
        notification::send_error(&msg);
        Arc::new(NullProvider)
      }
    };

    match DebugProvider::try_wrap(provider.clone()) {
      Some(debug) => Arc::new(debug),
      None => provider,
    }
  }

  /// Refetches latest data from the API and updates the UI.
  fn refresh(&self) {
    let provider = Arc::clone(&self.ivars().provider());
    let profile_cache = Arc::clone(&self.ivars().profile_cache);
    let mtm = self.mtm();
    let this = MainThreadBound::new(self.retain(), mtm);

    std::thread::spawn(move || {
      let data = provider.fetch_data();
      let profile = profile_cache.resolve(&*provider);

      DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("Must be on main thread");

        this.get(mtm).rebuild_ui(data.as_ref(), profile.as_ref());
      });
    });
  }

  /// Checks for updates on a background thread, updates state and menu when done.
  /// If `reopen_menu` is true, reopens the menu when an update is available.
  fn attempt_update(&self, reopen_menu: bool) {
    let mtm = self.mtm();
    let this = MainThreadBound::new(self.retain(), mtm);

    std::thread::spawn(move || {
      let new_state = updater::check_for_update();

      DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("Must be on main thread");
        let delegate = this.get(mtm);
        let is_available = matches!(&new_state, UpdateState::Available { .. });
        let should_auto_install = is_available && delegate.ivars().config().auto_update;

        delegate.ivars().updater.set_state(new_state);
        delegate.rebuild_update_menu();

        if should_auto_install {
          delegate.install_update();
          return;
        }

        if reopen_menu && is_available {
          let Some(button) = delegate.ivars().status_item.button(mtm)
          else {
            return;
          };

          unsafe { button.performClick(None) };
        }
      });
    });
  }

  /// Installs the available update on a background thread.
  fn install_update(&self) {
    let state = self.ivars().updater.state().clone();

    let url = match &state {
      UpdateState::Available { download_url, .. } => download_url.clone(),
      _ => return,
    };

    self.ivars().updater.set_state(UpdateState::Downloading);
    self.rebuild_update_menu();

    let mtm = self.mtm();
    let this = MainThreadBound::new(self.retain(), mtm);

    std::thread::spawn(move || {
      if let Err(e) = updater::download_and_install(&url) {
        let msg = format!("{e:#}");
        log::error!("Update failed: {msg}");

        DispatchQueue::main().exec_async(move || {
          let mtm = MainThreadMarker::new().expect("Must be on main thread");
          let delegate = this.get(mtm);

          delegate.ivars().updater.set_state(UpdateState::Failed { error: msg });
          delegate.rebuild_update_menu();
        });
      }
    });
  }

  fn rebuild_update_menu(&self) {
    let mtm = MainThreadMarker::from(self);
    let status_item = &self.ivars().status_item;

    if let Some(menu) = status_item.menu(mtm) {
      let update_state = self.ivars().update_state();
      views::update_update_item(&menu, mtm, self, &update_state);
    }
  }

  fn rebuild_ui(&self, data: Option<&UsageData>, profile: Option<&TierInfo>) {
    let mtm = MainThreadMarker::from(self);
    let status_item = &self.ivars().status_item;

    let config = self.ivars().config();
    let tray_icon_svg = self.ivars().provider().tray_icon_svg();

    let Some(data) = data
    else {
      if let Some(tray_button) = status_item.button(mtm) {
        let line_count = if config.tray_windows.is_empty() {
          TRAY_MAX_LINES
        }
        else {
          config.tray_windows.len().min(TRAY_MAX_LINES)
        };

        // A single window is stacked (label over value), so its placeholder is stacked too.
        let texts: &[&str] = if line_count == 1 { &["--", "--%"] } else { &["-- --", "-- --"] };

        let buckets: Vec<TrayBucket> = texts
          .iter()
          .map(|text| {
            return TrayBucket { text, utilization: 0.0, warn: false };
          })
          .collect();

        let img =
          Self::build_tray_image(tray_icon_svg, &buckets, line_count == 1, config.monochrome_icon, config.stats_colors);

        tray_button.setImage(Some(&img));
      }
      return;
    };

    if let Some(tray_button) = status_item.button(mtm) {
      let tray_windows = Self::select_tray_windows(&data.windows, &config.tray_windows);
      let is_remaining = config.display_mode == DisplayMode::Remaining;

      let values: Vec<i64> = tray_windows
        .iter()
        .map(|w| {
          let pct = if is_remaining { 100.0 - w.utilization } else { w.utilization };
          return pct as i64;
        })
        .collect();

      let width = values.iter().map(|v| v.max(&1).ilog10() as usize + 1).max().unwrap_or(1);

      // A single window is rendered stacked ("5h" over "94%") to keep the tray narrow;
      // several windows get one "label value" line each.
      let stacked = tray_windows.len() == 1;

      let lines: Vec<String> = if stacked {
        vec![
          tray_windows[0].short_title.clone().unwrap_or_else(|| "--".to_string()),
          format!("{}%", values[0]),
        ]
      }
      else {
        tray_windows
          .iter()
          .zip(&values)
          .map(|(w, v)| {
            return format!("{} {:>width$}%", w.short_title.as_deref().unwrap_or("--"), v);
          })
          .collect()
      };

      let tray_warn_enabled = config.show_tray_pacing_warning;
      let buckets: Vec<TrayBucket> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
          // Stacked mode draws one window across two lines, so both take its color and the
          // warning sits on the value line.
          let window = if stacked { tray_windows[0] } else { tray_windows[i] };
          let warn = tray_warn_enabled && window.is_pacing_warning() && (!stacked || i == 1);

          return TrayBucket {
            text: line,
            utilization: window.utilization / 100.0,
            warn,
          };
        })
        .collect();

      let placeholder = [TrayBucket {
        text: "-- --",
        utilization: 0.0,
        warn: false,
      }];
      let buckets = if buckets.is_empty() { &placeholder[..] } else { &buckets[..] };

      let img = Self::build_tray_image(tray_icon_svg, buckets, stacked, config.monochrome_icon, config.stats_colors);

      tray_button.setImage(Some(&img));
    }

    let menu = status_item.menu(mtm).unwrap_or_else(|| {
      return objc2_app_kit::NSMenu::new(mtm).tap(|menu| {
        status_item.setMenu(Some(menu));
      });
    });

    views::populate_menu(&menu, mtm, self, data, profile);
  }

  /// Picks the usage windows shown in the tray, honoring the `tray_windows` config list
  /// (matched against `short_title`, order preserved). Labels that don't resolve are skipped;
  /// if none of them do, falls back to the first available windows.
  fn select_tray_windows<'a>(windows: &'a [UsageWindow], wanted: &[String]) -> Vec<&'a UsageWindow> {
    let available = || return windows.iter().filter(|w| w.short_title.is_some());

    if !wanted.is_empty() {
      let picked: Vec<&UsageWindow> = wanted
        .iter()
        .take(TRAY_MAX_LINES)
        .filter_map(|label| {
          return available().find(|w| w.short_title.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(label)));
        })
        .collect();

      if !picked.is_empty() {
        return picked;
      }

      log::warn!("None of the configured tray_windows {wanted:?} matched, falling back to defaults");
    }

    return available().take(TRAY_MAX_LINES).collect();
  }

  /// Builds a single tray line as an attributed string colored by utilization.
  fn build_attributed_line(text: &str, p: f64, font_size: f64, stats_colors: bool) -> Retained<NSAttributedString> {
    let font = NSFont::monospacedSystemFontOfSize_weight(font_size, unsafe { NSFontWeightSemibold });
    let str = NSString::from_str(text);

    let attr = unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &str, None) };

    // Wrap in mutable to add attributes.
    let result = NSMutableAttributedString::initWithAttributedString(NSMutableAttributedString::alloc(), &attr);
    // NSAttributedString indexes characters in UTF-16 code units, not bytes.
    let range = NSRange::new(0, text.encode_utf16().count());

    let color = if stats_colors { Self::utilization_color(p) } else { NSColor::controlTextColor() };
    unsafe {
      result.addAttribute_value_range(NSFontAttributeName, &font, range);
      result.addAttribute_value_range(NSForegroundColorAttributeName, &color, range);
    }

    // Upcast to immutable.
    return Retained::into_super(result);
  }

  /// Renders provider logo and two colored lines into an NSImage for the tray button.
  /// Using an image instead of an attributed title allows macOS to properly
  /// dim the content on inactive displays via menu bar compositing.
  fn build_tray_image(
    icon_svg: &'static [u8],
    buckets: &[TrayBucket],
    stacked: bool,
    monochrome_icon: bool,
    stats_colors: bool,
  ) -> Retained<NSImage> {
    // Two lines have to squeeze into the ~22pt menu bar, so they use a small font; a single
    // line gets the whole row and is sized close to the regular menu bar text instead.
    let single_line = buckets.len() <= 1;
    let (font_size, line_height) = if single_line { (12.5_f64, 15.0_f64) } else { (9.0_f64, 10.0_f64) };
    let warn_font_size = if single_line { 13.0 } else { 11.0 };

    let attrs: Vec<Retained<NSAttributedString>> = buckets
      .iter()
      .map(|b| Self::build_attributed_line(b.text, b.utilization, font_size, stats_colors))
      .collect();

    // Pre-build the warning character once if any line needs it; we use its measured size
    // to reserve space in the tray image layout and draw it in the block.
    let any_warn = buckets.iter().any(|b| b.warn);
    let warn_attr: Option<Retained<NSAttributedString>> =
      if any_warn { Some(Self::build_warning_char(warn_font_size)) } else { None };
    let warn_size = warn_attr.as_ref().map(|a| a.size()).unwrap_or(NSSize::new(0.0, 0.0));
    let warn_width = warn_size.width;
    let warn_height = warn_size.height;

    const TRI_PADDING: f64 = 6.0;
    let line_widths: Vec<f64> = attrs.iter().map(|a| a.size().width).collect();

    // Find longest line width (text + optional trailing warning glyph).
    let block_widths: Vec<f64> = line_widths
      .iter()
      .zip(buckets)
      .map(|(w, b)| return w + if b.warn { TRI_PADDING + warn_width } else { 0.0 })
      .collect();
    let text_width = block_widths.iter().fold(0.0_f64, |acc, w| return acc.max(*w)).ceil();

    let line_count = attrs.len().max(1) as f64;

    // Logo size and padding.
    let logo_size = 14.0_f64;
    let logo_padding = 8.0_f64;

    // Offset for text: logo width + x padding.
    let text_x = logo_size + logo_padding;

    // Total button image size. A single line is shorter than the logo, so the image keeps
    // the logo height as a floor and the text block is centered inside it.
    let (width, height) = (text_x + text_width, (line_height * line_count).max(logo_size));
    let text_bottom = (height - line_height * line_count) / 2.0;
    let image_size = NSSize::new(width, height);

    // Load provider logo from embedded SVG.
    let logo_data = unsafe { NSData::dataWithBytes_length(icon_svg.as_ptr() as *const c_void, icon_svg.len()) };
    let logo_img = NSImage::initWithData(NSImage::alloc(), &logo_data).expect("failed to load provider logo");

    logo_img.setSize(NSSize::new(logo_size, logo_size));

    let warn_flags: Vec<bool> = buckets.iter().map(|b| b.warn).collect();

    // If the warn glyph box is taller than a single line row, push the top-line draw point
    // down by half the overhang. The other half of the overflow sits in the glyph's empty
    // ascender padding (⚠ has no pixels near the top of its metric box), so a half-shift
    // keeps the visible glyph vertically centered in the line without clipping.
    let warn_y_overhang = (warn_height - line_height).max(0.0) / 2.0;

    let block = RcBlock::new(move |_rect: NSRect| -> Bool {
      // Draw logo on the left, vertically centered.
      let logo_y = (height - logo_size) / 2.0;
      let logo_rect = NSRect::new(CGPoint::new(0.0, logo_y), NSSize::new(logo_size, logo_size));
      logo_img.drawInRect(logo_rect);

      // Tint the logo to match the system text color by filling with SourceIn compositing,
      // which replaces the color of non-transparent pixels while preserving alpha.
      if monochrome_icon {
        NSColor::controlTextColor().setFill();
        NSRectFillUsingOperation(logo_rect, NSCompositingOperation::SourceIn);
      }

      // Draw text lines to the right of the logo, top to bottom. Stacked lines are centered
      // against each other, inline ones are left-aligned (their columns already line up).
      for (i, attr) in attrs.iter().enumerate() {
        let y = text_bottom + (attrs.len() - 1 - i) as f64 * line_height;
        let x = if stacked { text_x + (text_width - block_widths[i]) / 2.0 } else { text_x };
        attr.drawAtPoint(CGPoint::new(x, y));

        // Draw a yellow warning glyph at the end of each line that needs one. The top line is
        // shifted down by the warn glyph's vertical overhang so its top edge fits in the image.
        if let (Some(wa), true) = (warn_attr.as_ref(), warn_flags[i]) {
          let shift = if i == 0 { warn_y_overhang } else { 0.0 };
          wa.drawAtPoint(CGPoint::new(x + line_widths[i] + TRI_PADDING, y - shift));
        }
      }

      return Bool::YES;
    });

    let img = NSImage::imageWithSize_flipped_drawingHandler(image_size, false, &block);

    return img;
  }

  /// Builds a yellow, bold `⚠` attributed string used as the pacing-warning indicator
  /// next to a tray line.
  fn build_warning_char(font_size: f64) -> Retained<NSAttributedString> {
    let font = NSFont::systemFontOfSize_weight(font_size, unsafe { NSFontWeightSemibold });
    let str = NSString::from_str("⚠");

    let attr = unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &str, None) };
    let result = NSMutableAttributedString::initWithAttributedString(NSMutableAttributedString::alloc(), &attr);

    let range = NSRange::new(0, "⚠".encode_utf16().count());
    unsafe {
      result.addAttribute_value_range(NSFontAttributeName, &font, range);
      result.addAttribute_value_range(NSForegroundColorAttributeName, &NSColor::yellowColor(), range);
    }

    return Retained::into_super(result);
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
