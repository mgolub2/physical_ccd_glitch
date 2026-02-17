use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb, RgbImage};
use std::path::Path;

pub fn load_image(path: &Path) -> Result<DynamicImage, String> {
    image::open(path).map_err(|e| format!("Failed to load image: {e}"))
}

/// Resize image to fit within sensor dimensions, preserving aspect ratio.
/// Letterboxes/pillarboxes remaining area with black.
pub fn resize_to_sensor(img: &DynamicImage, sensor_w: u32, sensor_h: u32) -> RgbImage {
    let (iw, ih) = img.dimensions();
    let scale = f64::min(
        sensor_w as f64 / iw as f64,
        sensor_h as f64 / ih as f64,
    );
    let new_w = (iw as f64 * scale).round() as u32;
    let new_h = (ih as f64 * scale).round() as u32;

    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
    let resized_rgb = resized.to_rgb8();

    let mut output = ImageBuffer::from_pixel(sensor_w, sensor_h, Rgb([0u8, 0, 0]));
    let offset_x = (sensor_w.saturating_sub(new_w)) / 2;
    let offset_y = (sensor_h.saturating_sub(new_h)) / 2;

    for y in 0..new_h {
        for x in 0..new_w {
            let pixel = resized_rgb.get_pixel(x, y);
            if x + offset_x < sensor_w && y + offset_y < sensor_h {
                output.put_pixel(x + offset_x, y + offset_y, *pixel);
            }
        }
    }
    output
}

pub fn save_image(img: &RgbImage, path: &Path) -> Result<(), String> {
    img.save(path).map_err(|e| format!("Failed to save image: {e}"))
}
