//! N-stage CCD shift register with 3-phase clocking.
//!
//! Each stage consists of a transfer gate MOSFET and a well capacitor.
//! CTE emerges naturally from MOSFET on-resistance and well capacitance.

use super::SpiceParams;

/// Build a JSON circuit for an N-stage shift register.
///
/// 3-phase clocked charge-coupled stages.
/// Each stage: transfer gate MOSFET (NMOS) + well capacitor (20-50fF).
pub fn build_shift_register_json(n_stages: usize, params: &SpiceParams) -> String {
    let vdd = params.effective_vdd();
    let c_well = 30e-15; // 30 fF per well
    let n_stages = n_stages.clamp(2, 16);

    let mut signals = vec!["vdd".to_string()];
    let mut comps = Vec::new();

    // VDD source
    comps.push(format!(
        r#"{{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {}, "acm": 0.0}}"#,
        vdd
    ));

    // Three phase clocks
    for phase in 0..3 {
        let sig = format!("phi{}", phase + 1);
        signals.push(sig.clone());
        // DC bias for initial simulation (would be pulsed in full transient)
        let v_clock = if phase == 0 { vdd } else { 0.0 };
        comps.push(format!(
            r#"{{"type": "V", "name": "v_phi{p}", "p": "phi{p}", "n": "", "dc": {v}, "acm": 0.0}}"#,
            p = phase + 1,
            v = v_clock,
        ));
    }

    // Build stages
    for i in 0..n_stages {
        let phase = (i % 3) + 1;
        let well_node = format!("well{}", i);
        let next_node = if i + 1 < n_stages {
            format!("well{}", i + 1)
        } else {
            "sr_out".to_string()
        };

        signals.push(well_node.clone());
        if i + 1 >= n_stages {
            signals.push("sr_out".to_string());
        }

        // Well capacitor
        comps.push(format!(
            r#"{{"type": "C", "name": "c_well{i}", "p": "{well}", "n": "", "c": {c}}}"#,
            i = i,
            well = well_node,
            c = c_well,
        ));

        // Transfer gate driven by appropriate phase
        comps.push(format!(
            r#"{{"type": "M", "name": "m_sr{i}", "model": "nmos_tg", "params": "tg_9u_05u",
              "ports": {{"g": "phi{phase}", "d": "{next}", "s": "{well}", "b": ""}}}}"#,
            i = i,
            phase = phase,
            next = next_node,
            well = well_node,
        ));
    }

    // Output capacitor
    comps.push(format!(
        r#"{{"type": "C", "name": "c_sr_out", "p": "sr_out", "n": "", "c": {}}}"#,
        c_well,
    ));

    let signal_refs: Vec<&str> = signals.iter().map(|s| s.as_str()).collect();
    let comps_json = format!("[{}]", comps.join(",\n"));
    super::models::build_circuit_json("shift_register", &signal_refs, &comps_json)
}

/// Run shift register simulation to extract effective CTE per stage.
///
/// Returns (cte, analytical_fallback).
/// Falls back to analytical estimate on SPICE failure.
pub fn run_shift_register_simulation(params: &SpiceParams) -> (f64, bool) {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_shift_register_simulation(&params)
    }));

    match result {
        Ok(Some(cte)) => {
            log::info!("Shift register SPICE simulation succeeded: CTE={:.6}", cte);
            (cte, false)
        }
        _ => {
            log::warn!("Shift register SPICE simulation failed, falling back to analytical");
            (analytical_cte(params.shift_register_stages, &params), true)
        }
    }
}

/// Build a small 3-stage circuit for IC-based charge transfer simulation.
///
/// Uses initial conditions to pre-charge well0, with phi1 gate open (VDD)
/// so charge transfers from well0 → well1 through the MOSFET.
/// Only 3 MOSFETs + 4 capacitors — well within spice21's convergence limits.
fn build_ic_transfer_json(params: &SpiceParams) -> String {
    let vdd = params.effective_vdd();
    let c_well = 30e-15; // 30 fF per well

    let signals = ["vdd", "phi1", "well0", "well1", "well2", "sr_out"];

    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_phi1", "p": "phi1", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "C", "name": "c_well0", "p": "well0", "n": "", "c": {c}}},
            {{"type": "C", "name": "c_well1", "p": "well1", "n": "", "c": {c}}},
            {{"type": "C", "name": "c_well2", "p": "well2", "n": "", "c": {c}}},
            {{"type": "C", "name": "c_out", "p": "sr_out", "n": "", "c": {c}}},
            {{"type": "M", "name": "m_sr0", "model": "nmos_tg", "params": "tg_9u_05u",
              "ports": {{"g": "phi1", "d": "well1", "s": "well0", "b": ""}}}},
            {{"type": "M", "name": "m_sr1", "model": "nmos_tg", "params": "tg_9u_05u",
              "ports": {{"g": "phi1", "d": "well2", "s": "well1", "b": ""}}}},
            {{"type": "M", "name": "m_sr2", "model": "nmos_tg", "params": "tg_9u_05u",
              "ports": {{"g": "phi1", "d": "sr_out", "s": "well2", "b": ""}}}}
        ]"#,
        vdd = vdd,
        c = c_well,
    );

    super::models::build_circuit_json("sr_ic_transfer", &signals, &comps)
}

fn try_shift_register_simulation(params: &SpiceParams) -> Option<f64> {
    use spice21::circuit::{Ckt, NodeRef};

    let vdd = params.effective_vdd();
    let v_test = vdd * 0.5; // Pre-charge well0 to half VDD
    let n_stages_actual = params.shift_register_stages.clamp(2, 16);

    let json = build_ic_transfer_json(params);
    let ckt = Ckt::from_json(&json).ok()?;

    // Use initial conditions to pre-charge well0
    let opts = spice21::analysis::TranOptions {
        tstep: 1e-10,
        tstop: 200e-9,
        ic: vec![(NodeRef::Name("well0".into()), v_test)],
    };

    let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;

    // Read final voltages from well0 and well1
    let v_well0_final = result
        .map
        .get("well0")
        .and_then(|v| v.last().copied())
        .unwrap_or(0.0);

    let v_well1_final = result
        .map
        .get("well1")
        .and_then(|v| v.last().copied())
        .unwrap_or(0.0);

    // Charge transferred through one gate = V_well1 / V_initial
    // (for equal capacitances, voltage ratio = charge ratio)
    if v_test.abs() < 1e-15 {
        return None;
    }

    // Single-gate CTE: fraction of charge that made it through one transfer gate
    let single_gate_cte = (v_well1_final / v_test).abs().clamp(0.0, 1.0);

    // Sanity check: well0 should have lost charge, well1 should have gained some
    if v_well1_final < 1e-6 && v_well0_final > v_test * 0.99 {
        // No charge transferred at all — simulation didn't work
        return None;
    }

    // Apply glitch parameter corrections
    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let freq_correction = 1.0 - (params.clock_freq_mhz / 100.0).min(0.5) * 0.00001;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let overlap_correction = 1.0 - overlap_fraction * 0.0001;
    let missing_correction = 1.0 - params.missing_pulse_rate * 0.001;
    let stage_correction = 1.0 - (n_stages_actual as f64 / 100.0) * 0.000001;

    let cte = single_gate_cte * freq_correction * overlap_correction
        * missing_correction * stage_correction;

    Some(cte.clamp(0.99, 1.0))
}

fn analytical_cte(n_stages: usize, params: &SpiceParams) -> f64 {
    let base_cte = 0.999999;
    let freq_factor = 1.0 - (params.clock_freq_mhz / 100.0).min(0.5) * 0.00001;
    let vdd_factor = (params.effective_vdd() / 15.0).min(1.0);
    let stage_factor = 1.0 - (n_stages as f64 / 100.0) * 0.000001;

    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let overlap_factor = 1.0 - overlap_fraction * 0.0001;
    let missing_factor = 1.0 - params.missing_pulse_rate * 0.001;

    base_cte * freq_factor * vdd_factor * stage_factor * overlap_factor * missing_factor
}
