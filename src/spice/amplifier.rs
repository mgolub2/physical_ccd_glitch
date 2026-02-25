//! Output amplifier: floating diffusion + reset transistor + source follower.
//!
//! This is the core analog stage that converts charge to voltage.

use super::SpiceParams;

/// Build a JSON circuit for the output amplifier.
///
/// Components:
/// - Reset MOSFET (NMOS, W/L = 2u/0.5u) driven by phi_reset clock
/// - Source follower MOSFET (NMOS, W/L = 10u/1u) with resistive load (10k)
/// - Floating diffusion capacitor C_fd = 10fF
/// - VDD = effective VDD, V_rd (reset drain) = VDD * 0.8
pub fn build_amplifier_json(params: &SpiceParams) -> String {
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
            {{"type": "V", "name": "v_sig", "p": "sig_in", "n": "", "dc": 0.0, "acm": 0.0}},
            {{"type": "C", "name": "c_fd", "p": "fd", "n": "", "c": {c_fd}}},
            {{"type": "M", "name": "m_reset", "model": "nmos_tg", "params": "reset_2u_05u",
              "ports": {{"g": "phi_reset", "d": "v_rd", "s": "fd", "b": ""}}}},
            {{"type": "M", "name": "m_sf", "model": "nmos_sf", "params": "sf_10u_1u",
              "ports": {{"g": "fd", "d": "vdd", "s": "amp_out", "b": ""}}}},
            {{"type": "R", "name": "r_load", "p": "amp_out", "n": "", "g": {g_load}}}
        ]"#,
        vdd = vdd,
        v_rd = v_rd,
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
