use eframe::egui;

use crate::pipeline::PipelineParams;

struct PipelineStage {
    label: &'static str,
    active: bool,
    effects: Vec<(&'static str, bool)>,
    #[cfg(feature = "spice")]
    spice_driven: bool,
}

fn pipeline_stages(p: &PipelineParams) -> Vec<PipelineStage> {
    let d = PipelineParams::default();

    // Determine which stages are replaced by SPICE
    #[cfg(feature = "spice")]
    let spice_mode = p.spice.mode;
    #[cfg(feature = "spice")]
    let spice_full = spice_mode == crate::spice::SpiceMode::FullReadout;
    #[cfg(feature = "spice")]
    let spice_amp = spice_mode == crate::spice::SpiceMode::AmplifierOnly
        || spice_mode == crate::spice::SpiceMode::FullReadout;
    #[cfg(feature = "spice")]
    let spice_tf = spice_mode == crate::spice::SpiceMode::TransferCurveOnly;

    vec![
        PipelineStage {
            label: "SENSOR",
            active: true,
            effects: vec![
                ("ABG", p.use_abg),
            ],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
        PipelineStage {
            label: "CFA",
            active: p.bayer_pattern != d.bayer_pattern,
            effects: vec![],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
        PipelineStage {
            label: "NOISE",
            active: p.dark_current_rate > 0.0
                || p.read_noise > 0.0
                || p.shot_noise_enabled,
            effects: vec![
                ("Dark", p.dark_current_rate > 0.0),
                ("Shot", p.shot_noise_enabled),
                ("Read", p.read_noise > 0.0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
        PipelineStage {
            label: "BLOOM",
            active: p.abg_strength < d.abg_strength
                || p.bloom_threshold != d.bloom_threshold
                || p.bloom_vertical != d.bloom_vertical,
            effects: vec![
                ("ABG", p.abg_strength < 1.0),
                ("Vert", p.bloom_vertical),
            ],
            #[cfg(feature = "spice")]
            spice_driven: spice_full,
        },
        PipelineStage {
            label: "V-CLK",
            active: p.v_cte < d.v_cte
                || p.v_glitch_rate > 0.0
                || p.v_waveform_distortion > 0.0
                || p.parallel_smear > 0.0,
            effects: vec![
                ("CTE", p.v_cte < d.v_cte),
                ("Glitch", p.v_glitch_rate > 0.0),
                ("Wave", p.v_waveform_distortion > 0.0),
                ("Smear", p.parallel_smear > 0.0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: spice_full,
        },
        PipelineStage {
            label: "H-CLK",
            active: p.h_cte < d.h_cte
                || p.h_glitch_rate > 0.0
                || p.h_ringing > 0.0
                || p.readout_direction != d.readout_direction,
            effects: vec![
                ("CTE", p.h_cte < d.h_cte),
                ("Glitch", p.h_glitch_rate > 0.0),
                ("Ring", p.h_ringing > 0.0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: spice_full,
        },
        PipelineStage {
            label: "AMP",
            active: (p.amp_gain - d.amp_gain).abs() > 0.001
                || p.nonlinearity > 0.0
                || p.reset_noise > 0.0
                || p.amp_glow > 0.0,
            effects: vec![
                ("Gain", (p.amp_gain - d.amp_gain).abs() > 0.001),
                ("NL", p.nonlinearity > 0.0),
                ("kTC", p.reset_noise > 0.0),
                ("Glow", p.amp_glow > 0.0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: spice_amp || spice_tf,
        },
        PipelineStage {
            label: "ADC",
            active: p.bit_depth != d.bit_depth
                || p.cds_mode != d.cds_mode
                || (p.adc_gain - d.adc_gain).abs() > 0.001
                || p.bias > 0.0
                || p.dnl_errors > 0.0
                || p.bit_errors > 0.0
                || p.adc_jitter > 0.0,
            effects: vec![
                ("Bits", p.bit_depth != d.bit_depth),
                ("DNL", p.dnl_errors > 0.0),
                ("Err", p.bit_errors > 0.0),
                ("Jit", p.adc_jitter > 0.0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: spice_amp,
        },
        PipelineStage {
            label: "GLITCH",
            active: p.pixel_shift_amount > 0.0
                || p.block_shift_amount > 0.0
                || p.scan_line_frequency > 0.0
                || p.bit_xor_mask > 0
                || p.bit_rotation != 0
                || p.bit_plane_swaps > 0,
            effects: vec![
                ("Px", p.pixel_shift_amount > 0.0),
                ("Blk", p.block_shift_amount > 0.0),
                ("Scan", p.scan_line_frequency > 0.0),
                ("XOR", p.bit_xor_mask > 0),
                ("Rot", p.bit_rotation != 0),
            ],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
        PipelineStage {
            label: "DEMSC",
            active: p.demosaic_algo != d.demosaic_algo,
            effects: vec![],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
        PipelineStage {
            label: "COLOR",
            active: p.channel_swap != d.channel_swap
                || (p.channel_r_gain - d.channel_r_gain).abs() > 0.001
                || (p.channel_g_gain - d.channel_g_gain).abs() > 0.001
                || (p.channel_b_gain - d.channel_b_gain).abs() > 0.001
                || p.channel_r_offset.abs() > 0.001
                || p.channel_g_offset.abs() > 0.001
                || p.channel_b_offset.abs() > 0.001
                || p.chromatic_r_x != 0
                || p.chromatic_r_y != 0
                || p.chromatic_b_x != 0
                || p.chromatic_b_y != 0
                || (p.white_balance_r - d.white_balance_r).abs() > 0.001
                || (p.white_balance_g - d.white_balance_g).abs() > 0.001
                || (p.white_balance_b - d.white_balance_b).abs() > 0.001
                || (p.gamma - d.gamma).abs() > 0.001
                || p.brightness.abs() > 0.001
                || (p.contrast - d.contrast).abs() > 0.001,
            effects: vec![
                ("Swap", p.channel_swap != d.channel_swap),
                ("Gain", (p.channel_r_gain - 1.0).abs() > 0.001
                    || (p.channel_g_gain - 1.0).abs() > 0.001
                    || (p.channel_b_gain - 1.0).abs() > 0.001),
                ("CA", p.chromatic_r_x != 0
                    || p.chromatic_r_y != 0
                    || p.chromatic_b_x != 0
                    || p.chromatic_b_y != 0),
                ("WB", (p.white_balance_r - 1.0).abs() > 0.001
                    || (p.white_balance_g - 1.0).abs() > 0.001
                    || (p.white_balance_b - 1.0).abs() > 0.001),
            ],
            #[cfg(feature = "spice")]
            spice_driven: false,
        },
    ]
}

// Colors
const CHIP_BG: egui::Color32 = egui::Color32::from_rgb(22, 22, 32);
const CHIP_BORDER: egui::Color32 = egui::Color32::from_rgb(60, 65, 80);
const ACTIVE_BORDER: egui::Color32 = egui::Color32::from_rgb(0, 220, 110);
const ACTIVE_FILL: egui::Color32 = egui::Color32::from_rgb(8, 40, 25);
const ACTIVE_TEXT: egui::Color32 = egui::Color32::from_rgb(0, 255, 128);
const INACTIVE_BORDER: egui::Color32 = egui::Color32::from_rgb(55, 55, 65);
const INACTIVE_FILL: egui::Color32 = egui::Color32::from_rgb(28, 28, 36);
const INACTIVE_TEXT: egui::Color32 = egui::Color32::from_rgb(100, 100, 115);
const WIRE_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0, 180, 90);
const WIRE_INACTIVE: egui::Color32 = egui::Color32::from_rgb(45, 50, 55);
const DOT_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0, 255, 100);
const DOT_INACTIVE: egui::Color32 = egui::Color32::from_rgb(50, 50, 60);
const PIN_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 180, 160);
const CHIP_LABEL: egui::Color32 = egui::Color32::from_rgb(80, 85, 100);
#[cfg(feature = "spice")]
const SPICE_BORDER: egui::Color32 = egui::Color32::from_rgb(255, 180, 40);
#[cfg(feature = "spice")]
const SPICE_FILL: egui::Color32 = egui::Color32::from_rgb(40, 30, 8);
#[cfg(feature = "spice")]
const SPICE_TEXT: egui::Color32 = egui::Color32::from_rgb(255, 200, 60);

pub fn draw_circuit(ui: &mut egui::Ui, params: &PipelineParams) {
    let stages = pipeline_stages(params);
    let available_width = ui.available_width();

    // Layout calculations
    let block_w: f32 = 48.0;
    let block_h: f32 = 26.0;
    let h_gap: f32 = 6.0;
    let v_gap: f32 = 14.0;
    let wire_len: f32 = h_gap;
    let chip_pad: f32 = 10.0;
    let pin_w: f32 = 14.0;
    let dot_row_h: f32 = 8.0;

    let inner_w = available_width - chip_pad * 2.0 - pin_w * 2.0;
    let block_stride = block_w + wire_len;
    let blocks_per_row = ((inner_w + wire_len) / block_stride).floor().max(1.0) as usize;

    let num_rows = (stages.len() + blocks_per_row - 1) / blocks_per_row;
    let row_h = block_h + dot_row_h + v_gap;
    let total_h = chip_pad * 2.0 + num_rows as f32 * row_h + 18.0; // 18 for title

    let (response, painter) = ui.allocate_painter(
        egui::vec2(available_width, total_h),
        egui::Sense::hover(),
    );
    let origin = response.rect.min;

    // Draw chip body
    let chip_rect = egui::Rect::from_min_size(
        egui::pos2(origin.x + pin_w, origin.y),
        egui::vec2(available_width - pin_w * 2.0, total_h),
    );
    painter.rect(chip_rect, 6.0, CHIP_BG, egui::Stroke::new(1.5, CHIP_BORDER), egui::StrokeKind::Outside);

    // Notch at top center (IC package indicator)
    let notch_center = egui::pos2(chip_rect.center().x, chip_rect.min.y);
    painter.circle_stroke(notch_center, 5.0, egui::Stroke::new(1.0, CHIP_BORDER));

    // Chip title
    painter.text(
        egui::pos2(chip_rect.center().x, origin.y + 12.0),
        egui::Align2::CENTER_CENTER,
        "CCD SIGNAL CHAIN",
        egui::FontId::monospace(9.0),
        CHIP_LABEL,
    );

    // Draw input pin
    let first_block_y = origin.y + chip_pad + 18.0 + block_h / 2.0;
    let pin_y = first_block_y;
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(origin.x, pin_y - 4.0),
            egui::vec2(pin_w, 8.0),
        ),
        1.0,
        PIN_COLOR,
    );
    painter.text(
        egui::pos2(origin.x + pin_w / 2.0, pin_y - 7.0),
        egui::Align2::CENTER_BOTTOM,
        "IN",
        egui::FontId::monospace(7.0),
        PIN_COLOR,
    );

    // Track block positions for wiring
    let mut block_positions: Vec<(egui::Rect, bool)> = Vec::new();

    let content_start_x = origin.x + pin_w + chip_pad;

    for (i, stage) in stages.iter().enumerate() {
        let row = i / blocks_per_row;
        let col_in_row = i % blocks_per_row;

        // Serpentine: even rows go left-to-right, odd rows go right-to-left
        let col = if row % 2 == 0 {
            col_in_row
        } else {
            let items_in_this_row = (stages.len() - row * blocks_per_row).min(blocks_per_row);
            items_in_this_row - 1 - col_in_row
        };

        let x = content_start_x + col as f32 * block_stride;
        let y = origin.y + chip_pad + 18.0 + row as f32 * row_h;

        let block_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(block_w, block_h),
        );

        // Draw block
        #[cfg(feature = "spice")]
        let is_spice = stage.spice_driven;
        #[cfg(not(feature = "spice"))]
        let is_spice = false;

        let (fill, stroke_color, text_color) = if is_spice {
            #[cfg(feature = "spice")]
            { (SPICE_FILL, SPICE_BORDER, SPICE_TEXT) }
            #[cfg(not(feature = "spice"))]
            { (ACTIVE_FILL, ACTIVE_BORDER, ACTIVE_TEXT) }
        } else if stage.active {
            (ACTIVE_FILL, ACTIVE_BORDER, ACTIVE_TEXT)
        } else {
            (INACTIVE_FILL, INACTIVE_BORDER, INACTIVE_TEXT)
        };

        painter.rect(block_rect, 3.0, fill, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Outside);

        // Active glow effect
        if stage.active {
            painter.rect(
                block_rect.expand(2.0),
                5.0,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(1.0, ACTIVE_BORDER.gamma_multiply(0.3)),
                egui::StrokeKind::Outside,
            );
        }

        // Block label
        painter.text(
            block_rect.center(),
            egui::Align2::CENTER_CENTER,
            stage.label,
            egui::FontId::monospace(8.0),
            text_color,
        );

        // Draw effect indicator dots below the block
        if !stage.effects.is_empty() {
            let active_effects: Vec<bool> = stage.effects.iter().map(|(_, a)| *a).collect();
            let total_dots = active_effects.len();
            let dot_spacing = 5.0f32;
            let dots_width = (total_dots as f32 - 1.0) * dot_spacing;
            let dot_start_x = block_rect.center().x - dots_width / 2.0;
            let dot_y = block_rect.max.y + 4.0;

            for (j, &is_on) in active_effects.iter().enumerate() {
                let dx = dot_start_x + j as f32 * dot_spacing;
                let color = if is_on { DOT_ACTIVE } else { DOT_INACTIVE };
                painter.circle_filled(egui::pos2(dx, dot_y), 1.5, color);
            }
        }

        block_positions.push((block_rect, stage.active));
    }

    // Draw wires between consecutive blocks
    for i in 0..block_positions.len() - 1 {
        let (rect_a, active_a) = block_positions[i];
        let (rect_b, active_b) = block_positions[i + 1];
        let wire_color = if active_a || active_b { WIRE_ACTIVE } else { WIRE_INACTIVE };
        let wire_stroke = egui::Stroke::new(1.5, wire_color);

        let row_a = i / blocks_per_row;
        let row_b = (i + 1) / blocks_per_row;

        if row_a == row_b {
            // Same row: horizontal wire
            let (start, end) = if row_a % 2 == 0 {
                // Left to right
                (
                    egui::pos2(rect_a.max.x, rect_a.center().y),
                    egui::pos2(rect_b.min.x, rect_b.center().y),
                )
            } else {
                // Right to left
                (
                    egui::pos2(rect_a.min.x, rect_a.center().y),
                    egui::pos2(rect_b.max.x, rect_b.center().y),
                )
            };
            painter.line_segment([start, end], wire_stroke);

            // Small arrow
            let mid = egui::pos2((start.x + end.x) / 2.0, start.y);
            let dir = if end.x > start.x { 1.0 } else { -1.0 };
            painter.line_segment(
                [
                    egui::pos2(mid.x - 2.0 * dir, mid.y - 2.0),
                    mid,
                ],
                wire_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(mid.x - 2.0 * dir, mid.y + 2.0),
                    mid,
                ],
                wire_stroke,
            );
        } else {
            // Row transition: vertical connector
            let turn_x = if row_a % 2 == 0 {
                // End of left-to-right row: turn at right side
                rect_a.max.x + 3.0
            } else {
                // End of right-to-left row: turn at left side
                rect_a.min.x - 3.0
            };

            let y_start = rect_a.center().y;
            let y_end = rect_b.center().y;

            // Horizontal segment from block A to turn point
            if row_a % 2 == 0 {
                painter.line_segment(
                    [egui::pos2(rect_a.max.x, y_start), egui::pos2(turn_x, y_start)],
                    wire_stroke,
                );
            } else {
                painter.line_segment(
                    [egui::pos2(rect_a.min.x, y_start), egui::pos2(turn_x, y_start)],
                    wire_stroke,
                );
            }

            // Vertical segment
            painter.line_segment(
                [egui::pos2(turn_x, y_start), egui::pos2(turn_x, y_end)],
                wire_stroke,
            );

            // Horizontal segment from turn point to block B
            if row_b % 2 == 0 {
                painter.line_segment(
                    [egui::pos2(turn_x, y_end), egui::pos2(rect_b.min.x, y_end)],
                    wire_stroke,
                );
            } else {
                painter.line_segment(
                    [egui::pos2(turn_x, y_end), egui::pos2(rect_b.max.x, y_end)],
                    wire_stroke,
                );
            }

            // Down arrow on vertical segment
            let mid_y = (y_start + y_end) / 2.0;
            painter.line_segment(
                [
                    egui::pos2(turn_x - 2.0, mid_y - 2.0),
                    egui::pos2(turn_x, mid_y),
                ],
                wire_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(turn_x + 2.0, mid_y - 2.0),
                    egui::pos2(turn_x, mid_y),
                ],
                wire_stroke,
            );
        }
    }

    // Draw input wire from pin to first block
    if let Some((first_rect, active)) = block_positions.first() {
        let wire_color = if *active { WIRE_ACTIVE } else { WIRE_INACTIVE };
        painter.line_segment(
            [
                egui::pos2(origin.x + pin_w, pin_y),
                egui::pos2(first_rect.min.x, first_rect.center().y),
            ],
            egui::Stroke::new(1.5, wire_color),
        );
    }

    // Draw output pin
    if let Some((last_rect, active)) = block_positions.last() {
        let last_row = (stages.len() - 1) / blocks_per_row;
        let out_x = if last_row % 2 == 0 {
            last_rect.max.x
        } else {
            last_rect.min.x
        };
        let out_y = last_rect.center().y;

        // Wire to chip edge
        let chip_edge_x = if last_row % 2 == 0 {
            chip_rect.max.x
        } else {
            chip_rect.min.x
        };
        let wire_color = if *active { WIRE_ACTIVE } else { WIRE_INACTIVE };
        painter.line_segment(
            [egui::pos2(out_x, out_y), egui::pos2(chip_edge_x, out_y)],
            egui::Stroke::new(1.5, wire_color),
        );

        // Output pin
        let pin_x = if last_row % 2 == 0 {
            origin.x + available_width - pin_w
        } else {
            origin.x
        };
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(pin_x, out_y - 4.0),
                egui::vec2(pin_w, 8.0),
            ),
            1.0,
            PIN_COLOR,
        );
        painter.text(
            egui::pos2(pin_x + pin_w / 2.0, out_y - 7.0),
            egui::Align2::CENTER_BOTTOM,
            "OUT",
            egui::FontId::monospace(7.0),
            PIN_COLOR,
        );
    }

    // Draw pin markers along chip edges (decorative IC pins)
    let num_pins_per_side = 4;
    let pin_spacing = (total_h - chip_pad * 2.0) / (num_pins_per_side as f32 + 1.0);
    for i in 1..=num_pins_per_side {
        let py = origin.y + chip_pad + i as f32 * pin_spacing;

        // Left side pins
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(origin.x, py - 2.5),
                egui::vec2(pin_w - 2.0, 5.0),
            ),
            0.5,
            CHIP_BORDER.gamma_multiply(0.5),
        );

        // Right side pins
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(origin.x + available_width - pin_w + 2.0, py - 2.5),
                egui::vec2(pin_w - 2.0, 5.0),
            ),
            0.5,
            CHIP_BORDER.gamma_multiply(0.5),
        );
    }

    // Count active modifications
    let active_count = stages.iter().filter(|s| s.active).count();
    let total_effects: usize = stages.iter().map(|s| s.effects.iter().filter(|(_, a)| *a).count()).sum();

    painter.text(
        egui::pos2(chip_rect.center().x, chip_rect.max.y - 6.0),
        egui::Align2::CENTER_CENTER,
        &format!("{} stages active | {} effects", active_count, total_effects),
        egui::FontId::monospace(7.0),
        CHIP_LABEL,
    );

    // Tooltip for hovered block
    if let Some(hover_pos) = response.hover_pos() {
        for (i, (rect, _)) in block_positions.iter().enumerate() {
            if rect.expand(2.0).contains(hover_pos) {
                let stage = &stages[i];
                egui::show_tooltip_at_pointer(
                    ui.ctx(),
                    ui.layer_id(),
                    ui.id().with("circuit_tip"),
                    |ui: &mut egui::Ui| {
                        ui.label(egui::RichText::new(stage.label).strong().monospace());
                        for (name, active) in &stage.effects {
                            let icon = if *active { "+" } else { "-" };
                            let color = if *active { ACTIVE_TEXT } else { INACTIVE_TEXT };
                            ui.label(egui::RichText::new(format!(" {icon} {name}")).monospace().color(color));
                        }
                        if stage.effects.is_empty() {
                            let status = if stage.active { "Modified" } else { "Default" };
                            ui.label(egui::RichText::new(status).monospace().color(CHIP_LABEL));
                        }
                    },
                );
                break;
            }
        }
    }
}
