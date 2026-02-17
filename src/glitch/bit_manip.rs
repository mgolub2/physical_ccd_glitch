use rand::Rng;

/// Apply bit-plane XOR patterns.
/// `xor_mask`: bitmask of which bit planes to XOR with a pattern.
pub fn apply_bit_xor(grid: &mut [f64], max_code: f64, xor_mask: u16) {
    if xor_mask == 0 {
        return;
    }
    for pixel in grid.iter_mut() {
        let code = (*pixel).clamp(0.0, max_code) as u16;
        let result = code ^ xor_mask;
        *pixel = (result as f64).min(max_code);
    }
}

/// Apply bit rotation: rotate bits by `amount` positions.
pub fn apply_bit_rotation(grid: &mut [f64], bit_depth: u8, amount: i32) {
    if amount == 0 {
        return;
    }
    let mask = ((1u32 << bit_depth) - 1) as u16;
    let shift = ((amount % bit_depth as i32) + bit_depth as i32) as u32 % bit_depth as u32;

    for pixel in grid.iter_mut() {
        let code = (*pixel).clamp(0.0, mask as f64) as u16;
        let rotated = ((code << shift) | (code >> (bit_depth as u32 - shift))) & mask;
        *pixel = rotated as f64;
    }
}

/// Apply random bit-plane swaps: swap two bit planes across the image.
pub fn apply_bit_plane_swap(grid: &mut [f64], bit_depth: u8, swap_count: u32) {
    if swap_count == 0 {
        return;
    }
    let mut rng = rand::rng();
    let max_code = ((1u64 << bit_depth) - 1) as f64;

    for _ in 0..swap_count {
        let bit_a = rng.random_range(0..bit_depth);
        let bit_b = rng.random_range(0..bit_depth);
        if bit_a == bit_b {
            continue;
        }

        for pixel in grid.iter_mut() {
            let mut code = (*pixel).clamp(0.0, max_code) as u32;
            let a_val = (code >> bit_a) & 1;
            let b_val = (code >> bit_b) & 1;
            if a_val != b_val {
                code ^= (1 << bit_a) | (1 << bit_b);
            }
            *pixel = (code as f64).min(max_code);
        }
    }
}
