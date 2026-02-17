/// Simulate blooming: excess charge spills vertically (or horizontally).
///
/// - `abg_strength`: 0.0 = no anti-blooming drain (full bloom), 1.0 = perfect drain (no bloom)
/// - `bloom_threshold`: fraction of full_well at which blooming starts (0.0 to 1.0)
/// - `full_well`: full well capacity in electrons
/// - `vertical`: if true, bloom vertically (column direction); if false, horizontally
pub fn apply_blooming(
    grid: &mut [f64],
    width: usize,
    height: usize,
    full_well: f64,
    abg_strength: f64,
    bloom_threshold: f64,
    vertical: bool,
) {
    let threshold = full_well * bloom_threshold.clamp(0.0, 1.0);
    let drain_fraction = abg_strength.clamp(0.0, 1.0);

    if vertical {
        bloom_vertical(grid, width, height, threshold, full_well, drain_fraction);
    } else {
        bloom_horizontal(grid, width, height, threshold, full_well, drain_fraction);
    }
}

fn bloom_vertical(
    grid: &mut [f64],
    width: usize,
    height: usize,
    threshold: f64,
    full_well: f64,
    drain_fraction: f64,
) {
    // Process each column independently
    for x in 0..width {
        // Multiple passes to propagate overflow
        for _pass in 0..3 {
            for y in 0..height {
                let idx = y * width + x;
                if grid[idx] > threshold {
                    let excess = grid[idx] - threshold;
                    let drained = excess * drain_fraction;
                    let spill = excess - drained;
                    grid[idx] = threshold;

                    if spill > 0.0 {
                        // Split spill between upper and lower neighbors
                        let spill_each = spill * 0.5;
                        if y > 0 {
                            let above = (y - 1) * width + x;
                            grid[above] = (grid[above] + spill_each).min(full_well);
                        }
                        if y + 1 < height {
                            let below = (y + 1) * width + x;
                            grid[below] = (grid[below] + spill_each).min(full_well);
                        }
                    }
                }
            }
        }
    }
}

fn bloom_horizontal(
    grid: &mut [f64],
    width: usize,
    height: usize,
    threshold: f64,
    full_well: f64,
    drain_fraction: f64,
) {
    for y in 0..height {
        for _pass in 0..3 {
            for x in 0..width {
                let idx = y * width + x;
                if grid[idx] > threshold {
                    let excess = grid[idx] - threshold;
                    let drained = excess * drain_fraction;
                    let spill = excess - drained;
                    grid[idx] = threshold;

                    if spill > 0.0 {
                        let spill_each = spill * 0.5;
                        if x > 0 {
                            grid[idx - 1] = (grid[idx - 1] + spill_each).min(full_well);
                        }
                        if x + 1 < width {
                            grid[idx + 1] = (grid[idx + 1] + spill_each).min(full_well);
                        }
                    }
                }
            }
        }
    }
}
