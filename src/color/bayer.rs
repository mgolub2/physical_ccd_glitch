#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BayerPattern {
    Rggb,
    Bggr,
    Grbg,
    Gbrg,
}

impl BayerPattern {
    pub const ALL: &[BayerPattern] = &[
        BayerPattern::Rggb,
        BayerPattern::Bggr,
        BayerPattern::Grbg,
        BayerPattern::Gbrg,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BayerPattern::Rggb => "RGGB",
            BayerPattern::Bggr => "BGGR",
            BayerPattern::Grbg => "GRBG",
            BayerPattern::Gbrg => "GBRG",
        }
    }

    /// Returns the color channel index (0=R, 1=G, 2=B) at position (x, y).
    pub fn channel_at(self, x: usize, y: usize) -> usize {
        let xm = x % 2;
        let ym = y % 2;
        match self {
            BayerPattern::Rggb => match (xm, ym) {
                (0, 0) => 0,
                (1, 0) => 1,
                (0, 1) => 1,
                (1, 1) => 2,
                _ => unreachable!(),
            },
            BayerPattern::Bggr => match (xm, ym) {
                (0, 0) => 2,
                (1, 0) => 1,
                (0, 1) => 1,
                (1, 1) => 0,
                _ => unreachable!(),
            },
            BayerPattern::Grbg => match (xm, ym) {
                (0, 0) => 1,
                (1, 0) => 0,
                (0, 1) => 2,
                (1, 1) => 1,
                _ => unreachable!(),
            },
            BayerPattern::Gbrg => match (xm, ym) {
                (0, 0) => 1,
                (1, 0) => 2,
                (0, 1) => 0,
                (1, 1) => 1,
                _ => unreachable!(),
            },
        }
    }
}

/// Apply Bayer CFA: convert 3-channel electron grid to single-channel mosaic.
/// Each pixel retains only the channel matching its Bayer position.
pub fn apply_bayer(
    rgb_electrons: &[[f64; 3]],
    width: usize,
    height: usize,
    pattern: BayerPattern,
) -> Vec<f64> {
    let mut mosaic = vec![0.0f64; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let ch = pattern.channel_at(x, y);
            mosaic[idx] = rgb_electrons[idx][ch];
        }
    }
    mosaic
}
