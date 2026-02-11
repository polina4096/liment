use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::api::{ApiClient, ProfileResponse, UsageResponse};
use crate::icon;

use super::popup;

const WM_TRAY_ICON: u32 = WM_APP + 1;
const WM_DATA_UPDATE: u32 = WM_APP + 2;

pub const IDM_REFRESH: u16 = 1;
const IDM_QUIT: u16 = 2;

pub struct SharedState {
  pub usage: Option<UsageResponse>,
  pub profile: Option<ProfileResponse>,
}

struct TrayApp {
  state: Arc<Mutex<SharedState>>,
  api: Arc<ApiClient>,
  tray_hwnd: HWND,
  popup_hwnd: HWND,
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

static APP: Global<Option<TrayApp>> = Global::new(None);

fn wide(s: &str) -> Vec<u16> {
  s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn create_hicon(size: u32) -> HICON {
  let bgra = icon::render_bgra(size);

  let bmi = BITMAPINFO {
    bmiHeader: BITMAPINFOHEADER {
      biSize: size_of::<BITMAPINFOHEADER>() as u32,
      biWidth: size as i32,
      biHeight: -(size as i32),
      biPlanes: 1,
      biBitCount: 32,
      biCompression: BI_RGB.0,
      ..Default::default()
    },
    ..Default::default()
  };

  let hdc = unsafe { CreateCompatibleDC(None) };
  let mut bits = std::ptr::null_mut();
  let hbm_color =
    unsafe { CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap() };

  unsafe { std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len()) };

  let hbm_mask = unsafe { CreateBitmap(size as i32, size as i32, 1, 1, None) };

  let icon_info = ICONINFO {
    fIcon: true.into(),
    xHotspot: 0,
    yHotspot: 0,
    hbmMask: hbm_mask,
    hbmColor: hbm_color,
  };

  let hicon = unsafe { CreateIconIndirect(&icon_info).unwrap() };

  unsafe {
    let _ = DeleteObject(hbm_color.into());
    let _ = DeleteObject(hbm_mask.into());
    let _ = DeleteDC(hdc);
  }

  return hicon;
}

unsafe fn add_tray_icon(hwnd: HWND, hicon: HICON) {
  let mut nid = NOTIFYICONDATAW {
    cbSize: size_of::<NOTIFYICONDATAW>() as u32,
    hWnd: hwnd,
    uID: 1,
    uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
    uCallbackMessage: WM_TRAY_ICON,
    hIcon: hicon,
    ..Default::default()
  };

  let tip = wide("Claude Usage");
  let len = tip.len().min(128);
  nid.szTip[..len].copy_from_slice(&tip[..len]);

  unsafe { let _ = Shell_NotifyIconW(NIM_ADD, &nid); }
}

unsafe fn update_tray_tooltip(hwnd: HWND, text: &str) {
  let mut nid = NOTIFYICONDATAW {
    cbSize: size_of::<NOTIFYICONDATAW>() as u32,
    hWnd: hwnd,
    uID: 1,
    uFlags: NIF_TIP,
    ..Default::default()
  };

  let tip = wide(text);
  let len = tip.len().min(128);
  nid.szTip[..len].copy_from_slice(&tip[..len]);

  unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &nid); }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
  let nid = NOTIFYICONDATAW {
    cbSize: size_of::<NOTIFYICONDATAW>() as u32,
    hWnd: hwnd,
    uID: 1,
    ..Default::default()
  };
  unsafe { let _ = Shell_NotifyIconW(NIM_DELETE, &nid); }
}

fn show_context_menu(hwnd: HWND) {
  unsafe {
    let hmenu = CreatePopupMenu().unwrap();
    AppendMenuW(hmenu, MF_STRING, IDM_REFRESH as usize, w!("Refresh")).unwrap();
    AppendMenuW(hmenu, MF_STRING, IDM_QUIT as usize, w!("Quit")).unwrap();

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(hmenu, TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(hmenu);
  }
}

fn spawn_refresh(api: Arc<ApiClient>, state: Arc<Mutex<SharedState>>, hwnd_raw: isize) {
  std::thread::spawn(move || {
    let usage = api.fetch_usage();
    let profile = api.fetch_profile();
    {
      let mut s = state.lock().unwrap();
      s.usage = usage;
      s.profile = profile;
    }
    unsafe {
      let _ = PostMessageW(
        Some(HWND(hwnd_raw as *mut _)),
        WM_DATA_UPDATE,
        WPARAM(0),
        LPARAM(0),
      );
    }
  });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
  let app_ptr = unsafe { APP.get() };

  match msg {
    WM_TRAY_ICON => {
      let event = (lparam.0 & 0xFFFF) as u32;
      match event {
        WM_LBUTTONUP => {
          if let Some(app) = unsafe { &*app_ptr } {
            popup::toggle(app.popup_hwnd);
          }
        }
        WM_RBUTTONUP => {
          show_context_menu(hwnd);
        }
        _ => {}
      }
      return LRESULT(0);
    }
    WM_COMMAND => {
      let id = (wparam.0 & 0xFFFF) as u16;
      match id {
        IDM_REFRESH => {
          if let Some(app) = unsafe { &*app_ptr } {
            spawn_refresh(
              Arc::clone(&app.api),
              Arc::clone(&app.state),
              app.tray_hwnd.0 as isize,
            );
          }
        }
        IDM_QUIT => {
          unsafe { PostQuitMessage(0) };
        }
        _ => {}
      }
      return LRESULT(0);
    }
    WM_DATA_UPDATE => {
      if let Some(app) = unsafe { &*app_ptr } {
        let state = app.state.lock().unwrap();
        if let Some(ref usage) = state.usage {
          let seven_d = usage.seven_day.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
          let five_h = usage.five_hour.as_ref().map(|b| b.utilization as u32).unwrap_or(0);
          let tip = format!("Claude Usage — 7d {}% | 5h {}%", seven_d, five_h);
          unsafe { update_tray_tooltip(app.tray_hwnd, &tip) };
        }
        popup::repaint(app.popup_hwnd);
      }
      return LRESULT(0);
    }
    WM_DESTROY => {
      if let Some(app) = unsafe { &*app_ptr } {
        unsafe { remove_tray_icon(app.tray_hwnd) };
      }
      unsafe { PostQuitMessage(0) };
      return LRESULT(0);
    }
    _ => {}
  }
  return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
}

pub fn run(api: Arc<ApiClient>) {
  unsafe {
    let hinstance = GetModuleHandleW(None).unwrap();

    let class_name = w!("LimentTray");
    let wc = WNDCLASSEXW {
      cbSize: size_of::<WNDCLASSEXW>() as u32,
      lpfnWndProc: Some(wndproc),
      hInstance: hinstance.into(),
      lpszClassName: class_name,
      ..Default::default()
    };
    RegisterClassExW(&wc);

    let tray_hwnd = CreateWindowExW(
      WINDOW_EX_STYLE::default(),
      class_name,
      w!("LimentTray"),
      WINDOW_STYLE::default(),
      0,
      0,
      0,
      0,
      Some(HWND_MESSAGE),
      None,
      Some(hinstance.into()),
      None,
    )
    .unwrap();

    let state = Arc::new(Mutex::new(SharedState { usage: None, profile: None }));

    let popup_hwnd = popup::create_popup(hinstance.into(), Arc::clone(&state), tray_hwnd);

    let hicon = create_hicon(32);
    add_tray_icon(tray_hwnd, hicon);

    APP.get().write(Some(TrayApp {
      state: Arc::clone(&state),
      api: Arc::clone(&api),
      tray_hwnd,
      popup_hwnd,
    }));

    let api_bg = Arc::clone(&api);
    let state_bg = Arc::clone(&state);
    let hwnd_raw = tray_hwnd.0 as isize;
    std::thread::spawn(move || {
      let do_fetch = || {
        let usage = api_bg.fetch_usage();
        let profile = api_bg.fetch_profile();
        {
          let mut s = state_bg.lock().unwrap();
          s.usage = usage;
          s.profile = profile;
        }
        let _ = PostMessageW(
          Some(HWND(hwnd_raw as *mut _)),
          WM_DATA_UPDATE,
          WPARAM(0),
          LPARAM(0),
        );
      };

      do_fetch();
      loop {
        std::thread::sleep(Duration::from_secs(60));
        do_fetch();
      }
    });

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
      let _ = TranslateMessage(&msg);
      DispatchMessageW(&msg);
    }

    let _ = DestroyIcon(hicon);
  }
}
