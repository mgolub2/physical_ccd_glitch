//! Simulation result caching.
//!
//! Caches transfer curves, ringing kernels, and noise parameters
//! to avoid re-running SPICE simulations on every frame.

use super::{SpiceCache, SpiceParams};

/// Check if the cache is still valid for the given parameters.
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
            let fb = &c.fallbacks;
            let tag = |analytical: bool| if analytical { "(A)" } else { "" };
            format!(
                "pixel={}pts{}, CTE={:.6}{}, amp={}pts{}, CDS={:.2}{}, ADC={}codes{}, noise={:.1}e-, {:.1}ms [{}/6 SPICE]",
                c.pixel_transfer.len(), tag(fb.pixel),
                c.effective_cte, tag(fb.shift_register),
                c.amp_transfer_curve.len(), tag(fb.amplifier),
                c.cds_rejection, tag(fb.cds),
                c.adc_transfer.len(), tag(fb.adc),
                c.noise_sigma,
                c.sim_time_ms,
                fb.spice_count(),
            )
        }
        None => "No simulation data".to_string(),
    }
}
