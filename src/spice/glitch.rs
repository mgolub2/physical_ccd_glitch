//! Circuit-level glitch injection.
//!
//! These functions modify SpiceParams before circuit construction,
//! so glitch effects emerge naturally from the simulation.

use super::SpiceParams;

/// Apply all configured glitches to create modified parameters for simulation.
///
/// Returns a new SpiceParams with glitch effects baked in.
pub fn apply_glitches(params: &SpiceParams) -> SpiceParams {
    let mut p = params.clone();
    apply_supply_droop(&mut p);
    apply_phase_overlap(&mut p);
    apply_charge_injection_scale(&mut p);
    apply_substrate_noise_scale(&mut p);
    p
}

/// Supply droop: reduces effective VDD.
///
/// In a real CCD, heavy readout current can droop the supply,
/// reducing clock swing and degrading transfer efficiency.
fn apply_supply_droop(params: &mut SpiceParams) {
    // Supply droop is already handled by effective_vdd(),
    // but we can add secondary effects here
    if params.supply_droop > 0.0 {
        // Droop also slightly increases temperature (self-heating)
        params.temperature_k += params.supply_droop * 5.0;
    }
}

/// Phase overlap: modifies clock timing to allow overlap between phases.
///
/// In a real CCD, clock driver skew can cause phase overlap,
/// creating charge sharing between adjacent wells. Overlapping clocks
/// cause extra gate-channel coupling and substrate noise.
fn apply_phase_overlap(params: &mut SpiceParams) {
    if params.phase_overlap_ns <= 0.0 {
        return;
    }
    // Normalize overlap: typical clock period is ~100ns, so 50ns overlap = 0.5 fraction
    let clock_period_ns = 1e3 / params.clock_freq_mhz; // MHz -> ns
    let overlap_fraction = (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0);

    // Overlapping clocks cause extra gate-channel coupling (charge injection)
    params.charge_injection += overlap_fraction * 0.5;

    // Overlapping clock switching creates more substrate noise
    params.substrate_noise += overlap_fraction * 0.15;
}

/// Charge injection: scales gate-channel coupling effects.
///
/// When a clock transistor turns off, charge from the channel is injected
/// into the adjacent well, creating a signal-dependent offset.
fn apply_charge_injection_scale(params: &mut SpiceParams) {
    // Charge injection is used directly by transfer_function
    let _ = params;
}

/// Substrate noise: adds temperature-dependent noise floor.
fn apply_substrate_noise_scale(params: &mut SpiceParams) {
    // Substrate noise increases with temperature
    if params.substrate_noise > 0.0 {
        // Noise scales as sqrt(T)
        let temp_factor = (params.temperature_k / 300.0).sqrt();
        params.substrate_noise *= temp_factor;
    }
}

/// Determine which clock pulses should be skipped based on missing pulse rate.
///
/// Returns a vector of booleans (true = pulse present, false = missing).
pub fn missing_pulse_pattern(n_pulses: usize, rate: f64) -> Vec<bool> {
    (0..n_pulses)
        .map(|i| {
            if rate <= 0.0 {
                return true;
            }
            // Deterministic pseudo-random pattern
            let hash = ((i as f64 * 13.7 + 3.1).sin() * 10000.0).fract().abs();
            hash >= rate
        })
        .collect()
}
