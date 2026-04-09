pub const LIMENT_NO_LOGS: &str = "LIMENT_NO_LOGS";
pub const LIMENT_NO_DISK_LOGS: &str = "LIMENT_NO_DISK_LOGS";
pub const LIMENT_OVERRIDE_LOG_DIR: &str = "LIMENT_OVERRIDE_LOG_DIR";
pub const LIMENT_OVERRIDE_VERSION: &str = "LIMENT_OVERRIDE_VERSION";

/// Override utilization percentage (0-100) for all windows.
pub const LIMENT_DEBUG_UTILIZATION: &str = "LIMENT_DEBUG_UTILIZATION";

/// Override reset time as seconds from now for all windows.
pub const LIMENT_DEBUG_RESETS_IN: &str = "LIMENT_DEBUG_RESETS_IN";

/// Override refetch interval in seconds.
pub const LIMENT_DEBUG_REFETCH_INTERVAL: &str = "LIMENT_DEBUG_REFETCH_INTERVAL";

/// Override tier name and color: "name:r,g,b" (e.g. "Pro:90,145,210").
pub const LIMENT_DEBUG_TIER: &str = "LIMENT_DEBUG_TIER";

/// Override extra usage in USD. Forms:
///   "used"                       — only used credits, no cap, no grant
///   "used:max_paid"              — used + paid spending cap
///   "used:max_paid:free"         — used + paid cap + free overage grant
/// Append ":disabled" to force `is_enabled = false` (e.g. "0:0:50:disabled").
pub const LIMENT_DEBUG_EXTRA_USAGE: &str = "LIMENT_DEBUG_EXTRA_USAGE";

/// Override peak hours state: "1"/"true" forces peak, "0"/"false" forces off-peak.
pub const LIMENT_DEBUG_PEAK_HOURS: &str = "LIMENT_DEBUG_PEAK_HOURS";
