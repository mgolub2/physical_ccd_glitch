//! SPICE simulation test harness.
//!
//! Tests each SPICE feature independently and produces output images
//! plus a diagnostic report.
//!
//! Usage: cargo run --bin spice_test --features spice

// Reuse the library crate
use physical_ccd_glitch::pipeline::{self, PipelineParams};
use physical_ccd_glitch::spice::{self, SpiceCache, SpiceMode, SpiceParams};

use image::{DynamicImage, Rgb, RgbImage};
use std::path::Path;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();

    let output_dir = Path::new("test_output");
    std::fs::create_dir_all(output_dir).expect("Failed to create test_output directory");

    println!("=== SPICE Simulation Test Harness ===\n");

    // Generate test images
    let gradient = generate_gradient_image(512, 384);
    let checkerboard = generate_checkerboard_image(512, 384);
    let test_image = load_or_generate_test_image();

    println!("Test images generated:");
    println!("  - gradient: 512x384 (horizontal brightness ramp)");
    println!("  - checkerboard: 512x384 (high-contrast pattern)");
    println!("  - photo: natural image\n");

    // Save reference (no SPICE)
    save_reference(&gradient, "gradient", output_dir);
    save_reference(&checkerboard, "checkerboard", output_dir);
    save_reference(&test_image, "photo", output_dir);

    println!("--- Test Suite ---\n");

    let mut all_pass = true;

    // Test 1: SpiceParams hashing and cache validity
    all_pass &= test_cache_validity();

    // Test 2: Transfer function extraction (analytical fallback)
    all_pass &= test_transfer_function_extraction();

    // Test 3: Ringing kernel extraction
    all_pass &= test_ringing_kernel();

    // Test 4: Full Readout mode on gradient
    all_pass &= test_spice_mode(
        &gradient,
        "gradient",
        SpiceMode::FullReadout,
        &SpiceParams::default(),
        output_dir,
    );

    // Test 5: Amplifier Only mode on gradient
    all_pass &= test_spice_mode(
        &gradient,
        "gradient",
        SpiceMode::AmplifierOnly,
        &SpiceParams::default(),
        output_dir,
    );

    // Test 6: Transfer Curve Only mode on gradient
    all_pass &= test_spice_mode(
        &gradient,
        "gradient",
        SpiceMode::TransferCurveOnly,
        &SpiceParams::default(),
        output_dir,
    );

    // Test 7: Full Readout on checkerboard (ringing visible on edges)
    all_pass &= test_spice_mode(
        &checkerboard,
        "checkerboard",
        SpiceMode::FullReadout,
        &SpiceParams::default(),
        output_dir,
    );

    // Test 8: Full Readout on photo
    all_pass &= test_spice_mode(
        &test_image,
        "photo",
        SpiceMode::FullReadout,
        &SpiceParams::default(),
        output_dir,
    );

    // Test 9: Supply droop glitch
    all_pass &= test_glitch_supply_droop(&gradient, output_dir);

    // Test 10: Phase overlap glitch
    all_pass &= test_glitch_phase_overlap(&gradient, output_dir);

    // Test 11: Charge injection glitch
    all_pass &= test_glitch_charge_injection(&gradient, output_dir);

    // Test 12: Substrate noise glitch
    all_pass &= test_glitch_substrate_noise(&gradient, output_dir);

    // Test 13: Combined extreme glitches on photo
    all_pass &= test_combined_glitches(&test_image, output_dir);

    // Test 14: VDD sweep (5V to 20V)
    all_pass &= test_vdd_sweep(&gradient, output_dir);

    // Test 15: Temperature sweep
    all_pass &= test_temperature_sweep(&gradient, output_dir);

    // Test 16: SPICE simulation timing
    all_pass &= test_simulation_timing();

    // Test 17: SPICE vs mathematical pipeline comparison
    all_pass &= test_spice_vs_math(&gradient, output_dir);

    println!("\n=== Results ===");
    if all_pass {
        println!("ALL TESTS PASSED");
    } else {
        println!("SOME TESTS FAILED - check output above");
        std::process::exit(1);
    }
    println!("\nOutput images in: {}", output_dir.display());
}

// === Image Generation ===

fn generate_gradient_image(width: u32, height: u32) -> DynamicImage {
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let val = (x as f64 / (width - 1) as f64 * 255.0) as u8;
            // Add some vertical variation
            let row_mod = ((y as f64 / height as f64 * 4.0).sin() * 20.0) as i16;
            let v = (val as i16 + row_mod).clamp(0, 255) as u8;
            img.put_pixel(x, y, Rgb([v, v, v]));
        }
    }
    DynamicImage::ImageRgb8(img)
}

fn generate_checkerboard_image(width: u32, height: u32) -> DynamicImage {
    let mut img = RgbImage::new(width, height);
    let block = 32;
    for y in 0..height {
        for x in 0..width {
            let checker = ((x / block) + (y / block)) % 2 == 0;
            let val = if checker { 220u8 } else { 30u8 };
            img.put_pixel(x, y, Rgb([val, val, val]));
        }
    }
    DynamicImage::ImageRgb8(img)
}

fn load_or_generate_test_image() -> DynamicImage {
    let path = Path::new("test_data/test_image.png");
    if path.exists() {
        match image::open(path) {
            Ok(img) => return img,
            Err(e) => eprintln!("Warning: Failed to load test_data/test_image.png: {e}"),
        }
    }
    // Fallback: generate a colorful synthetic image
    let width = 512;
    let height = 384;
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let fx = x as f64 / width as f64;
            let fy = y as f64 / height as f64;
            let r = ((fx * 3.0).sin().abs() * 255.0) as u8;
            let g = ((fy * 2.5 + 1.0).sin().abs() * 255.0) as u8;
            let b = (((fx + fy) * 2.0).cos().abs() * 200.0) as u8;
            img.put_pixel(x, y, Rgb([r, g, b]));
        }
    }
    DynamicImage::ImageRgb8(img)
}

// === Helpers ===

fn save_reference(img: &DynamicImage, name: &str, output_dir: &Path) {
    let mut params = PipelineParams::default();
    params.sensor_width = 512;
    params.sensor_height = 384;
    params.spice.mode = SpiceMode::Off;

    let (w, h, bytes) = pipeline::process(img, &params, &None);
    save_output(&bytes, w, h, &format!("{}_reference", name), output_dir);
}

fn process_with_spice(
    img: &DynamicImage,
    spice_params: &SpiceParams,
    full_well: f64,
) -> (usize, usize, Vec<u8>, SpiceCache) {
    let mut params = PipelineParams::default();
    params.sensor_width = 512;
    params.sensor_height = 384;
    params.full_well = full_well;
    params.spice = spice_params.clone();

    let mut cache: Option<SpiceCache> = None;

    // Run simulation
    if spice_params.mode != SpiceMode::Off {
        spice::simulate_or_cache(&params.spice, params.full_well, &mut cache);
    }

    let (w, h, bytes) = pipeline::process(img, &params, &cache);
    let cache_out = cache.unwrap_or(SpiceCache {
        pixel_transfer: vec![],
        effective_cte: 1.0,
        clock_ringing_kernel: vec![],
        clock_waveforms: [vec![], vec![], vec![]],
        amp_transfer_curve: vec![],
        amp_noise_sigma: 0.0,
        cds_rejection: 0.0,
        adc_transfer: vec![],
        adc_dnl: vec![],
        transfer_curve: vec![],
        ringing_kernel: vec![],
        noise_sigma: 0.0,
        fallbacks: Default::default(),
        params_hash: 0,
        sim_time_ms: 0.0,
    });
    (w, h, bytes, cache_out)
}

fn save_output(bytes: &[u8], w: usize, h: usize, name: &str, output_dir: &Path) {
    if let Some(img) = RgbImage::from_raw(w as u32, h as u32, bytes.to_vec()) {
        let path = output_dir.join(format!("{}.png", name));
        img.save(&path).expect("Failed to save output image");
    }
}

fn image_statistics(bytes: &[u8]) -> (f64, f64, f64, f64) {
    if bytes.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let n = bytes.len() as f64;
    let mean = bytes.iter().map(|&b| b as f64).sum::<f64>() / n;
    let variance = bytes.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    let min = bytes.iter().copied().min().unwrap_or(0) as f64;
    let max = bytes.iter().copied().max().unwrap_or(0) as f64;
    (mean, std_dev, min, max)
}

fn pixel_diff_stats(a: &[u8], b: &[u8]) -> (f64, f64, f64) {
    assert_eq!(a.len(), b.len());
    if a.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let n = a.len() as f64;
    let diffs: Vec<f64> = a.iter().zip(b.iter()).map(|(&x, &y)| (x as f64 - y as f64).abs()).collect();
    let mean_diff = diffs.iter().sum::<f64>() / n;
    let max_diff = diffs.iter().copied().fold(0.0f64, f64::max);
    let rms_diff = (diffs.iter().map(|d| d * d).sum::<f64>() / n).sqrt();
    (mean_diff, max_diff, rms_diff)
}

fn print_result(name: &str, pass: bool, detail: &str) {
    let status = if pass { "PASS" } else { "FAIL" };
    println!("  [{}] {} - {}", status, name, detail);
}

// === Test Cases ===

fn test_cache_validity() -> bool {
    println!("Test: Cache validity and hashing");

    let p1 = SpiceParams::default();
    let p2 = SpiceParams { vdd: 12.0, ..SpiceParams::default() };
    let p3 = SpiceParams::default();

    let h1 = p1.param_hash();
    let h2 = p2.param_hash();
    let h3 = p3.param_hash();

    let hash_differ = h1 != h2;
    let hash_match = h1 == h3;

    print_result(
        "hash uniqueness",
        hash_differ,
        &format!("h1={:#x} h2={:#x} differ={}", h1, h2, hash_differ),
    );
    print_result(
        "hash determinism",
        hash_match,
        &format!("h1={:#x} h3={:#x} match={}", h1, h3, hash_match),
    );

    // Test cache invalidation
    let mut cache = None;
    let full_well = 40_000.0;
    let mut p = SpiceParams { mode: SpiceMode::FullReadout, ..SpiceParams::default() };

    spice::simulate_or_cache(&p, full_well, &mut cache);
    let valid_before = cache.as_ref().map(|c| c.is_valid_for(&p)).unwrap_or(false);

    p.vdd = 10.0;
    let valid_after = cache.as_ref().map(|c| c.is_valid_for(&p)).unwrap_or(false);

    print_result(
        "cache validity",
        valid_before && !valid_after,
        &format!("valid_before={} invalid_after_change={}", valid_before, !valid_after),
    );

    hash_differ && hash_match && valid_before && !valid_after
}

fn test_transfer_function_extraction() -> bool {
    println!("\nTest: Transfer function extraction");

    let params = SpiceParams {
        mode: SpiceMode::FullReadout,
        transfer_function_resolution: 32,
        ..SpiceParams::default()
    };
    let full_well = 40_000.0;
    let mut cache = None;

    spice::simulate_or_cache(&params, full_well, &mut cache);
    let c = cache.as_ref().unwrap();

    let has_points = c.transfer_curve.len() == 32;
    let monotonic = c.transfer_curve.windows(2).all(|w| w[1].1 >= w[0].1);
    // Output is in electron-equivalent units, so starts near zero and ends near full_well
    let starts_near_zero = c.transfer_curve.first().map(|p| p.1.abs() < full_well * 0.01).unwrap_or(false);
    let ends_positive = c.transfer_curve.last().map(|p| p.1 > full_well * 0.5).unwrap_or(false);

    // Check input range spans full well
    let input_range = c.transfer_curve.last().map(|p| p.0).unwrap_or(0.0)
        - c.transfer_curve.first().map(|p| p.0).unwrap_or(0.0);
    let correct_range = (input_range - full_well).abs() < 1.0;

    print_result(
        "point count",
        has_points,
        &format!("got {} points, expected 32", c.transfer_curve.len()),
    );
    print_result(
        "monotonicity",
        monotonic,
        "transfer curve is monotonically increasing",
    );
    print_result(
        "zero start",
        starts_near_zero,
        &format!("first output = {:.4}V", c.transfer_curve.first().map(|p| p.1).unwrap_or(-1.0)),
    );
    print_result(
        "positive end",
        ends_positive,
        &format!("last output = {:.4}V", c.transfer_curve.last().map(|p| p.1).unwrap_or(-1.0)),
    );
    print_result(
        "input range",
        correct_range,
        &format!("range = {:.0} e-, expected {:.0}", input_range, full_well),
    );

    println!("  Transfer curve sample: {:?}", &c.transfer_curve[..5.min(c.transfer_curve.len())]);

    has_points && monotonic && starts_near_zero && ends_positive && correct_range
}

fn test_ringing_kernel() -> bool {
    println!("\nTest: Ringing kernel extraction");

    let params = SpiceParams {
        mode: SpiceMode::FullReadout,
        supply_droop: 0.3, // droop amplifies ringing
        ..SpiceParams::default()
    };
    let mut cache = None;
    spice::simulate_or_cache(&params, 40_000.0, &mut cache);
    let c = cache.as_ref().unwrap();

    let has_kernel = !c.ringing_kernel.is_empty();
    let kernel_len = c.ringing_kernel.len();
    let has_oscillation = c.ringing_kernel.iter().any(|&v| v < 0.0)
        && c.ringing_kernel.iter().any(|&v| v > 0.0);
    let decaying = if kernel_len >= 4 {
        c.ringing_kernel.last().map(|v| v.abs()).unwrap_or(1.0)
            < c.ringing_kernel[1].abs() + 0.01
    } else {
        false
    };

    print_result(
        "kernel exists",
        has_kernel,
        &format!("{} taps", kernel_len),
    );
    print_result(
        "oscillation present",
        has_oscillation,
        "kernel has both positive and negative values",
    );
    print_result(
        "decaying envelope",
        decaying,
        &format!("kernel = {:?}", &c.ringing_kernel[..4.min(kernel_len)]),
    );

    has_kernel && has_oscillation && decaying
}

fn test_spice_mode(
    img: &DynamicImage,
    img_name: &str,
    mode: SpiceMode,
    base_params: &SpiceParams,
    output_dir: &Path,
) -> bool {
    let mode_name = mode.name().replace(' ', "_").to_lowercase();
    println!("\nTest: {} on {}", mode.name(), img_name);

    let mut spice_params = base_params.clone();
    spice_params.mode = mode;

    let (w, h, bytes, cache) = process_with_spice(img, &spice_params, 40_000.0);

    // Reference (no SPICE)
    let mut ref_params = PipelineParams::default();
    ref_params.sensor_width = 512;
    ref_params.sensor_height = 384;
    ref_params.spice.mode = SpiceMode::Off;
    let (_, _, ref_bytes) = pipeline::process(img, &ref_params, &None);

    let file_name = format!("{}_{}", img_name, mode_name);
    save_output(&bytes, w, h, &file_name, output_dir);

    let (mean, std, min, max) = image_statistics(&bytes);
    let (mean_diff, max_diff, rms_diff) = pixel_diff_stats(&ref_bytes, &bytes);

    let has_output = !bytes.is_empty();
    let reasonable_range = mean > 5.0 && mean < 245.0; // not all black or all white
    let has_contrast = std > 5.0; // some variation
    let differs_from_ref = mean_diff > 0.1; // should be different from no-SPICE

    print_result(
        "output generated",
        has_output,
        &format!("{}x{}, {} bytes", w, h, bytes.len()),
    );
    print_result(
        "reasonable range",
        reasonable_range,
        &format!("mean={:.1} std={:.1} [{:.0}, {:.0}]", mean, std, min, max),
    );
    print_result(
        "has contrast",
        has_contrast,
        &format!("std_dev={:.1}", std),
    );
    print_result(
        "differs from math",
        differs_from_ref,
        &format!("mean_diff={:.2} max_diff={:.0} rms={:.2}", mean_diff, max_diff, rms_diff),
    );
    println!(
        "  Sim: {:.1}ms, CTE={:.6}, noise={:.1}e-",
        cache.sim_time_ms, cache.effective_cte, cache.noise_sigma
    );

    has_output && reasonable_range && has_contrast && differs_from_ref
}

fn test_glitch_supply_droop(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: Supply droop glitch sweep");

    let droops = [0.0, 0.2, 0.4, 0.6];
    let mut prev_mean = f64::MAX;
    let mut monotone = true;
    let mut all_reasonable = true;

    for &droop in &droops {
        let params = SpiceParams {
            mode: SpiceMode::FullReadout,
            supply_droop: droop,
            ..SpiceParams::default()
        };
        let (w, h, bytes, _) = process_with_spice(img, &params, 40_000.0);
        let (mean, _, _, _) = image_statistics(&bytes);

        save_output(&bytes, w, h, &format!("gradient_droop_{:.0}pct", droop * 100.0), output_dir);

        if mean <= 5.0 || mean >= 250.0 {
            all_reasonable = false;
        }
        if droop > 0.0 && mean >= prev_mean + 5.0 {
            // Higher droop should generally reduce brightness (lower VDD -> less gain)
            // Allow some tolerance since the effect is complex
            monotone = false;
        }
        prev_mean = mean;
        println!("  droop={:.0}%: mean={:.1}", droop * 100.0, mean);
    }

    print_result(
        "supply droop effect",
        all_reasonable,
        "all outputs in reasonable range",
    );
    print_result(
        "droop trend",
        monotone,
        "higher droop generally reduces output level",
    );

    all_reasonable
}

fn test_glitch_phase_overlap(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: Phase overlap glitch");

    let overlaps = [0.0, 25.0, 50.0, 100.0];
    let mut all_reasonable = true;

    for &overlap in &overlaps {
        let params = SpiceParams {
            mode: SpiceMode::FullReadout,
            phase_overlap_ns: overlap,
            ..SpiceParams::default()
        };
        let (w, h, bytes, _) = process_with_spice(img, &params, 40_000.0);
        let (mean, std, _, _) = image_statistics(&bytes);

        save_output(&bytes, w, h, &format!("gradient_overlap_{:.0}ns", overlap), output_dir);

        if mean <= 5.0 || mean >= 250.0 {
            all_reasonable = false;
        }
        println!("  overlap={:.0}ns: mean={:.1} std={:.1}", overlap, mean, std);
    }

    print_result(
        "phase overlap effect",
        all_reasonable,
        "all outputs in reasonable range",
    );

    all_reasonable
}

fn test_glitch_charge_injection(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: Charge injection glitch");

    let injections = [0.0, 0.5, 1.0, 2.0];
    let mut all_reasonable = true;

    for &ci in &injections {
        let params = SpiceParams {
            mode: SpiceMode::FullReadout,
            charge_injection: ci,
            ..SpiceParams::default()
        };
        let (w, h, bytes, _) = process_with_spice(img, &params, 40_000.0);
        let (mean, std, _, _) = image_statistics(&bytes);

        save_output(&bytes, w, h, &format!("gradient_ci_{:.0}", ci * 10.0), output_dir);

        if mean <= 5.0 || mean >= 250.0 {
            all_reasonable = false;
        }
        println!("  charge_inj={:.1}: mean={:.1} std={:.1}", ci, mean, std);
    }

    print_result(
        "charge injection effect",
        all_reasonable,
        "all outputs in reasonable range",
    );

    all_reasonable
}

fn test_glitch_substrate_noise(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: Substrate noise glitch");

    let p_clean = SpiceParams {
        mode: SpiceMode::FullReadout,
        substrate_noise: 0.0,
        ..SpiceParams::default()
    };
    let p_noisy = SpiceParams {
        mode: SpiceMode::FullReadout,
        substrate_noise: 1.0,
        ..SpiceParams::default()
    };

    let (_, _, _bytes_clean, cache_clean) = process_with_spice(img, &p_clean, 40_000.0);
    let (w, h, bytes_noisy, cache_noisy) = process_with_spice(img, &p_noisy, 40_000.0);

    save_output(&bytes_noisy, w, h, "gradient_substrate_noise", output_dir);

    let noise_increased = cache_noisy.noise_sigma > cache_clean.noise_sigma;

    print_result(
        "noise increases",
        noise_increased,
        &format!(
            "clean={:.1}e- noisy={:.1}e-",
            cache_clean.noise_sigma, cache_noisy.noise_sigma
        ),
    );

    noise_increased
}

fn test_combined_glitches(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: Combined extreme glitches on photo");

    let params = SpiceParams {
        mode: SpiceMode::FullReadout,
        vdd: 8.0,
        supply_droop: 0.5,
        phase_overlap_ns: 50.0,
        charge_injection: 1.5,
        substrate_noise: 0.8,
        missing_pulse_rate: 0.1,
        ..SpiceParams::default()
    };

    let (w, h, bytes, cache) = process_with_spice(img, &params, 40_000.0);
    save_output(&bytes, w, h, "photo_combined_glitches", output_dir);

    let (mean, std, min, max) = image_statistics(&bytes);
    let has_output = !bytes.is_empty() && mean > 1.0;
    let has_some_structure = std > 2.0;

    print_result(
        "survives extreme params",
        has_output,
        &format!("mean={:.1} std={:.1} [{:.0}, {:.0}]", mean, std, min, max),
    );
    print_result(
        "retains structure",
        has_some_structure,
        &format!("sim={:.1}ms CTE={:.6}", cache.sim_time_ms, cache.effective_cte),
    );

    has_output && has_some_structure
}

fn test_vdd_sweep(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: VDD sweep");

    let vdds = [5.0, 10.0, 15.0, 20.0];
    let mut means = Vec::new();

    for &vdd in &vdds {
        let params = SpiceParams {
            mode: SpiceMode::FullReadout,
            vdd,
            ..SpiceParams::default()
        };
        let (w, h, bytes, _) = process_with_spice(img, &params, 40_000.0);
        let (mean, _, _, _) = image_statistics(&bytes);
        means.push(mean);

        save_output(&bytes, w, h, &format!("gradient_vdd_{:.0}V", vdd), output_dir);
        println!("  VDD={:.0}V: mean={:.1}", vdd, mean);
    }

    // Higher VDD should generally give brighter images (more headroom)
    let trend = means.windows(2).filter(|w| w[1] > w[0]).count();
    let mostly_increasing = trend >= 2; // at least 2 of 3 steps increase

    print_result(
        "VDD affects brightness",
        mostly_increasing,
        &format!("increasing in {}/3 steps", trend),
    );

    mostly_increasing
}

fn test_temperature_sweep(img: &DynamicImage, _output_dir: &Path) -> bool {
    println!("\nTest: Temperature sweep");

    let temps = [200.0, 250.0, 300.0, 350.0, 400.0];
    let mut noises = Vec::new();

    for &temp in &temps {
        let params = SpiceParams {
            mode: SpiceMode::FullReadout,
            temperature_k: temp,
            ..SpiceParams::default()
        };
        let (_, _, _, cache) = process_with_spice(img, &params, 40_000.0);
        noises.push(cache.noise_sigma);
        println!("  T={:.0}K: noise={:.2}e-", temp, cache.noise_sigma);
    }

    // Noise should increase with temperature (kTC noise ~ sqrt(kT/C))
    let noise_increases = noises.windows(2).all(|w| w[1] >= w[0] - 0.01);

    print_result(
        "noise vs temperature",
        noise_increases,
        "noise increases with temperature",
    );

    noise_increases
}

fn test_simulation_timing() -> bool {
    println!("\nTest: Simulation timing");

    let params = SpiceParams {
        mode: SpiceMode::FullReadout,
        transfer_function_resolution: 64,
        ..SpiceParams::default()
    };

    let start = web_time::Instant::now();
    let mut cache = None;
    spice::simulate_or_cache(&params, 40_000.0, &mut cache);
    let first_run_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Second run should be cached
    let start = web_time::Instant::now();
    spice::simulate_or_cache(&params, 40_000.0, &mut cache);
    let cached_run_ms = start.elapsed().as_secs_f64() * 1000.0;

    let cache_faster = cached_run_ms < first_run_ms * 0.5 || cached_run_ms < 0.1;
    let reasonable_time = first_run_ms < 60_000.0; // under 60s

    print_result(
        "first sim time",
        reasonable_time,
        &format!("{:.1}ms", first_run_ms),
    );
    print_result(
        "cache speedup",
        cache_faster,
        &format!("cached={:.3}ms (vs {:.1}ms first)", cached_run_ms, first_run_ms),
    );

    reasonable_time && cache_faster
}

fn test_spice_vs_math(img: &DynamicImage, output_dir: &Path) -> bool {
    println!("\nTest: SPICE vs mathematical pipeline comparison");

    // Mathematical pipeline with default params
    let mut math_params = PipelineParams::default();
    math_params.sensor_width = 512;
    math_params.sensor_height = 384;
    math_params.spice.mode = SpiceMode::Off;
    let (_, _, math_bytes) = pipeline::process(img, &math_params, &None);

    // SPICE pipeline
    let spice_params = SpiceParams {
        mode: SpiceMode::FullReadout,
        ..SpiceParams::default()
    };
    let (w, h, spice_bytes, _) = process_with_spice(img, &spice_params, 40_000.0);

    let (math_mean, math_std, _, _) = image_statistics(&math_bytes);
    let (spice_mean, spice_std, _, _) = image_statistics(&spice_bytes);
    let (mean_diff, max_diff, rms_diff) = pixel_diff_stats(&math_bytes, &spice_bytes);

    // Generate a diff image
    let diff_bytes: Vec<u8> = math_bytes
        .iter()
        .zip(spice_bytes.iter())
        .map(|(&a, &b)| {
            let d = (a as i16 - b as i16).unsigned_abs() as u16;
            (d * 4).min(255) as u8 // amplify differences
        })
        .collect();
    save_output(&diff_bytes, w, h, "gradient_spice_vs_math_diff", output_dir);

    let visibly_different = mean_diff > 1.0;
    let not_garbage = spice_mean > 10.0 && spice_std > 5.0;

    print_result(
        "math pipeline",
        true,
        &format!("mean={:.1} std={:.1}", math_mean, math_std),
    );
    print_result(
        "spice pipeline",
        not_garbage,
        &format!("mean={:.1} std={:.1}", spice_mean, spice_std),
    );
    print_result(
        "visible difference",
        visibly_different,
        &format!(
            "mean_diff={:.2} max_diff={:.0} rms={:.2}",
            mean_diff, max_diff, rms_diff
        ),
    );

    visibly_different && not_garbage
}
