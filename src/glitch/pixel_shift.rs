use rand::Rng;

/// Apply horizontal pixel shift to rows/blocks.
/// `amount`: 0.0 = no shift, higher = more displacement.
pub fn apply_pixel_shift(
    grid: &mut [f64],
    width: usize,
    height: usize,
    amount: f64,
) {
    if amount <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    let max_shift = (width as f64 * amount * 0.1).ceil() as usize;
    if max_shift == 0 {
        return;
    }

    let mut temp_row = vec![0.0f64; width];

    for y in 0..height {
        // Per-row random shift with some probability
        if rng.random::<f64>() < amount.min(1.0) * 0.3 {
            let shift = rng.random_range(0..max_shift.max(1));
            let direction: bool = rng.random();
            let row_start = y * width;

            temp_row.copy_from_slice(&grid[row_start..row_start + width]);

            for x in 0..width {
                let src = if direction {
                    (x + width - shift) % width
                } else {
                    (x + shift) % width
                };
                grid[row_start + x] = temp_row[src];
            }
        }
    }
}

/// Apply block-based displacement: shift rectangular regions.
pub fn apply_block_shift(
    grid: &mut [f64],
    width: usize,
    height: usize,
    amount: f64,
) {
    if amount <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    let num_blocks = (amount * 5.0).ceil() as usize;
    let max_shift = (width as f64 * amount * 0.15).ceil() as usize;

    for _ in 0..num_blocks {
        let block_y = rng.random_range(0..height);
        let block_h = rng.random_range(1..((height as f64 * 0.1).ceil() as usize).max(2));
        let shift = rng.random_range(0..max_shift.max(1));
        let direction: bool = rng.random();

        let mut temp_row = vec![0.0f64; width];
        for dy in 0..block_h {
            let y = block_y + dy;
            if y >= height {
                break;
            }
            let row_start = y * width;
            temp_row.copy_from_slice(&grid[row_start..row_start + width]);

            for x in 0..width {
                let src = if direction {
                    (x + width - shift) % width
                } else {
                    (x + shift) % width
                };
                grid[row_start + x] = temp_row[src];
            }
        }
    }
}
