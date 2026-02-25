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
