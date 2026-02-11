use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::api::{SubscriptionTier, UsageBucket};
use crate::util::format_reset_time;

use super::tray::SharedState;

const POPUP_WIDTH: i32 = 280;
const BG_COLOR: u32 = 0x001e1e1e;
const SEP_COLOR: u32 = 0x00333333;
const TEXT_PRIMARY: u32 = 0x00e0e0e0;
const TEXT_SECONDARY: u32 = 0x00888888;
const TEXT_WHITE: u32 = 0x00ffffff;
const BTN_BG: u32 = 0x002a2a2a;
const BTN_BORDER: u32 = 0x00444444;
const BTN_HOVER: u32 = 0x00383838;
const BAR_TROUGH: u32 = 0x00333333;
const BAR_NORMAL: u32 = 0x00cccccc;
const BAR_YELLOW: u32 = 0x0000ccff;
const BAR_ORANGE: u32 = 0x000095ff;
const BAR_RED: u32 = 0x00303bff;

const TIER_FREE: u32 = 0x00999999;
const TIER_PRO: u32 = 0x00E68C4D;
const TIER_MAX5X: u32 = 0x00D9598C;
const TIER_MAX20X: u32 = 0x003373D9;

fn colorref(rgb: u32) -> COLORREF {
  COLORREF(rgb)
}

fn bar_color(utilization: f64) -> u32 {
  match utilization {
    u if u < 50.0 => BAR_NORMAL,
    u if u < 75.0 => BAR_YELLOW,
    u if u < 90.0 => BAR_ORANGE,
    _ => BAR_RED,
  }
}

fn tier_color(tier: SubscriptionTier) -> u32 {
  match tier {
    SubscriptionTier::Free => TIER_FREE,
    SubscriptionTier::Pro => TIER_PRO,
    SubscriptionTier::Max5x => TIER_MAX5X,
    SubscriptionTier::Max20x => TIER_MAX20X,
  }
}

struct ButtonRect {
  rect: RECT,
  id: u16,
}

struct PopupGlobals {
  state: Option<Arc<Mutex<SharedState>>>,
  hover_btn: Option<u16>,
  buttons: Vec<ButtonRect>,
  tray_hwnd_raw: isize,
}

struct Global<T>(UnsafeCell<T>);
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
  const fn new(val: T) -> Self {
    Self(UnsafeCell::new(val))
  }

  unsafe fn get(&self) -> *mut T {
    self.0.get()
  }
}

static GLOBALS: Global<PopupGlobals> = Global::new(PopupGlobals {
  state: None,
  hover_btn: None,
  buttons: Vec::new(),
  tray_hwnd_raw: 0,
});

fn wide(s: &str) -> Vec<u16> {
  s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn create_font(hdc: HDC, size: i32, weight: i32) -> HFONT {
  let name = wide("Segoe UI");
  let dpi = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSY) };
  let mut lf = LOGFONTW {
    lfHeight: -(size as i64 * dpi as i64 / 72) as i32,
    lfWeight: weight,
    lfQuality: CLEARTYPE_QUALITY,
    ..Default::default()
  };
  let len = name.len().min(32);
  lf.lfFaceName[..len].copy_from_slice(&name[..len]);
  return unsafe { CreateFontIndirectW(&lf) };
}

unsafe fn draw_text_left(hdc: HDC, text: &str, x: i32, y: i32, color: u32) -> i32 {
  unsafe { SetTextColor(hdc, colorref(color)) };
  let mut w = wide(text);
  w.pop();
  let mut rc = RECT { left: x, top: y, right: x + 500, bottom: y + 100 };
  unsafe { DrawTextW(hdc, &mut w, &mut rc, DT_LEFT | DT_SINGLELINE | DT_NOCLIP) };
  return rc.bottom;
}

unsafe fn draw_text_right(hdc: HDC, text: &str, right: i32, y: i32, color: u32) {
  unsafe { SetTextColor(hdc, colorref(color)) };
  let mut w = wide(text);
  w.pop();
  let mut rc = RECT { left: 0, top: y, right, bottom: y + 100 };
  unsafe { DrawTextW(hdc, &mut w, &mut rc, DT_RIGHT | DT_SINGLELINE | DT_NOCLIP) };
}

unsafe fn fill_rect_color(hdc: HDC, rect: &RECT, color: u32) {
  let brush = unsafe { CreateSolidBrush(colorref(color)) };
  unsafe { FillRect(hdc, rect, brush) };
  unsafe { let _ = DeleteObject(brush.into()); }
}

unsafe fn draw_rounded_rect_filled(hdc: HDC, rect: &RECT, radius: i32, color: u32) {
  let rgn = unsafe {
    CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, radius, radius)
  };
  let brush = unsafe { CreateSolidBrush(colorref(color)) };
  unsafe { let _ = FillRgn(hdc, rgn, brush); }
  unsafe {
    let _ = DeleteObject(brush.into());
    let _ = DeleteObject(rgn.into());
  }
}

fn draw_progress_bar(hdc: HDC, x: i32, y: i32, width: i32, utilization: f64) -> i32 {
  let h = 8;
  let trough = RECT { left: x, top: y, right: x + width, bottom: y + h };
  unsafe { draw_rounded_rect_filled(hdc, &trough, 4, BAR_TROUGH) };

  let filled_w = ((utilization / 100.0) * width as f64).round() as i32;
  if filled_w > 0 {
    let bar = RECT { left: x, top: y, right: x + filled_w, bottom: y + h };
    unsafe { draw_rounded_rect_filled(hdc, &bar, 4, bar_color(utilization)) };
  }

  return y + h;
}

fn draw_bucket(hdc: HDC, label: &str, bucket: &UsageBucket, x: i32, y: i32, w: i32) -> i32 {
  let pct = bucket.utilization as u32;
  let reset = format_reset_time(&bucket.resets_at);

  let font_body = unsafe { create_font(hdc, 9, 400) };
  let font_small = unsafe { create_font(hdc, 8, 300) };

  let old = unsafe { SelectObject(hdc, font_body.into()) };
  let text = format!("{}  {}%", label, pct);
  unsafe { draw_text_left(hdc, &text, x, y, TEXT_PRIMARY) };

  unsafe { SelectObject(hdc, font_small.into()) };
  let reset_text = format!("resets in {}", reset);
  unsafe { draw_text_right(hdc, &reset_text, x + w, y + 1, TEXT_SECONDARY) };

  unsafe { SelectObject(hdc, old) };

  let bar_y = y + 16;
  let bottom = draw_progress_bar(hdc, x, bar_y, w, bucket.utilization);

  unsafe {
    let _ = DeleteObject(font_body.into());
    let _ = DeleteObject(font_small.into());
  }

  return bottom + 6;
}

fn draw_separator(hdc: HDC, x: i32, y: i32, w: i32) -> i32 {
  let rc = RECT { left: x, top: y, right: x + w, bottom: y + 1 };
  unsafe { fill_rect_color(hdc, &rc, SEP_COLOR) };
  return y + 1;
}

fn draw_button(hdc: HDC, text: &str, rect: &RECT, hovered: bool) {
  let bg = if hovered { BTN_HOVER } else { BTN_BG };
  unsafe { draw_rounded_rect_filled(hdc, rect, 6, bg) };

  let border_brush = unsafe { CreateSolidBrush(colorref(BTN_BORDER)) };
  let rgn = unsafe { CreateRoundRectRgn(rect.left, rect.top, rect.right, rect.bottom, 6, 6) };
  unsafe { let _ = FrameRgn(hdc, rgn, border_brush, 1, 1); }
  unsafe {
    let _ = DeleteObject(border_brush.into());
    let _ = DeleteObject(rgn.into());
  }

  let font = unsafe { create_font(hdc, 9, 400) };
  let old = unsafe { SelectObject(hdc, font.into()) };
  unsafe { SetTextColor(hdc, colorref(TEXT_PRIMARY)) };
  let mut w = wide(text);
  w.pop();
  let mut rc = *rect;
  unsafe { DrawTextW(hdc, &mut w, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE) };
  unsafe { SelectObject(hdc, old) };
  unsafe { let _ = DeleteObject(font.into()); }
}

unsafe fn paint(hwnd: HWND) {
  let mut ps = PAINTSTRUCT::default();
  let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

  let mut rc = RECT::default();
  unsafe { GetClientRect(hwnd, &mut rc).unwrap() };
  let w = rc.right - rc.left;

  unsafe { fill_rect_color(hdc, &rc, BG_COLOR) };
  unsafe { SetBkMode(hdc, TRANSPARENT) };

  let pad = 14;
  let content_w = w - pad * 2;
  let mut y = 10;

  let g = unsafe { &*GLOBALS.get() };
  let state_guard = g.state.as_ref().unwrap().lock().unwrap();

  let font_header = unsafe { create_font(hdc, 11, 600) };
  let old_font = unsafe { SelectObject(hdc, font_header.into()) };
  unsafe { draw_text_left(hdc, "Claude Usage", pad, y, TEXT_WHITE) };

  if let Some(ref profile) = state_guard.profile {
    let tier = profile.organization.rate_limit_tier;
    let tier_text = tier.to_string();
    let tw = wide(&tier_text);

    let font_badge = unsafe { create_font(hdc, 8, 500) };
    unsafe { SelectObject(hdc, font_badge.into()) };

    let mut sz = SIZE::default();
    unsafe { GetTextExtentPoint32W(hdc, &tw[..tw.len() - 1], &mut sz).unwrap() };

    let badge_x = pad + 100;
    let badge_rect = RECT {
      left: badge_x,
      top: y + 1,
      right: badge_x + sz.cx + 16,
      bottom: y + sz.cy + 4,
    };
    unsafe { draw_rounded_rect_filled(hdc, &badge_rect, 8, tier_color(tier)) };
    unsafe { SetTextColor(hdc, colorref(TEXT_WHITE)) };
    let mut badge_w = wide(&tier_text);
    badge_w.pop();
    let mut badge_text_rc = badge_rect;
    unsafe { DrawTextW(hdc, &mut badge_w, &mut badge_text_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE) };

    unsafe { let _ = DeleteObject(font_badge.into()); }
  }

  unsafe { SelectObject(hdc, old_font) };
  unsafe { let _ = DeleteObject(font_header.into()); }

  y += 22;
  y = draw_separator(hdc, pad, y, content_w);
  y += 4;

  if let Some(ref usage) = state_guard.usage {
    let buckets: [(&str, &Option<UsageBucket>); 4] = [
      ("5h Limit", &usage.five_hour),
      ("7d Limit", &usage.seven_day),
      ("7d Sonnet", &usage.seven_day_sonnet),
      ("7d Opus", &usage.seven_day_opus),
    ];

    for (name, bucket_opt) in &buckets {
      if let Some(bucket) = bucket_opt {
        y = draw_bucket(hdc, name, bucket, pad, y, content_w);
      }
    }

    if let Some(ref extra) = usage.extra_usage {
      if extra.is_enabled {
        y = draw_separator(hdc, pad, y, content_w);
        y += 6;

        let font_section = unsafe { create_font(hdc, 9, 600) };
        let old = unsafe { SelectObject(hdc, font_section.into()) };
        unsafe { draw_text_left(hdc, "Extra Usage", pad, y, TEXT_PRIMARY) };
        y += 16;

        let font_body = unsafe { create_font(hdc, 9, 400) };
        unsafe { SelectObject(hdc, font_body.into()) };
        unsafe { draw_text_left(hdc, "Spent", pad, y, TEXT_PRIMARY) };

        let limit = extra.monthly_limit / 100.0;
        let used = extra.used_credits / 100.0;
        let val = format!("${:.2} / ${:.2}", used, limit);
        unsafe { draw_text_right(hdc, &val, pad + content_w, y, TEXT_SECONDARY) };

        y += 16;
        unsafe { SelectObject(hdc, old) };
        unsafe {
          let _ = DeleteObject(font_section.into());
          let _ = DeleteObject(font_body.into());
        }
      }
    }
  } else {
    let font_loading = unsafe { create_font(hdc, 9, 400) };
    let old = unsafe { SelectObject(hdc, font_loading.into()) };
    unsafe { draw_text_left(hdc, "Loading...", pad, y, TEXT_SECONDARY) };
    y += 18;
    unsafe { SelectObject(hdc, old) };
    unsafe { let _ = DeleteObject(font_loading.into()); }
  }

  drop(state_guard);

  y = draw_separator(hdc, pad, y, content_w);
  y += 8;

  let btn_h = 28;
  let btn_gap = 8;
  let btn_w = (content_w - btn_gap) / 2;

  let refresh_rect = RECT { left: pad, top: y, right: pad + btn_w, bottom: y + btn_h };
  let quit_rect = RECT {
    left: pad + btn_w + btn_gap,
    top: y,
    right: pad + content_w,
    bottom: y + btn_h,
  };

  let hover = g.hover_btn;
  draw_button(hdc, "Refresh", &refresh_rect, hover == Some(1));
  draw_button(hdc, "Quit", &quit_rect, hover == Some(2));

  // Update button rects for hit testing
  let g = unsafe { &mut *GLOBALS.get() };
  g.buttons.clear();
  g.buttons.push(ButtonRect { rect: refresh_rect, id: 1 });
  g.buttons.push(ButtonRect { rect: quit_rect, id: 2 });

  y += btn_h + 10;

  unsafe { let _ = EndPaint(hwnd, &ps); }

  let total_h = y;
  let mut win_rc = RECT::default();
  unsafe { GetWindowRect(hwnd, &mut win_rc).unwrap() };
  let current_h = win_rc.bottom - win_rc.top;
  if current_h != total_h {
    unsafe {
      let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        win_rc.left,
        win_rc.top,
        POPUP_WIDTH,
        total_h,
        SWP_NOMOVE | SWP_NOACTIVATE,
      );
    }
  }
}

fn hit_test_button(x: i32, y: i32) -> Option<u16> {
  let g = unsafe { &*GLOBALS.get() };
  for btn in &g.buttons {
    if x >= btn.rect.left && x < btn.rect.right && y >= btn.rect.top && y < btn.rect.bottom {
      return Some(btn.id);
    }
  }
  return None;
}

unsafe extern "system" fn popup_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
  match msg {
    WM_PAINT => {
      unsafe { paint(hwnd) };
      return LRESULT(0);
    }
    WM_ACTIVATE => {
      let activation = (wparam.0 & 0xFFFF) as u32;
      if activation == WA_INACTIVE as u32 {
        unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
      }
      return LRESULT(0);
    }
    WM_LBUTTONUP => {
      let x = (lparam.0 & 0xFFFF) as i16 as i32;
      let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
      let g = unsafe { &*GLOBALS.get() };
      match hit_test_button(x, y) {
        Some(1) => {
          unsafe {
            let _ = PostMessageW(
              Some(HWND(g.tray_hwnd_raw as *mut _)),
              WM_COMMAND,
              WPARAM(super::tray::IDM_REFRESH as usize),
              LPARAM(0),
            );
          }
        }
        Some(2) => {
          unsafe { PostQuitMessage(0) };
        }
        _ => {}
      }
      return LRESULT(0);
    }
    WM_MOUSEMOVE => {
      let x = (lparam.0 & 0xFFFF) as i16 as i32;
      let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
      let btn = hit_test_button(x, y);
      let g = unsafe { &mut *GLOBALS.get() };
      let prev = g.hover_btn;
      if btn != prev {
        g.hover_btn = btn;
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
      }
      unsafe { SetTimer(Some(hwnd), 1, 50, None) };
      return LRESULT(0);
    }
    WM_TIMER => {
      if wparam.0 == 1 {
        let mut pt = POINT::default();
        unsafe { let _ = GetCursorPos(&mut pt); }
        let mut rc = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rc).unwrap() };
        if pt.x < rc.left || pt.x >= rc.right || pt.y < rc.top || pt.y >= rc.bottom {
          unsafe { KillTimer(Some(hwnd), 1).unwrap() };
          let g = unsafe { &mut *GLOBALS.get() };
          if g.hover_btn.is_some() {
            g.hover_btn = None;
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
          }
        }
      }
      return LRESULT(0);
    }
    _ => {}
  }
  return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
}

pub unsafe fn create_popup(hinstance: HINSTANCE, state: Arc<Mutex<SharedState>>, tray_hwnd: HWND) -> HWND {
  let g = unsafe { &mut *GLOBALS.get() };
  g.state = Some(state);
  g.tray_hwnd_raw = tray_hwnd.0 as isize;

  let class_name = w!("LimentPopup");
  let wc = WNDCLASSEXW {
    cbSize: size_of::<WNDCLASSEXW>() as u32,
    lpfnWndProc: Some(popup_wndproc),
    hInstance: hinstance,
    lpszClassName: class_name,
    hbrBackground: unsafe { CreateSolidBrush(colorref(BG_COLOR)) },
    hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap() },
    ..Default::default()
  };
  unsafe { RegisterClassExW(&wc) };

  let hwnd = unsafe {
    CreateWindowExW(
      WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
      class_name,
      w!("Claude Usage"),
      WS_POPUP,
      0,
      0,
      POPUP_WIDTH,
      200,
      None,
      None,
      Some(hinstance),
      None,
    )
    .unwrap()
  };

  let preference = DWM_WINDOW_CORNER_PREFERENCE(2); // DWMWCP_ROUND
  let _ = unsafe {
    DwmSetWindowAttribute(
      hwnd,
      DWMWA_WINDOW_CORNER_PREFERENCE,
      &preference as *const _ as *const _,
      size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
    )
  };

  return hwnd;
}

pub fn toggle(hwnd: HWND) {
  unsafe {
    if IsWindowVisible(hwnd).as_bool() {
      let _ = ShowWindow(hwnd, SW_HIDE);
      return;
    }

    let mut work_area = RECT::default();
    let _ = SystemParametersInfoW(
      SPI_GETWORKAREA,
      0,
      Some(&mut work_area as *mut _ as *mut _),
      SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );

    let mut win_rc = RECT::default();
    let _ = GetWindowRect(hwnd, &mut win_rc);
    let win_h = win_rc.bottom - win_rc.top;

    let x = work_area.right - POPUP_WIDTH - 12;
    let y = work_area.bottom - win_h - 12;

    let _ = SetWindowPos(
      hwnd,
      Some(HWND_TOPMOST),
      x,
      y,
      POPUP_WIDTH,
      win_h,
      SWP_NOACTIVATE,
    );

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
  }
}

pub fn repaint(hwnd: HWND) {
  unsafe {
    let _ = InvalidateRect(Some(hwnd), None, true);
  }
}
