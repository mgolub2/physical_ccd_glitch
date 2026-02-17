use image::RgbImage;
use rand_distr::{Distribution, Normal, Poisson};

/// Convert an RGB image to a 3-channel electron grid.
/// Each pixel's channel value is scaled by full_well_capacity.
pub fn image_to_electrons(img: &RgbImage, full_well: f64) -> (Vec<[f64; 3]>, usize, usize) {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let mut electrons = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x as u32, y as u32);
            electrons.push([
                (p[0] as f64 / 255.0) * full_well,
                (p[1] as f64 / 255.0) * full_well,
                (p[2] as f64 / 255.0) * full_well,
            ]);
        }
    }
    (electrons, w, h)
}

/// Add dark current noise (Poisson-distributed).
/// `dark_rate` is in electrons (already scaled by temperature/exposure).
pub fn add_dark_current(grid: &mut [f64], dark_rate: f64) {
    if dark_rate <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    let dist = Poisson::new(dark_rate).unwrap_or_else(|_| Poisson::new(1.0).unwrap());
    for pixel in grid.iter_mut() {
        let dark: f64 = dist.sample(&mut rng);
        *pixel += dark;
    }
}

/// Add photon shot noise (replace signal with Poisson sample of that signal).
pub fn add_shot_noise(grid: &mut [f64]) {
    let mut rng = rand::rng();
    for pixel in grid.iter_mut() {
        if *pixel > 0.0 {
            let lambda = (*pixel).min(1e8); // cap to avoid overflow
            if lambda < 1e6 {
                if let Ok(dist) = Poisson::new(lambda) {
                    *pixel = dist.sample(&mut rng);
                }
            } else {
                // For very large values, use Gaussian approximation
                let sigma = lambda.sqrt();
                let normal = Normal::new(lambda, sigma).unwrap();
                *pixel = normal.sample(&mut rng).max(0.0);
            }
        }
    }
}

/// Add read noise (Gaussian-distributed).
pub fn add_read_noise(grid: &mut [f64], sigma: f64) {
    if sigma <= 0.0 {
        return;
    }
    let mut rng = rand::rng();
    let dist = Normal::new(0.0, sigma).unwrap();
    for pixel in grid.iter_mut() {
        *pixel += dist.sample(&mut rng);
        if *pixel < 0.0 {
            *pixel = 0.0;
        }
    }
}
