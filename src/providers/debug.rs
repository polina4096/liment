use std::sync::Arc;

use jiff::Timestamp;
use rgb::Rgb;

use crate::{
  constants::*,
  providers::{ApiUsage, DataProvider, PeakHoursInfo, ProviderKind, TierInfo, UsageData},
};

/// Wraps another provider and overrides its data with values from environment variables.
pub struct DebugProvider {
  inner: Arc<dyn DataProvider>,
  tier: Option<TierInfo>,
  utilization: Option<f64>,
  resets_in: Option<i64>,
  extra_usage: Option<ApiUsage>,
  force_peak: bool,
}

impl DebugProvider {
  /// Returns `Some(DebugProvider)` if any `LIMENT_DEBUG_` env vars are set, otherwise `None`.
  pub fn try_wrap(inner: Arc<dyn DataProvider>) -> Option<Self> {
    let utilization = std::env::var(LIMENT_DEBUG_UTILIZATION).ok().and_then(|v| v.parse().ok());
    let resets_in = std::env::var(LIMENT_DEBUG_RESETS_IN).ok().and_then(|v| v.parse().ok());
    let tier = std::env::var(LIMENT_DEBUG_TIER).ok().and_then(|v| parse_tier(&v));
    let extra_usage = std::env::var(LIMENT_DEBUG_EXTRA_USAGE).ok().and_then(|v| parse_extra_usage(&v));
    let force_peak = std::env::var(LIMENT_DEBUG_PEAK_HOURS).is_ok();

    if utilization.is_none() && resets_in.is_none() && tier.is_none() && extra_usage.is_none() && !force_peak {
      return None;
    }

    log::info!(
      "Debug overrides active: utilization={utilization:?}, resets_in={resets_in:?}, \
       tier={}, extra_usage={}, force_peak={force_peak}",
      tier.is_some(),
      extra_usage.is_some(),
    );

    return Some(Self {
      inner,
      utilization,
      resets_in,
      tier,
      extra_usage,
      force_peak,
    });
  }
}

/// Parses "name:r,g,b" into a TierInfo.
fn parse_tier(s: &str) -> Option<TierInfo> {
  let (name, rgb) = s.split_once(':')?;
  let mut parts = rgb.split(',');
  let r = parts.next()?.trim().parse().ok()?;
  let g = parts.next()?.trim().parse().ok()?;
  let b = parts.next()?.trim().parse().ok()?;

  return Some(TierInfo {
    name: name.to_string(),
    color: Rgb::new(r, g, b),
  });
}

/// Parses "used:limit" or "used" into an ApiUsage.
fn parse_extra_usage(s: &str) -> Option<ApiUsage> {
  return match s.split_once(':') {
    Some((used, limit)) => {
      Some(ApiUsage {
        usage_usd: used.trim().parse().ok()?,
        limit_usd: Some(limit.trim().parse().ok()?),
      })
    }
    None => {
      Some(ApiUsage {
        usage_usd: s.trim().parse().ok()?,
        limit_usd: None,
      })
    }
  };
}

impl DataProvider for DebugProvider {
  fn kind(&self) -> ProviderKind {
    return self.inner.kind();
  }

  fn fetch_data(&self) -> Option<UsageData> {
    let mut data = self.inner.fetch_data()?;

    for window in &mut data.windows {
      if let Some(utilization) = self.utilization {
        window.utilization = utilization;
      }

      if let Some(resets_in) = self.resets_in {
        window.resets_at = Some(Timestamp::now().checked_add(jiff::SignedDuration::from_secs(resets_in)).unwrap());
      }
    }

    if let Some(ref extra_usage) = self.extra_usage {
      data.api_usage = Some(ApiUsage {
        usage_usd: extra_usage.usage_usd,
        limit_usd: extra_usage.limit_usd,
      });
    }

    if self.force_peak {
      let ends_at = data.peak_hours.as_ref().map(|p| p.ends_at).unwrap_or_else(|| {
        Timestamp::now().checked_add(jiff::SignedDuration::from_secs(3600)).unwrap()
      });
      data.peak_hours = Some(PeakHoursInfo { is_peak: true, ends_at });
    }

    return Some(data);
  }

  fn fetch_profile(&self) -> Option<TierInfo> {
    if let Some(ref tier) = self.tier {
      return Some(TierInfo {
        name: tier.name.clone(),
        color: tier.color,
      });
    }

    return self.inner.fetch_profile();
  }

  fn tray_icon_svg(&self) -> &'static [u8] {
    return self.inner.tray_icon_svg();
  }
}
