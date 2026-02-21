use std::time::SystemTime;

use jiff::Timestamp;

use crate::providers::{ApiUsage, DataProvider, TierInfo, UsageData, UsageWindow};

/// Wraps another provider and overrides its data with cycling debug values.
pub struct DebugProvider {
  tiers: Vec<TierInfo>,
}

impl DebugProvider {
  pub fn new(inner: &dyn DataProvider) -> Self {
    let tiers = inner.all_tiers();
    return Self { tiers };
  }
}

impl DataProvider for DebugProvider {
  fn fetch_data(&self) -> Option<UsageData> {
    let secs = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();

    // Cycle utilization 0 -> 100 over 10 seconds.
    let utilization = (secs % 10.0) / 10.0 * 100.0;

    // Cycle through tiers, switching every 3 seconds.
    let tier = &self.tiers[(secs / 3.0) as usize % self.tiers.len()];

    let now = Timestamp::now();
    let windows = vec![
      UsageWindow {
        title: "5h Limit".into(),
        short_title: Some("5h".into()),
        utilization,
        resets_at: Some(now),
        period_seconds: Some(5 * 3600),
      },
      UsageWindow {
        title: "7d Limit".into(),
        short_title: Some("7d".into()),
        utilization,
        resets_at: Some(now),
        period_seconds: Some(7 * 86400),
      },
    ];

    return Some(UsageData {
      account_tier: Some(TierInfo {
        name: tier.name.clone(),
        color: tier.color,
      }),
      api_usage: Some(ApiUsage { usage_usd: 4.20, limit_usd: Some(10.0) }),
      windows,
    });
  }

  fn all_tiers(&self) -> Vec<TierInfo> {
    return self.tiers.iter().map(|t| TierInfo { name: t.name.clone(), color: t.color }).collect();
  }
}
