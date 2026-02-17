#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelSwap {
    None,
    Rg,
    Rb,
    Gb,
    RgbBrg,
    RgbGbr,
}

impl ChannelSwap {
    pub const ALL: &[ChannelSwap] = &[
        ChannelSwap::None,
        ChannelSwap::Rg,
        ChannelSwap::Rb,
        ChannelSwap::Gb,
        ChannelSwap::RgbBrg,
        ChannelSwap::RgbGbr,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ChannelSwap::None => "None",
            ChannelSwap::Rg => "R <-> G",
            ChannelSwap::Rb => "R <-> B",
            ChannelSwap::Gb => "G <-> B",
            ChannelSwap::RgbBrg => "RGB -> BRG",
            ChannelSwap::RgbGbr => "RGB -> GBR",
        }
    }
}

/// Apply per-channel gain and offset.
pub fn apply_channel_gain_offset(
    rgb: &mut [[f64; 3]],
    r_gain: f64,
    g_gain: f64,
    b_gain: f64,
    r_offset: f64,
    g_offset: f64,
    b_offset: f64,
) {
    for pixel in rgb.iter_mut() {
        pixel[0] = pixel[0] * r_gain + r_offset;
        pixel[1] = pixel[1] * g_gain + g_offset;
        pixel[2] = pixel[2] * b_gain + b_offset;
    }
}

/// Apply channel swap.
pub fn apply_channel_swap(rgb: &mut [[f64; 3]], swap: ChannelSwap) {
    match swap {
        ChannelSwap::None => {}
        ChannelSwap::Rg => {
            for pixel in rgb.iter_mut() {
                let tmp = pixel[0];
                pixel[0] = pixel[1];
                pixel[1] = tmp;
            }
        }
        ChannelSwap::Rb => {
            for pixel in rgb.iter_mut() {
                let tmp = pixel[0];
                pixel[0] = pixel[2];
                pixel[2] = tmp;
            }
        }
        ChannelSwap::Gb => {
            for pixel in rgb.iter_mut() {
                let tmp = pixel[1];
                pixel[1] = pixel[2];
                pixel[2] = tmp;
            }
        }
        ChannelSwap::RgbBrg => {
            for pixel in rgb.iter_mut() {
                let [r, g, b] = *pixel;
                *pixel = [b, r, g];
            }
        }
        ChannelSwap::RgbGbr => {
            for pixel in rgb.iter_mut() {
                let [r, g, b] = *pixel;
                *pixel = [g, b, r];
            }
        }
    }
}

/// Apply chromatic aberration simulation by offsetting color channels spatially.
pub fn apply_chromatic_aberration(
    rgb: &mut [[f64; 3]],
    width: usize,
    height: usize,
    r_offset_x: i32,
    r_offset_y: i32,
    b_offset_x: i32,
    b_offset_y: i32,
) {
    if r_offset_x == 0 && r_offset_y == 0 && b_offset_x == 0 && b_offset_y == 0 {
        return;
    }

    let original: Vec<[f64; 3]> = rgb.to_vec();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            // Red channel from offset position
            let rx = (x as i32 + r_offset_x).clamp(0, width as i32 - 1) as usize;
            let ry = (y as i32 + r_offset_y).clamp(0, height as i32 - 1) as usize;
            rgb[idx][0] = original[ry * width + rx][0];

            // Green stays in place

            // Blue channel from offset position
            let bx = (x as i32 + b_offset_x).clamp(0, width as i32 - 1) as usize;
            let by = (y as i32 + b_offset_y).clamp(0, height as i32 - 1) as usize;
            rgb[idx][2] = original[by * width + bx][2];
        }
    }
}
