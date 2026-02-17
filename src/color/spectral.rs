/// Apply white balance: multiply each channel by its respective multiplier.
pub fn apply_white_balance(rgb: &mut [[f64; 3]], wb_r: f64, wb_g: f64, wb_b: f64) {
    for pixel in rgb.iter_mut() {
        pixel[0] *= wb_r;
        pixel[1] *= wb_g;
        pixel[2] *= wb_b;
    }
}

/// Apply sRGB gamma correction (linear → gamma-compressed).
/// Standard sRGB transfer function with linear toe.
pub fn apply_gamma(rgb: &mut [[f64; 3]], gamma: f64) {
    if gamma <= 0.0 {
        return;
    }
    let inv_gamma = 1.0 / gamma;
    for pixel in rgb.iter_mut() {
        for c in 0..3 {
            let v = pixel[c].clamp(0.0, 1.0);
            // sRGB-like: linear toe below threshold
            pixel[c] = if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(inv_gamma) - 0.055
            };
        }
    }
}

/// Apply brightness and contrast adjustment.
/// brightness: -1.0 to 1.0 (added to normalized value)
/// contrast: 0.0 to 3.0 (multiplied around midpoint 0.5)
pub fn apply_brightness_contrast(rgb: &mut [[f64; 3]], brightness: f64, contrast: f64) {
    for pixel in rgb.iter_mut() {
        for c in 0..3 {
            let mut v = pixel[c];
            // Contrast: scale around 0.5
            v = (v - 0.5) * contrast + 0.5;
            // Brightness: shift
            v += brightness;
            pixel[c] = v.clamp(0.0, 1.0);
        }
    }
}

/// Convert floating-point RGB [0..1] to 8-bit sRGB image buffer.
pub fn rgb_to_bytes(rgb: &[[f64; 3]], width: usize, height: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(width * height * 3);
    for pixel in rgb.iter() {
        bytes.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    bytes
}
