use rand_distr::{Distribution, Normal};

/// Apply output amplifier simulation.
///
/// Converts electrons to voltage-like values, applies gain/nonlinearity/noise.
pub fn apply_amplifier(
    grid: &mut [f64],
    width: usize,
    height: usize,
    gain: f64,
    nonlinearity: f64,
    reset_noise: f64,
    amp_glow: f64,
) {
    let mut rng = rand::rng();

    // Find max value for normalization in nonlinearity
    let max_val = grid.iter().cloned().fold(0.0f64, f64::max).max(1.0);

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut val = grid[idx];

            // Apply gain (linear scaling)
            val *= gain;

            // Apply nonlinearity: polynomial distortion
            // 0.0 = perfectly linear, higher = more S-curve compression
            if nonlinearity > 0.0 {
                let normalized = (val / (max_val * gain)).clamp(0.0, 1.0);
                let curved = apply_s_curve(normalized, nonlinearity);
                val = curved * max_val * gain;
            }

            // Reset noise (kTC): random offset per pixel
            if reset_noise > 0.0 {
                let noise_dist = Normal::new(0.0, reset_noise).unwrap();
                val += noise_dist.sample(&mut rng);
            }

            // Amplifier glow: gradient from bottom-right corner (typical amp location)
            if amp_glow > 0.0 {
                let dx = (width as f64 - x as f64) / width as f64;
                let dy = (height as f64 - y as f64) / height as f64;
                let dist_sq = dx * dx + dy * dy;
                let glow = amp_glow * 1000.0 / (1.0 + dist_sq * 50.0);
                val += glow;
            }

            grid[idx] = val.max(0.0);
        }
    }
}

/// S-curve distortion: amount 0.0 = linear, 1.0 = strong S-curve
fn apply_s_curve(x: f64, amount: f64) -> f64 {
    let linear = x;
    // Sigmoid-like S-curve centered at 0.5
    let s = 1.0 / (1.0 + (-(x - 0.5) * (2.0 + amount * 10.0)).exp());
    // Blend between linear and S-curve
    linear * (1.0 - amount) + s * amount
}
