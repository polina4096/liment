use std::{
  sync::Mutex,
  time::{Duration, Instant},
};

#[derive(Default)]
struct BackoffState {
  retry_after: Option<Instant>,
  consecutive_failures: u32,
}

/// Exponential backoff shared by providers polling rate-limited endpoints.
#[derive(Default)]
pub struct Backoff(Mutex<BackoffState>);

impl Backoff {
  pub fn new() -> Self {
    return Self::default();
  }

  /// Returns whether requests should be withheld because of an earlier 429.
  pub fn should_skip(&self, url: &str) -> bool {
    let state = self.0.lock().unwrap();

    let Some(retry_after) = state.retry_after
    else {
      return false;
    };

    let now = Instant::now();
    if now >= retry_after {
      return false;
    }

    log::debug!("Skipping request to {} (rate limit backoff, {}s remaining)", url, (retry_after - now).as_secs());

    return true;
  }

  pub fn note_rate_limited(&self) {
    let mut state = self.0.lock().unwrap();

    state.consecutive_failures += 1;
    let delay = Duration::from_secs(60 * (1u64 << state.consecutive_failures.min(4)));
    state.retry_after = Some(Instant::now() + delay);

    log::warn!("Rate limited (429), backing off for {}s", delay.as_secs());
  }

  pub fn note_success(&self) {
    let mut state = self.0.lock().unwrap();

    if state.consecutive_failures > 0 {
      log::info!("Request succeeded, resetting backoff");
      state.consecutive_failures = 0;
      state.retry_after = None;
    }
  }
}
