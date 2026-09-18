use std::{
  sync::Mutex,
  time::{Duration, Instant},
};

/// Why a request through [`Client`] produced no body.
#[derive(Debug)]
pub enum HttpError {
  /// The request was withheld because an earlier 429 put the client into backoff.
  Skipped,
  /// The endpoint answered 429; the client has already scheduled a backoff.
  RateLimited,
  /// Any other non-2xx status.
  Status(u16),
  /// Transport-level failure (DNS, TLS, timeout, body read, ...).
  Transport(ureq::Error),
}

impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      HttpError::Skipped => write!(f, "skipped (rate limit backoff)"),
      HttpError::RateLimited => write!(f, "rate limited (429)"),
      HttpError::Status(code) => write!(f, "http status {}", code),
      HttpError::Transport(e) => write!(f, "{}", e),
    };
  }
}

impl std::error::Error for HttpError {}

/// HTTP client for one rate-limited API: wraps a ureq agent with exponential backoff
/// so callers never have to check for 429s or remember to reset it on success.
pub struct Client {
  agent: ureq::Agent,
  backoff: Mutex<Backoff>,
}

impl Client {
  pub fn new() -> Self {
    // ureq has no timeouts by default, so a connection that dies mid-request (e.g. the Mac
    // going back to sleep during a DarkWake) only fails once the kernel gives up on it.
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

    let config = ureq::Agent::config_builder().timeout_global(Some(REQUEST_TIMEOUT)).build();

    return Self {
      agent: ureq::Agent::new_with_config(config),
      backoff: Mutex::new(Backoff::default()),
    };
  }

  /// Sends a GET with the given headers and returns the response body.
  pub fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, HttpError> {
    return self.send(url, |agent| {
      let mut request = agent.get(url);

      for (name, value) in headers {
        request = request.header(*name, *value);
      }

      return request.call();
    });
  }

  /// Sends a POST with a JSON body and the given headers and returns the response body.
  pub fn post_json(&self, url: &str, headers: &[(&str, &str)], body: &str) -> Result<String, HttpError> {
    return self.send(url, |agent| {
      let mut request = agent.post(url).header("Content-Type", "application/json");

      for (name, value) in headers {
        request = request.header(*name, *value);
      }

      return request.send(body);
    });
  }

  fn send(
    &self,
    url: &str,
    call: impl FnOnce(&ureq::Agent) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
  ) -> Result<String, HttpError> {
    if self.backoff.lock().unwrap().should_skip(url) {
      return Err(HttpError::Skipped);
    }

    log::debug!("Request {}", url);

    let mut response = match call(&self.agent) {
      Ok(response) => response,
      Err(ureq::Error::StatusCode(429)) => {
        self.backoff.lock().unwrap().note_rate_limited();
        return Err(HttpError::RateLimited);
      }
      Err(ureq::Error::StatusCode(code)) => return Err(HttpError::Status(code)),
      Err(e) => return Err(HttpError::Transport(e)),
    };

    let body = response.body_mut().read_to_string().map_err(HttpError::Transport)?;

    self.backoff.lock().unwrap().note_success();

    return Ok(body);
  }
}

#[derive(Default)]
struct Backoff {
  retry_after: Option<Instant>,
  consecutive_failures: u32,
}

impl Backoff {
  fn should_skip(&self, url: &str) -> bool {
    let Some(retry_after) = self.retry_after
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

  fn note_rate_limited(&mut self) {
    self.consecutive_failures += 1;

    const BACKOFF_BASE: Duration = Duration::from_secs(60);
    const BACKOFF_MAX_EXPONENT: u32 = 4;

    // Exponential backoff: 2, 4, 8, 16 minutes, then stays at 16.
    let exponent = self.consecutive_failures.min(BACKOFF_MAX_EXPONENT);
    let delay = BACKOFF_BASE * 2u32.pow(exponent);

    self.retry_after = Some(Instant::now() + delay);

    log::warn!("Rate limited (429), backing off for {}s", delay.as_secs());
  }

  fn note_success(&mut self) {
    if self.consecutive_failures > 0 {
      log::info!("Request succeeded, resetting backoff");
      self.consecutive_failures = 0;
      self.retry_after = None;
    }
  }
}
