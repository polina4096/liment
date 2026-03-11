use std::{
  collections::HashMap,
  sync::Mutex,
  time::{Duration, Instant},
};

use crate::providers::{DataProvider, ProviderKind, TierInfo};

const PROFILE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

struct CacheEntry {
  tier: TierInfo,
  last: Instant,
}

#[derive(Default)]
pub struct ProfileCache(Mutex<HashMap<ProviderKind, CacheEntry>>);

impl ProfileCache {
  /// Returns cached profile if fresh, otherwise fetches from the provider and caches it.
  pub fn resolve(&self, provider: &dyn DataProvider) -> Option<TierInfo> {
    let kind = provider.kind();

    // Retrieve cached entry if exists and fresh.
    if { true }
      && let Some(entry) = self.0.lock().unwrap().get(&kind)
      && entry.last.elapsed() < PROFILE_CACHE_TTL
    {
      log::debug!("Using cached profile for {} ({}s old)", kind, entry.last.elapsed().as_secs());

      return Some(TierInfo {
        name: entry.tier.name.clone(),
        color: entry.tier.color,
      });
    }

    // Fetch fresh profile from the provider and cache it.
    return provider.fetch_profile().inspect(|profile| {
      self.0.lock().unwrap().insert(kind, CacheEntry {
        tier: TierInfo {
          name: profile.name.clone(),
          color: profile.color,
        },
        last: Instant::now(),
      });
    });
  }
}
