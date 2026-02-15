/// Creates a repeating `NSTimer`, adds it to the current run loop, and drops the reference.
/// The run loop retains the timer, so it stays alive for the app's lifetime.
///
/// Usage: `schedule_timer!(interval_secs, target, selector)`
macro_rules! schedule_timer {
  ($interval:expr, $target:expr, $selector:ident) => {{
    let timer = unsafe {
      objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
        $interval as f64, $target, objc2::sel!($selector:), None, true,
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
