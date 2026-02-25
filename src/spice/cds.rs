//! Correlated Double Sampler (CDS) circuit.
//!
//! Removes reset noise (kTC) by subtracting the reset level from the signal level.
//! CDS failure mode: partial clamp leaks reset noise through.

use super::SpiceParams;

/// Build a JSON circuit for the CDS stage.
///
/// Components:
/// - Coupling capacitor C_couple = 10pF
/// - Hold capacitor C_hold = 5pF
/// - Sample switch M_sample (NMOS, W/L = 5u/0.5u)
/// - Clamp switch M_clamp (NMOS, W/L = 5u/0.5u)
///
/// Operation: clamp during reset, sample during signal.
pub fn build_cds_json(params: &SpiceParams) -> String {
    let vdd = params.effective_vdd();
    let c_couple = 10e-12; // 10 pF
    let c_hold = 5e-12; // 5 pF

    let signals = [
        "vdd", "cds_in", "coupled", "cds_out", "phi_clamp", "phi_sample",
    ];

    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_in", "p": "cds_in", "n": "", "dc": 0.0, "acm": 0.0}},
            {{"type": "V", "name": "v_clamp", "p": "phi_clamp", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_sample", "p": "phi_sample", "n": "", "dc": 0.0, "acm": 0.0}},
            {{"type": "C", "name": "c_couple", "p": "cds_in", "n": "coupled", "c": {c_couple}}},
            {{"type": "C", "name": "c_hold", "p": "cds_out", "n": "", "c": {c_hold}}},
            {{"type": "M", "name": "m_clamp", "model": "nmos_tg", "params": "switch_5u_05u",
              "ports": {{"g": "phi_clamp", "d": "coupled", "s": "", "b": ""}}}},
            {{"type": "M", "name": "m_sample", "model": "nmos_tg", "params": "switch_5u_05u",
              "ports": {{"g": "phi_sample", "d": "cds_out", "s": "coupled", "b": ""}}}}
        ]"#,
        vdd = vdd,
        c_couple = c_couple,
        c_hold = c_hold,
    );

    super::models::build_circuit_json("cds", &signals, &comps)
}

/// Estimate CDS rejection ratio.
///
/// Perfect CDS removes kTC noise completely. Partial clamp (glitch mode)
/// leaves a fraction of reset noise proportional to timing mismatch.
pub fn cds_rejection_factor(phase_overlap_ns: f64) -> f64 {
    // Phase overlap degrades CDS by allowing signal to leak into clamp period
    let overlap_fraction = phase_overlap_ns / 100.0;
    (1.0 - overlap_fraction).clamp(0.0, 1.0)
}

/// Run CDS simulation to extract noise rejection factor.
///
/// Returns (rejection_ratio, analytical_fallback).
/// Measures how much of an input offset appears at the output.
/// Falls back to analytical on SPICE failure.
pub fn run_cds_simulation(params: &SpiceParams) -> (f64, bool) {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_cds_simulation(&params)
    }));

    match result {
        Ok(Some(rejection)) => {
            log::info!("CDS SPICE simulation succeeded: rejection={:.3}", rejection);
            (rejection, false)
        }
        _ => {
            log::warn!("CDS SPICE simulation failed, falling back to analytical");
            (cds_rejection_factor(params.phase_overlap_ns), true)
        }
    }
}

fn try_cds_simulation(params: &SpiceParams) -> Option<f64> {
    use spice21::circuit::Ckt;

    // Run two simulations with different input offsets to measure rejection
    let offsets = [0.5, 1.5]; // Two DC input levels (V)
    let mut outputs = Vec::new();

    for &v_in in &offsets {
        let json = build_cds_json_with_input(params, v_in);
        let ckt = Ckt::from_json(&json).ok()?;
        let opts = spice21::analysis::TranOptions {
            tstep: 1e-10,
            tstop: 100e-9,
            ..Default::default()
        };

        let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;
        let v_out = result
            .map
            .get("cds_out")
            .and_then(|v| v.last().copied())
            .unwrap_or(0.0);
        outputs.push(v_out);
    }

    if outputs.len() < 2 {
        return None;
    }

    let input_variation = (offsets[1] - offsets[0]).abs();
    let output_variation = (outputs[1] - outputs[0]).abs();

    if input_variation < 1e-10 {
        return None;
    }

    let rejection = (1.0 - output_variation / input_variation).clamp(0.0, 1.0);
    Some(rejection)
}

fn build_cds_json_with_input(params: &SpiceParams, v_in: f64) -> String {
    let vdd = params.effective_vdd();
    let c_couple = 10e-12;
    let c_hold = 5e-12;

    let signals = [
        "vdd", "cds_in", "coupled", "cds_out", "phi_clamp", "phi_sample",
    ];

    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_in", "p": "cds_in", "n": "", "dc": {v_in}, "acm": 0.0}},
            {{"type": "V", "name": "v_clamp", "p": "phi_clamp", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_sample", "p": "phi_sample", "n": "", "dc": 0.0, "acm": 0.0}},
            {{"type": "C", "name": "c_couple", "p": "cds_in", "n": "coupled", "c": {c_couple}}},
            {{"type": "C", "name": "c_hold", "p": "cds_out", "n": "", "c": {c_hold}}},
            {{"type": "M", "name": "m_clamp", "model": "nmos_tg", "params": "switch_5u_05u",
              "ports": {{"g": "phi_clamp", "d": "coupled", "s": "", "b": ""}}}},
            {{"type": "M", "name": "m_sample", "model": "nmos_tg", "params": "switch_5u_05u",
              "ports": {{"g": "phi_sample", "d": "cds_out", "s": "coupled", "b": ""}}}}
        ]"#,
        vdd = vdd,
        v_in = v_in,
        c_couple = c_couple,
        c_hold = c_hold,
    );

    super::models::build_circuit_json("cds", &signals, &comps)
}
