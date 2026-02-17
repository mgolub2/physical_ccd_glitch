use super::bayer::BayerPattern;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DemosaicAlgo {
    Bilinear,
    MalvarHeCutler,
}

impl DemosaicAlgo {
    pub const ALL: &[DemosaicAlgo] = &[DemosaicAlgo::Bilinear, DemosaicAlgo::MalvarHeCutler];

    pub fn name(self) -> &'static str {
        match self {
            DemosaicAlgo::Bilinear => "Bilinear",
            DemosaicAlgo::MalvarHeCutler => "Malvar-He-Cutler",
        }
    }
}

/// Demosaic a single-channel Bayer mosaic into 3-channel RGB.
pub fn demosaic(
    mosaic: &[f64],
    width: usize,
    height: usize,
    pattern: BayerPattern,
    algo: DemosaicAlgo,
) -> Vec<[f64; 3]> {
    match algo {
        DemosaicAlgo::Bilinear => demosaic_bilinear(mosaic, width, height, pattern),
        DemosaicAlgo::MalvarHeCutler => demosaic_malvar(mosaic, width, height, pattern),
    }
}

fn get(mosaic: &[f64], width: usize, height: usize, x: isize, y: isize) -> f64 {
    let cx = x.clamp(0, width as isize - 1) as usize;
    let cy = y.clamp(0, height as isize - 1) as usize;
    mosaic[cy * width + cx]
}

/// Bilinear demosaicing: simple averaging of nearest same-color neighbors.
fn demosaic_bilinear(
    mosaic: &[f64],
    width: usize,
    height: usize,
    pattern: BayerPattern,
) -> Vec<[f64; 3]> {
    let mut result = vec![[0.0f64; 3]; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let ch = pattern.channel_at(x, y);
            let ix = x as isize;
            let iy = y as isize;

            // The channel at this position is known
            result[idx][ch] = mosaic[idx];

            // Interpolate the other two channels
            for c in 0..3usize {
                if c == ch {
                    continue;
                }
                result[idx][c] = interpolate_bilinear(
                    mosaic, width, height, ix, iy, c, pattern,
                );
            }
        }
    }
    result
}

fn interpolate_bilinear(
    mosaic: &[f64],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
    target_ch: usize,
    pattern: BayerPattern,
) -> f64 {
    // Collect neighboring pixels that have the target channel
    let mut sum = 0.0;
    let mut count = 0;

    for dy in -1..=1isize {
        for dx in -1..=1isize {
            let nx = x + dx;
            let ny = y + dy;
            if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                if pattern.channel_at(nx as usize, ny as usize) == target_ch {
                    sum += mosaic[ny as usize * width + nx as usize];
                    count += 1;
                }
            }
        }
    }

    // If no neighbors found in 3x3, expand to 5x5
    if count == 0 {
        for dy in -2..=2isize {
            for dx in -2..=2isize {
                let nx = x + dx;
                let ny = y + dy;
                if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                    if pattern.channel_at(nx as usize, ny as usize) == target_ch {
                        sum += mosaic[ny as usize * width + nx as usize];
                        count += 1;
                    }
                }
            }
        }
    }

    if count > 0 { sum / count as f64 } else { 0.0 }
}

/// Malvar-He-Cutler demosaicing: bilinear with Laplacian correction.
/// Uses 5x5 kernels for higher quality edge preservation.
fn demosaic_malvar(
    mosaic: &[f64],
    width: usize,
    height: usize,
    pattern: BayerPattern,
) -> Vec<[f64; 3]> {
    let mut result = vec![[0.0f64; 3]; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let ch = pattern.channel_at(x, y);
            let ix = x as isize;
            let iy = y as isize;

            result[idx][ch] = mosaic[idx];

            match ch {
                0 => {
                    // Red pixel: need to interpolate G and B
                    result[idx][1] = malvar_g_at_rb(mosaic, width, height, ix, iy);
                    result[idx][2] = malvar_b_at_r(mosaic, width, height, ix, iy, pattern);
                }
                1 => {
                    // Green pixel: need to interpolate R and B
                    let (r, b) = malvar_rb_at_g(mosaic, width, height, ix, iy, pattern);
                    result[idx][0] = r;
                    result[idx][2] = b;
                }
                2 => {
                    // Blue pixel: need to interpolate R and G
                    result[idx][1] = malvar_g_at_rb(mosaic, width, height, ix, iy);
                    result[idx][0] = malvar_r_at_b(mosaic, width, height, ix, iy, pattern);
                }
                _ => unreachable!(),
            }
        }
    }
    result
}

/// Estimate G at an R or B pixel using Malvar-He-Cutler kernel.
fn malvar_g_at_rb(mosaic: &[f64], w: usize, h: usize, x: isize, y: isize) -> f64 {
    let g = |dx: isize, dy: isize| get(mosaic, w, h, x + dx, y + dy);
    // Kernel: [-1, 2, -1; 2, 4, 2; -1, 2, -1] / 8 applied to same-channel
    // But simplified Malvar approach:
    let val = (
        4.0 * g(0, 0)
        + 2.0 * (g(-1, 0) + g(1, 0) + g(0, -1) + g(0, 1))
        - 1.0 * (g(-2, 0) + g(2, 0) + g(0, -2) + g(0, 2))
    ) / 8.0;
    val.max(0.0)
}

/// Estimate R and B at a G pixel.
fn malvar_rb_at_g(
    mosaic: &[f64],
    w: usize,
    h: usize,
    x: isize,
    y: isize,
    pattern: BayerPattern,
) -> (f64, f64) {
    let g = |dx: isize, dy: isize| get(mosaic, w, h, x + dx, y + dy);

    // Determine if this green pixel is on a red row or blue row
    let is_red_row = {
        // Check if horizontal neighbor is red
        let left_ch = pattern.channel_at(
            (x - 1).clamp(0, w as isize - 1) as usize,
            y as usize,
        );
        let right_ch = pattern.channel_at(
            (x + 1).clamp(0, w as isize - 1) as usize,
            y as usize,
        );
        left_ch == 0 || right_ch == 0
    };

    let (r, b) = if is_red_row {
        // R is on left/right, B is above/below
        let r = (
            5.0 * g(0, 0)
            + 4.0 * (g(-1, 0) + g(1, 0))
            - 1.0 * (g(-2, 0) + g(2, 0) + g(0, -1) + g(0, 1) + g(0, -2) + g(0, 2))
            + 0.5 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        ) / 8.0;
        let b = (
            5.0 * g(0, 0)
            + 4.0 * (g(0, -1) + g(0, 1))
            - 1.0 * (g(0, -2) + g(0, 2) + g(-1, 0) + g(1, 0) + g(-2, 0) + g(2, 0))
            + 0.5 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        ) / 8.0;
        (r, b)
    } else {
        // B is on left/right, R is above/below
        let b = (
            5.0 * g(0, 0)
            + 4.0 * (g(-1, 0) + g(1, 0))
            - 1.0 * (g(-2, 0) + g(2, 0) + g(0, -1) + g(0, 1) + g(0, -2) + g(0, 2))
            + 0.5 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        ) / 8.0;
        let r = (
            5.0 * g(0, 0)
            + 4.0 * (g(0, -1) + g(0, 1))
            - 1.0 * (g(0, -2) + g(0, 2) + g(-1, 0) + g(1, 0) + g(-2, 0) + g(2, 0))
            + 0.5 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        ) / 8.0;
        (r, b)
    };
    (r.max(0.0), b.max(0.0))
}

/// Estimate B at an R pixel.
fn malvar_b_at_r(
    mosaic: &[f64],
    w: usize,
    h: usize,
    x: isize,
    y: isize,
    _pattern: BayerPattern,
) -> f64 {
    let g = |dx: isize, dy: isize| get(mosaic, w, h, x + dx, y + dy);
    // B is at diagonal positions from R
    let val = (
        6.0 * g(0, 0)
        + 2.0 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        - 1.5 * (g(-2, 0) + g(2, 0) + g(0, -2) + g(0, 2))
    ) / 8.0;
    val.max(0.0)
}

/// Estimate R at a B pixel.
fn malvar_r_at_b(
    mosaic: &[f64],
    w: usize,
    h: usize,
    x: isize,
    y: isize,
    _pattern: BayerPattern,
) -> f64 {
    let g = |dx: isize, dy: isize| get(mosaic, w, h, x + dx, y + dy);
    // R is at diagonal positions from B
    let val = (
        6.0 * g(0, 0)
        + 2.0 * (g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1))
        - 1.5 * (g(-2, 0) + g(2, 0) + g(0, -2) + g(0, 2))
    ) / 8.0;
    val.max(0.0)
}
