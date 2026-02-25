//! CCD pixel circuit: photodiode + transfer gate + floating diffusion.

use super::SpiceParams;

/// Build a JSON circuit for a CCD pixel readout.
///
/// The pixel is modeled as:
/// - Photodiode: initial voltage on pixel capacitor (V = Q*e / C_pixel)
/// - Transfer gate MOSFET (NMOS, W/L = 9u/0.5u)
/// - Anti-blooming drain MOSFET (NMOS, W/L = 2u/1u)
/// - Floating diffusion capacitor at output (10fF)
///
/// The transfer gate clock drives charge from pixel to floating diffusion.
pub fn build_pixel_json(charge_electrons: f64, params: &SpiceParams) -> String {
    let c_pixel = 30e-15; // 30 fF
    let c_fd = 10e-15; // 10 fF
    let q = 1.6e-19;
    let vdd = params.effective_vdd();

    // Initial voltage on pixel well from accumulated charge
    let _v_pixel = (charge_electrons * q / c_pixel).min(vdd);

    // Transfer gate clock voltage
    let v_tg = vdd;
    // Anti-blooming gate sits at a bias below VDD
    let v_abg = vdd * 0.6;

    let signals = vec!["vdd", "pixel", "fd", "phi_tg", "v_abg", "abg_drain"];
    let _signals_json: Vec<String> = signals.iter().map(|s| format!("\"{}\"", s)).collect();

    let comps = format!(
        r#"[
            {{"type": "V", "name": "v_vdd", "p": "vdd", "n": "", "dc": {vdd}, "acm": 0.0}},
            {{"type": "V", "name": "v_tg", "p": "phi_tg", "n": "", "dc": {v_tg}, "acm": 0.0}},
            {{"type": "V", "name": "v_abg", "p": "v_abg", "n": "", "dc": {v_abg}, "acm": 0.0}},
            {{"type": "C", "name": "c_pixel", "p": "pixel", "n": "", "c": {c_pixel}}},
            {{"type": "C", "name": "c_fd", "p": "fd", "n": "", "c": {c_fd}}},
            {{"type": "M", "name": "m_tg", "model": "nmos_tg", "params": "tg_9u_05u",
              "ports": {{"g": "phi_tg", "d": "fd", "s": "pixel", "b": ""}}}},
            {{"type": "M", "name": "m_abg", "model": "nmos_tg", "params": "abg_2u_1u",
              "ports": {{"g": "v_abg", "d": "abg_drain", "s": "pixel", "b": ""}}}},
            {{"type": "R", "name": "r_abg_drain", "p": "abg_drain", "n": "vdd", "g": {g_drain}}}
        ]"#,
        vdd = vdd,
        v_tg = v_tg,
        v_abg = v_abg,
        c_pixel = c_pixel,
        c_fd = c_fd,
        g_drain = 1.0 / 10_000.0, // 10k drain resistor
    );

    super::models::build_circuit_json("pixel", &signals, &comps)
}

/// Compute the initial pixel voltage for a given electron count.
pub fn charge_to_voltage(charge_electrons: f64) -> f64 {
    let c_pixel = 30e-15;
    let q = 1.6e-19;
    charge_electrons * q / c_pixel
}

/// Compute the floating diffusion voltage for a given charge.
pub fn charge_to_fd_voltage(charge_electrons: f64) -> f64 {
    let c_fd = 10e-15;
    let q = 1.6e-19;
    charge_electrons * q / c_fd
}

/// Compute the pixel transfer curve: charge (electrons) → FD signal voltage.
///
/// Uses the analytical Q/C model directly, since the pixel circuit JSON
/// cannot encode initial charge state (spice21 doesn't support IC on caps).
/// Returns signal voltage V = Q * e / C_fd (0 at zero charge, ~0.64V at full well).
/// Returns (transfer_curve, analytical_fallback).
pub fn run_pixel_simulation(
    _params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> (Vec<(f64, f64)>, bool) {
    log::info!("Pixel transfer: using analytical Q/C model ({} points)", n_points);
    let curve = (0..n_points)
        .map(|i| {
            let charge = full_well * i as f64 / (n_points - 1).max(1) as f64;
            (charge, charge_to_fd_voltage(charge))
        })
        .collect();
    (curve, true) // Always analytical (spice21 can't encode initial charge on caps)
}
