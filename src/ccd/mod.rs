pub mod adc;
pub mod amplifier;
pub mod blooming;
pub mod sensor;
pub mod transfer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CcdArchitecture {
    FullFrame,
    Interline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SensorPreset {
    Kaf6303,
    Kaf4320,
    Kaf16803,
    Icx059cl,
    Custom,
}

impl SensorPreset {
    pub const ALL: &[SensorPreset] = &[
        SensorPreset::Kaf6303,
        SensorPreset::Kaf4320,
        SensorPreset::Kaf16803,
        SensorPreset::Icx059cl,
        SensorPreset::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SensorPreset::Kaf6303 => "KAF-6303",
            SensorPreset::Kaf4320 => "KAF-4320",
            SensorPreset::Kaf16803 => "KAF-16803",
            SensorPreset::Icx059cl => "ICX059CL",
            SensorPreset::Custom => "Custom",
        }
    }

    pub fn config(self) -> SensorConfig {
        match self {
            SensorPreset::Kaf6303 => SensorConfig {
                width: 3072,
                height: 2048,
                pixel_size_um: (9.0, 9.0),
                full_well_no_abg: 100_000.0,
                full_well_abg: 40_000.0,
                architecture: CcdArchitecture::FullFrame,
                v_phases: 3,
                read_noise_e: 11.0,
                dark_current_pa_cm2: 15.0,
                cte_vertical: 0.999995,
                cte_horizontal: 0.999999,
                gain_uv_per_e: 7.5,
            },
            SensorPreset::Kaf4320 => SensorConfig {
                width: 2048,
                height: 2048,
                pixel_size_um: (24.0, 24.0),
                full_well_no_abg: 150_000.0,
                full_well_abg: 90_000.0,
                architecture: CcdArchitecture::FullFrame,
                v_phases: 3,
                read_noise_e: 12.0,
                dark_current_pa_cm2: 20.0,
                cte_vertical: 0.999995,
                cte_horizontal: 0.999999,
                gain_uv_per_e: 4.5,
            },
            SensorPreset::Kaf16803 => SensorConfig {
                width: 4096,
                height: 4096,
                pixel_size_um: (9.0, 9.0),
                full_well_no_abg: 100_000.0,
                full_well_abg: 60_000.0,
                architecture: CcdArchitecture::FullFrame,
                v_phases: 2,
                read_noise_e: 9.0,
                dark_current_pa_cm2: 5.0,
                cte_vertical: 0.999998,
                cte_horizontal: 0.999999,
                gain_uv_per_e: 8.0,
            },
            SensorPreset::Icx059cl => SensorConfig {
                width: 500,
                height: 582,
                pixel_size_um: (9.8, 6.3),
                full_well_no_abg: 30_000.0,
                full_well_abg: 30_000.0,
                architecture: CcdArchitecture::Interline,
                v_phases: 4,
                read_noise_e: 40.0,
                dark_current_pa_cm2: 15.0,
                cte_vertical: 0.99999,
                cte_horizontal: 0.99999,
                gain_uv_per_e: 10.0,
            },
            SensorPreset::Custom => SensorConfig {
                width: 1024,
                height: 1024,
                pixel_size_um: (9.0, 9.0),
                full_well_no_abg: 100_000.0,
                full_well_abg: 40_000.0,
                architecture: CcdArchitecture::FullFrame,
                v_phases: 3,
                read_noise_e: 10.0,
                dark_current_pa_cm2: 10.0,
                cte_vertical: 0.999995,
                cte_horizontal: 0.999999,
                gain_uv_per_e: 8.0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct SensorConfig {
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub pixel_size_um: (f64, f64),
    pub full_well_no_abg: f64,
    pub full_well_abg: f64,
    #[allow(dead_code)]
    pub architecture: CcdArchitecture,
    #[allow(dead_code)]
    pub v_phases: u8,
    #[allow(dead_code)]
    pub read_noise_e: f64,
    #[allow(dead_code)]
    pub dark_current_pa_cm2: f64,
    pub cte_vertical: f64,
    pub cte_horizontal: f64,
    #[allow(dead_code)]
    pub gain_uv_per_e: f64,
}
