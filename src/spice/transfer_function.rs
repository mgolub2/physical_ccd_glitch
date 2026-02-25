//! Transfer function application and analytical fallbacks.
//!
//! Applies cached SPICE-derived (or analytical) transfer curves, ringing
//! kernels, and timing artifacts to the image pipeline.

use super::SpiceParams;

/// Analytical fallback transfer function when SPICE simulation fails.
///
/// Outputs (input_electrons, output_electrons) pairs directly.
/// The output is in electron-equivalent units so VDD, gain, and nonlinearity
/// effects are baked into the curve without needing re-normalization.
pub fn analytical_transfer_function(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Vec<(f64, f64)> {
    let vdd = params.effective_vdd();
    let nominal_vdd = 15.0;

    // VDD-dependent responsivity: source follower gain decreases at lower VDD
    let vdd_ratio = vdd / nominal_vdd;
    let responsivity = vdd_ratio.powf(0.4).min(1.05);

    // Charge injection adds a fixed offset (in electrons)
    let ci_offset = params.charge_injection * 0.02 * full_well;

    // Phase overlap effects on transfer function
    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let overlap_gain_loss = 1.0 - overlap_fraction * 0.15;
    let overlap_pedestal = overlap_fraction * 0.01 * full_well;

    (0..n_points)
        .map(|i| {
            let charge = full_well * i as f64 / (n_points - 1) as f64;
            let frac = charge / full_well;

            let linear = charge * responsivity;
            let compression = 0.05 + (1.0 - vdd_ratio).max(0.0) * 0.15;
            let body_factor = 1.0 - compression * frac * frac;

            let output = (linear * body_factor * overlap_gain_loss + ci_offset + overlap_pedestal)
                .clamp(0.0, full_well);

            (charge, output)
        })
        .collect()
}

/// Analytical ringing kernel when SPICE simulation is unavailable.
pub fn analytical_ringing_kernel(params: &SpiceParams) -> Vec<f64> {
    let kernel_len = 8;
    let ring_freq_pixels = 0.3;
    let omega = 2.0 * std::f64::consts::PI * ring_freq_pixels;

    let freq_factor = (params.clock_freq_mhz / 10.0).min(3.0);
    let damping = 0.4 / freq_factor.max(0.5);

    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let overlap_amp_boost = 1.0 + overlap_fraction * 2.0;
    let overlap_damping_factor = 1.0 - overlap_fraction * 0.5;

    let ring_amplitude = (0.02 + params.supply_droop * 0.1) * overlap_amp_boost;
    let effective_damping = damping * overlap_damping_factor.max(0.1);

    (0..kernel_len)
        .map(|i| {
            let t = i as f64;
            ring_amplitude * (-effective_damping * t).exp() * (omega * t).sin()
        })
        .collect()
}

/// Apply the transfer function to pixel data.
///
/// Uses linear interpolation through the transfer curve for each pixel value.
/// The curve outputs electron-equivalent values directly.
pub fn apply_transfer_function(
    grid: &mut [f64],
    curve: &[(f64, f64)],
    full_well: f64,
) {
    if curve.len() < 2 {
        return;
    }

    for val in grid.iter_mut() {
        let t = (*val / full_well).clamp(0.0, 1.0) * (curve.len() - 1) as f64;
        let lo = t.floor() as usize;
        let hi = (lo + 1).min(curve.len() - 1);
        let frac = t - lo as f64;

        *val = curve[lo].1 * (1.0 - frac) + curve[hi].1 * frac;
    }
}

/// Apply missing-pulse artifacts to the image grid.
///
/// When a clock pulse is missing during readout, the affected row has incomplete
/// charge transfer: it retains most of the previous row's signal blended with
/// a fraction of its own.
pub fn apply_missing_pulses(
    grid: &mut [f64],
    width: usize,
    height: usize,
    missing_pulse_rate: f64,
) {
    if missing_pulse_rate <= 0.0 {
        return;
    }

    let pattern = super::glitch::missing_pulse_pattern(height, missing_pulse_rate);
    let mut prev_row = vec![0.0; width];

    for y in 0..height {
        let row_start = y * width;
        if !pattern[y] {
            for x in 0..width {
                let own = grid[row_start + x];
                grid[row_start + x] = own * 0.3 + prev_row[x] * 0.4;
            }
        }
        prev_row.copy_from_slice(&grid[row_start..row_start + width]);
    }
}

/// Apply ringing convolution along each row.
pub fn apply_ringing(
    grid: &mut [f64],
    width: usize,
    height: usize,
    kernel: &[f64],
) {
    if kernel.is_empty() || kernel.iter().all(|&v| v.abs() < 1e-12) {
        return;
    }

    let klen = kernel.len();
    let mut row_buf = vec![0.0; width];

    for y in 0..height {
        let row_start = y * width;
        row_buf.copy_from_slice(&grid[row_start..row_start + width]);

        for x in klen..width {
            let mut sum = 0.0;
            for k in 0..klen {
                sum += row_buf[x - k - 1] * kernel[k];
            }
            grid[row_start + x] += sum;
        }
    }
}
