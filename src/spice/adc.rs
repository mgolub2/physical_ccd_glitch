//! Flash ADC circuit (4-bit representative).
//!
//! 15 differential pair comparators for 4-bit resolution.
//! Results are scaled to actual bit depth via interpolation.

use super::SpiceParams;

/// Build a JSON circuit for a representative 4-bit flash ADC.
///
/// Components per comparator:
/// - Differential pair: M_n1/M_n2 (NMOS, W/L = 2u/0.5u)
/// - Tail current source: M_tail
/// - Load resistors: R_load (20k)
///
/// Reference ladder from resistor divider.
pub fn build_adc_json(params: &SpiceParams) -> String {
    let vdd = params.effective_vdd();
    let n_comparators = 15; // 4-bit: 2^4 - 1
    let r_ladder = 1_000.0; // 1k per ladder segment
    let g_ladder = 1.0 / r_ladder;
    let r_load = 20_000.0;
    let g_load = 1.0 / r_load;
    let v_ref_top = vdd * 0.8;

    let mut signals = vec!["vdd".to_string(), "adc_in".to_string(), "v_ref_top".to_string()];
    let mut comps = Vec::new();

    // Supply and reference
    comps.push(format!(
        r#"{{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {}, "acm": 0.0}}"#,
        vdd
    ));
    comps.push(format!(
        r#"{{"type": "V", "name": "v_ref", "p": "v_ref_top", "n": "", "dc": {}, "acm": 0.0}}"#,
        v_ref_top
    ));
    comps.push(format!(
        r#"{{"type": "V", "name": "v_adc_in", "p": "adc_in", "n": "", "dc": 0.0, "acm": 0.0}}"#,
    ));

    // Reference ladder
    for i in 0..=n_comparators {
        let node = format!("ref{}", i);
        signals.push(node.clone());

        if i == 0 {
            // Bottom of ladder to ground
            comps.push(format!(
                r#"{{"type": "R", "name": "r_lad0", "p": "ref0", "n": "", "g": {}}}"#,
                g_ladder
            ));
        } else {
            let prev = format!("ref{}", i - 1);
            comps.push(format!(
                r#"{{"type": "R", "name": "r_lad{i}", "p": "{node}", "n": "{prev}", "g": {g}}}"#,
                i = i,
                node = node,
                prev = prev,
                g = g_ladder,
            ));
        }
    }
    // Top of ladder to reference
    comps.push(format!(
        r#"{{"type": "R", "name": "r_lad_top", "p": "v_ref_top", "n": "ref{}", "g": {}}}"#,
        n_comparators, g_ladder,
    ));

    // Comparators (simplified as differential pairs with load)
    for i in 0..n_comparators {
        let out_p = format!("comp{}_p", i);
        let out_n = format!("comp{}_n", i);
        let tail = format!("comp{}_tail", i);
        let ref_node = format!("ref{}", i + 1);
        signals.push(out_p.clone());
        signals.push(out_n.clone());
        signals.push(tail.clone());

        // Load resistors
        comps.push(format!(
            r#"{{"type": "R", "name": "r_lp{i}", "p": "vdd", "n": "{out_p}", "g": {g}}}"#,
            i = i, out_p = out_p, g = g_load,
        ));
        comps.push(format!(
            r#"{{"type": "R", "name": "r_ln{i}", "p": "vdd", "n": "{out_n}", "g": {g}}}"#,
            i = i, out_n = out_n, g = g_load,
        ));

        // Differential pair
        comps.push(format!(
            r#"{{"type": "M", "name": "m_dp{i}", "model": "nmos_sf", "params": "comp_2u_05u",
              "ports": {{"g": "adc_in", "d": "{out_p}", "s": "{tail}", "b": ""}}}}"#,
            i = i, out_p = out_p, tail = tail,
        ));
        comps.push(format!(
            r#"{{"type": "M", "name": "m_dn{i}", "model": "nmos_sf", "params": "comp_2u_05u",
              "ports": {{"g": "{ref_node}", "d": "{out_n}", "s": "{tail}", "b": ""}}}}"#,
            i = i, ref_node = ref_node, out_n = out_n, tail = tail,
        ));

        // Tail current source (resistor approximation)
        comps.push(format!(
            r#"{{"type": "R", "name": "r_tail{i}", "p": "{tail}", "n": "", "g": {g}}}"#,
            i = i,
            tail = tail,
            g = 1.0 / 50_000.0, // 50k for ~100uA tail current
        ));
    }

    let signal_refs: Vec<&str> = signals.iter().map(|s| s.as_str()).collect();
    let comps_json = format!("[{}]", comps.join(",\n"));
    super::models::build_circuit_json("flash_adc", &signal_refs, &comps_json)
}

/// Estimate DNL errors from comparator Vt mismatch.
///
/// In a real flash ADC, transistor mismatch causes differential nonlinearity.
/// Returns DNL in LSB units for each code transition.
pub fn estimate_dnl(n_bits: u8, vt_mismatch_sigma: f64, v_ref: f64) -> Vec<f64> {
    let n_codes = (1u32 << n_bits) - 1;
    let lsb = v_ref / n_codes as f64;

    (0..n_codes as usize)
        .map(|i| {
            // Deterministic pseudo-random mismatch per comparator
            let hash = ((i as f64 * 7.3 + 2.1).sin() * 10000.0).fract();
            hash * vt_mismatch_sigma / lsb
        })
        .collect()
}

/// Run ADC simulation: sweep input voltage and extract digital output codes + DNL.
///
/// Returns (transfer: Vec<(voltage, code)>, dnl: Vec<f64>).
/// Falls back to analytical on SPICE failure.
/// Returns (transfer, dnl, analytical_fallback).
pub fn run_adc_simulation(params: &SpiceParams) -> (Vec<(f64, u16)>, Vec<f64>, bool) {
    use std::panic;

    let params = params.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        try_adc_simulation(&params)
    }));

    match result {
        Ok(Some(ref r)) if is_valid_adc_transfer(&r.0) => {
            log::info!(
                "ADC SPICE simulation succeeded ({} points, {} DNL entries)",
                r.0.len(),
                r.1.len()
            );
            let r = result.unwrap().unwrap();
            (r.0, r.1, false)
        }
        Ok(Some(_)) => {
            log::warn!("ADC SPICE simulation produced degenerate results, falling back to analytical");
            let r = analytical_adc(&params);
            (r.0, r.1, true)
        }
        _ => {
            log::warn!("ADC SPICE simulation failed, falling back to analytical");
            let r = analytical_adc(&params);
            (r.0, r.1, true)
        }
    }
}

/// Validate that the ADC transfer function is usable (not degenerate).
///
/// Checks that the transfer covers a reasonable range of codes and is monotonic.
fn is_valid_adc_transfer(transfer: &[(f64, u16)]) -> bool {
    if transfer.len() < 4 {
        return false;
    }
    let min_code = transfer.iter().map(|(_, c)| *c).min().unwrap_or(0);
    let max_code = transfer.iter().map(|(_, c)| *c).max().unwrap_or(0);
    // Must span at least 8 of the 16 possible codes (0-15)
    let code_range = max_code - min_code;
    if code_range < 8 {
        return false;
    }
    // Must be monotonically non-decreasing
    transfer.windows(2).all(|w| w[1].1 >= w[0].1)
}

fn try_adc_simulation(params: &SpiceParams) -> Option<(Vec<(f64, u16)>, Vec<f64>)> {
    use spice21::circuit::Ckt;

    let vdd = params.effective_vdd();
    let v_ref_top = vdd * 0.8;
    let n_sweep = 64; // sweep points
    let n_comparators: usize = 15;

    let mut transfer = Vec::with_capacity(n_sweep);

    for i in 0..n_sweep {
        let v_in = v_ref_top * i as f64 / (n_sweep - 1).max(1) as f64;
        let json = build_adc_json_with_input(params, v_in);

        let ckt = Ckt::from_json(&json).ok()?;
        let opts = spice21::analysis::TranOptions {
            tstep: 1e-10,
            tstop: 50e-9,
            ..Default::default()
        };

        let result = spice21::analysis::tran(ckt, None, Some(opts)).ok()?;

        // Read comparator outputs (thermometer code)
        let mut code: u16 = 0;
        for c in 0..n_comparators {
            let vp = result
                .map
                .get(&format!("comp{}_p", c))
                .and_then(|v| v.last().copied())
                .unwrap_or(0.0);
            let vn = result
                .map
                .get(&format!("comp{}_n", c))
                .and_then(|v| v.last().copied())
                .unwrap_or(0.0);
            if vp > vn {
                code += 1;
            }
        }

        transfer.push((v_in, code));
    }

    // Extract DNL from transition voltage spacing
    let ideal_lsb = v_ref_top / n_comparators as f64;
    let mut dnl = Vec::new();
    let mut last_code = 0u16;
    let mut last_v = 0.0f64;

    for &(v, code) in &transfer {
        if code != last_code && code > 0 {
            let actual_lsb = v - last_v;
            dnl.push(actual_lsb / ideal_lsb - 1.0);
            last_v = v;
            last_code = code;
        }
    }

    // Pad DNL to expected length
    while dnl.len() < n_comparators {
        dnl.push(0.0);
    }

    Some((transfer, dnl))
}

fn build_adc_json_with_input(params: &SpiceParams, v_in: f64) -> String {
    let vdd = params.effective_vdd();
    let n_comparators = 15;
    let r_ladder = 1_000.0;
    let g_ladder = 1.0 / r_ladder;
    let r_load = 20_000.0;
    let g_load = 1.0 / r_load;
    let v_ref_top = vdd * 0.8;

    let mut signals = vec!["vdd".to_string(), "adc_in".to_string(), "v_ref_top".to_string()];
    let mut comps = Vec::new();

    comps.push(format!(
        r#"{{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {}, "acm": 0.0}}"#,
        vdd
    ));
    comps.push(format!(
        r#"{{"type": "V", "name": "v_ref", "p": "v_ref_top", "n": "", "dc": {}, "acm": 0.0}}"#,
        v_ref_top
    ));
    comps.push(format!(
        r#"{{"type": "V", "name": "v_adc_in", "p": "adc_in", "n": "", "dc": {}, "acm": 0.0}}"#,
        v_in
    ));

    // Reference ladder
    for i in 0..=n_comparators {
        let node = format!("ref{}", i);
        signals.push(node.clone());

        if i == 0 {
            comps.push(format!(
                r#"{{"type": "R", "name": "r_lad0", "p": "ref0", "n": "", "g": {}}}"#,
                g_ladder
            ));
        } else {
            let prev = format!("ref{}", i - 1);
            comps.push(format!(
                r#"{{"type": "R", "name": "r_lad{i}", "p": "{node}", "n": "{prev}", "g": {g}}}"#,
                i = i, node = node, prev = prev, g = g_ladder,
            ));
        }
    }
    comps.push(format!(
        r#"{{"type": "R", "name": "r_lad_top", "p": "v_ref_top", "n": "ref{}", "g": {}}}"#,
        n_comparators, g_ladder,
    ));

    // Comparators
    for i in 0..n_comparators {
        let out_p = format!("comp{}_p", i);
        let out_n = format!("comp{}_n", i);
        let tail = format!("comp{}_tail", i);
        let ref_node = format!("ref{}", i + 1);
        signals.push(out_p.clone());
        signals.push(out_n.clone());
        signals.push(tail.clone());

        comps.push(format!(
            r#"{{"type": "R", "name": "r_lp{i}", "p": "vdd", "n": "{out_p}", "g": {g}}}"#,
            i = i, out_p = out_p, g = g_load,
        ));
        comps.push(format!(
            r#"{{"type": "R", "name": "r_ln{i}", "p": "vdd", "n": "{out_n}", "g": {g}}}"#,
            i = i, out_n = out_n, g = g_load,
        ));
        comps.push(format!(
            r#"{{"type": "M", "name": "m_dp{i}", "model": "nmos_sf", "params": "comp_2u_05u",
              "ports": {{"g": "adc_in", "d": "{out_p}", "s": "{tail}", "b": ""}}}}"#,
            i = i, out_p = out_p, tail = tail,
        ));
        comps.push(format!(
            r#"{{"type": "M", "name": "m_dn{i}", "model": "nmos_sf", "params": "comp_2u_05u",
              "ports": {{"g": "{ref_node}", "d": "{out_n}", "s": "{tail}", "b": ""}}}}"#,
            i = i, ref_node = ref_node, out_n = out_n, tail = tail,
        ));
        comps.push(format!(
            r#"{{"type": "R", "name": "r_tail{i}", "p": "{tail}", "n": "", "g": {g}}}"#,
            i = i, tail = tail, g = 1.0 / 50_000.0,
        ));
    }

    let signal_refs: Vec<&str> = signals.iter().map(|s| s.as_str()).collect();
    let comps_json = format!("[{}]", comps.join(",\n"));
    super::models::build_circuit_json("flash_adc", &signal_refs, &comps_json)
}

fn analytical_adc(params: &SpiceParams) -> (Vec<(f64, u16)>, Vec<f64>) {
    let vdd = params.effective_vdd();
    let v_ref_top = vdd * 0.8;
    let n_codes = 16u16; // 4-bit

    let transfer: Vec<(f64, u16)> = (0..64)
        .map(|i| {
            let v = v_ref_top * i as f64 / 63.0;
            let code = ((v / v_ref_top) * (n_codes - 1) as f64).round().min((n_codes - 1) as f64) as u16;
            (v, code)
        })
        .collect();

    let dnl = estimate_dnl(4, 0.005, v_ref_top);

    (transfer, dnl)
}

/// Scale 4-bit ADC transfer function to arbitrary bit depth via interpolation.
pub fn scale_to_bit_depth(
    transfer_4bit: &[(f64, f64)],
    target_bits: u8,
) -> Vec<(f64, f64)> {
    let n_target = (1u32 << target_bits) as usize;
    let n_source = transfer_4bit.len();

    (0..n_target)
        .map(|i| {
            let t = i as f64 / (n_target - 1) as f64;
            let src_idx = t * (n_source - 1) as f64;
            let lo = src_idx.floor() as usize;
            let hi = (lo + 1).min(n_source - 1);
            let frac = src_idx - lo as f64;

            let x = transfer_4bit[lo].0 * (1.0 - frac) + transfer_4bit[hi].0 * frac;
            let y = transfer_4bit[lo].1 * (1.0 - frac) + transfer_4bit[hi].1 * frac;
            (x, y)
        })
        .collect()
}
