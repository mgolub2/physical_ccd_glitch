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
