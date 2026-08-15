use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SysSnapshot {
    pub cpu_percent: f32,
    /// Per-logical-core usage (0..=100), same order as the OS.
    #[serde(default)]
    pub cpu_per_core: Vec<f32>,
    pub memory_percent: f32,
    pub memory_used_gb: f32,
    pub memory_total_gb: f32,
    pub gpu_percent: Option<f32>,
    /// Per-GPU device usage (0..=100) when multiple adapters are found.
    #[serde(default)]
    pub gpu_per_device: Vec<f32>,
    pub gpu_label: Option<String>,
}

impl SysSnapshot {
    /// True when meters would look the same (avoids pointless UI redraws).
    pub fn visually_eq(&self, other: &Self) -> bool {
        const EPS: f32 = 0.4;
        float_eq(self.cpu_percent, other.cpu_percent, EPS)
            && float_eq(self.memory_percent, other.memory_percent, EPS)
            && float_eq(self.memory_used_gb, other.memory_used_gb, 0.02)
            && float_eq(self.memory_total_gb, other.memory_total_gb, 0.02)
            && opt_float_eq(self.gpu_percent, other.gpu_percent, EPS)
            && self.gpu_label == other.gpu_label
            && self.cpu_per_core.len() == other.cpu_per_core.len()
            && self
                .cpu_per_core
                .iter()
                .zip(other.cpu_per_core.iter())
                .all(|(a, b)| float_eq(*a, *b, EPS))
            && self.gpu_per_device.len() == other.gpu_per_device.len()
            && self
                .gpu_per_device
                .iter()
                .zip(other.gpu_per_device.iter())
                .all(|(a, b)| float_eq(*a, *b, EPS))
    }
}

fn float_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn opt_float_eq(a: Option<f32>, b: Option<f32>, eps: f32) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => float_eq(a, b, eps),
        _ => false,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NetworkSnapshot {
    pub connected: bool,
    /// Display label (SSID, connection name, or interface).
    pub label: String,
    pub strength: Option<u8>,
    /// Underlying interface when known (`wlan0`, `enp…`).
    pub interface: Option<String>,
    /// `wifi`, `ethernet`, or other.
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeSnapshot {
    pub percent: f64,
    pub muted: bool,
    /// True when the default sink looks like a Bluetooth device.
    pub bluetooth: bool,
    /// Short sink / device label when known.
    pub device: Option<String>,
}

impl VolumeSnapshot {
    pub fn visually_eq(&self, other: &Self) -> bool {
        self.muted == other.muted
            && self.bluetooth == other.bluetooth
            && self.device == other.device
            && (self.percent - other.percent).abs() < 0.5
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrightnessSnapshot {
    pub percent: f64,
    pub available: bool,
}

impl BrightnessSnapshot {
    pub fn visually_eq(&self, other: &Self) -> bool {
        self.available == other.available && (self.percent - other.percent).abs() < 0.5
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSnapshot {
    pub id: String,
    pub text: String,
}

/// Current conditions from wttr.in (`format=j1`). Stores both °C and °F so the
/// configured unit can hot-reload without refetching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WeatherSnapshot {
    pub emoji: String,
    pub condition: String,
    pub location: String,
    pub temp_c: f32,
    pub temp_f: f32,
    pub feels_c: f32,
    pub feels_f: f32,
    pub humidity: Option<u8>,
    pub wind_kmph: Option<f32>,
    pub wind_mph: Option<f32>,
    pub wind_dir: Option<String>,
    pub precip_mm: Option<f32>,
    pub pressure_hpa: Option<f32>,
    pub uv_index: Option<u8>,
    pub high_c: Option<f32>,
    pub high_f: Option<f32>,
    pub low_c: Option<f32>,
    pub low_f: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrayItemSnapshot {
    pub id: String,
    /// StatusNotifierItem `Id` (application identity).
    pub app_id: String,
    pub title: String,
    pub icon_name: Option<String>,
    pub icon_theme_path: Option<String>,
    pub attention_icon_name: Option<String>,
    pub needs_attention: bool,
    pub icon_pixmap: Option<TrayPixmap>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrayPixmap {
    pub width: i32,
    pub height: i32,
    /// ARGB32 bytes.
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayMenuSnapshot {
    pub item_id: String,
    pub entries: Vec<TrayMenuEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayMenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub separator: bool,
    pub children: Vec<TrayMenuEntry>,
}

/// Events pushed from background services into the UI.
#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Tick,
    Clock(String),
    Compositor(crate::CompositorState),
    Sys(SysSnapshot),
    Network(NetworkSnapshot),
    Volume(VolumeSnapshot),
    Brightness(BrightnessSnapshot),
    Custom(CustomSnapshot),
    Weather(WeatherSnapshot),
    TrayItems(Vec<TrayItemSnapshot>),
    TrayMenu(TrayMenuSnapshot),
    /// Config file changed and parsed successfully.
    ConfigReloaded(Box<crate::Config>),
    Error(String),
}
