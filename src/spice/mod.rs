//! SPICE transistor-level CCD simulation module.
//!
//! Provides real circuit-level simulation of the CCD readout chain using the
//! spice21 library, replacing mathematical approximations with physically
//! authentic transfer functions derived from transistor-level analysis.

pub mod amplifier;
pub mod cache;
pub mod cds;
pub mod clock_driver;
pub mod glitch;
pub mod models;
pub mod pixel;
pub mod shift_register;
pub mod transfer_function;

// Internal ADC module (not the ccd::adc)
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
        Self::FullReadout
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
            mode: SpiceMode::FullReadout,
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
    pub fn clock_period_s(&self) -> f64 {
        1.0 / (self.clock_freq_mhz * 1e6)
    }
}

/// Tracks which simulation stages fell back to analytical models.
#[derive(Debug, Clone, Default)]
pub struct SpiceFallbacks {
    pub pixel: bool,
    pub shift_register: bool,
    pub clock_driver: bool,
    pub amplifier: bool,
    pub cds: bool,
    pub adc: bool,
}

impl SpiceFallbacks {
    /// Returns the number of stages that used analytical fallback.
    pub fn count(&self) -> usize {
        [self.pixel, self.shift_register, self.clock_driver,
         self.amplifier, self.cds, self.adc]
            .iter().filter(|&&f| f).count()
    }

    /// Returns the number of stages that ran real SPICE circuits.
    pub fn spice_count(&self) -> usize {
        6 - self.count()
    }
}

/// Cached results from a SPICE simulation run.
#[derive(Debug, Clone)]
pub struct SpiceCache {
    // Per-stage results
    /// Charge (electrons) -> FD voltage transfer curve from pixel simulation.
    pub pixel_transfer: Vec<(f64, f64)>,
    /// Effective CTE per stage from shift register simulation.
    pub effective_cte: f64,
    /// Ringing kernel from clock driver simulation.
    pub clock_ringing_kernel: Vec<f64>,
    /// Clock waveform shapes [phi1, phi2, phi3].
    pub clock_waveforms: [Vec<f64>; 3],
    /// FD voltage -> amp output voltage transfer curve.
    pub amp_transfer_curve: Vec<(f64, f64)>,
    /// Amplifier noise sigma in electrons.
    pub amp_noise_sigma: f64,
    /// CDS noise rejection factor (0..1).
    pub cds_rejection: f64,
    /// ADC voltage -> digital code transfer function.
    pub adc_transfer: Vec<(f64, u16)>,
    /// DNL per code from ADC simulation.
    pub adc_dnl: Vec<f64>,

    // Composed results (used by pipeline)
    /// Charge (electrons) -> electron-equivalent output (composed pixel+amp).
    pub transfer_curve: Vec<(f64, f64)>,
    /// Combined ringing kernel.
    pub ringing_kernel: Vec<f64>,
    /// Combined noise sigma after CDS.
    pub noise_sigma: f64,

    /// Which stages fell back to analytical models.
    pub fallbacks: SpiceFallbacks,

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

    // 1. Pixel simulation: charge -> FD voltage (analytical Q/C)
    let (pixel_transfer, fb_pixel) = pixel::run_pixel_simulation(&glitch_params, full_well, n_points);

    // 2. Shift register: extract effective CTE
    let (effective_cte, fb_sr) = shift_register::run_shift_register_simulation(&glitch_params);

    // 3. Clock driver: ringing kernel + clock waveforms
    let (clock_ringing_kernel, clock_waveforms, fb_clk) =
        clock_driver::run_clock_simulation(&glitch_params);

    // 4. Amplifier: transfer curve + noise
    let (amp_transfer_curve, amp_noise_sigma, fb_amp) =
        amplifier::run_amplifier_simulation(&glitch_params, full_well, n_points);

    // 5. CDS: noise rejection factor
    let (cds_rejection, fb_cds) = cds::run_cds_simulation(&glitch_params);

    // 6. ADC: transfer function + DNL
    let (adc_transfer, adc_dnl, fb_adc) = adc::run_adc_simulation(&glitch_params);

    // 7. Build transfer curve: analytical model modulated by SPICE amp gain
    let transfer_curve = build_transfer_curve(
        &amp_transfer_curve,
        &glitch_params,
        full_well,
        n_points,
    );

    // 8. Use clock ringing kernel as the combined ringing kernel
    let ringing_kernel = clock_ringing_kernel.clone();

    // 9. Combined noise: amplifier noise attenuated by CDS
    let noise_sigma = amp_noise_sigma * (1.0 - cds_rejection).max(0.01)
        + analytical_substrate_noise(params.substrate_noise);

    SpiceCache {
        pixel_transfer,
        effective_cte,
        clock_ringing_kernel,
        clock_waveforms,
        amp_transfer_curve,
        amp_noise_sigma,
        cds_rejection,
        adc_transfer,
        adc_dnl,
        transfer_curve,
        ringing_kernel,
        noise_sigma,
        fallbacks: SpiceFallbacks {
            pixel: fb_pixel,
            shift_register: fb_sr,
            clock_driver: fb_clk,
            amplifier: fb_amp,
            cds: fb_cds,
            adc: fb_adc,
        },
        params_hash: params.param_hash(),
        sim_time_ms: 0.0,
    }
}

/// Build end-to-end transfer curve using analytical model modulated by SPICE amp gain.
///
/// The analytical_transfer_function already accounts for VDD-dependent gain,
/// body effect nonlinearity, charge injection, and phase overlap effects.
/// If the SPICE amp simulation succeeded, we extract a gain factor from it
/// and apply it to modulate the analytical curve.
fn build_transfer_curve(
    amp_transfer: &[(f64, f64)],
    params: &SpiceParams,
    full_well: f64,
    n_points: usize,
) -> Vec<(f64, f64)> {
    let mut curve = transfer_function::analytical_transfer_function(params, full_well, n_points);

    // If we have a valid SPICE amp curve, extract gain deviation and modulate
    if amp_transfer.len() >= 2 {
        let amp_max_in = amp_transfer.last().map(|(x, _)| *x).unwrap_or(1.0);
        let amp_max_out = amp_transfer.last().map(|(_, y)| *y).unwrap_or(1.0);

        if amp_max_in > 1e-10 && amp_max_out > 1e-10 {
            let spice_gain = amp_max_out / amp_max_in;
            let analytical_gain = amplifier::analytical_sf_gain(params.effective_vdd());

            if analytical_gain > 1e-10 {
                let gain_ratio = spice_gain / analytical_gain;
                // Only apply if gain ratio is reasonable (0.5 to 2.0)
                // Outside this range, the SPICE result is likely degenerate
                if gain_ratio > 0.5 && gain_ratio < 2.0 {
                    for (_, v) in curve.iter_mut() {
                        *v *= gain_ratio;
                        *v = v.clamp(0.0, full_well);
                    }
                }
            }
        }
    }

    curve
}

/// Substrate noise contribution in electrons.
fn analytical_substrate_noise(substrate_noise_param: f64) -> f64 {
    substrate_noise_param * 20.0
}
