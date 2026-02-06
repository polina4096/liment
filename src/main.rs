use std::{cell::RefCell, process::Command, ptr::NonNull, thread};

use block2::RcBlock;
use objc2::{
  DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
  rc::Retained,
  runtime::{AnyObject, NSObject, ProtocolObject},
  sel,
};
use objc2_app_kit::{
  NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSColor, NSFont, NSMenu, NSMenuItem,
  NSProgressIndicator, NSProgressIndicatorStyle, NSStatusBar, NSStatusItem, NSTextField, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSDefaultRunLoopMode, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSSize, NSString, NSTimer};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
struct UsageResponse {
  five_hour: Option<UsageBucket>,
  seven_day: Option<UsageBucket>,
  seven_day_sonnet: Option<UsageBucket>,
  seven_day_opus: Option<UsageBucket>,
  extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize, Clone)]
struct UsageBucket {
  utilization: f64,
  resets_at: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraUsage {
  is_enabled: bool,
  monthly_limit: u64,
  used_credits: f64,
}

#[derive(Debug, Deserialize, Clone)]
struct ProfileResponse {
  organization: ProfileOrganization,
}

#[derive(Debug, Deserialize, Clone)]
struct ProfileOrganization {
  rate_limit_tier: String,
}

// ---------------------------------------------------------------------------
// Keychain + API helpers
// ---------------------------------------------------------------------------

fn read_access_token() -> Option<String> {
  let output = Command::new("security")
    .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let json_str = String::from_utf8(output.stdout).ok()?;
  let value: serde_json::Value = serde_json::from_str(json_str.trim()).ok()?;
  value.get("claudeAiOauth")?.get("accessToken")?.as_str().map(String::from)
}

fn fetch_usage(token: &str) -> Option<UsageResponse> {
  let mut response = ureq::get("https://api.anthropic.com/api/oauth/usage")
    .header("Authorization", &format!("Bearer {}", token))
    .header("anthropic-beta", "oauth-2025-04-20")
    .header("Content-Type", "application/json")
    .call()
    .ok()?;

  let body = response.body_mut().read_to_string().ok()?;
  serde_json::from_str(&body).ok()
}

fn fetch_profile(token: &str) -> Option<ProfileResponse> {
  let mut response = ureq::get("https://api.anthropic.com/api/oauth/profile")
    .header("Authorization", &format!("Bearer {}", token))
    .header("anthropic-beta", "oauth-2025-04-20")
    .header("Content-Type", "application/json")
    .call()
    .ok()?;

  let body = response.body_mut().read_to_string().ok()?;
  serde_json::from_str(&body).ok()
}

fn format_tier(tier: &str) -> &str {
  match tier {
    "default_claude_free" => "Free",
    "default_claude_pro" => "Pro",
    "default_claude_max_5x" => "Max 5x",
    "default_claude_max_20x" => "Max 20x",
    _ => tier,
  }
}

fn format_reset_time(resets_at: &str) -> String {
  use std::time::SystemTime;

  let Ok(reset) = parse_rfc3339_timestamp(resets_at)
  else {
    return resets_at.to_string();
  };

  let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
  let diff = reset - now;

  if diff <= 0 {
    return "now".to_string();
  }

  let days = diff / 86400;
  let hours = (diff % 86400) / 3600;
  let mins = (diff % 3600) / 60;

  if days > 0 {
    format!("{}d {}h", days, hours)
  }
  else if hours > 0 {
    format!("{}h {}m", hours, mins)
  }
  else {
    format!("{}m", mins)
  }
}

/// Minimal RFC 3339 parser returning UNIX timestamp.
fn parse_rfc3339_timestamp(s: &str) -> Result<i64, ()> {
  let s = s.trim();
  if s.len() < 19 {
    return Err(());
  }

  let year: i64 = s[0 .. 4].parse().map_err(|_| ())?;
  let month: i64 = s[5 .. 7].parse().map_err(|_| ())?;
  let day: i64 = s[8 .. 10].parse().map_err(|_| ())?;
  let hour: i64 = s[11 .. 13].parse().map_err(|_| ())?;
  let min: i64 = s[14 .. 16].parse().map_err(|_| ())?;
  let sec: i64 = s[17 .. 19].parse().map_err(|_| ())?;

  let tz_offset_secs = if s.ends_with('Z') {
    0i64
  }
  else {
    let tz_part = &s[s.len().saturating_sub(6) ..];
    if let Some(sign_pos) = tz_part.rfind('+').or_else(|| tz_part.rfind('-')) {
      let tz_str = &tz_part[sign_pos ..];
      let sign = if tz_str.starts_with('-') { -1i64 } else { 1i64 };
      let parts: Vec<&str> = tz_str[1 ..].split(':').collect();
      if parts.len() == 2 {
        let tz_h: i64 = parts[0].parse().map_err(|_| ())?;
        let tz_m: i64 = parts[1].parse().map_err(|_| ())?;
        sign * (tz_h * 3600 + tz_m * 60)
      }
      else {
        0
      }
    }
    else {
      0
    }
  };

  let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
  let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

  let mut days = 0i64;
  for y in 1970 .. year {
    let ly = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    days += if ly { 366 } else { 365 };
  }
  for m in 1 .. month {
    days += month_days[m as usize];
    if m == 2 && is_leap {
      days += 1;
    }
  }
  days += day - 1;

  Ok(days * 86400 + hour * 3600 + min * 60 + sec - tz_offset_secs)
}

// ---------------------------------------------------------------------------
// View builders for menu items
// ---------------------------------------------------------------------------

fn font_weight_regular() -> CGFloat {
  unsafe { objc2_app_kit::NSFontWeightRegular }
}

fn font_weight_medium() -> CGFloat {
  unsafe { objc2_app_kit::NSFontWeightMedium }
}

fn font_weight_semibold() -> CGFloat {
  unsafe { objc2_app_kit::NSFontWeightSemibold }
}

fn font_weight_light() -> CGFloat {
  unsafe { objc2_app_kit::NSFontWeightLight }
}

fn make_progress_row(mtm: MainThreadMarker, label: &str, utilization: f64, reset_str: &str) -> Retained<NSView> {
  let width: CGFloat = 280.0;
  let row_height: CGFloat = 48.0;

  let container =
    NSView::initWithFrame(mtm.alloc::<NSView>(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, row_height)));

  // Label: "5h Limit  8%"
  let label_text = format!("{}  {}%", label, utilization as u32);
  let label_field = NSTextField::labelWithString(&NSString::from_str(&label_text), mtm);
  label_field.setEditable(false);
  label_field.setBezeled(false);
  label_field.setDrawsBackground(false);
  label_field.setFrame(NSRect::new(NSPoint::new(14.0, 26.0), NSSize::new(width - 28.0, 17.0)));
  let font = NSFont::systemFontOfSize_weight(12.0, font_weight_regular());
  label_field.setFont(Some(&font));
  container.addSubview(&label_field);

  // Progress bar
  let progress = NSProgressIndicator::init(mtm.alloc::<NSProgressIndicator>());
  progress.setStyle(NSProgressIndicatorStyle::Bar);
  progress.setIndeterminate(false);
  progress.setMinValue(0.0);
  progress.setMaxValue(100.0);
  progress.setDoubleValue(utilization);
  progress.setFrame(NSRect::new(NSPoint::new(14.0, 6.0), NSSize::new(width - 28.0, 14.0)));
  container.addSubview(&progress);

  // Reset time label (right-aligned)
  let reset_text = format!("resets in {}", reset_str);
  let reset_field = NSTextField::labelWithString(&NSString::from_str(&reset_text), mtm);
  reset_field.setEditable(false);
  reset_field.setBezeled(false);
  reset_field.setDrawsBackground(false);
  let small_font = NSFont::systemFontOfSize_weight(10.0, font_weight_light());
  reset_field.setFont(Some(&small_font));
  reset_field.setAlignment(objc2_app_kit::NSTextAlignment::Right);
  reset_field.setFrame(NSRect::new(NSPoint::new(14.0, 26.0), NSSize::new(width - 28.0, 17.0)));
  reset_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  container.addSubview(&reset_field);

  container
}

fn make_header_row(mtm: MainThreadMarker, text: &str) -> Retained<NSView> {
  let width: CGFloat = 280.0;
  let container =
    NSView::initWithFrame(mtm.alloc::<NSView>(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, 26.0)));

  let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
  field.setEditable(false);
  field.setBezeled(false);
  field.setDrawsBackground(false);
  field.setFrame(NSRect::new(NSPoint::new(14.0, 2.0), NSSize::new(width - 28.0, 21.0)));

  let font = NSFont::systemFontOfSize_weight(14.0, font_weight_semibold());
  field.setFont(Some(&font));

  container.addSubview(&field);
  container
}

fn make_label_row(mtm: MainThreadMarker, text: &str, bold: bool) -> Retained<NSView> {
  let width: CGFloat = 280.0;
  let container =
    NSView::initWithFrame(mtm.alloc::<NSView>(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, 22.0)));

  let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
  field.setEditable(false);
  field.setBezeled(false);
  field.setDrawsBackground(false);
  field.setFrame(NSRect::new(NSPoint::new(14.0, 2.0), NSSize::new(width - 28.0, 17.0)));

  let weight = if bold { font_weight_medium() } else { font_weight_regular() };
  let font = NSFont::systemFontOfSize_weight(12.0, weight);
  field.setFont(Some(&font));

  container.addSubview(&field);
  container
}

// ---------------------------------------------------------------------------
// App delegate
// ---------------------------------------------------------------------------

struct AppDelegateIvars {
  status_item: RefCell<Option<Retained<NSStatusItem>>>,
  timer: RefCell<Option<Retained<NSTimer>>>,
}

define_class!(
  #[unsafe(super(NSObject))]
  #[thread_kind = MainThreadOnly]
  #[name = "AppDelegate"]
  #[ivars = AppDelegateIvars]
  struct AppDelegate;

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
    fn did_finish_launching(&self, _notification: &objc2_foundation::NSNotification) {
      let mtm = MainThreadMarker::from(self);

      // Create status bar item
      let status_bar = NSStatusBar::systemStatusBar();
      let status_item =
        status_bar.statusItemWithLength(objc2_app_kit::NSVariableStatusItemLength);

      // Set initial title
      if let Some(button) = status_item.button(mtm) {
        button.setTitle(&NSString::from_str("L: ..."));
        let font = NSFont::monospacedSystemFontOfSize_weight(12.0, font_weight_regular());
        button.setFont(Some(&font));
      }

      *self.ivars().status_item.borrow_mut() = Some(status_item);

      // Initial refresh
      self.refresh();

      // Set up a repeating timer (every 60 seconds)
      let timer = unsafe {
        NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
          60.0,
          self,
          sel!(onTimer:),
          None,
          true,
        )
      };
      unsafe { NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSDefaultRunLoopMode) };
      *self.ivars().timer.borrow_mut() = Some(timer);
    }
  }
);

impl AppDelegate {
  fn new(mtm: MainThreadMarker) -> Retained<Self> {
    let this = mtm.alloc::<AppDelegate>();
    let this = this.set_ivars(AppDelegateIvars {
      status_item: RefCell::new(None),
      timer: RefCell::new(None),
    });
    unsafe { msg_send![super(this), init] }
  }

  fn refresh(&self) {
    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
      let token = read_access_token();
      let usage = token.as_deref().and_then(fetch_usage);
      let profile = token.as_deref().and_then(fetch_profile);
      let _ = tx.send((usage, profile));
    });

    // Poll for the result without blocking the run loop.
    // The timer repeats every 0.25s until the background thread delivers a result.
    let delegate = self as *const AppDelegate;
    let block = RcBlock::new(move |timer: NonNull<NSTimer>| {
      if let Ok((usage, profile)) = rx.try_recv() {
        unsafe { timer.as_ref().invalidate() };
        let delegate = unsafe { &*delegate };
        delegate.update_ui(usage, profile);
      }
    });

    unsafe {
      let timer = NSTimer::timerWithTimeInterval_repeats_block(0.25, true, &block);
      NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSDefaultRunLoopMode);
    }
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

    // Header with tier
    let header_text = if let Some(ref profile) = profile {
      let tier = format_tier(&profile.organization.rate_limit_tier);
      format!("Claude Usage ({})", tier)
    }
    else {
      "Claude Usage".to_string()
    };
    let header_item = NSMenuItem::new(mtm);
    let header_view = make_header_row(mtm, &header_text);
    header_item.setView(Some(&header_view));
    menu.addItem(&header_item);

    // 5-hour usage
    if let Some(ref bucket) = usage.five_hour {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = make_progress_row(mtm, "5h Limit", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day overall usage
    if let Some(ref bucket) = usage.seven_day {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = make_progress_row(mtm, "7d Limit", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day sonnet usage
    if let Some(ref bucket) = usage.seven_day_sonnet {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = make_progress_row(mtm, "7d Sonnet", bucket.utilization, &reset_str);
      let item = NSMenuItem::new(mtm);
      item.setView(Some(&view));
      menu.addItem(&item);
    }

    // 7-day opus usage
    if let Some(ref bucket) = usage.seven_day_opus {
      let reset_str = format_reset_time(&bucket.resets_at);
      let view = make_progress_row(mtm, "7d Opus", bucket.utilization, &reset_str);
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

      let header_view = make_label_row(mtm, "Extra Usage", true);
      let header_item = NSMenuItem::new(mtm);
      header_item.setView(Some(&header_view));
      menu.addItem(&header_item);

      let used_text = format!("Spent: ${:.2} / ${:.2} limit", used, limit);
      let used_view = make_label_row(mtm, &used_text, false);
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

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
  let mtm = MainThreadMarker::new().unwrap();
  let app = NSApplication::sharedApplication(mtm);
  app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

  let delegate = AppDelegate::new(mtm);
  let delegate_proto: &ProtocolObject<dyn NSApplicationDelegate> = ProtocolObject::from_ref(&*delegate);
  app.setDelegate(Some(delegate_proto));

  app.run();
}
