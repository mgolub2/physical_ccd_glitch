//! CMOS clock driver circuit.
//!
//! Generates 3-phase non-overlapping clocks at configurable frequency.
//! Glitch effects: supply droop reduces swing, phase overlap, ringing from LC.

use super::SpiceParams;

/// Build a JSON circuit for a CMOS clock driver.
///
/// PMOS/NMOS push-pull driver per phase.
/// C_load = 100pF (clock bus capacitance).
pub fn build_clock_driver_json(params: &SpiceParams) -> String {
    let vdd = params.effective_vdd();
    let c_load = 100e-12; // 100 pF clock bus capacitance

    let mut signals = vec!["vdd".to_string()];
    let mut comps = Vec::new();

    // Supply
    comps.push(format!(
        r#"{{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {}, "acm": 0.0}}"#,
        vdd
    ));

    // Three phase drivers
    for phase in 1..=3u32 {
        let input = format!("drv_in{}", phase);
        let output = format!("clk_out{}", phase);
        signals.push(input.clone());
        signals.push(output.clone());

        // Input drive
        let v_in = if phase == 1 { vdd } else { 0.0 };
        comps.push(format!(
            r#"{{"type": "V", "name": "v_in{p}", "p": "{input}", "n": "", "dc": {v}, "acm": 0.0}}"#,
            p = phase,
            input = input,
            v = v_in,
        ));

        // PMOS pull-up
        comps.push(format!(
            r#"{{"type": "M", "name": "mp_drv{p}", "model": "pmos_clk", "params": "clkdrv_20u_05u",
              "ports": {{"g": "{input}", "d": "{output}", "s": "vdd", "b": "vdd"}}}}"#,
            p = phase,
            input = input,
            output = output,
        ));

        // NMOS pull-down
        comps.push(format!(
            r#"{{"type": "M", "name": "mn_drv{p}", "model": "nmos_tg", "params": "clkdrv_20u_05u",
              "ports": {{"g": "{input}", "d": "{output}", "s": "", "b": ""}}}}"#,
            p = phase,
            input = input,
            output = output,
        ));

        // Load capacitor
        comps.push(format!(
            r#"{{"type": "C", "name": "c_load{p}", "p": "{output}", "n": "", "c": {c}}}"#,
            p = phase,
            output = output,
            c = c_load,
        ));
    }

    let signal_refs: Vec<&str> = signals.iter().map(|s| s.as_str()).collect();
    let comps_json = format!("[{}]", comps.join(",\n"));
    super::models::build_circuit_json("clock_driver", &signal_refs, &comps_json)
}

/// Run clock driver simulation to extract ringing kernel and clock waveforms.
///
/// Returns (ringing_kernel, [phi1_waveform, phi2_waveform, phi3_waveform], analytical_fallback).
/// Falls back to analytical models on SPICE failure.
pub fn run_clock_simulation(params: &SpiceParams) -> (Vec<f64>, [Vec<f64>; 3], bool) {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_clock_simulation(&params)
    }));

    match result {
        Ok(Some((kernel, waveforms))) => {
            log::info!(
                "Clock driver SPICE simulation succeeded: {} kernel taps",
                kernel.len()
            );
            (kernel, waveforms, false)
        }
        _ => {
            log::warn!("Clock driver SPICE simulation failed, falling back to analytical");
            let kernel = analytical_ringing_kernel(&params);
            let (phi1, phi2, phi3) = generate_clock_pattern(
                4,
                64,
                params.effective_vdd(),
                params.phase_overlap_ns,
                1.0 / (params.clock_freq_mhz * 1e6),
            );
            (kernel, [phi1, phi2, phi3], true)
        }
    }
}

fn try_clock_simulation(params: &SpiceParams) -> Option<(Vec<f64>, [Vec<f64>; 3])> {
    use spice21::circuit::Ckt;

    let json = build_clock_driver_json(params);
    let ckt = Ckt::from_json(&json).ok()?;

    let opts = spice21::analysis::TranOptions {
        tstep: 0.1e-9,
        tstop: 500e-9,
        ..Default::default()
    };

    let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;

    let clk1 = result.map.get("clk_out1")?.clone();
    let clk2 = result.map.get("clk_out2").cloned().unwrap_or_default();
    let clk3 = result.map.get("clk_out3").cloned().unwrap_or_default();

    if clk1.len() < 10 {
        return None;
    }

    // Extract ringing kernel from clk_out1 settling
    let steady_state = clk1.last().copied().unwrap_or(0.0);
    let kernel: Vec<f64> = clk1
        .iter()
        .rev()
        .take(16)
        .rev()
        .map(|&v| v - steady_state)
        .collect();

    let max_abs = kernel.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
    let normalized_kernel = if max_abs > 1e-10 {
        kernel.iter().map(|v| v / max_abs * 0.1).collect()
    } else {
        // Fallback: no significant ringing detected
        analytical_ringing_kernel(params)
    };

    Some((normalized_kernel, [clk1, clk2, clk3]))
}

fn analytical_ringing_kernel(params: &SpiceParams) -> Vec<f64> {
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

/// Calculate ringing parameters from LC circuit.
///
/// The clock bus forms an LC circuit with bond wire inductance and bus capacitance.
/// Returns (frequency_hz, damping_ratio).
pub fn ringing_params(c_load: f64, l_bond: f64, r_driver: f64) -> (f64, f64) {
    let omega_0 = 1.0 / (l_bond * c_load).sqrt();
    let freq = omega_0 / (2.0 * std::f64::consts::PI);
    let zeta = r_driver / (2.0 * (l_bond / c_load).sqrt());
    (freq, zeta)
}

/// Generate a 3-phase non-overlapping clock pattern.
///
/// Returns (phi1, phi2, phi3) as vectors of voltage values at each time step.
pub fn generate_clock_pattern(
    n_cycles: usize,
    samples_per_cycle: usize,
    vdd: f64,
    phase_overlap_ns: f64,
    clock_period_s: f64,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let total_samples = n_cycles * samples_per_cycle;
    let mut phi1 = vec![0.0; total_samples];
    let mut phi2 = vec![0.0; total_samples];
    let mut phi3 = vec![0.0; total_samples];

    let overlap_fraction = phase_overlap_ns * 1e-9 / clock_period_s;

    for i in 0..total_samples {
        let t = (i % samples_per_cycle) as f64 / samples_per_cycle as f64;

        // Phase 1: 0.0 - 0.333
        let p1_start = 0.0;
        let p1_end = 1.0 / 3.0 + overlap_fraction;
        phi1[i] = if t >= p1_start && t < p1_end { vdd } else { 0.0 };

        // Phase 2: 0.333 - 0.667
        let p2_start = 1.0 / 3.0 - overlap_fraction;
        let p2_end = 2.0 / 3.0 + overlap_fraction;
        phi2[i] = if t >= p2_start && t < p2_end { vdd } else { 0.0 };

        // Phase 3: 0.667 - 1.0
        let p3_start = 2.0 / 3.0 - overlap_fraction;
        let p3_end = 1.0;
        phi3[i] = if t >= p3_start && t < p3_end { vdd } else { 0.0 };
    }

    (phi1, phi2, phi3)
}
