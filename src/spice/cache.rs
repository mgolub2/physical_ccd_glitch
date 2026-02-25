//! Simulation result caching.
//!
//! Caches transfer curves, ringing kernels, and noise parameters
//! to avoid re-running SPICE simulations on every frame.

use super::{SpiceCache, SpiceParams};

/// Check if the cache is still valid for the given parameters.
#[allow(dead_code)]
pub fn is_cache_valid(cache: &Option<SpiceCache>, params: &SpiceParams) -> bool {
    match cache {
        Some(c) => c.is_valid_for(params),
        None => false,
    }
}

/// Invalidate the cache, forcing re-simulation on next use.
pub fn invalidate(cache: &mut Option<SpiceCache>) {
    *cache = None;
}

/// Get a summary string of the cached simulation results.
pub fn cache_summary(cache: &Option<SpiceCache>) -> String {
    match cache {
        Some(c) => {
            format!(
                "{} pts, CTE={:.6}, noise={:.1}e-, {:.1}ms",
                c.transfer_curve.len(),
                c.effective_cte,
                c.noise_sigma,
                c.sim_time_ms,
            )
        }
        None => "No simulation data".to_string(),
    }
}
