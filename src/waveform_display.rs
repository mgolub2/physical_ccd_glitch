use eframe::egui;

use crate::pipeline::PipelineParams;

// Oscilloscope colors
const SCOPE_BG: egui::Color32 = egui::Color32::from_rgb(6, 8, 16);
const SCOPE_GRID: egui::Color32 = egui::Color32::from_rgb(20, 30, 25);
const SCOPE_BORDER: egui::Color32 = egui::Color32::from_rgb(35, 45, 40);
const TRACE_GREEN: egui::Color32 = egui::Color32::from_rgb(0, 255, 80);
const TRACE_CYAN: egui::Color32 = egui::Color32::from_rgb(0, 190, 255);
const TRACE_YELLOW: egui::Color32 = egui::Color32::from_rgb(255, 220, 0);
const TRACE_MAGENTA: egui::Color32 = egui::Color32::from_rgb(255, 80, 200);
const LABEL_DIM: egui::Color32 = egui::Color32::from_rgb(80, 90, 80);

const NUM_PIXELS: usize = 24;
const SAMPLES_PER_PIXEL: usize = 12;
const NUM_SAMPLES: usize = NUM_PIXELS * SAMPLES_PER_PIXEL;

// Test pixel pattern (normalized 0-1 brightness) with bright/dim transitions
const TEST_PIXELS: [f32; NUM_PIXELS] = [
    0.10, 0.10, 0.15, 0.80, 0.15, 0.10, 0.10, 0.20,
    0.30, 0.40, 0.50, 0.60, 0.50, 0.40, 0.30, 0.10,
    0.10, 0.95, 0.10, 0.10, 0.10, 0.10, 0.10, 0.10,
];

pub fn draw_waveforms(ui: &mut egui::Ui, params: &PipelineParams) {
    draw_clock_panel(ui, params);
    ui.add_space(2.0);
    draw_video_panel(ui, params);
}

// --- Clock timing diagram ---

fn draw_clock_panel(ui: &mut egui::Ui, params: &PipelineParams) {
    let width = ui.available_width();
    let height = 62.0;
    let (response, painter) = ui.allocate_painter(
        egui::vec2(width, height),
        egui::Sense::hover(),
    );
    let rect = response.rect;

    draw_scope_bg(&painter, rect);

    // V-Clock: 3-phase, 8 row transfer cycles
    let v_cycles = 8;
    let v_samples = v_cycles * 30; // 30 samples per cycle
    let phase_offsets = [0.0f32, 0.33, 0.67];
    let pulse_width = 0.36;
    let colors = [TRACE_GREEN, TRACE_CYAN, TRACE_YELLOW];
    let labels = ["Φ1", "Φ2", "Φ3"];

    let band_h = (height - 4.0) / 3.0;

    for (p, (&offset, &color)) in phase_offsets.iter().zip(colors.iter()).enumerate() {
        let band_top = rect.min.y + 2.0 + p as f32 * band_h;
        let band = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 18.0, band_top),
            egui::vec2(width - 20.0, band_h),
        );

        // Label
        painter.text(
            egui::pos2(rect.min.x + 2.0, band_top + band_h / 2.0),
            egui::Align2::LEFT_CENTER,
            labels[p],
            egui::FontId::monospace(7.0),
            color.gamma_multiply(0.7),
        );

        // Generate square wave samples
        let mut samples = vec![0.0f32; v_samples];
        for i in 0..v_samples {
            let cycle_idx = i / 30;
            let t = (i % 30) as f32 / 30.0;
            let phase_t = (t - offset + 1.0) % 1.0;
            let is_high = phase_t < pulse_width;

            let mut amp = 1.0f32;

            // Waveform distortion: sinusoidal amplitude modulation
            if params.v_waveform_distortion > 0.0 {
                let mod_phase = cycle_idx as f32 / v_cycles as f32 * std::f32::consts::TAU * 4.0;
                amp *= 1.0 + params.v_waveform_distortion as f32 * 0.4 * mod_phase.sin();
            }

            // Glitch: skip or double certain pulses
            let glitched = params.v_glitch_rate > 0.0
                && (cycle_idx == 3 || cycle_idx == 6)
                && params.v_glitch_rate as f32 > 0.05;

            if glitched && p == 0 {
                samples[i] = 0.0; // Phase 1 drops out during glitch
            } else if glitched && p == 1 {
                samples[i] = amp.max(0.0); // Phase 2 stays high during glitch
            } else {
                samples[i] = if is_high { amp.max(0.0) } else { 0.0 };
            }
        }

        draw_digital_trace(&painter, band, &samples, color);
    }
}

// --- Video output (analog + ADC) ---

fn draw_video_panel(ui: &mut egui::Ui, params: &PipelineParams) {
    let width = ui.available_width();
    let height = 80.0;
    let (response, painter) = ui.allocate_painter(
        egui::vec2(width, height),
        egui::Sense::hover(),
    );
    let rect = response.rect;

    draw_scope_bg(&painter, rect);

    let (analog, digital) = generate_video_signal(params);

    let trace_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 2.0, rect.min.y + 10.0),
        egui::vec2(width - 4.0, height - 14.0),
    );

    // Draw analog trace
    draw_analog_trace(&painter, trace_rect, &analog, TRACE_GREEN, 1.2);

    // Draw digital (ADC) trace if different from analog
    if params.bit_depth < 16 || params.dnl_errors > 0.0 || params.bit_errors > 0.0 {
        draw_analog_trace(&painter, trace_rect, &digital, TRACE_CYAN.gamma_multiply(0.6), 1.0);
    }

    // Labels
    painter.text(
        egui::pos2(rect.min.x + 3.0, rect.min.y + 2.0),
        egui::Align2::LEFT_TOP,
        "VIDEO OUT",
        egui::FontId::monospace(7.0),
        LABEL_DIM,
    );

    if params.bit_depth < 16 || params.dnl_errors > 0.0 || params.bit_errors > 0.0 {
        painter.text(
            egui::pos2(rect.max.x - 3.0, rect.min.y + 2.0),
            egui::Align2::RIGHT_TOP,
            &format!("ADC {}bit", params.bit_depth),
            egui::FontId::monospace(7.0),
            TRACE_CYAN.gamma_multiply(0.5),
        );
    }

    // Pixel markers at bottom
    let marker_y = rect.max.y - 2.0;
    let px_width = trace_rect.width() / NUM_PIXELS as f32;
    for px in 0..NUM_PIXELS {
        if TEST_PIXELS[px] > 0.5 {
            let x = trace_rect.min.x + (px as f32 + 0.5) * px_width;
            painter.circle_filled(egui::pos2(x, marker_y), 1.5, TRACE_MAGENTA.gamma_multiply(0.5));
        }
    }

    // Tooltip showing what effects are visible
    if let Some(hover_pos) = response.hover_pos() {
        if rect.contains(hover_pos) {
            let mut effects = Vec::new();
            let cti_h = 1.0 - params.h_cte;
            if cti_h > 1e-7 { effects.push("CTE trailing"); }
            if params.h_ringing > 0.0 { effects.push("Ringing"); }
            if params.nonlinearity > 0.0 { effects.push("Nonlinearity"); }
            if params.reset_noise > 0.0 { effects.push("Reset noise"); }
            if params.amp_glow > 0.0 { effects.push("Amp glow"); }
            if params.bit_depth < 16 { effects.push("Quantization"); }
            if params.dnl_errors > 0.0 { effects.push("DNL errors"); }
            if params.bit_errors > 0.0 { effects.push("Bit errors"); }

            egui::show_tooltip_at_pointer(
                ui.ctx(),
                ui.layer_id(),
                ui.id().with("video_tip"),
                |ui: &mut egui::Ui| {
                    ui.label(egui::RichText::new("Video Output Signal").monospace().strong());
                    if effects.is_empty() {
                        ui.label(egui::RichText::new("Clean signal (no effects)").monospace().color(LABEL_DIM));
                    } else {
                        for e in &effects {
                            ui.label(egui::RichText::new(format!("  + {e}")).monospace().color(TRACE_GREEN));
                        }
                    }
                },
            );
        }
    }
}

fn generate_video_signal(params: &PipelineParams) -> (Vec<f32>, Vec<f32>) {
    let mut pixels = TEST_PIXELS.to_vec();

    // Apply gain
    let gain = params.amp_gain as f32;
    for v in pixels.iter_mut() {
        *v *= gain;
    }

    // Apply nonlinearity (S-curve)
    if params.nonlinearity > 0.0 {
        let nl = params.nonlinearity as f32;
        for v in pixels.iter_mut() {
            let x = v.clamp(0.0, 1.0);
            let s = 1.0 / (1.0 + (-(x - 0.5) * (2.0 + nl * 10.0)).exp());
            *v = x * (1.0 - nl) + s * nl;
        }
    }

    // Apply H-CTE trailing
    let cti = (1.0 - params.h_cte) as f32;
    if cti > 1e-7 {
        // Amplify for visibility: real CTE trailing is tiny per pixel
        // but accumulates over thousands of transfers
        let vis_cti = cti * 100.0; // ~100 transfers worth
        for i in 1..NUM_PIXELS {
            let trail = pixels[i - 1] * vis_cti;
            pixels[i] += trail;
        }
    }

    // Build per-sample analog waveform
    let mut analog = vec![0.0f32; NUM_SAMPLES];
    let reset_noise_level = (params.reset_noise as f32 / 500.0).min(0.15);

    for px in 0..NUM_PIXELS {
        let sig = pixels[px].clamp(0.0, 1.0);
        let base = px * SAMPLES_PER_PIXEL;

        // Deterministic per-pixel reset noise
        let noise = if reset_noise_level > 0.0 {
            (px as f32 * 7.3 + 2.1).sin() * reset_noise_level
        } else {
            0.0
        };
        let reset_level = 0.03 + noise;

        for s in 0..SAMPLES_PER_PIXEL {
            let t = s as f32 / SAMPLES_PER_PIXEL as f32;
            let idx = base + s;
            if idx >= NUM_SAMPLES {
                break;
            }

            analog[idx] = if t < 0.12 {
                // Reset phase
                reset_level
            } else if t < 0.20 {
                // Rising edge
                let rise = (t - 0.12) / 0.08;
                reset_level + rise * (sig - reset_level)
            } else if t < 0.82 {
                // Signal hold
                sig
            } else {
                // Falling edge toward next reset
                let fall = (t - 0.82) / 0.18;
                sig * (1.0 - fall) + reset_level * fall
            };
        }
    }

    // Apply ringing after bright-to-dim transitions
    if params.h_ringing > 0.0 {
        let ringing = params.h_ringing as f32;
        for px in 1..NUM_PIXELS {
            let transition = pixels[px - 1] - pixels[px];
            if transition > 0.15 {
                let ring_amp = transition * ringing * 0.25;
                let start = px * SAMPLES_PER_PIXEL + SAMPLES_PER_PIXEL / 5;
                for s in 0..(SAMPLES_PER_PIXEL * 2) {
                    let idx = start + s;
                    if idx >= NUM_SAMPLES {
                        break;
                    }
                    let t = s as f32 / SAMPLES_PER_PIXEL as f32;
                    analog[idx] += ring_amp * (-t * 4.0).exp() * (t * 18.0).sin();
                }
            }
        }
    }

    // Apply amp glow (gradient from one side)
    if params.amp_glow > 0.0 {
        let glow = params.amp_glow as f32;
        for i in 0..NUM_SAMPLES {
            let x = 1.0 - i as f32 / NUM_SAMPLES as f32;
            analog[i] += glow * 0.12 / (1.0 + x * x * 50.0);
        }
    }

    // Generate ADC-quantized digital output
    let max_code = ((1u32 << params.bit_depth) - 1) as f32;
    let mut digital = vec![0.0f32; NUM_SAMPLES];

    for i in 0..NUM_SAMPLES {
        let mut val = analog[i].clamp(0.0, 1.0) * max_code;
        val = val.round();

        // DNL: some codes shift
        if params.dnl_errors > 0.0 {
            let code = val as u32;
            let hash = ((code.wrapping_mul(7).wrapping_add(13)) % 100) as f64;
            if code > 0 && hash / 100.0 < params.dnl_errors * 0.5 {
                val += if code % 2 == 0 { 1.0 } else { -1.0 };
            }
        }

        // Bit errors: deterministic flips
        if params.bit_errors > 0.0 {
            let hash = ((i.wrapping_mul(13).wrapping_add(7)) % 200) as f64;
            if hash / 200.0 < params.bit_errors * 0.02 {
                let code = val as u32;
                let bit = i % params.bit_depth as usize;
                val = (code ^ (1 << bit)) as f32;
            }
        }

        digital[i] = (val / max_code).clamp(0.0, 1.0);
    }

    (analog, digital)
}

// --- Drawing helpers ---

fn draw_scope_bg(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 2.0, SCOPE_BG);
    painter.rect(
        rect, 2.0, egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, SCOPE_BORDER),
        egui::StrokeKind::Inside,
    );

    // Horizontal grid (4 divisions)
    for i in 1..4 {
        let y = rect.min.y + rect.height() * i as f32 / 4.0;
        for x_step in 0..((rect.width() / 4.0) as usize) {
            let x = rect.min.x + x_step as f32 * 4.0;
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x + 1.5, y)],
                egui::Stroke::new(0.5, SCOPE_GRID),
            );
        }
    }

    // Vertical grid (8 divisions)
    for i in 1..8 {
        let x = rect.min.x + rect.width() * i as f32 / 8.0;
        for y_step in 0..((rect.height() / 4.0) as usize) {
            let y = rect.min.y + y_step as f32 * 4.0;
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, y + 1.5)],
                egui::Stroke::new(0.5, SCOPE_GRID),
            );
        }
    }
}

fn draw_digital_trace(
    painter: &egui::Painter,
    band: egui::Rect,
    samples: &[f32],
    color: egui::Color32,
) {
    if samples.len() < 2 {
        return;
    }

    let n = samples.len();
    let x_step = band.width() / n as f32;
    let y_high = band.min.y + 3.0;
    let y_low = band.max.y - 2.0;

    let mut prev_y = if samples[0] > 0.5 { y_high } else { y_low };

    for i in 0..n {
        let x = band.min.x + i as f32 * x_step;
        let val = samples[i].clamp(0.0, 1.5);
        let y = y_low - val * (y_low - y_high);

        // Vertical transition
        if (prev_y - y).abs() > 1.0 {
            painter.line_segment(
                [egui::pos2(x, prev_y), egui::pos2(x, y)],
                egui::Stroke::new(1.0, color),
            );
        }

        // Horizontal segment
        let next_x = (x + x_step).min(band.max.x);
        painter.line_segment(
            [egui::pos2(x, y), egui::pos2(next_x, y)],
            egui::Stroke::new(1.0, color),
        );

        prev_y = y;
    }
}

fn draw_analog_trace(
    painter: &egui::Painter,
    rect: egui::Rect,
    samples: &[f32],
    color: egui::Color32,
    thickness: f32,
) {
    if samples.len() < 2 {
        return;
    }

    let n = samples.len();
    let x_step = rect.width() / (n - 1) as f32;

    for i in 0..n - 1 {
        let x0 = rect.min.x + i as f32 * x_step;
        let x1 = rect.min.x + (i + 1) as f32 * x_step;
        let y0 = rect.max.y - samples[i].clamp(0.0, 1.0) * rect.height();
        let y1 = rect.max.y - samples[i + 1].clamp(0.0, 1.0) * rect.height();

        painter.line_segment(
            [egui::pos2(x0, y0), egui::pos2(x1, y1)],
            egui::Stroke::new(thickness, color),
        );
    }
}
