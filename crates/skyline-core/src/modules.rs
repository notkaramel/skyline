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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrightnessSnapshot {
    pub percent: f64,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSnapshot {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    TrayItems(Vec<TrayItemSnapshot>),
    TrayMenu(TrayMenuSnapshot),
    /// Config file changed and parsed successfully.
    ConfigReloaded(Box<crate::Config>),
    Error(String),
}
