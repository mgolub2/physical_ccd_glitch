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
        }
    }
}

/// Run the full CCD processing pipeline on an input image.
/// Returns the final RGB image as (width, height, rgb_bytes).
pub fn process(
    source: &image::DynamicImage,
    params: &PipelineParams,
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
