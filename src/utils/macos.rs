/// Creates an `NSTimer`, adds it to the current run loop, and drops the reference.
/// The run loop retains the timer, so it stays alive until it's done firing.
///
/// Usage: `schedule_timer!(interval_secs, target, selector)` for a repeating timer,
/// or `schedule_timer!(interval_secs, target, selector, once)` for a single shot.
macro_rules! schedule_timer {
  ($interval:expr, $target:expr, $selector:ident) => {
    $crate::utils::macos::schedule_timer!(@build $interval, $target, $selector, true)
  };

  ($interval:expr, $target:expr, $selector:ident, once) => {
    $crate::utils::macos::schedule_timer!(@build $interval, $target, $selector, false)
  };

  (@build $interval:expr, $target:expr, $selector:ident, $repeats:expr) => {{
    let timer = unsafe {
      objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
        $interval, $target, objc2::sel!($selector:), None, $repeats,
      )
    };

    unsafe {
      objc2_foundation::NSRunLoop::currentRunLoop()
        .addTimer_forMode(&timer, objc2_foundation::NSDefaultRunLoopMode);
    }
  }};
}

use objc2_app_kit::NSView;
pub(crate) use schedule_timer;

pub trait NSViewExt {
  #[expect(non_snake_case)]
  fn noAutoresize(&self);
}

impl NSViewExt for NSView {
  fn noAutoresize(&self) {
    return self.setTranslatesAutoresizingMaskIntoConstraints(false);
  }
}
