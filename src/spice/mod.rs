//! SPICE transistor-level CCD simulation module.
//!
//! Provides real circuit-level simulation of the CCD readout chain using the
//! spice21 library, replacing mathematical approximations with physically
//! authentic transfer functions derived from transistor-level analysis.

#[allow(dead_code)]
pub mod amplifier;
pub mod cache;
#[allow(dead_code)]
pub mod cds;
#[allow(dead_code)]
pub mod clock_driver;
pub mod glitch;
#[allow(dead_code)]
pub mod models;
#[allow(dead_code)]
pub mod pixel;
#[allow(dead_code)]
pub mod shift_register;
pub mod transfer_function;

// Internal ADC module (not the ccd::adc)
#[allow(dead_code)]
pub mod adc;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Simulation mode for the SPICE engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiceMode {
    /// No SPICE simulation; use mathematical pipeline.
    Off,
    /// Full readout chain: pixel -> shift register -> amp -> CDS -> ADC.
    FullReadout,
    /// Only simulate the output amplifier + ADC stages.
    AmplifierOnly,
    /// Only apply the SPICE-derived nonlinear transfer curve.
    TransferCurveOnly,
}

impl Default for SpiceMode {
    fn default() -> Self {
        Self::Off
    }
}

impl SpiceMode {
    pub const ALL: &[SpiceMode] = &[
        SpiceMode::Off,
        SpiceMode::FullReadout,
        SpiceMode::AmplifierOnly,
        SpiceMode::TransferCurveOnly,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::FullReadout => "Full Readout",
            Self::AmplifierOnly => "Amplifier Only",
            Self::TransferCurveOnly => "Transfer Curve Only",
        }
    }
}

/// Parameters for the SPICE simulation.
#[derive(Debug, Clone)]
pub struct SpiceParams {
    pub mode: SpiceMode,

    // Circuit parameters
    pub vdd: f64,
    pub clock_freq_mhz: f64,
    pub temperature_k: f64,
    pub shift_register_stages: usize,
    pub transfer_function_resolution: usize,

    // Glitch parameters
    pub supply_droop: f64,
    pub phase_overlap_ns: f64,
    pub missing_pulse_rate: f64,
    pub charge_injection: f64,
    pub substrate_noise: f64,
}

impl Default for SpiceParams {
    fn default() -> Self {
        Self {
            mode: SpiceMode::Off,
            vdd: 15.0,
            clock_freq_mhz: 10.0,
            temperature_k: 300.0,
            shift_register_stages: 8,
            transfer_function_resolution: 32,
            supply_droop: 0.0,
            phase_overlap_ns: 0.0,
            missing_pulse_rate: 0.0,
            charge_injection: 0.0,
            substrate_noise: 0.0,
        }
    }
}

impl SpiceParams {
    /// Compute a hash for cache invalidation.
    pub fn param_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        (self.mode as u8).hash(&mut hasher);
        self.vdd.to_bits().hash(&mut hasher);
        self.clock_freq_mhz.to_bits().hash(&mut hasher);
        self.temperature_k.to_bits().hash(&mut hasher);
        self.shift_register_stages.hash(&mut hasher);
        self.transfer_function_resolution.hash(&mut hasher);
        self.supply_droop.to_bits().hash(&mut hasher);
        self.phase_overlap_ns.to_bits().hash(&mut hasher);
        self.missing_pulse_rate.to_bits().hash(&mut hasher);
        self.charge_injection.to_bits().hash(&mut hasher);
        self.substrate_noise.to_bits().hash(&mut hasher);
        hasher.finish()
    }

    /// Return effective VDD after supply droop is applied.
    pub fn effective_vdd(&self) -> f64 {
        self.vdd * (1.0 - self.supply_droop)
    }

    /// Return clock period in seconds.
    #[allow(dead_code)]
    pub fn clock_period_s(&self) -> f64 {
        1.0 / (self.clock_freq_mhz * 1e6)
    }
}

/// Cached results from a SPICE simulation run.
#[derive(Debug, Clone)]
pub struct SpiceCache {
    /// Input (electrons) -> output (voltage) transfer curve.
    pub transfer_curve: Vec<(f64, f64)>,
    /// Ringing kernel for convolution along readout direction.
    pub ringing_kernel: Vec<f64>,
    /// RMS noise in electrons from kTC + substrate.
    pub noise_sigma: f64,
    /// Effective CTE derived from shift register simulation.
    pub effective_cte: f64,
    /// Hash of the params that produced this cache.
    pub params_hash: u64,
    /// Simulation time in milliseconds.
    pub sim_time_ms: f64,
}

impl SpiceCache {
    pub fn is_valid_for(&self, params: &SpiceParams) -> bool {
        self.params_hash == params.param_hash()
    }
}

/// Run the SPICE simulation (or return cached results).
pub fn simulate_or_cache(
    params: &SpiceParams,
    full_well: f64,
    cache: &mut Option<SpiceCache>,
) {
    if let Some(c) = &*cache {
        if c.is_valid_for(params) {
            return;
        }
    }

    let start = web_time::Instant::now();
    let new_cache = run_simulation(params, full_well);
    let sim_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    *cache = Some(SpiceCache {
        sim_time_ms,
        ..new_cache
    });
}

fn run_simulation(params: &SpiceParams, full_well: f64) -> SpiceCache {
    let glitch_params = glitch::apply_glitches(params);

    let n_points = params.transfer_function_resolution;
    let transfer_curve =
        transfer_function::extract_transfer_function(&glitch_params, full_well, n_points);

    let ringing_kernel = transfer_function::extract_ringing_kernel(&glitch_params);

    let noise_sigma = estimate_noise(&glitch_params);
    let effective_cte = estimate_cte(&glitch_params);

    SpiceCache {
        transfer_curve,
        ringing_kernel,
        noise_sigma,
        effective_cte,
        params_hash: params.param_hash(),
        sim_time_ms: 0.0,
    }
}

/// Estimate kTC + substrate noise from circuit parameters.
fn estimate_noise(params: &SpiceParams) -> f64 {
    // kTC noise on floating diffusion: sigma = sqrt(kT/C) / (q/C_fd)
    let k = 1.38e-23;
    let t = params.temperature_k;
    let c_fd = 10e-15; // 10 fF
    let q = 1.6e-19;

    let ktc_voltage = (k * t / c_fd).sqrt();
    let ktc_electrons = ktc_voltage * c_fd / q;

    // Substrate noise contribution
    let substrate = params.substrate_noise * 20.0; // up to 20 electrons RMS

    (ktc_electrons * ktc_electrons + substrate * substrate).sqrt()
}

/// Estimate effective CTE from shift register parameters.
fn estimate_cte(params: &SpiceParams) -> f64 {
    // CTE degrades with more stages, higher frequency, lower VDD
    let base_cte = 0.999999;
    let freq_factor = 1.0 - (params.clock_freq_mhz / 100.0).min(0.5) * 0.00001;
    let vdd_factor = (params.effective_vdd() / 15.0).min(1.0);
    let stage_factor = 1.0 - (params.shift_register_stages as f64 / 100.0) * 0.000001;

    // Phase overlap degrades CTE: overlapping clocks cause incomplete charge transfer
    let clock_period_ns = 1e3 / params.clock_freq_mhz;
    let overlap_fraction = if params.phase_overlap_ns > 0.0 {
        (params.phase_overlap_ns / clock_period_ns).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let overlap_factor = 1.0 - overlap_fraction * 0.0001;

    // Missing pulses directly degrade CTE
    let missing_factor = 1.0 - params.missing_pulse_rate * 0.001;

    base_cte * freq_factor * vdd_factor * stage_factor * overlap_factor * missing_factor
}
