//! MOS model definitions for CCD circuit simulation.
//!
//! Provides JSON circuit fragments with Mos1 NMOS/PMOS model definitions
//! and instance parameters appropriate for CCD operation.

/// Generate JSON defs array for CCD MOS models.
///
/// Returns a JSON string fragment for the `defs` array of a spice21 circuit,
/// containing NMOS (transfer gates) and PMOS (clock drivers) model definitions
/// plus instance parameter sets for various W/L ratios.
pub fn mos_model_defs_json() -> String {
    // spice21's serde config uses internally-tagged enums with flattened fields:
    //   #[serde(tag = "type")] + #[serde(flatten)]
    // So each def must have {"type": "Mos1model", "name": ..., ...} format,
    // NOT the external-tagged {"mos1model": {"name": ...}} format.
    r#"[
        {
            "type": "Mos1model",
            "name": "nmos_tg",
            "mos_type": 0,
            "vt0": 0.7,
            "kp": 1.1e-4,
            "lambda": 0.01,
            "gamma": 0.4,
            "phi": 0.6,
            "tox": 2e-8,
            "cgso": 3.5e-10,
            "cgdo": 3.5e-10,
            "cgbo": 5e-10
        },
        {
            "type": "Mos1model",
            "name": "pmos_clk",
            "mos_type": 1,
            "vt0": -0.7,
            "kp": 5e-5,
            "lambda": 0.02,
            "gamma": 0.5,
            "phi": 0.6,
            "tox": 2e-8,
            "cgso": 3.5e-10,
            "cgdo": 3.5e-10,
            "cgbo": 5e-10
        },
        {
            "type": "Mos1model",
            "name": "nmos_sf",
            "mos_type": 0,
            "vt0": 0.5,
            "kp": 1.1e-4,
            "lambda": 0.02,
            "gamma": 0.4,
            "phi": 0.6,
            "tox": 2e-8
        },
        {
            "type": "Mos1inst",
            "name": "tg_9u_05u",
            "w": 9e-6,
            "l": 0.5e-6
        },
        {
            "type": "Mos1inst",
            "name": "abg_2u_1u",
            "w": 2e-6,
            "l": 1e-6
        },
        {
            "type": "Mos1inst",
            "name": "reset_2u_05u",
            "w": 2e-6,
            "l": 0.5e-6
        },
        {
            "type": "Mos1inst",
            "name": "sf_10u_1u",
            "w": 10e-6,
            "l": 1e-6
        },
        {
            "type": "Mos1inst",
            "name": "switch_5u_05u",
            "w": 5e-6,
            "l": 0.5e-6
        },
        {
            "type": "Mos1inst",
            "name": "comp_2u_05u",
            "w": 2e-6,
            "l": 0.5e-6
        },
        {
            "type": "Mos1inst",
            "name": "clkdrv_20u_05u",
            "w": 20e-6,
            "l": 0.5e-6
        },
        {
            "type": "Mos1inst",
            "name": "default",
            "w": 5e-6,
            "l": 1e-6
        }
    ]"#
    .to_string()
}

/// Build a complete JSON circuit string with models, signals, and components.
pub fn build_circuit_json(
    name: &str,
    signals: &[&str],
    components_json: &str,
) -> String {
    let defs = mos_model_defs_json();
    let signals_json: Vec<String> = signals.iter().map(|s| format!("\"{}\"", s)).collect();
    format!(
        r#"{{
            "name": "{}",
            "signals": [{}],
            "defs": {},
            "comps": {}
        }}"#,
        name,
        signals_json.join(", "),
        defs,
        components_json,
    )
}
