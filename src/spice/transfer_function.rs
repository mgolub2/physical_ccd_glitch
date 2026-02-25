//! Transfer function extraction and application.
//!
//! Bridges the SPICE circuit simulation to the image pipeline by extracting
//! an input-output transfer curve and timing artifacts (ringing kernel).

use super::SpiceParams;

/// Extract the readout transfer function by simulating the chain at N charge levels.
///
/// Builds the full readout chain for each charge level from 0 to full_well,
/// runs a transient simulation, and extracts the final output voltage.
/// Returns a vector of (input_electrons, output_voltage) pairs.
pub fn extract_transfer_function(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Vec<(f64, f64)> {
    let n_points = n_points.max(4);

    // Try SPICE simulation first, fall back to analytical model
    match try_spice_transfer_function(params, full_well, n_points) {
        Some(curve) => curve,
        None => analytical_transfer_function(params, full_well, n_points),
    }
}

/// Attempt to run actual SPICE simulations for the transfer function.
///
/// Uses catch_unwind to handle panics from spice21 internals gracefully.
fn try_spice_transfer_function(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Option<Vec<(f64, f64)>> {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        run_spice_transfer_function(&params, full_well, n_points)
    }));

    match result {
        Ok(curve) => curve,
        Err(_) => {
            log::warn!("SPICE simulation panicked, falling back to analytical model");
            None
        }
    }
}

fn run_spice_transfer_function(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Option<Vec<(f64, f64)>> {
    use spice21::circuit::Ckt;

    let mut curve = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let charge = full_well * i as f64 / (n_points - 1) as f64;

        let v_fd = super::pixel::charge_to_fd_voltage(charge);
        let json = build_readout_circuit_json(params, v_fd);

        let ckt = match Ckt::from_json(&json) {
            Ok(c) => c,
            Err(_) => return None,
        };

        let opts = spice21::analysis::TranOptions {
            tstep: 1e-10,
            tstop: 100e-9,
            ..Default::default()
        };

        match spice21::analysis::tran(ckt, None, Some(opts)) {
            Ok(result) => {
                let out_voltage = result
                    .map
                    .get("amp_out")
                    .and_then(|v| v.last().copied())
                    .unwrap_or(0.0);
                curve.push((charge, out_voltage));
            }
            Err(_) => return None,
        }
    }

    // Normalize output voltages to electron-equivalent units so the transfer
    // curve is compatible with the rest of the pipeline (which expects values
    // in the [0, full_well] range, matching the analytical fallback).
    let max_output = curve.iter().map(|(_, v)| *v).fold(0.0f64, f64::max);
    if max_output > 1e-10 {
        let scale = full_well / max_output;
        for (_, v) in curve.iter_mut() {
            *v *= scale;
        }
    }

    Some(curve)
}

/// Build a simplified readout circuit for transfer function extraction.
fn build_readout_circuit_json(params: &SpiceParams, v_fd: f64) -> String {
    let vdd = params.effective_vdd();
    let g_load = 1.0 / 10_000.0;

    // Simple source follower with the FD voltage as input
    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_fd", "p": "fd", "n": "", "dc": {v_fd}, "acm": 0.0}},
            {{"type": "M", "name": "m_sf", "model": "nmos_sf", "params": "sf_10u_1u",
              "ports": {{"g": "fd", "d": "vdd", "s": "amp_out", "b": ""}}}},
            {{"type": "R", "name": "r_load", "p": "amp_out", "n": "", "g": {g_load}}}
        ]"#,
        vdd = vdd,
        v_fd = v_fd,
        g_load = g_load,
    );

    super::models::build_circuit_json("readout", &["vdd", "fd", "amp_out"], &comps)
}

/// Analytical fallback transfer function when SPICE simulation fails.
///
/// Outputs (input_electrons, output_electrons) pairs directly.
/// The output is in electron-equivalent units so VDD, gain, and nonlinearity
/// effects are baked into the curve without needing re-normalization.
fn analytical_transfer_function(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Vec<(f64, f64)> {
    let vdd = params.effective_vdd();
    let nominal_vdd = 15.0;

    // VDD-dependent responsivity: source follower gain decreases at lower VDD
    // due to reduced overdrive and worse body effect
    let vdd_ratio = vdd / nominal_vdd;
    let responsivity = vdd_ratio.powf(0.4).min(1.05); // ~0.75 at 5V, ~1.0 at 15V

    // Charge injection adds a fixed offset (in electrons)
    let ci_offset = params.charge_injection * 0.02 * full_well;

    // Phase overlap effects on transfer function
    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Overlap causes gain loss from charge sharing between adjacent wells (up to 15%)
    let overlap_gain_loss = 1.0 - overlap_fraction * 0.15;
    // Signal-independent leakage from adjacent wells (pedestal offset)
    let overlap_pedestal = overlap_fraction * 0.01 * full_well;

    (0..n_points)
        .map(|i| {
            let charge = full_well * i as f64 / (n_points - 1) as f64;
            let frac = charge / full_well;

            // Linear transfer with VDD-dependent gain
            let linear = charge * responsivity;

            // Body effect nonlinearity: compresses highlights progressively
            // More compression at lower VDD (less headroom)
            let compression = 0.05 + (1.0 - vdd_ratio).max(0.0) * 0.15;
            let body_factor = 1.0 - compression * frac * frac;

            // Combine with overlap effects
            let output = (linear * body_factor * overlap_gain_loss + ci_offset + overlap_pedestal)
                .clamp(0.0, full_well);

            (charge, output)
        })
        .collect()
}

/// Extract a ringing kernel from a simulated bright-to-dark step response.
///
/// Simulates a step from full signal to zero and captures the post-transition
/// oscillation as a convolution kernel.
pub fn extract_ringing_kernel(params: &SpiceParams) -> Vec<f64> {
    // Try SPICE simulation, fall back to analytical
    match try_spice_ringing_kernel(params) {
        Some(kernel) => kernel,
        None => analytical_ringing_kernel(params),
    }
}

fn try_spice_ringing_kernel(params: &SpiceParams) -> Option<Vec<f64>> {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        run_spice_ringing_kernel(&params)
    }));

    match result {
        Ok(kernel) => kernel,
        Err(_) => {
            log::warn!("SPICE ringing simulation panicked, falling back to analytical model");
            None
        }
    }
}

fn run_spice_ringing_kernel(params: &SpiceParams) -> Option<Vec<f64>> {
    use spice21::circuit::Ckt;

    let vdd = params.effective_vdd();
    let v_bright = vdd * 0.7;

    let json = build_readout_circuit_json(params, v_bright);
    let ckt = match Ckt::from_json(&json) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let opts = spice21::analysis::TranOptions {
        tstep: 1e-10,
        tstop: 200e-9,
        ..Default::default()
    };

    match spice21::analysis::tran(ckt, None, Some(opts)) {
        Ok(result) => {
            let out = result.map.get("amp_out")?;
            if out.len() < 10 {
                return None;
            }

            let steady_state = out.last().copied().unwrap_or(0.0);
            let kernel: Vec<f64> = out
                .iter()
                .rev()
                .take(16)
                .rev()
                .map(|&v| v - steady_state)
                .collect();

            let max_abs = kernel.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
            if max_abs > 1e-10 {
                Some(kernel.iter().map(|v| v / max_abs * 0.1).collect())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Analytical ringing kernel when SPICE simulation is unavailable.
///
/// Models the pixel-to-pixel ringing artifact visible in CCD readout.
/// The kernel operates at pixel clock rate, so we model the ringing
/// as a damped oscillation that spans several pixels.
fn analytical_ringing_kernel(params: &SpiceParams) -> Vec<f64> {
    let kernel_len = 8;

    // Ringing visible at pixel rate: the output amplifier and sample-and-hold
    // circuit has a settling time that spans multiple pixel periods.
    // Model as damped sinusoid at ~0.3x pixel rate (ringing period ≈ 3 pixels)
    let ring_freq_pixels = 0.3; // oscillation frequency in cycles per pixel
    let omega = 2.0 * std::f64::consts::PI * ring_freq_pixels;

    // Damping: faster clock = less settling time per pixel = more ringing
    let freq_factor = (params.clock_freq_mhz / 10.0).min(3.0);
    let damping = 0.4 / freq_factor.max(0.5);

    // Phase overlap: sloppy clock transitions boost ringing amplitude and reduce damping
    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Overlap boosts ringing amplitude (up to 3x) due to coupled clock edges
    let overlap_amp_boost = 1.0 + overlap_fraction * 2.0;
    // Overlap reduces damping (sloppier transitions ring longer)
    let overlap_damping_factor = 1.0 - overlap_fraction * 0.5;

    // Scale by supply droop (more droop = less drive = more ringing)
    let ring_amplitude = (0.02 + params.supply_droop * 0.1) * overlap_amp_boost;
    let effective_damping = damping * overlap_damping_factor.max(0.1);

    (0..kernel_len)
        .map(|i| {
            let t = i as f64; // in pixel units
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
        // Map input charge through the transfer curve via linear interpolation
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
            // Missing pulse: incomplete charge transfer
            // 30% own signal + 40% previous row (rest is lost/dark)
            for x in 0..width {
                let own = grid[row_start + x];
                grid[row_start + x] = own * 0.3 + prev_row[x] * 0.4;
            }
        }
        // Save current row (after modification) as previous for next iteration
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

        // Copy row
        row_buf.copy_from_slice(&grid[row_start..row_start + width]);

        // Apply convolution (causal: kernel only affects pixels after a transition)
        for x in klen..width {
            let mut sum = 0.0;
            for k in 0..klen {
                sum += row_buf[x - k - 1] * kernel[k];
            }
            grid[row_start + x] += sum;
        }
    }
}
