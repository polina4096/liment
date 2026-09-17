use std::sync::Mutex;

use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::eyre::{Context as _, ContextCompat as _, Result};
use jiff::Timestamp;
use rgb::Rgb;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::{
  providers::{DataProvider, ProviderKind, TierInfo, UsageData, UsageDetail, UsageWindow},
  utils::http::{Client, HttpError},
};

pub const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
/// The Codex CLI's public OAuth client id, embedded in the CLI.
const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const USER_AGENT: &str = "codex_cli_rs/0.76.0 (Debian 13.0.0; x86_64) WindowsTerminal";

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CodexSettings {
  /// Access token override. If not set, reads from `~/.codex/auth.json`.
  pub token: Option<String>,

  /// ChatGPT account id override. If not set, reads from `~/.codex/auth.json`.
  pub account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
  pub plan_type: Option<SubscriptionTier>,
  pub rate_limit: Option<RateLimit>,
  pub code_review_rate_limit: Option<RateLimit>,
  #[serde(default)]
  pub additional_rate_limits: Vec<AdditionalRateLimit>,
  pub rate_limit_reset_credits: Option<ResetCredits>,
}

/// Credits that can be spent to reset a rate limit window early.
#[derive(Debug, Deserialize)]
pub struct ResetCredits {
  pub available_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimit {
  pub primary_window: Option<UsageBucket>,
  pub secondary_window: Option<UsageBucket>,
}

#[derive(Debug, Deserialize)]
pub struct AdditionalRateLimit {
  pub limit_name: Option<String>,
  pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize)]
pub struct UsageBucket {
  pub used_percent: f64,
  pub limit_window_seconds: i64,
  pub reset_at: i64,
}

impl UsageBucket {
  /// Short human-readable window duration, e.g. "5h" or "7d". `None` if the API reported
  /// no usable period.
  fn format_duration(&self) -> Option<String> {
    let seconds = self.limit_window_seconds;

    if seconds <= 0 {
      return None;
    }

    if seconds % 86400 == 0 {
      return Some(format!("{}d", seconds / 86400));
    }

    if seconds % 3600 == 0 {
      return Some(format!("{}h", seconds / 3600));
    }

    return Some(format!("{}m", (seconds / 60).max(1)));
  }

  fn to_window(&self, title: String, short_title: Option<String>) -> UsageWindow {
    return UsageWindow {
      title,
      short_title,
      utilization: self.used_percent,
      resets_at: Timestamp::from_second(self.reset_at).ok(),
      period_seconds: Some(self.limit_window_seconds),
    };
  }
}

impl RateLimit {
  fn windows(&self) -> impl Iterator<Item = &UsageBucket> {
    return self.primary_window.iter().chain(self.secondary_window.iter());
  }
}

impl From<UsageResponse> for UsageData {
  fn from(usage: UsageResponse) -> Self {
    let mut windows = Vec::new();

    if let Some(rate_limit) = &usage.rate_limit {
      for bucket in rate_limit.windows() {
        let window = match bucket.format_duration() {
          Some(duration) => bucket.to_window(format!("{} Limit", duration), Some(duration)),
          None => bucket.to_window("Usage Limit".to_string(), None),
        };
        windows.push(window);
      }
    }

    if let Some(code_review) = &usage.code_review_rate_limit {
      for bucket in code_review.windows() {
        let title = match bucket.format_duration() {
          Some(duration) => format!("Review {}", duration),
          None => "Review".to_string(),
        };
        windows.push(bucket.to_window(title, None));
      }
    }

    for additional in &usage.additional_rate_limits {
      let Some(rate_limit) = &additional.rate_limit
      else {
        continue;
      };

      let name = additional.limit_name.as_deref().unwrap_or("Extra");
      for bucket in rate_limit.windows() {
        let title = match bucket.format_duration() {
          Some(duration) => format!("{} {}", name, duration),
          None => name.to_string(),
        };
        windows.push(bucket.to_window(title, None));
      }
    }

    let details = usage
      .rate_limit_reset_credits
      .and_then(|credits| credits.available_count)
      .map(|count| {
        return UsageDetail {
          label: "Reset credits".to_string(),
          value: count.to_string(),
        };
      })
      .into_iter()
      .collect();

    return UsageData {
      api_usage: None,
      peak_hours: None,
      windows,
      details,
      tier: usage.plan_type.as_ref().map(SubscriptionTier::tier_info),
    };
  }
}

#[derive(Debug, Clone, Deserialize, strum::EnumString, strum::Display)]
#[serde(from = "String")]
pub enum SubscriptionTier {
  #[strum(serialize = "free", to_string = "Free")]
  Free,
  #[strum(serialize = "plus", to_string = "Plus")]
  Plus,
  #[strum(serialize = "pro", to_string = "Pro")]
  Pro,
  #[strum(serialize = "prolite", to_string = "Pro Lite")]
  ProLite,
  #[strum(serialize = "team", serialize = "business", to_string = "Team")]
  Team,
  #[strum(serialize = "enterprise", to_string = "Enterprise")]
  Enterprise,
  /// A plan this app doesn't know about yet; keeps the raw API identifier so a new plan
  /// still shows a badge instead of failing the usage parse.
  #[strum(default)]
  Unknown(String),
}

impl From<String> for SubscriptionTier {
  fn from(value: String) -> Self {
    // Never fails: the `default` variant absorbs anything unrecognized.
    return value.parse().unwrap_or(SubscriptionTier::Unknown(value));
  }
}

impl SubscriptionTier {
  pub fn tier_info(&self) -> TierInfo {
    return TierInfo {
      name: self.to_string(),
      color: match self {
        SubscriptionTier::Free | SubscriptionTier::Unknown(_) => Rgb::new(140, 140, 155),
        SubscriptionTier::Plus => Rgb::new(90, 145, 210),
        SubscriptionTier::Pro => Rgb::new(75, 175, 155),
        SubscriptionTier::ProLite => Rgb::new(95, 160, 145),
        SubscriptionTier::Team => Rgb::new(185, 135, 90),
        SubscriptionTier::Enterprise => Rgb::new(130, 115, 180),
      },
    };
  }
}

/// Path to the Codex CLI's `auth.json`. Honors `CODEX_HOME` like the CLI does, falling
/// back to `~/.codex`.
fn get_auth_path() -> Result<Utf8PathBuf> {
  if let Some(codex_home) = std::env::var("CODEX_HOME").ok().filter(|v| !v.is_empty()) {
    return Ok(Utf8PathBuf::from(codex_home).join("auth.json"));
  }

  let home = etcetera::home_dir()?;
  let home = Utf8Path::from_path(&home).context("Home directory path is not valid UTF-8")?;

  return Ok(home.join(".codex").join("auth.json"));
}

/// Shape of `~/.codex/auth.json`, written and refreshed by the Codex CLI.
#[derive(Debug, Deserialize)]
struct CodexAuthFile {
  tokens: Option<CodexAuthTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexAuthTokens {
  access_token: String,
  refresh_token: Option<String>,
  account_id: Option<String>,
}

struct Credentials {
  token: SecretString,
  account_id: Option<String>,
  refresh_token: Option<SecretString>,
  /// Expiry from the access token's JWT `exp` claim, if it could be decoded.
  expires_at: Option<Timestamp>,
  /// Whether the token came from the config override (never refreshed).
  from_config: bool,
}

/// Reads the `exp` claim out of a JWT without verifying it — we only need it to know when
/// to refresh, the server still validates the token.
fn jwt_expiry(token: &str) -> Option<Timestamp> {
  use base64::Engine as _;

  let payload = token.split('.').nth(1)?;
  let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
  let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;

  return Timestamp::from_second(claims.get("exp")?.as_i64()?).ok();
}

pub struct CodexProvider {
  settings: CodexSettings,
  client: Client,
  /// Serializes token refreshes so two concurrent fetches can't both spend the (rotating)
  /// refresh token.
  refresh_lock: Mutex<()>,
}

impl CodexProvider {
  pub fn new(settings: &CodexSettings) -> Result<Self> {
    log::info!("Initializing Codex provider");

    let provider = Self {
      settings: settings.clone(),
      client: Client::new(),
      refresh_lock: Mutex::new(()),
    };

    provider.read_credentials()?;

    return Ok(provider);
  }

  /// Reads the Codex credentials fresh on every request: the CLI refreshes the tokens in place,
  /// and re-reading a local file is cheaper than tracking expiry ourselves.
  fn read_credentials(&self) -> Result<Credentials> {
    if let Some(token) = &self.settings.token {
      log::debug!("Using token from provider settings");

      return Ok(Credentials {
        token: SecretString::from(token.clone()),
        account_id: self.settings.account_id.clone(),
        refresh_token: None,
        expires_at: None,
        from_config: true,
      });
    }

    let path = get_auth_path()?;
    log::debug!("Reading Codex credentials from {}", path);

    let contents = fs_err::read_to_string(&path)?;
    let auth: CodexAuthFile = serde_json::from_str(&contents)?;
    let tokens = auth.tokens.context("No `tokens` in Codex auth file, log in with `codex login`")?;

    return Ok(Credentials {
      expires_at: jwt_expiry(&tokens.access_token),
      token: SecretString::from(tokens.access_token),
      account_id: self.settings.account_id.clone().or(tokens.account_id),
      refresh_token: tokens.refresh_token.map(SecretString::from),
      from_config: false,
    });
  }

  /// Exchanges the refresh token for new tokens and writes them back to `auth.json` so the
  /// Codex CLI keeps working (refresh tokens rotate; the old one is dead after this).
  fn refresh_via_oauth(&self, stale: &Credentials) -> Result<Credentials> {
    let refresh_token = stale.refresh_token.as_ref().context("Auth file has no refresh token")?;

    log::info!("Refreshing Codex access token via OAuth");

    let body = serde_json::json!({
      "client_id": OAUTH_CLIENT_ID,
      "grant_type": "refresh_token",
      "refresh_token": refresh_token.expose_secret(),
      "scope": "openid profile email",
    });

    #[derive(Deserialize)]
    struct TokenResponse {
      access_token: String,
      refresh_token: Option<String>,
      id_token: Option<String>,
    }

    let response = self.client.post_json(OAUTH_TOKEN_URL, &[], &body.to_string())?;
    let response: TokenResponse = serde_json::from_str(&response).context("Failed to parse token response")?;

    // Patch only the fields we own; everything else in the file stays as the CLI wrote it.
    let path = get_auth_path()?;
    let mut doc: serde_json::Value = serde_json::from_str(&fs_err::read_to_string(&path)?)?;
    let tokens = doc
      .get_mut("tokens")
      .and_then(serde_json::Value::as_object_mut)
      .context("Auth file has no `tokens` object")?;
    tokens.insert("access_token".into(), response.access_token.clone().into());
    if let Some(refresh) = &response.refresh_token {
      tokens.insert("refresh_token".into(), refresh.clone().into());
    }
    if let Some(id_token) = &response.id_token {
      tokens.insert("id_token".into(), id_token.clone().into());
    }
    doc["last_refresh"] = Timestamp::now().to_string().into();

    // Even if the write-back fails we must use the new token: the old refresh token is gone.
    if let Err(e) = Self::write_auth_file(&path, &doc) {
      log::error!("Refreshed token but failed to write it back to {} (Codex may need `codex login`): {e:#}", path);
    }

    return Ok(Credentials {
      expires_at: jwt_expiry(&response.access_token),
      token: SecretString::from(response.access_token),
      account_id: stale.account_id.clone(),
      refresh_token: response.refresh_token.map(SecretString::from).or_else(|| stale.refresh_token.clone()),
      from_config: false,
    });
  }

  /// Atomically replaces `auth.json`: write a sibling temp file with owner-only permissions,
  /// then rename over the original so the CLI never observes a half-written file.
  fn write_auth_file(path: &Utf8Path, doc: &serde_json::Value) -> Result<()> {
    use std::io::Write as _;

    use fs_err::os::unix::fs::OpenOptionsExt as _;

    let tmp = path.with_extension("json.tmp");
    let mut file = fs_err::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    file.write_all(serde_json::to_string_pretty(doc)?.as_bytes())?;
    file.sync_all()?;
    drop(file);

    fs_err::rename(&tmp, path)?;

    return Ok(());
  }

  /// Replaces stale credentials: first by re-reading `auth.json` (the CLI may already have
  /// refreshed it), and if that still yields the same token, by refreshing it ourselves.
  fn reload_or_refresh(&self, stale: &Credentials) -> Option<Credentials> {
    if stale.from_config {
      log::error!("Token from `settings.codex.token` was rejected; update or remove it to use `auth.json`");
      return None;
    }

    let _guard = self.refresh_lock.lock().unwrap();

    let from_file = match self.read_credentials() {
      Ok(credentials) => credentials,
      Err(e) => {
        log::error!("Failed to re-read Codex credentials: {e:#}");
        return None;
      }
    };

    if from_file.token.expose_secret() != stale.token.expose_secret() {
      log::info!("Loaded fresh Codex token from auth.json");
      return Some(from_file);
    }

    return self
      .refresh_via_oauth(&from_file)
      .inspect_err(|e| log::error!("Failed to refresh Codex access token: {e:#}"))
      .ok();
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.get(USAGE_URL)?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed codex usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse codex usage response: {}", e))
      .ok();
  }

  fn get(&self, url: &str) -> Option<String> {
    let mut credentials = self
      .read_credentials()
      .inspect_err(|e| log::error!("Failed to read Codex credentials: {:#}", e))
      .ok()?;

    // Proactive expiry check: the access token is a JWT, so we know when it dies.
    if credentials.expires_at.is_some_and(|ts| Timestamp::now() >= ts) && !credentials.from_config {
      log::debug!("Codex access token expired, refreshing before request");
      credentials = self.reload_or_refresh(&credentials)?;
    }

    let mut result = self.get_inner(url, &credentials);

    if let Err(HttpError::Status(401)) = &result {
      log::warn!("Got 401 for {}, refreshing token", url);

      if let Some(fresh) = self.reload_or_refresh(&credentials) {
        log::info!("Token refreshed, retrying request");
        result = self.get_inner(url, &fresh);
      }
    }

    return match result {
      Ok(body) => Some(body),
      // Backoff outcomes are already logged by the client.
      Err(HttpError::Skipped | HttpError::RateLimited) => None,
      Err(HttpError::Status(401)) => {
        log::error!("Codex token rejected (401), log in again with `codex login`");
        None
      }
      Err(e) => {
        log::error!("Request failed for {}: {}", url, e);
        None
      }
    };
  }

  fn get_inner(&self, url: &str, credentials: &Credentials) -> Result<String, HttpError> {
    let auth_header = format!("Bearer {}", credentials.token.expose_secret());
    let mut headers = vec![
      ("Authorization", auth_header.as_str()),
      ("Content-Type", "application/json"),
      ("User-Agent", USER_AGENT),
    ];

    if let Some(account_id) = &credentials.account_id {
      headers.push(("Chatgpt-Account-Id", account_id));
    }
    else {
      log::warn!("No ChatGPT account id available, the request will likely be rejected");
    }

    return self.client.get(url, &headers);
  }
}

impl DataProvider for CodexProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::Codex;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    return Some(self.fetch_usage()?.into());
  }

  fn fetch_profile(&self) -> Option<TierInfo> {
    // Normally unused: the tier is delivered through `UsageData::tier` by `fetch_data`.
    return self.fetch_usage().and_then(|u| u.plan_type.map(|t| t.tier_info()));
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../resources/codex.svg");
  }
}
