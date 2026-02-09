use std::collections::HashMap;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use super::{UsageData, UsageProvider};
use crate::utils::claude_api::{ProfileResponse, UsageResponse};

use crate::config::ProviderDef;

pub struct CliproxyClaudeProvider {
  base_url: String,
  management_token: String,
  auth_index: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiCallRequest {
  auth_index: String,
  method: String,
  url: String,
  header: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ApiCallResponse {
  status_code: u16,
  body: String,
}

impl CliproxyClaudeProvider {
  pub fn new(def: &ProviderDef) -> Result<Self> {
    let base_url = def
      .config
      .get("base_url")
      .and_then(|v| v.as_str())
      .context("cliproxy_claude: missing `base_url`")?
      .trim_end_matches('/')
      .to_string();

    let management_token = def
      .config
      .get("management_token")
      .and_then(|v| v.as_str())
      .context("cliproxy_claude: missing `management_token`")?
      .to_string();

    let auth_index = def
      .config
      .get("auth_index")
      .and_then(|v| v.as_str())
      .context("cliproxy_claude: missing `auth_index`")?
      .to_string();

    return Ok(Self { base_url, management_token, auth_index });
  }

  fn api_get(&self, url: &str) -> Option<String> {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer $TOKEN$".to_string());
    headers.insert(
      "Anthropic-Beta".to_string(),
      "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14,prompt-caching-2024-07-31".to_string(),
    );
    headers.insert("Anthropic-Version".to_string(), "2023-06-01".to_string());
    headers.insert("User-Agent".to_string(), "claude-cli/1.0.83 (external, cli)".to_string());
    headers.insert("X-app".to_string(), "cli".to_string());

    let req_body = ApiCallRequest {
      auth_index: self.auth_index.clone(),
      method: "GET".to_string(),
      url: url.to_string(),
      header: headers,
    };

    let endpoint = format!("{}/v0/management/api-call", self.base_url);
    let json_body = serde_json::to_string(&req_body).ok()?;

    let result: Result<String, ureq::Error> = (|| {
      let mut response = ureq::post(&endpoint)
        .header("Authorization", &format!("Bearer {}", self.management_token))
        .header("Content-Type", "application/json")
        .send(&json_body)?;

      return response.body_mut().read_to_string();
    })();

    let response_text = result.inspect_err(|e| eprintln!("cliproxy request error: {}", e)).ok()?;

    let parsed: ApiCallResponse = serde_json::from_str(&response_text)
      .inspect_err(|e| eprintln!("cliproxy response parse error: {}", e))
      .ok()?;

    if parsed.status_code != 200 {
      eprintln!("cliproxy API returned status {}: {}", parsed.status_code, parsed.body);
      return None;
    }

    return Some(parsed.body);
  }

  fn fetch_usage(&self) -> Option<UsageResponse> {
    let body = self.api_get("https://api.anthropic.com/api/oauth/usage")?;
    serde_json::from_str(&body).ok()
  }

  fn fetch_profile(&self) -> Option<ProfileResponse> {
    let body = self.api_get("https://api.anthropic.com/api/oauth/profile")?;
    serde_json::from_str(&body).ok()
  }
}

impl UsageProvider for CliproxyClaudeProvider {
  fn fetch_data(&self) -> Option<UsageData> {
    let usage = self.fetch_usage()?;
    let profile = self.fetch_profile();
    return Some(crate::utils::claude_api::into_usage_data(usage, profile));
  }

  fn placeholder_lines(&self) -> [&str; 2] {
    ["7d ..", "5h .."]
  }
}
