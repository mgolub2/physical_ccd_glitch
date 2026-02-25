use eframe::egui;
use image::DynamicImage;

use crate::ccd::adc::CdsMode;
use crate::ccd::transfer::ReadoutDirection;
use crate::ccd::{SensorConfig, SensorPreset};
use crate::color::bayer::BayerPattern;
use crate::color::demosaic::DemosaicAlgo;
use crate::glitch::channel::ChannelSwap;
use crate::pipeline::{self, PipelineParams};

pub struct CcdGlitchApp {
    source_image: Option<DynamicImage>,
    preview_texture: Option<egui::TextureHandle>,
    preview_width: usize,
    preview_height: usize,
    params: PipelineParams,
    sensor_preset: SensorPreset,
    needs_process: bool,
    auto_process: bool,
    processing_time_ms: f64,
    #[cfg(target_arch = "wasm32")]
    pending_file: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    spice_cache: Option<crate::spice::SpiceCache>,
}

impl CcdGlitchApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let preset = SensorPreset::Kaf6303;
        #[cfg(target_arch = "wasm32")]
        let preset = SensorPreset::Icx059cl;

        let config = preset.config();
        let mut params = PipelineParams::default();
        apply_sensor_config(&mut params, &config);

        Self {
            source_image: None,
            preview_texture: None,
            preview_width: 0,
            preview_height: 0,
            params,
            sensor_preset: preset,
            needs_process: false,
            auto_process: false,
            processing_time_ms: 0.0,
            #[cfg(target_arch = "wasm32")]
            pending_file: std::sync::Arc::new(std::sync::Mutex::new(None)),
            spice_cache: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open_image(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif", "bmp", "webp"])
            .pick_file()
        {
            match crate::image_io::load_image(&path) {
                Ok(img) => {
                    self.source_image = Some(img);
                    self.needs_process = true;
                }
                Err(e) => {
                    eprintln!("Error loading image: {e}");
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn open_image(&self) {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };
        let body = match document.body() {
            Some(b) => b,
            None => return,
        };

        let input: web_sys::HtmlInputElement = match document.create_element("input") {
            Ok(el) => match el.dyn_into() {
                Ok(input) => input,
                Err(_) => return,
            },
            Err(_) => return,
        };
        input.set_type("file");
        input.set_accept("image/png,image/jpeg,image/bmp,image/webp");
        let _ = input.style().set_property("display", "none");
        let _ = body.append_child(&input);

        let pending = self.pending_file.clone();
        let input_clone = input.clone();

        let onchange = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let input_ref = &input_clone;
            if let Some(files) = input_ref.files() {
                if let Some(file) = files.get(0) {
                    let reader = match web_sys::FileReader::new() {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                    let reader_clone = reader.clone();
                    let pending_inner = pending.clone();

                    let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
                        if let Ok(result) = reader_clone.result() {
                            let array = js_sys::Uint8Array::new(&result);
                            let bytes = array.to_vec();
                            if let Ok(mut guard) = pending_inner.lock() {
                                *guard = Some(bytes);
                            }
                        }
                    }) as Box<dyn FnMut(_)>);

                    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                    onload.forget();
                    let _ = reader.read_as_array_buffer(&file);
                }
            }
            // Clean up
            if let Some(parent) = input_ref.parent_node() {
                let _ = parent.remove_child(input_ref);
            }
        }) as Box<dyn FnMut(_)>);

        let _ = input.add_event_listener_with_callback("change", onchange.as_ref().unchecked_ref());
        onchange.forget();

        input.click();
    }

    #[cfg(not(target_arch = "wasm32"))]
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
                let (w, h, bytes) = pipeline::process(
                    source,
                    &self.params,
                    &self.spice_cache,
                );
                let img = image::RgbImage::from_raw(w as u32, h as u32, bytes)
                    .expect("Failed to create image buffer");
                if let Err(e) = crate::image_io::save_image(&img, &path) {
                    eprintln!("Error saving image: {e}");
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save_result(&self) {
        if self.preview_texture.is_none() {
            return;
        }
        if let Some(source) = &self.source_image {
            let (w, h, bytes) = pipeline::process(
                source,
                &self.params,
                &self.spice_cache,
            );
            if let Some(img) = image::RgbImage::from_raw(w as u32, h as u32, bytes) {
                let mut buf = std::io::Cursor::new(Vec::new());
                if img.write_to(&mut buf, image::ImageFormat::Png).is_ok() {
                    download_bytes(&buf.into_inner(), "ccd_glitch.png", "image/png");
                }
            }
        }
    }

    fn process_image(&mut self, ctx: &egui::Context) {
        if let Some(source) = &self.source_image {
            // Run SPICE simulation if needed
            {
                use crate::spice::SpiceMode;
                if self.params.spice.mode != SpiceMode::Off {
                    crate::spice::simulate_or_cache(
                        &self.params.spice,
                        self.params.full_well,
                        &mut self.spice_cache,
                    );
                }
            }

            let start = web_time::Instant::now();
            let (w, h, bytes) = pipeline::process(
                source,
                &self.params,
                &self.spice_cache,
            );
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

    fn load_image_from_bytes(&mut self, bytes: &[u8]) {
        match image::load_from_memory(bytes) {
            Ok(img) => {
                self.source_image = Some(img);
                self.needs_process = true;
            }
            Err(e) => {
                log::error!("Failed to load image from bytes: {e}");
            }
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
    params.read_noise = 0.0;
    params.v_cte = config.cte_vertical;
    params.h_cte = config.cte_horizontal;
}

#[cfg(target_arch = "wasm32")]
fn download_bytes(bytes: &[u8], filename: &str, mime: &str) {
    use wasm_bindgen::JsCast;

    let array = js_sys::Uint8Array::from(bytes);
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&array);

    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);

    let blob = match web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &options) {
        Ok(b) => b,
        Err(_) => return,
    };

    let url = match web_sys::Url::create_object_url_with_blob(&blob) {
        Ok(u) => u,
        Err(_) => return,
    };

    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Ok(el) = document.create_element("a") {
                if let Ok(a) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
                    a.set_href(&url);
                    a.set_download(filename);
                    a.click();
                }
            }
        }
    }

    let _ = web_sys::Url::revoke_object_url(&url);
}

impl eframe::App for CcdGlitchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for pending file from WASM file dialog
        #[cfg(target_arch = "wasm32")]
        {
            let mut pending = self.pending_file.lock().unwrap();
            if let Some(bytes) = pending.take() {
                drop(pending);
                self.load_image_from_bytes(&bytes);
                ctx.request_repaint();
            }
        }

        // Check for drag-and-drop
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped_files.first() {
            if let Some(bytes) = &file.bytes {
                self.load_image_from_bytes(bytes);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = &file.path {
                if let Ok(img) = crate::image_io::load_image(path) {
                    self.source_image = Some(img);
                    self.needs_process = true;
                }
            }
        }

        // Top panel: file operations and preset selection
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Image").clicked() {
                    self.open_image();
                }

                #[cfg(target_arch = "wasm32")]
                {
                    ui.label(
                        egui::RichText::new("or drag & drop")
                            .small()
                            .color(egui::Color32::from_rgb(120, 120, 140)),
                    );
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
                if self.source_image.is_some() {
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
                    // Circuit display at top
                    egui::CollapsingHeader::new(
                        egui::RichText::new("Circuit Display").monospace(),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        crate::circuit_display::draw_circuit(ui, &self.params, &self.spice_cache);
                    });

                    // Waveform display
                    egui::CollapsingHeader::new(
                        egui::RichText::new("Waveforms").monospace(),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        crate::waveform_display::draw_waveforms_with_spice(
                            ui,
                            &self.params,
                            &self.spice_cache,
                        );
                    });

                    ui.separator();

                    let mut changed = false;
                    changed |= ui_sensor_config(ui, &mut self.params, self.sensor_preset);

                    {
                        let (spice_changed, force_sim) = ui_spice_mode(ui, &mut self.params, &self.spice_cache);
                        changed |= spice_changed;
                        if force_sim {
                            crate::spice::cache::invalidate(&mut self.spice_cache);
                            self.needs_process = true;
                        }
                    }

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
                    #[cfg(not(target_arch = "wasm32"))]
                    ui.label("Open an image to begin");

                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 3.0);
                            ui.label(
                                egui::RichText::new("Physical CCD Glitch")
                                    .heading()
                                    .color(egui::Color32::from_rgb(0, 220, 110)),
                            );
                            ui.add_space(8.0);
                            ui.label("Drag & drop an image here, or click Open Image");
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Supported: PNG, JPEG, BMP, WebP")
                                    .small()
                                    .color(egui::Color32::from_rgb(120, 120, 140)),
                            );
                        });
                    }
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

fn ui_spice_mode(
    ui: &mut egui::Ui,
    params: &mut PipelineParams,
    cache: &Option<crate::spice::SpiceCache>,
) -> (bool, bool) {
    use crate::spice::SpiceMode;

    let mut changed = false;
    let mut force_simulate = false;

    egui::CollapsingHeader::new(
        egui::RichText::new("SPICE Mode").color(egui::Color32::from_rgb(255, 180, 40)),
    )
    .default_open(false)
    .show(ui, |ui| {
        // Mode selector
        let mode_name = params.spice.mode.name();
        egui::ComboBox::from_label("Mode")
            .selected_text(mode_name)
            .show_ui(ui, |ui| {
                for &mode in SpiceMode::ALL {
                    changed |= ui
                        .selectable_value(&mut params.spice.mode, mode, mode.name())
                        .changed();
                }
            });

        let is_active = params.spice.mode != SpiceMode::Off;

        if is_active {
            ui.separator();
            ui.label("Circuit Parameters");

            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.vdd, 5.0..=20.0)
                        .text("VDD (V)"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.clock_freq_mhz, 0.1..=50.0)
                        .text("Clock (MHz)"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.temperature_k, 200.0..=400.0)
                        .text("Temp (K)"),
                )
                .changed();

            let mut stages = params.spice.shift_register_stages as i32;
            changed |= ui
                .add(
                    egui::Slider::new(&mut stages, 2..=16).text("SR Stages"),
                )
                .changed();
            params.spice.shift_register_stages = stages as usize;

            let mut res = params.spice.transfer_function_resolution as i32;
            changed |= ui
                .add(
                    egui::Slider::new(&mut res, 8..=128).text("TF Resolution"),
                )
                .changed();
            params.spice.transfer_function_resolution = res as usize;

            ui.separator();
            ui.label("Glitch Parameters");

            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.supply_droop, 0.0..=0.8)
                        .text("Supply Droop"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.phase_overlap_ns, 0.0..=100.0)
                        .text("Phase Overlap (ns)"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.missing_pulse_rate, 0.0..=0.5)
                        .text("Missing Pulses"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.charge_injection, 0.0..=2.0)
                        .text("Charge Injection"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut params.spice.substrate_noise, 0.0..=1.0)
                        .text("Substrate Noise"),
                )
                .changed();

            ui.separator();

            if ui.button("Simulate").clicked() {
                force_simulate = true;
            }

            // Status
            let status = crate::spice::cache::cache_summary(cache);
            ui.label(
                egui::RichText::new(status)
                    .small()
                    .color(egui::Color32::from_rgb(120, 120, 140)),
            );

            // Show which stages are replaced
            ui.separator();
            let replaced = match params.spice.mode {
                SpiceMode::FullReadout => "Replaces: Bloom, V-CLK, H-CLK, AMP, ADC",
                SpiceMode::AmplifierOnly => "Replaces: AMP, ADC",
                SpiceMode::TransferCurveOnly => "Replaces: AMP (nonlinearity)",
                SpiceMode::Off => "",
            };
            if !replaced.is_empty() {
                ui.label(
                    egui::RichText::new(replaced)
                        .small()
                        .color(egui::Color32::from_rgb(255, 180, 40)),
                );
            }
        }
    });

    (changed, force_simulate)
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
