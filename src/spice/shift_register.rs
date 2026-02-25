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
