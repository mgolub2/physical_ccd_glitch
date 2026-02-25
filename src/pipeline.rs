use crate::ccd::adc::{self, CdsMode};
use crate::ccd::amplifier;
use crate::ccd::blooming;
use crate::ccd::sensor;
use crate::ccd::transfer::{self, ReadoutDirection};
use crate::color::bayer::{self, BayerPattern};
use crate::color::demosaic::{self, DemosaicAlgo};
use crate::color::spectral;
use crate::glitch::bit_manip;
use crate::glitch::channel::{self, ChannelSwap};
use crate::glitch::pixel_shift;
use crate::glitch::scan_line;
use crate::image_io;

/// All pipeline parameters controlled by the user.
#[derive(Debug, Clone)]
pub struct PipelineParams {
    // Sensor
    pub sensor_width: u32,
    pub sensor_height: u32,
    pub full_well: f64,
    pub use_abg: bool,

    // Exposure & Noise
    pub dark_current_rate: f64,
    pub read_noise: f64,
    pub shot_noise_enabled: bool,

    // Blooming
    pub abg_strength: f64,
    pub bloom_threshold: f64,
    pub bloom_vertical: bool,

    // V-Clock
    pub v_cte: f64,
    pub v_glitch_rate: f64,
    pub v_waveform_distortion: f64,
    pub parallel_smear: f64,

    // H-Clock
    pub h_cte: f64,
    pub h_glitch_rate: f64,
    pub h_ringing: f64,
    pub readout_direction: ReadoutDirection,

    // Amplifier
    pub amp_gain: f64,
    pub nonlinearity: f64,
    pub reset_noise: f64,
    pub amp_glow: f64,

    // ADC
    pub bit_depth: u8,
    pub cds_mode: CdsMode,
    pub adc_gain: f64,
    pub bias: f64,
    pub dnl_errors: f64,
    pub bit_errors: f64,
    pub adc_jitter: f64,

    // Glitch
    pub pixel_shift_amount: f64,
    pub block_shift_amount: f64,
    pub scan_line_frequency: f64,
    pub bit_xor_mask: u16,
    pub bit_rotation: i32,
    pub bit_plane_swaps: u32,

    // Channel
    pub channel_swap: ChannelSwap,
    pub channel_r_gain: f64,
    pub channel_g_gain: f64,
    pub channel_b_gain: f64,
    pub channel_r_offset: f64,
    pub channel_g_offset: f64,
    pub channel_b_offset: f64,
    pub chromatic_r_x: i32,
    pub chromatic_r_y: i32,
    pub chromatic_b_x: i32,
    pub chromatic_b_y: i32,

    // Color / Output
    pub bayer_pattern: BayerPattern,
    pub demosaic_algo: DemosaicAlgo,
    pub white_balance_r: f64,
    pub white_balance_g: f64,
    pub white_balance_b: f64,
    pub gamma: f64,
    pub brightness: f64,
    pub contrast: f64,

    // SPICE simulation
    pub spice: crate::spice::SpiceParams,
}

impl Default for PipelineParams {
    fn default() -> Self {
        Self {
            sensor_width: 3072,
            sensor_height: 2048,
            full_well: 40_000.0,
            use_abg: true,

            dark_current_rate: 0.0,
            read_noise: 0.0,
            shot_noise_enabled: false,

            abg_strength: 1.0,
            bloom_threshold: 0.8,
            bloom_vertical: true,

            v_cte: 0.999999,
            v_glitch_rate: 0.0,
            v_waveform_distortion: 0.0,
            parallel_smear: 0.0,

            h_cte: 0.999999,
            h_glitch_rate: 0.0,
            h_ringing: 0.0,
            readout_direction: ReadoutDirection::LeftToRight,

            amp_gain: 1.0,
            nonlinearity: 0.0,
            reset_noise: 0.0,
            amp_glow: 0.0,

            bit_depth: 16,
            cds_mode: CdsMode::On,
            adc_gain: 1.0,
            bias: 0.0,
            dnl_errors: 0.0,
            bit_errors: 0.0,
            adc_jitter: 0.0,

            pixel_shift_amount: 0.0,
            block_shift_amount: 0.0,
            scan_line_frequency: 0.0,
            bit_xor_mask: 0,
            bit_rotation: 0,
            bit_plane_swaps: 0,

            channel_swap: ChannelSwap::None,
            channel_r_gain: 1.0,
            channel_g_gain: 1.0,
            channel_b_gain: 1.0,
            channel_r_offset: 0.0,
            channel_g_offset: 0.0,
            channel_b_offset: 0.0,
            chromatic_r_x: 0,
            chromatic_r_y: 0,
            chromatic_b_x: 0,
            chromatic_b_y: 0,

            bayer_pattern: BayerPattern::Rggb,
            demosaic_algo: DemosaicAlgo::MalvarHeCutler,
            white_balance_r: 1.0,
            white_balance_g: 1.0,
            white_balance_b: 1.0,
            gamma: 2.2,
            brightness: 0.0,
            contrast: 1.0,

            spice: crate::spice::SpiceParams::default(),
        }
    }
}

/// Run the full CCD processing pipeline on an input image.
/// Returns the final RGB image as (width, height, rgb_bytes).
pub fn process(
    source: &image::DynamicImage,
    params: &PipelineParams,
    spice_cache: &Option<crate::spice::SpiceCache>,
) -> (usize, usize, Vec<u8>) {
    let w = params.sensor_width;
    let h = params.sensor_height;
    let width = w as usize;
    let height = h as usize;

    // Step 1: Resize image to sensor dimensions
    let resized = image_io::resize_to_sensor(source, w, h);

    // Step 1b: Convert to electron counts
    let (rgb_electrons, _, _) = sensor::image_to_electrons(&resized, params.full_well);

    // Step 2: Apply Bayer CFA
    let mut mosaic = bayer::apply_bayer(&rgb_electrons, width, height, params.bayer_pattern);

    // Step 3: Dark current + shot noise + read noise
    sensor::add_dark_current(&mut mosaic, params.dark_current_rate);
    if params.shot_noise_enabled {
        sensor::add_shot_noise(&mut mosaic);
    }
    sensor::add_read_noise(&mut mosaic, params.read_noise);

    // SPICE branch: replace mathematical pipeline stages with circuit-derived processing
    let spice_handled = process_spice_branch(
        &mut mosaic,
        width,
        height,
        params,
        spice_cache,
    );

    if !spice_handled {
        // Step 4: Blooming
        blooming::apply_blooming(
            &mut mosaic,
            width,
            height,
            params.full_well,
            params.abg_strength,
            params.bloom_threshold,
            params.bloom_vertical,
        );

        // Step 5: Vertical (parallel) transfer
        transfer::vertical_transfer(
            &mut mosaic,
            width,
            height,
            params.v_cte,
            params.v_glitch_rate,
            params.v_waveform_distortion,
            params.parallel_smear,
        );

        // Step 6: Horizontal (serial) transfer
        transfer::horizontal_transfer(
            &mut mosaic,
            width,
            height,
            params.h_cte,
            params.h_glitch_rate,
            params.h_ringing,
            params.readout_direction,
        );

        // Step 7: Output amplifier
        amplifier::apply_amplifier(
            &mut mosaic,
            width,
            height,
            params.amp_gain,
            params.nonlinearity,
            params.reset_noise,
            params.amp_glow,
        );

        // Step 8: ADC
        adc::apply_adc(
            &mut mosaic,
            width,
            height,
            params.bit_depth,
            params.cds_mode,
            params.adc_gain,
            params.bias,
            params.reset_noise,
            params.dnl_errors,
            params.bit_errors,
            params.adc_jitter,
        );
    }

    // Step 9a: Pre-demosaic glitch effects
    let max_code = ((1u64 << params.bit_depth) - 1) as f64;

    pixel_shift::apply_pixel_shift(&mut mosaic, width, height, params.pixel_shift_amount);
    pixel_shift::apply_block_shift(&mut mosaic, width, height, params.block_shift_amount);
    scan_line::apply_scan_line_corruption(
        &mut mosaic,
        width,
        height,
        params.scan_line_frequency,
        max_code,
    );
    bit_manip::apply_bit_xor(&mut mosaic, max_code, params.bit_xor_mask);
    bit_manip::apply_bit_rotation(&mut mosaic, params.bit_depth, params.bit_rotation);
    bit_manip::apply_bit_plane_swap(&mut mosaic, params.bit_depth, params.bit_plane_swaps);

    // Step 10: Demosaicing
    let mut rgb = demosaic::demosaic(
        &mosaic,
        width,
        height,
        params.bayer_pattern,
        params.demosaic_algo,
    );

    // Normalize from ADC counts to [0, 1] range
    if max_code > 0.0 {
        for pixel in rgb.iter_mut() {
            for c in 0..3 {
                pixel[c] = (pixel[c] / max_code).clamp(0.0, 1.0);
            }
        }
    }

    // Step 9b: Post-demosaic channel effects
    channel::apply_channel_gain_offset(
        &mut rgb,
        params.channel_r_gain,
        params.channel_g_gain,
        params.channel_b_gain,
        params.channel_r_offset,
        params.channel_g_offset,
        params.channel_b_offset,
    );
    channel::apply_channel_swap(&mut rgb, params.channel_swap);
    channel::apply_chromatic_aberration(
        &mut rgb,
        width,
        height,
        params.chromatic_r_x,
        params.chromatic_r_y,
        params.chromatic_b_x,
        params.chromatic_b_y,
    );

    // Step 11: Color rendering
    spectral::apply_white_balance(
        &mut rgb,
        params.white_balance_r,
        params.white_balance_g,
        params.white_balance_b,
    );

    // Clamp before gamma
    for pixel in rgb.iter_mut() {
        for c in 0..3 {
            pixel[c] = pixel[c].clamp(0.0, 1.0);
        }
    }

    spectral::apply_gamma(&mut rgb, params.gamma);
    spectral::apply_brightness_contrast(&mut rgb, params.brightness, params.contrast);

    let bytes = spectral::rgb_to_bytes(&rgb, width, height);
    (width, height, bytes)
}

/// Process using SPICE-derived transfer function and timing artifacts.
///
/// Returns true if SPICE processing was applied (replacing math pipeline stages),
/// false if SPICE mode is Off or no cache is available.
fn process_spice_branch(
    mosaic: &mut [f64],
    width: usize,
    height: usize,
    params: &PipelineParams,
    spice_cache: &Option<crate::spice::SpiceCache>,
) -> bool {
    use crate::spice::{SpiceMode, transfer_function};

    if params.spice.mode == SpiceMode::Off {
        return false;
    }

    let cache = match spice_cache {
        Some(c) => c,
        None => return false,
    };

    match params.spice.mode {
        SpiceMode::Off => false,

        SpiceMode::FullReadout => {
            // Full SPICE-driven pipeline: missing pulses -> CTE -> transfer -> CDS noise -> ADC -> ringing

            transfer_function::apply_missing_pulses(
                mosaic,
                width,
                height,
                params.spice.missing_pulse_rate,
            );

            // CTE degradation using SPICE-derived CTE
            apply_spice_cte(mosaic, width, height, cache.effective_cte, params);

            // Transfer function (composed pixel -> amp curve)
            transfer_function::apply_transfer_function(
                mosaic,
                &cache.transfer_curve,
                params.full_well,
            );

            // CDS residual noise
            apply_spice_cds_noise(mosaic, cache.cds_rejection, cache.noise_sigma);

            // ADC quantization using SPICE-derived transfer
            apply_spice_adc(mosaic, &cache.adc_transfer, &cache.adc_dnl, params);

            // Ringing from clock driver
            transfer_function::apply_ringing(mosaic, width, height, &cache.ringing_kernel);

            true
        }

        SpiceMode::AmplifierOnly => {
            // Math blooming + transfer, then SPICE amp + ADC

            transfer_function::apply_missing_pulses(
                mosaic,
                width,
                height,
                params.spice.missing_pulse_rate,
            );

            crate::ccd::blooming::apply_blooming(
                mosaic,
                width,
                height,
                params.full_well,
                params.abg_strength,
                params.bloom_threshold,
                params.bloom_vertical,
            );
            crate::ccd::transfer::vertical_transfer(
                mosaic,
                width,
                height,
                params.v_cte,
                params.v_glitch_rate,
                params.v_waveform_distortion,
                params.parallel_smear,
            );
            crate::ccd::transfer::horizontal_transfer(
                mosaic,
                width,
                height,
                params.h_cte,
                params.h_glitch_rate,
                params.h_ringing,
                params.readout_direction,
            );

            // SPICE amp transfer + ADC
            transfer_function::apply_transfer_function(
                mosaic,
                &cache.transfer_curve,
                params.full_well,
            );

            apply_spice_cds_noise(mosaic, cache.cds_rejection, cache.noise_sigma);
            apply_spice_adc(mosaic, &cache.adc_transfer, &cache.adc_dnl, params);

            true
        }

        SpiceMode::TransferCurveOnly => {
            // Full math pipeline but SPICE amp transfer curve for nonlinearity

            transfer_function::apply_missing_pulses(
                mosaic,
                width,
                height,
                params.spice.missing_pulse_rate,
            );

            crate::ccd::blooming::apply_blooming(
                mosaic,
                width,
                height,
                params.full_well,
                params.abg_strength,
                params.bloom_threshold,
                params.bloom_vertical,
            );
            crate::ccd::transfer::vertical_transfer(
                mosaic,
                width,
                height,
                params.v_cte,
                params.v_glitch_rate,
                params.v_waveform_distortion,
                params.parallel_smear,
            );
            crate::ccd::transfer::horizontal_transfer(
                mosaic,
                width,
                height,
                params.h_cte,
                params.h_glitch_rate,
                params.h_ringing,
                params.readout_direction,
            );

            // SPICE transfer curve replaces amplifier
            transfer_function::apply_transfer_function(
                mosaic,
                &cache.transfer_curve,
                params.full_well,
            );

            // Keep mathematical ADC
            crate::ccd::adc::apply_adc(
                mosaic,
                width,
                height,
                params.bit_depth,
                params.cds_mode,
                params.adc_gain,
                params.bias,
                params.reset_noise,
                params.dnl_errors,
                params.bit_errors,
                params.adc_jitter,
            );

            true
        }
    }
}

/// Apply CTE degradation using SPICE-derived CTE value.
///
/// Simulates vertical and horizontal charge trailing.
fn apply_spice_cte(
    mosaic: &mut [f64],
    width: usize,
    height: usize,
    cte: f64,
    params: &PipelineParams,
) {
    if cte >= 1.0 {
        return;
    }

    let loss = 1.0 - cte;

    // Vertical (parallel) CTE trailing
    for x in 0..width {
        let mut trail = 0.0;
        for y in 0..height {
            let idx = y * width + x;
            let lost = mosaic[idx] * loss;
            mosaic[idx] -= lost;
            mosaic[idx] += trail;
            trail = lost;
        }
    }

    // Horizontal (serial) CTE trailing
    for y in 0..height {
        let row_start = y * width;
        let mut trail = 0.0;
        let range: Box<dyn Iterator<Item = usize>> =
            match params.readout_direction {
                crate::ccd::transfer::ReadoutDirection::LeftToRight
                | crate::ccd::transfer::ReadoutDirection::Alternating => {
                    Box::new(0..width)
                }
                crate::ccd::transfer::ReadoutDirection::RightToLeft => {
                    Box::new((0..width).rev())
                }
            };
        for x in range {
            let idx = row_start + x;
            let lost = mosaic[idx] * loss;
            mosaic[idx] -= lost;
            mosaic[idx] += trail;
            trail = lost;
        }
    }
}

/// Apply CDS residual noise: Gaussian noise scaled by (1 - rejection).
fn apply_spice_cds_noise(mosaic: &mut [f64], rejection: f64, noise_sigma: f64) {
    let effective_noise = noise_sigma * (1.0 - rejection).max(0.0);
    if effective_noise < 0.01 {
        return;
    }

    // Simple deterministic noise based on index (reproducible)
    for (i, val) in mosaic.iter_mut().enumerate() {
        let hash = ((i as f64 * 0.6180339887).fract() * 2.0 - 1.0) * 2.0;
        *val += hash * effective_noise;
    }
}

/// Apply SPICE-derived ADC quantization.
///
/// Uses the 4-bit SPICE transfer function scaled to target bit depth,
/// with DNL applied.
fn apply_spice_adc(
    mosaic: &mut [f64],
    adc_transfer: &[(f64, u16)],
    adc_dnl: &[f64],
    params: &PipelineParams,
) {
    let max_code = ((1u64 << params.bit_depth) - 1) as f64;
    let full_well = params.full_well;

    if adc_transfer.is_empty() {
        // Simple quantization fallback
        for val in mosaic.iter_mut() {
            let normalized = (*val / full_well).clamp(0.0, 1.0);
            *val = (normalized * max_code).round();
        }
        return;
    }

    // ADC transfer is 4-bit (0..15). Scale to target bit depth.
    let adc_max_code = adc_transfer.iter().map(|(_, c)| *c).max().unwrap_or(15) as f64;
    let v_max = adc_transfer.last().map(|(v, _)| *v).unwrap_or(1.0);

    for val in mosaic.iter_mut() {
        let normalized = (*val / full_well).clamp(0.0, 1.0);
        let v_equiv = normalized * v_max;

        // Look up in ADC transfer function
        let adc_code = lookup_adc_transfer(adc_transfer, v_equiv);

        // Scale from 4-bit to target bit depth
        let scaled = (adc_code as f64 / adc_max_code) * max_code;

        // Apply DNL
        let code_idx = (adc_code as usize).min(adc_dnl.len().saturating_sub(1));
        let dnl_offset = if !adc_dnl.is_empty() {
            adc_dnl[code_idx] * (max_code / adc_max_code)
        } else {
            0.0
        };

        *val = (scaled + dnl_offset).round().clamp(0.0, max_code);
    }
}

/// Look up output code from ADC transfer function via interpolation.
fn lookup_adc_transfer(transfer: &[(f64, u16)], v_in: f64) -> u16 {
    if transfer.is_empty() {
        return 0;
    }

    for i in (0..transfer.len()).rev() {
        if v_in >= transfer[i].0 {
            return transfer[i].1;
        }
    }
    transfer[0].1
}
