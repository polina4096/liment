pub mod claude;
pub mod codex;

use std::collections::HashMap;

pub use claude::{CliproxyClaudeProvider, CliproxyClaudeSettings};
pub use codex::{CliproxyCodexProvider, CliproxyCodexSettings};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::utils::http::Client;

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
  client: Client,
}

impl CliproxyClient {
  pub fn new(base_url: &str, management_token: &str) -> Self {
    return Self {
      base_url: base_url.trim_end_matches('/').to_string(),
      management_token: SecretString::from(management_token.to_string()),
      client: Client::new(),
    };
  }

  fn auth_header(&self) -> String {
    return format!("Bearer {}", self.management_token.expose_secret());
  }

  pub fn management_get(&self, path: &str) -> Option<String> {
    let endpoint = format!("{}{}", self.base_url, path);
    let auth = self.auth_header();

    return self
      .client
      .get(&endpoint, &[("Authorization", &auth)])
      .inspect_err(|e| log::error!("Cliproxy management GET failed for {}: {}", path, e))
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
    let auth = self.auth_header();

    let response_text = self
      .client
      .post_json(&endpoint, &[("Authorization", &auth)], &json_body)
      .inspect_err(|e| log::error!("Cliproxy request failed for {}: {}", url, e))
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
