use rand::Rng;

/// Apply scan line corruption: random horizontal bands with corrupted data.
/// `frequency`: 0.0 = no corruption, 1.0 = heavy corruption.
pub fn apply_scan_line_corruption(
    grid: &mut [f64],
    width: usize,
    height: usize,
    frequency: f64,
    max_value: f64,
) {
    if frequency <= 0.0 {
        return;
    }
    let mut rng = rand::rng();

    let num_bands = (height as f64 * frequency * 0.05).ceil() as usize;

    for _ in 0..num_bands {
        let band_y = rng.random_range(0..height);
        let band_h = rng.random_range(1..((height as f64 * 0.02).ceil() as usize + 2));
        let corruption_type = rng.random_range(0u32..5);

        for dy in 0..band_h {
            let y = band_y + dy;
            if y >= height {
                break;
            }
            let row_start = y * width;

            match corruption_type {
                0 => {
                    // Zero out the band
                    for x in 0..width {
                        grid[row_start + x] = 0.0;
                    }
                }
                1 => {
                    // Max out the band
                    for x in 0..width {
                        grid[row_start + x] = max_value;
                    }
                }
                2 => {
                    // Random noise fill
                    for x in 0..width {
                        grid[row_start + x] = rng.random::<f64>() * max_value;
                    }
                }
                3 => {
                    // Copy from a random other row
                    let src_y = rng.random_range(0..height);
                    let src_start = src_y * width;
                    for x in 0..width {
                        grid[row_start + x] = grid[src_start + x];
                    }
                }
                4 => {
                    // Invert the band
                    for x in 0..width {
                        grid[row_start + x] = max_value - grid[row_start + x];
                    }
                }
                _ => {}
            }
        }
    }
}
