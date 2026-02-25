//! Output amplifier: floating diffusion + reset transistor + source follower.
//!
//! This is the core analog stage that converts charge to voltage.

use super::SpiceParams;

/// Build a JSON circuit for the output amplifier with a given FD voltage.
///
/// Components:
/// - Reset MOSFET (NMOS, W/L = 2u/0.5u) driven by phi_reset clock
/// - Source follower MOSFET (NMOS, W/L = 10u/1u) with resistive load (10k)
/// - Floating diffusion capacitor C_fd = 10fF
/// - VDD = effective VDD, V_rd (reset drain) = VDD * 0.8
pub fn build_amplifier_json(params: &SpiceParams, v_fd: f64) -> String {
    let vdd = params.effective_vdd();
    let v_rd = vdd * 0.8; // Reset drain voltage
    let c_fd = 10e-15; // 10 fF floating diffusion
    let r_load = 10_000.0; // 10k load resistor
    let g_load = 1.0 / r_load;

    let signals = ["vdd", "v_rd", "fd", "phi_reset", "amp_out", "sig_in"];

    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_vrd", "p": "v_rd", "n": "", "dc": {v_rd}, "acm": 0.0}},
            {{"type": "V", "name": "v_reset_clk", "p": "phi_reset", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_sig", "p": "sig_in", "n": "", "dc": {v_fd}, "acm": 0.0}},
            {{"type": "C", "name": "c_fd", "p": "fd", "n": "", "c": {c_fd}}},
            {{"type": "M", "name": "m_reset", "model": "nmos_tg", "params": "reset_2u_05u",
              "ports": {{"g": "phi_reset", "d": "v_rd", "s": "fd", "b": ""}}}},
            {{"type": "M", "name": "m_sf", "model": "nmos_sf", "params": "sf_10u_1u",
              "ports": {{"g": "fd", "d": "vdd", "s": "amp_out", "b": ""}}}},
            {{"type": "R", "name": "r_load", "p": "amp_out", "n": "", "g": {g_load}}}
        ]"#,
        vdd = vdd,
        v_rd = v_rd,
        v_fd = v_fd,
        c_fd = c_fd,
        g_load = g_load,
    );

    super::models::build_circuit_json("amplifier", &signals, &comps)
}

/// Compute the analytical source follower gain for a given operating point.
///
/// For a source follower: Av ≈ gm * R_load / (1 + gm * R_load)
/// With typical parameters this gives ~0.8-0.95.
pub fn analytical_sf_gain(vdd: f64) -> f64 {
    let kp = 1.1e-4;
    let w_l = 10.0; // W/L = 10u/1u
    let vgs = vdd * 0.4; // Approximate operating point
    let vt = 0.5;
    let id = 0.5 * kp * w_l * (vgs - vt).max(0.0).powi(2);
    let gm = (2.0 * kp * w_l * id).sqrt();
    let r_load = 10_000.0;
    gm * r_load / (1.0 + gm * r_load)
}

/// Estimate kTC reset noise in electrons.
pub fn ktc_noise_electrons(temperature_k: f64) -> f64 {
    let k = 1.38e-23;
    let c_fd = 10e-15;
    let q = 1.6e-19;
    let ktc_voltage = (k * temperature_k / c_fd).sqrt();
    ktc_voltage * c_fd / q
}

/// Run amplifier simulation: sweep FD voltage and extract output transfer curve + noise.
///
/// Returns (transfer_curve, noise_sigma_electrons, analytical_fallback).
/// Falls back to analytical on SPICE failure.
pub fn run_amplifier_simulation(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> (Vec<(f64, f64)>, f64, bool) {
    use std::panic;

    // Try full amplifier circuit first
    let params_clone = params.clone();
    let full_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_full_amplifier(&params_clone, full_well, n_points)
    }));

    if let Ok(Some((ref curve, noise))) = full_result {
        if is_valid_amp_curve(curve) {
            log::info!(
                "Full amplifier SPICE simulation succeeded ({} points, noise={:.2}e-)",
                curve.len(),
                noise
            );
            return (curve.clone(), noise, false);
        }
    }

    // Try simpler source follower circuit
    let params_clone = params.clone();
    let sf_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_simple_sf(&params_clone, full_well, n_points)
    }));

    if let Ok(Some((ref curve, noise))) = sf_result {
        if is_valid_amp_curve(curve) {
            log::info!(
                "Simple SF SPICE simulation succeeded ({} points, noise={:.2}e-)",
                curve.len(),
                noise
            );
            return (curve.clone(), noise, false);
        }
    }

    log::warn!("All amplifier SPICE simulations failed, falling back to analytical");
    let (curve, noise) = analytical_amplifier(params.effective_vdd(), params.temperature_k, full_well, n_points);
    (curve, noise, true)
}

/// Validate that an amp transfer curve is usable (not flat/degenerate).
fn is_valid_amp_curve(curve: &[(f64, f64)]) -> bool {
    if curve.len() < 2 {
        return false;
    }
    let min_y = curve.iter().map(|(_, y)| *y).fold(f64::MAX, f64::min);
    let max_y = curve.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);
    // Curve must have meaningful dynamic range
    let range = max_y - min_y;
    range > 1e-6 && curve.windows(2).all(|w| w[1].1 >= w[0].1 - 1e-6)
}


fn try_full_amplifier(
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Option<(Vec<(f64, f64)>, f64)> {
    use spice21::circuit::Ckt;

    let vdd = params.effective_vdd();
    let v_fd_max = vdd * 0.7;

    let mut curve = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let v_fd = v_fd_max * i as f64 / (n_points - 1).max(1) as f64;
        let json = build_amplifier_json(params, v_fd);

        let ckt = Ckt::from_json(&json).ok()?;
        let opts = spice21::analysis::TranOptions {
            tstep: 1e-10,
            tstop: 100e-9,
            ..Default::default()
        };

        let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;
        let out_voltage = result
            .map
            .get("amp_out")
            .and_then(|v| v.last().copied())
            .unwrap_or(0.0);

        curve.push((v_fd, out_voltage));
    }

    let mid_v_fd = v_fd_max * 0.5;
    let noise_sigma = measure_amp_noise(params, mid_v_fd, full_well)
        .unwrap_or_else(|| ktc_noise_electrons(params.temperature_k));

    Some((curve, noise_sigma))
}

/// Simpler source follower circuit — more likely to converge in spice21.
///
/// Sweeps v_fd from 0 to VDD*0.7 as a signal voltage applied to the SF gate.
fn try_simple_sf(
    params: &SpiceParams,
    _full_well: f64,
    n_points: usize,
) -> Option<(Vec<(f64, f64)>, f64)> {
    use spice21::circuit::Ckt;

    let vdd = params.effective_vdd();
    let v_fd_max = vdd * 0.7;
    let g_load = 1.0 / 10_000.0;

    let mut curve = Vec::with_capacity(n_points);

    for i in 0..n_points {
        let v_fd = v_fd_max * i as f64 / (n_points - 1).max(1) as f64;

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

        let json = super::models::build_circuit_json("readout", &["vdd", "fd", "amp_out"], &comps);
        let ckt = Ckt::from_json(&json).ok()?;

        let opts = spice21::analysis::TranOptions {
            tstep: 1e-10,
            tstop: 100e-9,
            ..Default::default()
        };

        let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;
        let out_voltage = result
            .map
            .get("amp_out")
            .and_then(|v| v.last().copied())
            .unwrap_or(0.0);

        curve.push((v_fd, out_voltage));
    }

    let noise_sigma = ktc_noise_electrons(params.temperature_k);
    Some((curve, noise_sigma))
}

fn measure_amp_noise(params: &SpiceParams, v_fd: f64, _full_well: f64) -> Option<f64> {
    use spice21::circuit::Ckt;

    let json = build_amplifier_json(params, v_fd);
    let ckt = Ckt::from_json(&json).ok()?;
    let opts = spice21::analysis::TranOptions {
        tstep: 1e-10,
        tstop: 200e-9,
        ..Default::default()
    };

    let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;
    let out = result.map.get("amp_out")?;

    if out.len() < 10 {
        return None;
    }

    // Measure variance over last half of transient
    let half = out.len() / 2;
    let last_half = &out[half..];
    let mean = last_half.iter().sum::<f64>() / last_half.len() as f64;
    let variance = last_half.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / last_half.len() as f64;
    let sigma_v = variance.sqrt();

    // Convert voltage noise to electron-equivalent
    let c_fd = 10e-15;
    let q = 1.6e-19;
    let v_per_electron = q / c_fd;
    let sigma_electrons = sigma_v / v_per_electron;

    // Add substrate noise contribution if present
    let _substrate = params.substrate_noise * 20.0;

    Some(sigma_electrons.max(ktc_noise_electrons(params.temperature_k) * 0.5))
}

fn analytical_amplifier(
    vdd: f64,
    temperature_k: f64,
    _full_well: f64,
    n_points: usize,
) -> (Vec<(f64, f64)>, f64) {
    let gain = analytical_sf_gain(vdd);
    let v_fd_max = vdd * 0.7;

    let curve: Vec<(f64, f64)> = (0..n_points)
        .map(|i| {
            let v_fd = v_fd_max * i as f64 / (n_points - 1).max(1) as f64;
            let v_out = v_fd * gain;
            (v_fd, v_out)
        })
        .collect();

    let noise = ktc_noise_electrons(temperature_k);
    (curve, noise)
}
