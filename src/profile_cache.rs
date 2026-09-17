use std::{
  collections::HashMap,
  sync::Mutex,
  time::{Duration, Instant},
};

use crate::providers::{DataProvider, ProviderKind, TierInfo, UsageData};

const PROFILE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

struct CacheEntry {
  tier: TierInfo,
  last: Instant,
}

#[derive(Default)]
pub struct ProfileCache(Mutex<HashMap<ProviderKind, CacheEntry>>);

impl ProfileCache {
  /// Resolves the account tier for a refresh. A tier carried by the usage data wins and
  /// refreshes the cache for free; otherwise the cached tier is used while fresh, and
  /// `fetch_profile` is called only once it goes stale.
  pub fn resolve(&self, provider: &dyn DataProvider, data: Option<&UsageData>) -> Option<TierInfo> {
    let kind = provider.kind();

    if let Some(tier) = data.and_then(|d| d.tier.clone()) {
      self.store(kind, &tier);
      return Some(tier);
    }

    if let Some(entry) = self.0.lock().unwrap().get(&kind)
      && entry.last.elapsed() < PROFILE_CACHE_TTL
    {
      log::debug!("Using cached profile for {} ({}s old)", kind, entry.last.elapsed().as_secs());
      return Some(entry.tier.clone());
    }

    return provider.fetch_profile().inspect(|tier| self.store(kind, tier));
  }

  fn store(&self, kind: ProviderKind, tier: &TierInfo) {
    self.0.lock().unwrap().insert(kind, CacheEntry { tier: tier.clone(), last: Instant::now() });
  }
}
