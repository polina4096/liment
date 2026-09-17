use std::{
  collections::HashMap,
  sync::Mutex,
  time::{Duration, Instant},
};

use super::{DataProvider, ProviderKind, Tier, UsageData};

const TIER_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

struct CacheEntry {
  tier: Tier,
  last: Instant,
}

/// Resolves and caches the account tier shown in the menu header.
#[derive(Default)]
pub struct TierCache(Mutex<HashMap<ProviderKind, CacheEntry>>);

impl TierCache {
  /// Resolves the account tier for a refresh. A tier carried by the usage data wins and
  /// refreshes the cache for free; otherwise the cached tier is used while fresh, and
  /// `fetch_tier` is called only once it goes stale.
  pub fn resolve(&self, provider: &dyn DataProvider, data: Option<&UsageData>) -> Option<Tier> {
    let kind = provider.kind();

    if let Some(tier) = data.and_then(|d| d.tier.clone()) {
      self.store(kind, &tier);
      return Some(tier);
    }

    if let Some(entry) = self.0.lock().unwrap().get(&kind)
      && entry.last.elapsed() < TIER_CACHE_TTL
    {
      log::debug!("Using cached tier for {} ({}s old)", kind, entry.last.elapsed().as_secs());
      return Some(entry.tier.clone());
    }

    return provider.fetch_tier().inspect(|tier| self.store(kind, tier));
  }

  fn store(&self, kind: ProviderKind, tier: &Tier) {
    self.0.lock().unwrap().insert(kind, CacheEntry { tier: tier.clone(), last: Instant::now() });
  }
}
