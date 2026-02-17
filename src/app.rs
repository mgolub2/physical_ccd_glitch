use eframe::egui;
use image::DynamicImage;
use std::path::PathBuf;

use crate::ccd::adc::CdsMode;
use crate::ccd::transfer::ReadoutDirection;
use crate::ccd::{SensorConfig, SensorPreset};
use crate::color::bayer::BayerPattern;
use crate::color::demosaic::DemosaicAlgo;
use crate::glitch::channel::ChannelSwap;
use crate::pipeline::{self, PipelineParams};

pub struct CcdGlitchApp {
    source_image: Option<DynamicImage>,
    source_path: Option<PathBuf>,
    preview_texture: Option<egui::TextureHandle>,
    preview_width: usize,
    preview_height: usize,
    params: PipelineParams,
    sensor_preset: SensorPreset,
    needs_process: bool,
    auto_process: bool,
    processing_time_ms: f64,
}

impl CcdGlitchApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let preset = SensorPreset::Kaf6303;
        let config = preset.config();
        let mut params = PipelineParams::default();
        apply_sensor_config(&mut params, &config);

        Self {
            source_image: None,
            source_path: None,
            preview_texture: None,
            preview_width: 0,
            preview_height: 0,
            params,
            sensor_preset: preset,
            needs_process: false,
            auto_process: false,
            processing_time_ms: 0.0,
        }
    }

    fn open_image(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif", "bmp", "webp"])
            .pick_file()
        {
            match crate::image_io::load_image(&path) {
                Ok(img) => {
                    self.source_image = Some(img);
                    self.source_path = Some(path);
                    self.needs_process = true;
                }
                Err(e) => {
                    eprintln!("Error loading image: {e}");
                }
            }
        }
    }

    fn save_result(&self) {
        if self.preview_texture.is_none() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .add_filter("TIFF", &["tiff", "tif"])
            .save_file()
        {
            if let Some(source) = &self.source_image {
                let (w, h, bytes) = pipeline::process(source, &self.params);
                let img = image::RgbImage::from_raw(w as u32, h as u32, bytes)
                    .expect("Failed to create image buffer");
                if let Err(e) = crate::image_io::save_image(&img, &path) {
                    eprintln!("Error saving image: {e}");
                }
            }
        }
    }

    fn process_image(&mut self, ctx: &egui::Context) {
        if let Some(source) = &self.source_image {
            let start = std::time::Instant::now();
            let (w, h, bytes) = pipeline::process(source, &self.params);
            self.processing_time_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.preview_width = w;
            self.preview_height = h;

            let color_image = egui::ColorImage::from_rgb([w, h], &bytes);
            self.preview_texture = Some(ctx.load_texture(
                "preview",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }
}

fn apply_sensor_config(params: &mut PipelineParams, config: &SensorConfig) {
    params.sensor_width = config.width;
    params.sensor_height = config.height;
    params.full_well = if params.use_abg {
        config.full_well_abg
    } else {
        config.full_well_no_abg
    };
    params.read_noise = 0.0; // Keep at 0 by default for clean output
    params.v_cte = config.cte_vertical;
    params.h_cte = config.cte_horizontal;
}

impl eframe::App for CcdGlitchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel: file operations and preset selection
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Image").clicked() {
                    self.open_image();
                }
                if ui.button("Save Result").clicked() {
                    self.save_result();
                }
                ui.separator();

                ui.label("Preset:");
                let current_name = self.sensor_preset.name();
                egui::ComboBox::from_id_salt("sensor_preset")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for &preset in SensorPreset::ALL {
                            if ui.selectable_value(
                                &mut self.sensor_preset,
                                preset,
                                preset.name(),
                            ).clicked() {
                                let config = preset.config();
                                apply_sensor_config(&mut self.params, &config);
                                self.needs_process = true;
                            }
                        }
                    });

                ui.separator();
                ui.checkbox(&mut self.auto_process, "Auto");

                if ui.button("Process").clicked() {
                    self.needs_process = true;
                }
                if ui.button("Reset").clicked() {
                    let config = self.sensor_preset.config();
                    self.params = PipelineParams::default();
                    apply_sensor_config(&mut self.params, &config);
                    self.needs_process = true;
                }

                ui.separator();
                if self.source_path.is_some() {
                    ui.label(format!(
                        "{}x{} | {:.0}ms",
                        self.params.sensor_width,
                        self.params.sensor_height,
                        self.processing_time_ms
                    ));
                }
            });
        });

        // Left panel: controls
        egui::SidePanel::left("controls")
            .default_width(300.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut changed = false;
                    changed |= ui_sensor_config(ui, &mut self.params, self.sensor_preset);
                    changed |= ui_exposure_noise(ui, &mut self.params);
                    changed |= ui_blooming(ui, &mut self.params);
                    changed |= ui_v_clock(ui, &mut self.params);
                    changed |= ui_h_clock(ui, &mut self.params);
                    changed |= ui_amplifier(ui, &mut self.params);
                    changed |= ui_adc(ui, &mut self.params);
                    changed |= ui_glitch(ui, &mut self.params);
                    changed |= ui_channel(ui, &mut self.params);
                    changed |= ui_color_output(ui, &mut self.params);

                    if changed && self.auto_process {
                        self.needs_process = true;
                    }
                });
            });

        // Process if needed
        if self.needs_process && self.source_image.is_some() {
            self.process_image(ctx);
            self.needs_process = false;
        }

        // Central panel: image preview
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.preview_texture {
                egui::ScrollArea::both().show(ui, |ui| {
                    let available = ui.available_size();
                    let img_w = self.preview_width as f32;
                    let img_h = self.preview_height as f32;
                    let scale = f32::min(
                        available.x / img_w,
                        available.y / img_h,
                    ).min(1.0);
                    let display_size = egui::vec2(img_w * scale, img_h * scale);
                    ui.image(egui::load::SizedTexture::new(tex.id(), display_size));
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Open an image to begin");
                });
            }
        });
    }
}

// --- UI Section Builders ---

fn ui_sensor_config(ui: &mut egui::Ui, params: &mut PipelineParams, preset: SensorPreset) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Sensor Config")
        .default_open(true)
        .show(ui, |ui| {
            if preset == SensorPreset::Custom {
                let mut w = params.sensor_width;
                let mut h = params.sensor_height;
                changed |= ui.add(egui::Slider::new(&mut w, 64..=8192).text("Width")).changed();
                changed |= ui.add(egui::Slider::new(&mut h, 64..=8192).text("Height")).changed();
                params.sensor_width = w;
                params.sensor_height = h;
                changed |= ui.add(
                    egui::Slider::new(&mut params.full_well, 1000.0..=500_000.0)
                        .logarithmic(true)
                        .text("Full Well (e-)"),
                ).changed();
            } else {
                ui.label(format!("Resolution: {}x{}", params.sensor_width, params.sensor_height));
                ui.label(format!("Full Well: {:.0} e-", params.full_well));
            }
            changed |= ui.checkbox(&mut params.use_abg, "Anti-Blooming Gate").changed();
            if changed && preset != SensorPreset::Custom {
                let config = preset.config();
                params.full_well = if params.use_abg {
                    config.full_well_abg
                } else {
                    config.full_well_no_abg
                };
            }
        });
    changed
}

fn ui_exposure_noise(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Exposure & Noise")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.dark_current_rate, 0.0..=1000.0)
                    .logarithmic(true)
                    .text("Dark Current (e-)"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.read_noise, 0.0..=100.0)
                    .text("Read Noise (e-)"),
            ).changed();
            changed |= ui.checkbox(&mut params.shot_noise_enabled, "Shot Noise").changed();
        });
    changed
}

fn ui_blooming(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Blooming")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.abg_strength, 0.0..=1.0)
                    .text("ABG Strength"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.bloom_threshold, 0.1..=1.0)
                    .text("Bloom Threshold"),
            ).changed();
            changed |= ui.checkbox(&mut params.bloom_vertical, "Vertical Bloom").changed();
        });
    changed
}

fn ui_v_clock(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("V-Clock (Parallel)")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.v_cte, 0.99..=1.0)
                    .min_decimals(6)
                    .max_decimals(6)
                    .text("CTE"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.v_glitch_rate, 0.0..=0.5)
                    .text("Glitch Rate"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.v_waveform_distortion, 0.0..=1.0)
                    .text("Waveform Distortion"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.parallel_smear, 0.0..=1.0)
                    .text("Parallel Smear"),
            ).changed();
        });
    changed
}

fn ui_h_clock(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("H-Clock (Serial)")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.h_cte, 0.99..=1.0)
                    .min_decimals(6)
                    .max_decimals(6)
                    .text("CTE"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.h_glitch_rate, 0.0..=0.1)
                    .text("Glitch Rate"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.h_ringing, 0.0..=1.0)
                    .text("Ringing"),
            ).changed();

            let current_dir = match params.readout_direction {
                ReadoutDirection::LeftToRight => "Left to Right",
                ReadoutDirection::RightToLeft => "Right to Left",
                ReadoutDirection::Alternating => "Alternating",
            };
            egui::ComboBox::from_label("Readout Dir")
                .selected_text(current_dir)
                .show_ui(ui, |ui| {
                    changed |= ui.selectable_value(
                        &mut params.readout_direction,
                        ReadoutDirection::LeftToRight,
                        "Left to Right",
                    ).changed();
                    changed |= ui.selectable_value(
                        &mut params.readout_direction,
                        ReadoutDirection::RightToLeft,
                        "Right to Left",
                    ).changed();
                    changed |= ui.selectable_value(
                        &mut params.readout_direction,
                        ReadoutDirection::Alternating,
                        "Alternating",
                    ).changed();
                });
        });
    changed
}

fn ui_amplifier(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Amplifier")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.amp_gain, 0.1..=10.0)
                    .logarithmic(true)
                    .text("Gain"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.nonlinearity, 0.0..=1.0)
                    .text("Nonlinearity"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.reset_noise, 0.0..=500.0)
                    .text("Reset Noise (e-)"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.amp_glow, 0.0..=1.0)
                    .text("Amp Glow"),
            ).changed();
        });
    changed
}

fn ui_adc(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("ADC")
        .default_open(false)
        .show(ui, |ui| {
            let mut bd = params.bit_depth as i32;
            changed |= ui.add(
                egui::Slider::new(&mut bd, 4..=16).text("Bit Depth"),
            ).changed();
            params.bit_depth = bd as u8;

            let cds_name = match params.cds_mode {
                CdsMode::On => "On",
                CdsMode::Off => "Off",
                CdsMode::Partial => "Partial",
            };
            egui::ComboBox::from_label("CDS Mode")
                .selected_text(cds_name)
                .show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut params.cds_mode, CdsMode::On, "On").changed();
                    changed |= ui.selectable_value(&mut params.cds_mode, CdsMode::Off, "Off").changed();
                    changed |= ui.selectable_value(&mut params.cds_mode, CdsMode::Partial, "Partial").changed();
                });

            changed |= ui.add(
                egui::Slider::new(&mut params.adc_gain, 0.1..=10.0)
                    .logarithmic(true)
                    .text("Gain (e-/ADU)"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.bias, 0.0..=1000.0)
                    .text("Bias"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.dnl_errors, 0.0..=1.0)
                    .text("DNL Errors"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.bit_errors, 0.0..=1.0)
                    .text("Bit Errors"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.adc_jitter, 0.0..=500.0)
                    .text("ADC Jitter"),
            ).changed();
        });
    changed
}

fn ui_glitch(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Glitch Effects")
        .default_open(false)
        .show(ui, |ui| {
            changed |= ui.add(
                egui::Slider::new(&mut params.pixel_shift_amount, 0.0..=2.0)
                    .text("Pixel Shift"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.block_shift_amount, 0.0..=2.0)
                    .text("Block Shift"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.scan_line_frequency, 0.0..=2.0)
                    .text("Scan Line Corruption"),
            ).changed();

            ui.separator();
            ui.label("Bit Manipulation");

            let mut xor = params.bit_xor_mask as i32;
            changed |= ui.add(
                egui::Slider::new(&mut xor, 0..=65535).text("XOR Mask"),
            ).changed();
            params.bit_xor_mask = xor as u16;

            changed |= ui.add(
                egui::Slider::new(&mut params.bit_rotation, -8..=8)
                    .text("Bit Rotation"),
            ).changed();

            let mut swaps = params.bit_plane_swaps as i32;
            changed |= ui.add(
                egui::Slider::new(&mut swaps, 0..=8).text("Bit Plane Swaps"),
            ).changed();
            params.bit_plane_swaps = swaps as u32;
        });
    changed
}

fn ui_channel(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Channel Effects")
        .default_open(false)
        .show(ui, |ui| {
            let swap_name = params.channel_swap.name();
            egui::ComboBox::from_label("Channel Swap")
                .selected_text(swap_name)
                .show_ui(ui, |ui| {
                    for &swap in ChannelSwap::ALL {
                        changed |= ui.selectable_value(
                            &mut params.channel_swap,
                            swap,
                            swap.name(),
                        ).changed();
                    }
                });

            ui.label("Channel Gain");
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_r_gain, 0.0..=3.0).text("R Gain"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_g_gain, 0.0..=3.0).text("G Gain"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_b_gain, 0.0..=3.0).text("B Gain"),
            ).changed();

            ui.label("Channel Offset");
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_r_offset, -0.5..=0.5).text("R Offset"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_g_offset, -0.5..=0.5).text("G Offset"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.channel_b_offset, -0.5..=0.5).text("B Offset"),
            ).changed();

            ui.separator();
            ui.label("Chromatic Aberration");
            changed |= ui.add(
                egui::Slider::new(&mut params.chromatic_r_x, -20..=20).text("R shift X"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.chromatic_r_y, -20..=20).text("R shift Y"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.chromatic_b_x, -20..=20).text("B shift X"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.chromatic_b_y, -20..=20).text("B shift Y"),
            ).changed();
        });
    changed
}

fn ui_color_output(ui: &mut egui::Ui, params: &mut PipelineParams) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Color / Output")
        .default_open(false)
        .show(ui, |ui| {
            let bayer_name = params.bayer_pattern.name();
            egui::ComboBox::from_label("Bayer Pattern")
                .selected_text(bayer_name)
                .show_ui(ui, |ui| {
                    for &pattern in BayerPattern::ALL {
                        changed |= ui.selectable_value(
                            &mut params.bayer_pattern,
                            pattern,
                            pattern.name(),
                        ).changed();
                    }
                });

            let demosaic_name = params.demosaic_algo.name();
            egui::ComboBox::from_label("Demosaic")
                .selected_text(demosaic_name)
                .show_ui(ui, |ui| {
                    for &algo in DemosaicAlgo::ALL {
                        changed |= ui.selectable_value(
                            &mut params.demosaic_algo,
                            algo,
                            algo.name(),
                        ).changed();
                    }
                });

            ui.separator();
            ui.label("White Balance");
            changed |= ui.add(
                egui::Slider::new(&mut params.white_balance_r, 0.0..=3.0).text("R"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.white_balance_g, 0.0..=3.0).text("G"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.white_balance_b, 0.0..=3.0).text("B"),
            ).changed();

            ui.separator();
            changed |= ui.add(
                egui::Slider::new(&mut params.gamma, 0.1..=4.0).text("Gamma"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.brightness, -1.0..=1.0).text("Brightness"),
            ).changed();
            changed |= ui.add(
                egui::Slider::new(&mut params.contrast, 0.0..=3.0).text("Contrast"),
            ).changed();
        });
    changed
}
