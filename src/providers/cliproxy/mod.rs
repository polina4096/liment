pub mod claude;
pub mod codex;

use std::collections::HashMap;

pub use claude::{CliproxyClaudeProvider, CliproxyClaudeSettings};
pub use codex::{CliproxyCodexProvider, CliproxyCodexSettings};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

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

pub struct CliproxyClient {
  base_url: String,
  management_token: SecretString,
}

impl CliproxyClient {
  pub fn new(base_url: &str, management_token: &str) -> Self {
    return Self {
      base_url: base_url.trim_end_matches('/').to_string(),
      management_token: SecretString::from(management_token.to_string()),
    };
  }

  pub fn management_get(&self, path: &str) -> Option<String> {
    let endpoint = format!("{}{}", self.base_url, path);

    let mut response = ureq::get(&endpoint)
      .header("Authorization", &format!("Bearer {}", self.management_token.expose_secret()))
      .call()
      .inspect_err(|e| log::error!("Cliproxy management GET failed for {}: {}", path, e))
      .ok()?;

    return response
      .body_mut()
      .read_to_string()
      .inspect_err(|e| log::error!("Failed to read cliproxy management GET response body: {}", e))
      .ok();
  }

  pub fn api_get(&self, auth_index: &str, url: &str, headers: HashMap<String, String>) -> Option<String> {
    log::debug!("Proxied GET {} via cliproxy", url);

    let request = ApiCallRequest {
      auth_index: auth_index.to_string(),
      method: "GET".to_string(),
      url: url.to_string(),
      header: headers,
    };

    let endpoint = format!("{}/v0/management/api-call", self.base_url);
    let json_body = serde_json::to_string(&request)
      .inspect_err(|e| log::error!("Failed to serialize api-call request: {}", e))
      .ok()?;

    let mut response = ureq::post(&endpoint)
      .header("Authorization", &format!("Bearer {}", self.management_token.expose_secret()))
      .header("Content-Type", "application/json")
      .send(&json_body)
      .inspect_err(|e| log::error!("Cliproxy request failed for {}: {}", url, e))
      .ok()?;

    let response_text = response
      .body_mut()
      .read_to_string()
      .inspect_err(|e| log::error!("Failed to read cliproxy response body: {}", e))
      .ok()?;

    let parsed: ApiCallResponse = serde_json::from_str(&response_text)
      .inspect_err(|e| log::error!("Failed to parse cliproxy response: {}", e))
      .ok()?;

    if parsed.status_code != 200 {
      log::error!("Cliproxy API returned status {}: {}", parsed.status_code, parsed.body);
      return None;
    }

    return Some(parsed.body);
  }
}
