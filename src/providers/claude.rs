use std::{
  sync::{LazyLock, Mutex},
  time::{Duration, Instant},
};

use color_eyre::eyre::{Context as _, ContextCompat as _, Result, bail};
use jiff::Timestamp;
use rgb::Rgb;
use secrecy::{ExposeSecret, SecretString};
use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use serde::{Deserialize, Serialize};

use super::{DataProvider, PeakHours, ProviderKind, UsageData};
use crate::{
  providers::{ApiUsage, Tier, UsageWindow},
  utils::http::{Client, HttpError},
};

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const OVERAGE_GRANT_URL: &str = "https://api.anthropic.com/api/oauth/organizations/{org}/overage_credit_grant";
const OAUTH_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
/// Claude Code's public OAuth client id, embedded in the CLI.
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const ANTHROPIC_BETA: &str = "oauth-2025-04-20";
pub const USER_AGENT: &str = "claude-code/2.1.71";
const KEYCHAIN_SERVICE_BASE: &str = "Claude Code-credentials";

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ClaudeSettings {
  /// OAuth token override. If not set, reads from keychain.
  pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageResponse {
  #[serde(default)]
  pub limits: Vec<Limit>,
  pub extra_usage: Option<ExtraUsage>,
}

/// One entry of the `limits` array: a single usage window.
#[derive(Debug, Deserialize, Clone)]
pub struct Limit {
  pub kind: String,
  #[serde(default)]
  pub percent: Option<f64>,
  pub resets_at: Option<Timestamp>,
  #[serde(default)]
  pub scope: Option<LimitScope>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LimitScope {
  #[serde(default)]
  pub model: Option<LimitScopeModel>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LimitScopeModel {
  #[serde(default)]
  pub display_name: Option<String>,
}

impl Limit {
  fn to_window(&self) -> Option<UsageWindow> {
    let (title, short_title, period_secs) = match self.kind.as_str() {
      "session" => ("5h Limit".to_string(), Some("5h".to_string()), 5 * 3600),
      "weekly_all" => ("7d Limit".to_string(), Some("7d".to_string()), 7 * 86400),
      "weekly_scoped" => {
        let name = self.scope.as_ref()?.model.as_ref()?.display_name.as_ref()?;
        (format!("7d {}", name), None, 7 * 86400)
      }
      _ => return None,
    };

    return Some(UsageWindow {
      title,
      short_title,
      utilization: self.percent.unwrap_or(0.0),
      resets_at: self.resets_at,
      period_seconds: Some(period_secs),
    });
  }
}

#[expect(unused)]
/// Weekdays 13:00–19:00 GMT are peak hours for Claude.
pub fn compute_claude_peak_hours() -> PeakHours {
  let now = Timestamp::now().to_zoned(jiff::tz::TimeZone::get("GMT").unwrap());
  let weekday = now.weekday();
  let hour = now.hour();

  let is_weekday = weekday != jiff::civil::Weekday::Saturday && weekday != jiff::civil::Weekday::Sunday;
  let is_peak = is_weekday && (13 .. 19).contains(&hour);

  let ends_at = if is_peak {
    // Peak ends at 19:00 today
    now.with().hour(19).minute(0).second(0).build().unwrap().timestamp()
  }
  else if is_weekday && hour < 13 {
    // Off-peak ends at 13:00 today
    now.with().hour(13).minute(0).second(0).build().unwrap().timestamp()
  }
  else {
    // Weekend or weekday after 19:00 — next peak is Monday 13:00 (or tomorrow if weekday)
    let days_until = match weekday {
      jiff::civil::Weekday::Friday if hour >= 19 => 3,
      jiff::civil::Weekday::Saturday => 2,
      jiff::civil::Weekday::Sunday => 1,
      _ => 1, // weekday after 19:00
    };
    let next_day = now.checked_add(jiff::SignedDuration::from_hours(days_until * 24)).unwrap();
    next_day.with().hour(13).minute(0).second(0).build().unwrap().timestamp()
  };

  return PeakHours { is_peak, ends_at };
}

impl From<UsageResponse> for UsageData {
  fn from(usage: UsageResponse) -> Self {
    let api_usage = usage.extra_usage.as_ref().map(|extra| {
      return ApiUsage {
        is_enabled: extra.is_enabled,
        usage_usd: extra.used_credits.unwrap_or(0.0) / 100.0,
        max_paid_usd: extra.monthly_limit.map(|l| l / 100.0),
        free_credits_usd: None,
      };
    });

    let windows: Vec<UsageWindow> = usage.limits.iter().filter_map(Limit::to_window).collect();

    return UsageData {
      api_usage,
      // Peak hours are disabled now.
      // peak_hours: Some(compute_claude_peak_hours()),
      peak_hours: None,
      windows,
      details: Vec::new(),
      tier: None,
    };
  }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtraUsage {
  pub is_enabled: bool,
  #[serde(default)]
  pub monthly_limit: Option<f64>,
  #[serde(default)]
  pub used_credits: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileResponse {
  pub organization: ProfileOrganization,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileOrganization {
  pub uuid: String,
  pub rate_limit_tier: SubscriptionTier,
}

/// Response shape of `/api/oauth/organizations/{uuid}/overage_credit_grant`. Anthropic
/// occasionally gifts overage credits to users; this endpoint reports the current grant.
#[derive(Debug, Deserialize, Clone)]
pub struct OverageCreditGrant {
  #[serde(default)]
  pub amount_minor_units: Option<i64>,
  #[serde(default)]
  pub currency: Option<String>,
}

impl OverageCreditGrant {
  /// Converts the grant to a USD dollar amount, mirroring Claude Code's `mEH` formatter:
  /// returns `None` unless both `amount_minor_units` and `currency == "USD"` are set.
  fn to_usd(&self) -> Option<f64> {
    let amount = self.amount_minor_units?;
    let currency = self.currency.as_ref()?;
    if !currency.eq_ignore_ascii_case("USD") {
      return None;
    }
    return Some(amount as f64 / 100.0);
  }
}

const OVERAGE_GRANT_TTL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Deserialize, strum::EnumString, strum::Display)]
#[serde(from = "String")]
pub enum SubscriptionTier {
  #[strum(serialize = "default_claude_free", to_string = "Free")]
  Free,
  #[strum(serialize = "default_claude_pro", to_string = "Pro")]
  Pro,
  #[strum(serialize = "default_claude_max_5x", to_string = "Max 5x")]
  Max5x,
  #[strum(serialize = "default_claude_max_20x", to_string = "Max 20x")]
  Max20x,
  /// A tier this app doesn't know about yet; keeps the raw API identifier so a new plan
  /// still shows a badge instead of failing the profile parse.
  #[strum(default)]
  Unknown(String),
}

impl From<String> for SubscriptionTier {
  fn from(value: String) -> Self {
    // Never fails: the `default` variant absorbs anything unrecognized.
    let tier = value.parse().unwrap_or(SubscriptionTier::Unknown(value));

    // Anthropic's identifiers all start with "default_claude_"; drop it so an unknown tier's
    // badge reads "team" rather than "default_claude_team".
    return match tier {
      SubscriptionTier::Unknown(raw) => {
        SubscriptionTier::Unknown(raw.strip_prefix("default_claude_").unwrap_or(&raw).to_string())
      }
      known => known,
    };
  }
}

impl SubscriptionTier {
  pub fn tier(&self) -> Tier {
    return Tier {
      name: self.to_string(),
      color: match self {
        SubscriptionTier::Free | SubscriptionTier::Unknown(_) => Rgb::new(140, 140, 155),
        SubscriptionTier::Pro => Rgb::new(90, 145, 210),
        SubscriptionTier::Max5x => Rgb::new(145, 110, 200),
        SubscriptionTier::Max20x => Rgb::new(205, 130, 95),
      },
    };
  }
}

/// Keychain service name of the Claude Code credentials item.
///
/// Claude Code keeps one item per config home: with `CLAUDE_CONFIG_DIR` set, the CLI appends
/// `-` plus the first 8 hex chars of SHA-256 of the *literal* env value (no path normalization —
/// a trailing slash changes the hash, and even the default `~/.claude` spelled out gets a suffix).
static KEYCHAIN_SERVICE: LazyLock<String> = LazyLock::new(|| {
  return match std::env::var("CLAUDE_CONFIG_DIR").ok().filter(|v| !v.is_empty()) {
    Some(config_dir) => format!("{}-{}", KEYCHAIN_SERVICE_BASE, config_dir_suffix(&config_dir)),
    None => KEYCHAIN_SERVICE_BASE.to_string(),
  };
});

fn config_dir_suffix(config_dir: &str) -> String {
  let digest = ring::digest::digest(&ring::digest::SHA256, config_dir.as_bytes());

  return digest.as_ref()[.. 4].iter().map(|b| format!("{:02x}", b)).collect();
}

pub struct ClaudeProvider {
  settings: ClaudeSettings,
  /// In-memory credentials. Unlike Codex, these aren't re-read on every request: a keychain
  /// read spawns `security` (or may prompt), so they're cached until known-stale.
  credentials: Mutex<Credentials>,
  /// Serializes token refreshes so two concurrent fetches can't both spend the (rotating)
  /// refresh token.
  refresh_lock: Mutex<()>,
  client: Client,
  /// Whether keychain reads (including token refreshes) go through the `security` CLI.
  cli_keychain: bool,
  /// Organization UUID, lazily populated from the first profile fetch.
  org_uuid: Mutex<Option<String>>,
  /// Cached overage credit grant info, refreshed at most once per `OVERAGE_GRANT_TTL`.
  overage_grant: Mutex<OverageGrantCache>,
}

#[derive(Clone)]
struct Credentials {
  token: SecretString,
  /// Known expiry from the keychain. `None` when the source doesn't provide one (e.g. config override).
  expires_at: Option<Timestamp>,
  /// Refresh token from the keychain, used to mint a new access token when the CLI hasn't.
  refresh_token: Option<SecretString>,
  /// The raw keychain JSON these were parsed from, so a refresh can write back without
  /// dropping fields liment doesn't know about.
  raw_json: Option<String>,
  /// Whether the token came from the config override (never refreshed or replaced).
  from_config: bool,
}

#[derive(Default)]
struct OverageGrantCache {
  grant: Option<OverageCreditGrant>,
  fetched_at: Option<Instant>,
}

impl ClaudeProvider {
  pub fn new(settings: &ClaudeSettings, cli_keychain: bool) -> Result<Self> {
    log::info!("Initializing Claude Code provider");

    let provider = Self {
      settings: settings.clone(),
      credentials: Mutex::new(Credentials {
        token: SecretString::from(String::new()),
        expires_at: None,
        refresh_token: None,
        raw_json: None,
        from_config: false,
      }),
      refresh_lock: Mutex::new(()),
      client: Client::new(),
      cli_keychain,
      org_uuid: Mutex::new(None),
      overage_grant: Mutex::new(OverageGrantCache::default()),
    };

    *provider.credentials.lock().unwrap() = provider.read_credentials()?;

    return Ok(provider);
  }

  /// Reads the credentials from the config override if set, otherwise from the keychain.
  fn read_credentials(&self) -> Result<Credentials> {
    if let Some(token) = &self.settings.token {
      log::debug!("Using token from provider settings");

      return Ok(Credentials {
        token: SecretString::from(token.clone()),
        expires_at: None,
        refresh_token: None,
        raw_json: None,
        from_config: true,
      });
    }

    log::debug!("Token not set in config, reading from keychain");

    return self.read_keychain_credentials();
  }

  /// Reads and parses the Claude Code OAuth credentials from the macOS login keychain.
  ///
  /// When `cli_keychain` is set, the read is delegated to the `security` command-line tool
  /// instead of the keychain API. Because the requesting process is then Apple's signed
  /// `security` binary (which the item's ACL trusts) rather than this app, the per-app keychain
  /// access prompt never appears.
  fn read_keychain_credentials(&self) -> Result<Credentials> {
    let json_str = if self.cli_keychain { Self::read_keychain_via_cli()? } else { Self::read_keychain_via_api()? };

    #[derive(Deserialize)]
    struct ClaudeOAuth {
      #[serde(rename = "accessToken")]
      access_token: String,
      #[serde(rename = "refreshToken")]
      refresh_token: Option<String>,
      #[serde(rename = "expiresAt")]
      expires_at: Option<i64>,
    }

    #[derive(Deserialize)]
    struct ClaudeKeychain {
      #[serde(rename = "claudeAiOauth")]
      claude_oauth: ClaudeOAuth,
    }

    let value: ClaudeKeychain = serde_json::from_str(&json_str)?;
    let expires_at = value.claude_oauth.expires_at.and_then(|ms| Timestamp::from_millisecond(ms).ok());

    return Ok(Credentials {
      token: SecretString::from(value.claude_oauth.access_token),
      expires_at,
      refresh_token: value.claude_oauth.refresh_token.map(SecretString::from),
      raw_json: Some(json_str),
      from_config: false,
    });
  }

  /// Writes the credentials JSON back to the keychain item, preserving its account name.
  ///
  /// Always goes through the `security` CLI (regardless of `cli_keychain`) — that's how Claude
  /// Code itself writes the item, so the ACL keeps trusting the same signed binary.
  fn write_credentials(json: &str) -> Result<()> {
    let attrs = std::process::Command::new("security")
      .args(["find-generic-password", "-s", &KEYCHAIN_SERVICE])
      .output()?;

    if !attrs.status.success() {
      bail!("`security find-generic-password` failed: {}", String::from_utf8_lossy(&attrs.stderr).trim());
    }

    // Attribute dump contains a line like: "acct"<blob>="username"
    let account = String::from_utf8_lossy(&attrs.stdout)
      .lines()
      .find_map(|line| line.trim().strip_prefix("\"acct\"<blob>=\""))
      .and_then(|rest| rest.strip_suffix('"'))
      .map(str::to_owned)
      .context("Keychain item has no account attribute")?;

    let status = std::process::Command::new("security")
      .args([
        "add-generic-password",
        "-U",
        "-a",
        &account,
        "-s",
        &KEYCHAIN_SERVICE,
        "-w",
        json,
      ])
      .output()?;

    if !status.status.success() {
      bail!("`security add-generic-password` failed: {}", String::from_utf8_lossy(&status.stderr).trim());
    }

    return Ok(());
  }

  /// Exchanges the refresh token for a new access token and persists the result to the
  /// keychain so Claude Code keeps working (refresh tokens rotate; the old one is dead after this).
  fn refresh_via_oauth(&self, stale: &Credentials) -> Result<Credentials> {
    let refresh_token = stale.refresh_token.as_ref().context("Keychain has no refresh token")?;
    let raw_json = stale.raw_json.as_ref().context("No keychain JSON to update")?;

    log::info!("Refreshing Claude Code access token via OAuth");

    let body = serde_json::json!({
      "grant_type": "refresh_token",
      "refresh_token": refresh_token.expose_secret(),
      "client_id": OAUTH_CLIENT_ID,
    });

    #[derive(Deserialize)]
    struct TokenResponse {
      access_token: String,
      refresh_token: Option<String>,
      expires_in: Option<i64>,
    }

    let response = self.client.post_json(OAUTH_TOKEN_URL, &[], &body.to_string())?;
    let response: TokenResponse = serde_json::from_str(&response).context("Failed to parse token response")?;

    let expires_at = response
      .expires_in
      .and_then(|secs| Timestamp::now().checked_add(jiff::SignedDuration::from_secs(secs)).ok());

    // Patch only the fields we own; everything else in the item stays as the CLI wrote it.
    let mut doc: serde_json::Value = serde_json::from_str(raw_json)?;
    let oauth = doc
      .get_mut("claudeAiOauth")
      .and_then(serde_json::Value::as_object_mut)
      .context("Keychain JSON has no claudeAiOauth object")?;
    oauth.insert("accessToken".into(), response.access_token.clone().into());
    if let Some(refresh) = &response.refresh_token {
      oauth.insert("refreshToken".into(), refresh.clone().into());
    }
    if let Some(expires_at) = expires_at {
      oauth.insert("expiresAt".into(), expires_at.as_millisecond().into());
    }
    let new_json = doc.to_string();

    // Even if the write-back fails we must use the new token: the old refresh token is gone.
    if let Err(e) = Self::write_credentials(&new_json) {
      log::error!(
        "Refreshed token but failed to write it back to the keychain (Claude Code may need `claude login`): {e:#}"
      );
    }

    return Ok(Credentials {
      token: SecretString::from(response.access_token),
      expires_at,
      refresh_token: response.refresh_token.map(SecretString::from).or_else(|| stale.refresh_token.clone()),
      raw_json: Some(new_json),
      from_config: false,
    });
  }

  /// Replaces stale credentials: first by re-reading the keychain (the CLI may already have
  /// refreshed it), and if that still yields the same token, by refreshing it ourselves.
  /// The result is also stored as the provider's current credentials.
  fn reload_or_refresh(&self, stale: &Credentials) -> Option<Credentials> {
    if stale.from_config {
      log::error!("Token from `settings.claude_code.token` was rejected; update or remove it to use the keychain");
      return None;
    }

    let _guard = self.refresh_lock.lock().unwrap();

    // Another thread may have replaced the credentials while we waited for the lock.
    {
      let current = self.credentials.lock().unwrap();
      if current.token.expose_secret() != stale.token.expose_secret() {
        return Some(current.clone());
      }
    }

    let from_keychain = match self.read_keychain_credentials() {
      Ok(credentials) => credentials,
      Err(e) => {
        log::error!("Failed to re-read keychain: {e:#}");
        return None;
      }
    };

    let fresh = if from_keychain.token.expose_secret() != stale.token.expose_secret() {
      log::info!("Loaded fresh token from keychain");
      from_keychain
    }
    else {
      self
        .refresh_via_oauth(&from_keychain)
        .inspect_err(|e| log::error!("Failed to refresh access token: {e:#}"))
        .ok()?
    };

    *self.credentials.lock().unwrap() = fresh.clone();

    return Some(fresh);
  }

  /// Reads the raw credentials JSON from the keychain using the `security_framework` API.
  fn read_keychain_via_api() -> Result<String> {
    let results = ItemSearchOptions::new()
      .class(ItemClass::generic_password())
      .service(&KEYCHAIN_SERVICE)
      .load_data(true)
      .search()?;

    let data = results
      .into_iter()
      .find_map(|r| {
        match r {
          SearchResult::Data(d) => Some(d),
          _ => None,
        }
      })
      .context("Failed to find Claude Code credentials in keychain")?;

    return Ok(String::from_utf8(data)?);
  }

  /// Reads the raw credentials JSON by spawning `security find-generic-password`.
  fn read_keychain_via_cli() -> Result<String> {
    log::debug!("Reading keychain via the `security` CLI");

    let output = std::process::Command::new("security")
      .args(["find-generic-password", "-s", &KEYCHAIN_SERVICE, "-w"])
      .output()?;

    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr);
      bail!("`security find-generic-password` failed: {}", stderr.trim());
    }

    return Ok(String::from_utf8(output.stdout)?.trim().to_string());
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    log::debug!("Fetching usage data");

    let body = self.get(USAGE_URL)?;

    return serde_json::from_str(&body)
      .inspect(|u: &UsageResponse| log::debug!("Parsed usage: {:?}", u))
      .inspect_err(|e| log::warn!("Failed to parse usage response: {}", e))
      .ok();
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    log::debug!("Fetching profile data");

    let body = self.get(PROFILE_URL)?;

    let response: Option<ProfileResponse> = serde_json::from_str(&body)
      .inspect(|p: &ProfileResponse| log::debug!("Parsed profile: {:?}", p))
      .inspect_err(|e| log::warn!("Failed to parse profile response: {}", e))
      .ok();

    // Cache the org UUID for use by other endpoints (e.g. overage credit grant).
    if let Some(ref response) = response {
      *self.org_uuid.lock().unwrap() = Some(response.organization.uuid.clone());
    }

    return response;
  }

  /// Fetches Anthropic-gifted overage credit info, with a 1-hour cache to match Claude Code's
  /// own cache TTL. Returns `None` if the org UUID hasn't been learned yet (i.e. we haven't
  /// fetched the profile yet) or if the request fails.
  fn fetch_overage_grant(&self) -> Option<OverageCreditGrant> {
    // Serve from cache if fresh.
    {
      let cache = self.overage_grant.lock().unwrap();
      if let Some(fetched_at) = cache.fetched_at
        && fetched_at.elapsed() < OVERAGE_GRANT_TTL
      {
        log::debug!("Using cached overage grant ({}s old)", fetched_at.elapsed().as_secs());
        return cache.grant.clone();
      }
    }

    let org_uuid = self.org_uuid.lock().unwrap().clone()?;

    log::debug!("Fetching overage credit grant");
    let url = OVERAGE_GRANT_URL.replace("{org}", &org_uuid);
    let body = self.get(&url)?;

    let grant: Option<OverageCreditGrant> = serde_json::from_str(&body)
      .inspect(|g: &OverageCreditGrant| log::debug!("Parsed overage grant: {:?}", g))
      .inspect_err(|e| log::warn!("Failed to parse overage grant: {}", e))
      .ok();

    // Update cache regardless of parse outcome so a parse failure doesn't trigger
    // a hot retry on every refresh.
    let mut cache = self.overage_grant.lock().unwrap();
    cache.grant = grant.clone();
    cache.fetched_at = Some(Instant::now());

    return grant;
  }

  fn get(&self, url: &str) -> Option<String> {
    let mut credentials = self.credentials.lock().unwrap().clone();

    // Proactive expiry check: if the current token is known to have expired,
    // get a fresh one before making the request.
    if credentials.expires_at.is_some_and(|ts| Timestamp::now() >= ts) && !credentials.from_config {
      log::debug!("Access token expired, refreshing before request");
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
      Err(e) => {
        log::error!("Request failed for {}: {}", url, e);
        None
      }
    };
  }

  fn get_inner(&self, url: &str, credentials: &Credentials) -> Result<String, HttpError> {
    let auth_header = format!("Bearer {}", credentials.token.expose_secret());

    return self.client.get(url, &[
      ("Authorization", &auth_header),
      ("anthropic-beta", ANTHROPIC_BETA),
      ("User-Agent", USER_AGENT),
    ]);
  }
}

impl DataProvider for ClaudeProvider {
  fn kind(&self) -> ProviderKind {
    return ProviderKind::ClaudeCode;
  }

  fn fetch_data(&self) -> Option<UsageData> {
    let mut data: UsageData = self.fetch_usage()?.into();

    // Only bother fetching the overage grant if there's an extra-usage section to plumb
    // it into. Free credits make no sense for accounts without extra usage in the first place.
    if data.api_usage.is_some() {
      // Lazily learn the org UUID via a profile fetch on first call. Subsequent calls
      // hit the cached UUID directly. The tier from that same response is passed along so
      // the profile cache doesn't repeat the request.
      if self.org_uuid.lock().unwrap().is_none()
        && let Some(profile) = self.fetch_profile()
      {
        data.tier = Some(profile.organization.rate_limit_tier.tier());
      }

      if let Some(grant) = self.fetch_overage_grant()
        && let Some(api_usage) = data.api_usage.as_mut()
      {
        api_usage.free_credits_usd = grant.to_usd();
      }
    }

    return Some(data);
  }

  fn fetch_tier(&self) -> Option<Tier> {
    return self.fetch_profile().map(|p| p.organization.rate_limit_tier.tier());
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return include_bytes!("../../resources/claude.svg");
  }
}
