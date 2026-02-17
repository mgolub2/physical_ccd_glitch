use rand::Rng;

/// Simulate vertical (parallel) charge transfer.
///
/// Processes row-by-row from bottom to top, simulating the physical transfer
/// of charge toward the serial register.
pub fn vertical_transfer(
    grid: &mut [f64],
    width: usize,
    height: usize,
    cte: f64,
    glitch_rate: f64,
    waveform_distortion: f64,
    parallel_smear: f64,
) {
    let mut rng = rand::rng();
    let cti = 1.0 - cte.clamp(0.0, 1.0);

    // Simulate charge trailing from CTE loss
    if cti > 0.0 {
        for x in 0..width {
            for y in (1..height).rev() {
                let idx = y * width + x;
                let prev_idx = (y - 1) * width + x;
                let lost = grid[idx] * cti;
                grid[idx] -= lost;
                grid[prev_idx] += lost;
            }
        }
    }

    // Parallel smear: fraction of charge left behind during transfer
    if parallel_smear > 0.0 {
        for x in 0..width {
            let mut column_sum = 0.0;
            for y in 0..height {
                column_sum += grid[y * width + x];
            }
            let smear_per_pixel = (column_sum / height as f64) * parallel_smear;
            for y in 0..height {
                grid[y * width + x] += smear_per_pixel;
            }
        }
    }

    // Waveform distortion: sinusoidal modulation of transfer amounts
    if waveform_distortion > 0.0 {
        for y in 0..height {
            let phase = (y as f64 / height as f64) * std::f64::consts::TAU * 4.0;
            let modulation = 1.0 + waveform_distortion * phase.sin();
            for x in 0..width {
                grid[y * width + x] *= modulation.max(0.0);
            }
        }
    }

    // V-clock glitches: random per-row faults
    if glitch_rate > 0.0 {
        let mut temp_row = vec![0.0f64; width];
        for y in 0..height {
            if rng.random::<f64>() < glitch_rate {
                let glitch_type = rng.random_range(0u32..4);
                match glitch_type {
                    0 => {
                        // Row skip: copy from adjacent row
                        let src_y = if y > 0 { y - 1 } else { y + 1 }.min(height - 1);
                        for x in 0..width {
                            grid[y * width + x] = grid[src_y * width + x];
                        }
                    }
                    1 => {
                        // Row repeat: duplicate this row to next
                        if y + 1 < height {
                            for x in 0..width {
                                grid[(y + 1) * width + x] = grid[y * width + x];
                            }
                        }
                    }
                    2 => {
                        // Row reverse
                        for x in 0..width {
                            temp_row[x] = grid[y * width + x];
                        }
                        for x in 0..width {
                            grid[y * width + x] = temp_row[width - 1 - x];
                        }
                    }
                    3 => {
                        // Row horizontal shift
                        let shift = rng.random_range(1..width.max(2).min(64));
                        for x in 0..width {
                            temp_row[x] = grid[y * width + x];
                        }
                        for x in 0..width {
                            grid[y * width + x] = temp_row[(x + width - shift) % width];
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadoutDirection {
    LeftToRight,
    RightToLeft,
    Alternating,
}

/// Simulate horizontal (serial) charge transfer.
pub fn horizontal_transfer(
    grid: &mut [f64],
    width: usize,
    height: usize,
    cte: f64,
    glitch_rate: f64,
    ringing: f64,
    direction: ReadoutDirection,
) {
    let mut rng = rand::rng();
    let cti = 1.0 - cte.clamp(0.0, 1.0);

    for y in 0..height {
        let left_to_right = match direction {
            ReadoutDirection::LeftToRight => true,
            ReadoutDirection::RightToLeft => false,
            ReadoutDirection::Alternating => y % 2 == 0,
        };

        // CTE trailing in readout direction
        if cti > 0.0 {
            if left_to_right {
                for x in (1..width).rev() {
                    let idx = y * width + x;
                    let prev = y * width + x - 1;
                    let lost = grid[idx] * cti;
                    grid[idx] -= lost;
                    grid[prev] += lost;
                }
            } else {
                for x in 0..width.saturating_sub(1) {
                    let idx = y * width + x;
                    let next = y * width + x + 1;
                    let lost = grid[idx] * cti;
                    grid[idx] -= lost;
                    grid[next] += lost;
                }
            }
        }

        // Ringing: damped oscillation after bright pixels
        if ringing > 0.0 {
            let row_start = y * width;
            let mut ring_energy = 0.0f64;
            if left_to_right {
                for x in 0..width {
                    let idx = row_start + x;
                    let bright = grid[idx] > 10000.0;
                    if bright {
                        ring_energy = grid[idx] * ringing * 0.01;
                    }
                    if ring_energy.abs() > 0.1 {
                        grid[idx] += ring_energy;
                        ring_energy *= -0.7; // damped oscillation
                    }
                }
            } else {
                for x in (0..width).rev() {
                    let idx = row_start + x;
                    let bright = grid[idx] > 10000.0;
                    if bright {
                        ring_energy = grid[idx] * ringing * 0.01;
                    }
                    if ring_energy.abs() > 0.1 {
                        grid[idx] += ring_energy;
                        ring_energy *= -0.7;
                    }
                }
            }
        }

        // H-clock glitches
        if glitch_rate > 0.0 {
            for x in 0..width {
                if rng.random::<f64>() < glitch_rate {
                    let idx = y * width + x;
                    let glitch_type = rng.random_range(0u32..3);
                    match glitch_type {
                        0 => {
                            // Pixel skip: replace with neighbor
                            let src_x = if x > 0 { x - 1 } else { x + 1 }.min(width - 1);
                            grid[idx] = grid[y * width + src_x];
                        }
                        1 => {
                            // Pixel repeat
                            if x + 1 < width {
                                grid[y * width + x + 1] = grid[idx];
                            }
                        }
                        2 => {
                            // Pixel offset: shift value from a nearby pixel
                            let offset = rng.random_range(1..8.min(width));
                            let src_x = (x + offset) % width;
                            grid[idx] = grid[y * width + src_x];
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
