use rand::Rng;
use rand_distr::{Distribution, Normal};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CdsMode {
    On,
    Off,
    Partial,
}

/// Simulate ADC conversion: voltage → digital counts.
pub fn apply_adc(
    grid: &mut [f64],
    _width: usize,
    _height: usize,
    bit_depth: u8,
    cds_mode: CdsMode,
    adc_gain: f64,
    bias: f64,
    reset_noise_sigma: f64,
    dnl_errors: f64,
    bit_errors: f64,
    jitter: f64,
) {
    let mut rng = rand::rng();
    let max_code = ((1u64 << bit_depth) - 1) as f64;

    // Pre-generate DNL lookup if needed
    let dnl_table = if dnl_errors > 0.0 {
        generate_dnl_table(bit_depth, dnl_errors, &mut rng)
    } else {
        Vec::new()
    };

    for pixel in grid.iter_mut() {
        let mut val = *pixel;

        // CDS: remove (or partially remove) reset noise
        match cds_mode {
            CdsMode::On => {
                // CDS removes reset noise — no extra noise added
            }
            CdsMode::Off => {
                // Without CDS, reset noise dominates
                if reset_noise_sigma > 0.0 {
                    let noise = Normal::new(0.0, reset_noise_sigma).unwrap();
                    val += noise.sample(&mut rng);
                }
            }
            CdsMode::Partial => {
                // Partial CDS: some reset noise leaks through
                if reset_noise_sigma > 0.0 {
                    let noise = Normal::new(0.0, reset_noise_sigma * 0.3).unwrap();
                    val += noise.sample(&mut rng);
                }
            }
        }

        // ADC jitter: random timing variation smears the digitization
        if jitter > 0.0 {
            let jitter_noise = Normal::new(0.0, jitter).unwrap();
            val += jitter_noise.sample(&mut rng);
        }

        // Apply ADC gain (electrons per ADU) and bias
        val = val / adc_gain.max(0.001) + bias;

        // Quantize to integer codes
        val = val.round().clamp(0.0, max_code);

        // Apply DNL (differential nonlinearity) errors
        if !dnl_table.is_empty() {
            let code = val as usize;
            if code < dnl_table.len() {
                val = dnl_table[code] as f64;
            }
        }

        // Apply bit errors: random flips in specific bit planes
        if bit_errors > 0.0 {
            let mut code = val as u64;
            for bit in 0..bit_depth {
                if rng.random::<f64>() < bit_errors * 0.01 {
                    code ^= 1 << bit;
                }
            }
            val = (code as f64).min(max_code);
        }

        *pixel = val;
    }
}

/// Generate a DNL error lookup table.
/// Maps ideal code → actual code (with missing/doubled codes).
fn generate_dnl_table(bit_depth: u8, strength: f64, rng: &mut impl Rng) -> Vec<u32> {
    let num_codes = 1usize << bit_depth;
    let mut table: Vec<u32> = (0..num_codes as u32).collect();

    // Randomly perturb some codes
    let num_errors = (num_codes as f64 * strength * 0.01).ceil() as usize;
    for _ in 0..num_errors {
        let idx = rng.random_range(1..num_codes);
        let offset: i32 = if rng.random::<bool>() { 1 } else { -1 };
        table[idx] = (table[idx] as i32 + offset).clamp(0, num_codes as i32 - 1) as u32;
    }
    table
}
