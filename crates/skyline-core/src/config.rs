use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub bar: BarConfig,
    pub theme: ThemeConfig,
    pub modules: ModulesConfig,
    pub compositor: CompositorConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bar: BarConfig::default(),
            theme: ThemeConfig::default(),
            modules: ModulesConfig::default(),
            compositor: CompositorConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    /// Bar height in logical pixels.
    pub height: u32,
    /// Margin around the layer surface (top, right, bottom, left).
    pub margin: [i32; 4],
    /// Exclusive zone reserved for the bar. Set 0 for floating islands.
    pub exclusive_zone: i32,
    /// Anchor: "top" or "bottom".
    pub anchor: String,
    /// Optional Wayland output name. When unset, spawn on all screens.
    pub output: Option<String>,
    /// Horizontal padding inside the bar.
    pub padding: u16,
    /// Gap between islands.
    pub island_gap: u16,
    /// Draw separators between modules inside each island.
    pub separators: bool,
    /// Separator glyph between modules (when `separators` is true).
    pub separator: String,
    /// Extra pixels between the bar and a tray context menu (top-anchored bar).
    pub tray_menu_gap: i32,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: 34,
            margin: [8, 12, 0, 12],
            exclusive_zone: 0,
            anchor: "top".into(),
            output: None,
            padding: 4,
            island_gap: 10,
            separators: true,
            separator: "│".into(),
            tray_menu_gap: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub background: [f32; 4],
    pub island_background: [f32; 4],
    pub text: [f32; 4],
    pub muted: [f32; 4],
    pub accent: [f32; 4],
    pub danger: [f32; 4],
    pub separator: [f32; 4],
    pub island_radius: f32,
    /// Vertical and horizontal padding inside every module island.
    pub island_padding: [u16; 2],
    /// Outer margin around every module island (vertical, horizontal).
    pub island_margin: [u16; 2],
    pub font_size: f32,
    /// UI font family (empty = system sans-serif). Example: `"JetBrains Mono"`.
    pub font: String,
    /// Font for emoji glyphs (volume/brightness). Empty = same as `font`.
    pub emoji_font: String,
    /// Height of realtime usage meter bars in logical pixels.
    pub meter_height: f32,
    /// Width of each usage meter column in logical pixels.
    pub meter_width: f32,
    /// Gap between usage meter columns.
    pub meter_gap: f32,
    /// Columns for memory’s fill meter (CPU uses core count; GPU uses device count).
    pub meter_bars: u8,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            // Transparent chrome — islands carry the dark pastel look.
            background: [0.0, 0.0, 0.0, 0.0],
            // Warm charcoal with a soft yellow undertone
            island_background: [0.14, 0.12, 0.08, 0.94],
            // Buttercream
            text: [0.97, 0.94, 0.84, 1.0],
            // Dusty straw
            muted: [0.66, 0.60, 0.46, 1.0],
            // Soft pastel yellow
            accent: [0.93, 0.86, 0.58, 1.0],
            // Soft apricot
            danger: [0.90, 0.62, 0.52, 1.0],
            // Dim honey separator
            separator: [0.42, 0.38, 0.26, 0.50],
            island_radius: 14.0,
            island_padding: [4, 10],
            island_margin: [0, 0],
            font_size: 13.0,
            font: String::new(),
            emoji_font: String::new(),
            meter_height: 16.0,
            meter_width: 3.0,
            meter_gap: 2.0,
            meter_bars: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModulesConfig {
    pub left: Vec<ModuleKind>,
    pub center: Vec<ModuleKind>,
    pub right: Vec<ModuleKind>,
    pub clock: ClockConfig,
    pub custom: Vec<CustomModuleConfig>,
    pub sys: SysConfig,
    pub volume: VolumeConfig,
    pub brightness: BrightnessConfig,
    pub window: WindowConfig,
    pub network: NetworkConfig,
    pub workspaces: WorkspacesConfig,
    pub taskbar: TaskbarConfig,
    pub cpu: MeterClickConfig,
    pub memory: MeterClickConfig,
    pub gpu: MeterClickConfig,
    pub tray: bool,
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            left: vec![ModuleKind::Workspaces, ModuleKind::Taskbar, ModuleKind::Window],
            center: vec![ModuleKind::Clock],
            right: vec![
                ModuleKind::Cpu,
                ModuleKind::Memory,
                ModuleKind::Gpu,
                ModuleKind::Network,
                ModuleKind::Brightness,
                ModuleKind::Volume,
                ModuleKind::Tray,
            ],
            clock: ClockConfig::default(),
            custom: vec![],
            sys: SysConfig::default(),
            volume: VolumeConfig::default(),
            brightness: BrightnessConfig::default(),
            window: WindowConfig::default(),
            network: NetworkConfig::default(),
            workspaces: WorkspacesConfig::default(),
            taskbar: TaskbarConfig::default(),
            cpu: MeterClickConfig::default(),
            memory: MeterClickConfig::default(),
            gpu: MeterClickConfig::default(),
            tray: true,
        }
    }
}

impl ModulesConfig {
    /// Shell command for a module mouse button, if configured.
    pub fn click_command(&self, kind: &ModuleKind, right: bool) -> Option<&str> {
        let actions = match kind {
            ModuleKind::Workspaces => &self.workspaces.clicks,
            ModuleKind::Window => &self.window.clicks,
            ModuleKind::Taskbar => &self.taskbar.clicks,
            ModuleKind::Clock => &self.clock.clicks,
            ModuleKind::Cpu => &self.cpu.clicks,
            ModuleKind::Memory => &self.memory.clicks,
            ModuleKind::Gpu => &self.gpu.clicks,
            ModuleKind::Network => &self.network.clicks,
            ModuleKind::Volume => &self.volume.clicks,
            ModuleKind::Brightness => &self.brightness.clicks,
            ModuleKind::Tray => return None,
            ModuleKind::Custom(id) => {
                let module = self.custom.iter().find(|m| m.id == *id)?;
                return if right {
                    module.on_right_click.as_deref()
                } else {
                    module.on_click.as_deref()
                };
            }
        };
        if right {
            actions.on_right_click.as_deref()
        } else {
            actions.on_click.as_deref()
        }
    }

    pub fn clicks_for(&self, kind: &ModuleKind) -> ClickActions {
        match kind {
            ModuleKind::Workspaces => self.workspaces.clicks.clone(),
            ModuleKind::Window => self.window.clicks.clone(),
            ModuleKind::Taskbar => self.taskbar.clicks.clone(),
            ModuleKind::Clock => self.clock.clicks.clone(),
            ModuleKind::Cpu => self.cpu.clicks.clone(),
            ModuleKind::Memory => self.memory.clicks.clone(),
            ModuleKind::Gpu => self.gpu.clicks.clone(),
            ModuleKind::Network => self.network.clicks.clone(),
            ModuleKind::Volume => self.volume.clicks.clone(),
            ModuleKind::Brightness => self.brightness.clicks.clone(),
            ModuleKind::Tray => ClickActions::default(),
            ModuleKind::Custom(id) => self
                .custom
                .iter()
                .find(|m| m.id == *id)
                .map(|m| ClickActions {
                    on_click: m.on_click.clone(),
                    on_right_click: m.on_right_click.clone(),
                })
                .unwrap_or_default(),
        }
    }
}

/// Left / right click shell commands (`sh -c …`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClickActions {
    /// Left-click command. Aliases: `on_left_click`.
    #[serde(alias = "on_left_click", default)]
    pub on_click: Option<String>,
    /// Right-click command.
    #[serde(default)]
    pub on_right_click: Option<String>,
}

impl ClickActions {
    pub fn has_any(&self) -> bool {
        self.on_click.as_ref().is_some_and(|s| !s.is_empty())
            || self.on_right_click.as_ref().is_some_and(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleKind {
    Workspaces,
    Window,
    Taskbar,
    Clock,
    Cpu,
    Memory,
    Gpu,
    Network,
    Volume,
    Brightness,
    Tray,
    Custom(String),
}

impl ModuleKind {
    pub fn as_config_str(&self) -> String {
        match self {
            Self::Workspaces => "workspaces".into(),
            Self::Window => "window".into(),
            Self::Taskbar => "taskbar".into(),
            Self::Clock => "clock".into(),
            Self::Cpu => "cpu".into(),
            Self::Memory => "memory".into(),
            Self::Gpu => "gpu".into(),
            Self::Network => "network".into(),
            Self::Volume => "volume".into(),
            Self::Brightness => "brightness".into(),
            Self::Tray => "tray".into(),
            Self::Custom(id) => format!("custom:{id}"),
        }
    }
}

impl fmt::Display for ModuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_config_str())
    }
}

impl Serialize for ModuleKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_config_str())
    }
}

impl<'de> Deserialize<'de> for ModuleKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "workspaces" => Self::Workspaces,
            "window" => Self::Window,
            "taskbar" => Self::Taskbar,
            "clock" => Self::Clock,
            "cpu" => Self::Cpu,
            "memory" => Self::Memory,
            "gpu" => Self::Gpu,
            "network" => Self::Network,
            "volume" => Self::Volume,
            "brightness" => Self::Brightness,
            "tray" => Self::Tray,
            other if other.starts_with("custom:") => {
                Self::Custom(other.trim_start_matches("custom:").to_string())
            }
            other => Self::Custom(other.to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeConfig {
    /// Scroll delta as a fraction of full scale (0.02 = 2% per notch).
    pub step: f64,
    /// Soft ceiling when scrolling up (PipeWire/Pulse often allow >100%).
    pub max_percent: f64,
    /// Detect Bluetooth sinks and show a headset glyph instead of speakers.
    pub detect_bluetooth: bool,
    /// Append a short device name when Bluetooth (or always if `show_device`).
    pub show_device: bool,
    /// Show numeric percent next to the glyph.
    pub show_percent: bool,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            step: 0.02,
            max_percent: 150.0,
            detect_bluetooth: true,
            show_device: false,
            show_percent: true,
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightnessConfig {
    /// Percent points per scroll notch.
    pub step: f64,
    pub show_percent: bool,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            step: 2.0,
            show_percent: true,
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Max characters for the focused window title before ellipsis.
    pub max_chars: usize,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            max_chars: 42,
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Show signal strength percent when available.
    pub show_strength: bool,
    /// Prefer Wi‑Fi SSID / ethernet connection name over interface (`wlan0`).
    pub show_name: bool,
    /// Truncate the displayed name after this many characters.
    pub max_chars: usize,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            show_strength: true,
            show_name: true,
            max_chars: 24,
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub format: String,
    pub tooltip_format: String,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: "%a %b %d  %I:%M %p".into(),
            tooltip_format: "%Y-%m-%d %I:%M:%S %p".into(),
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacesConfig {
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            clicks: ClickActions::default(),
        }
    }
}

/// Open windows on the active workspace (icon taskbar).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskbarConfig {
    /// Icon square size in logical pixels (not including highlight padding).
    #[serde(alias = "icon_size")]
    pub width: f32,
    /// Padding between the icon and the chip edge / focus highlight border.
    pub padding: f32,
    /// Gap between icon wrappers.
    pub gap: f32,
    /// Max icons shown (0 = unlimited).
    pub max_items: usize,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for TaskbarConfig {
    fn default() -> Self {
        Self {
            width: 20.0,
            padding: 4.0,
            gap: 2.0,
            max_items: 16,
            clicks: ClickActions::default(),
        }
    }
}

/// CPU / memory / GPU meter modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MeterClickConfig {
    /// Display label (empty = module default: cpu / ram / gpu).
    pub label: String,
    /// Layout tokens: `{label}`, `{bar}` / `{meter}`, `{percent}` / `{pct}`.
    /// Example: `"{label} {bar} {percent}"` or `"{percent}%"`.
    pub format: String,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for MeterClickConfig {
    fn default() -> Self {
        Self {
            label: String::new(),
            format: "{label} {bar} {percent}".into(),
            clicks: ClickActions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomModuleConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    #[serde(default, alias = "on_left_click")]
    pub on_click: Option<String>,
    #[serde(default)]
    pub on_right_click: Option<String>,
    #[serde(default)]
    pub json: bool,
}

fn default_interval() -> u64 {
    5_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SysConfig {
    pub refresh_ms: u64,
}

impl Default for SysConfig {
    fn default() -> Self {
        Self { refresh_ms: 500 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorConfig {
    /// Auto-detect from env (`NIRI_SOCKET` / `HYPRLAND_INSTANCE_SIGNATURE`).
    pub backend: CompositorBackendKind,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            backend: CompositorBackendKind::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompositorBackendKind {
    #[default]
    Auto,
    Niri,
    Hyprland,
    None,
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let path = Self::default_path();
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        // Accept legacy flat keys `volume_step` / `brightness_step` under [modules].
        let raw = migrate_legacy_module_keys(&raw);
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config {}", path.display()))?;
        Ok(config)
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("skyline")
            .join("config.toml")
    }

    pub fn example_toml() -> String {
        EXAMPLE_TOML.into()
    }
}

/// Rewrite deprecated `volume_step` / `brightness_step` into nested tables when needed.
fn migrate_legacy_module_keys(raw: &str) -> String {
    let mut volume_step: Option<String> = None;
    let mut brightness_step: Option<String> = None;
    let mut out = String::with_capacity(raw.len() + 64);
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("volume_step") {
            if rest.trim_start().starts_with('=') {
                volume_step = Some(rest.trim_start().trim_start_matches('=').trim().to_string());
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("brightness_step") {
            if rest.trim_start().starts_with('=') {
                brightness_step = Some(rest.trim_start().trim_start_matches('=').trim().to_string());
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if volume_step.is_none() && brightness_step.is_none() {
        return raw.to_string();
    }
    if let Some(step) = volume_step {
        if !out.contains("[modules.volume]") {
            out.push_str("\n[modules.volume]\n");
            out.push_str(&format!("step = {step}\n"));
        }
    }
    if let Some(step) = brightness_step {
        if !out.contains("[modules.brightness]") {
            out.push_str("\n[modules.brightness]\n");
            out.push_str(&format!("step = {step}\n"));
        }
    }
    out
}

const EXAMPLE_TOML: &str = r#"# Skyline configuration (~/.config/skyline/config.toml)
# All keys are shown with their defaults (except exclusive_zone, which is
# commonly set higher so windows clear the bar).

[bar]
# Bar height in logical pixels
height = 34
# Layer margin: top, right, bottom, left
margin = [8, 12, 0, 12]
# Compositor exclusive zone (0 = float over windows)
exclusive_zone = 54
# "top" or "bottom"
anchor = "top"
# Optional Wayland output name; omit / comment to use all screens
# output = "DP-1"
# Inner padding of the bar surface
padding = 4
# Gap between left / center / right islands
island_gap = 10
# Draw separators between modules inside each island
separators = true
separator = "│"
# Pixels between the bar bottom and a tray right-click menu
tray_menu_gap = 4

[theme]
# Soft yellow pastel on dark — colors are RGBA floats in 0..=1
background = [0.0, 0.0, 0.0, 0.0]
island_background = [0.14, 0.12, 0.08, 0.94]
text = [0.97, 0.94, 0.84, 1.0]
muted = [0.66, 0.60, 0.46, 1.0]
accent = [0.93, 0.86, 0.58, 1.0]
danger = [0.90, 0.62, 0.52, 1.0]
separator = [0.42, 0.38, 0.26, 0.50]
island_radius = 14.0
# Vertical, horizontal padding shared by every module island
island_padding = [4, 10]
# Outer margin around each island (vertical, horizontal)
island_margin = [0, 0]
font_size = 13.0
# UI font family (empty = system default). Examples: "Inter", "JetBrains Mono"
font = ""
# Font used for emoji glyphs (volume / brightness). Empty = same as font
emoji_font = ""
# Realtime CPU (per-core) / RAM (fill) / GPU (per-device) meters
meter_height = 16.0
meter_width = 3.0
meter_gap = 2.0
meter_bars = 8

[compositor]
# auto | niri | hyprland | none
backend = "auto"

[modules]
# Available: workspaces, taskbar, window, clock, cpu, memory, gpu, network,
# brightness, volume, tray, custom:<id>
left = ["workspaces", "taskbar", "window"]
center = ["clock"]
right = ["cpu", "memory", "gpu", "network", "brightness", "volume", "tray"]
# Host StatusNotifierItem tray
tray = true

[modules.clock]
format = "%a %b %d  %I:%M %p"
tooltip_format = "%Y-%m-%d %I:%M:%S %p"
# on_click = "gsimplecal"
# on_right_click = "xdg-open https://calendar.google.com"

[modules.sys]
# CPU / RAM / GPU refresh interval
refresh_ms = 500

[modules.workspaces]
# Left-click on a workspace button still focuses it.
# Optional commands for the workspaces strip:
# on_click = "niri msg action focus-workspace-previous"
# on_right_click = "niri msg action focus-workspace-next"

[modules.taskbar]
# Icon size (logical px). Chip / focus highlight grows with padding.
width = 20.0
# Space between the icon and the yellow focus border / chip edge
padding = 4.0
gap = 2.0
max_items = 16

[modules.window]
# Truncate focused window title after this many characters
max_chars = 42
# on_click = "niri msg action center-window"
# on_right_click = "niri msg action close-window"

[modules.cpu]
# label = "cpu"
# Tokens: {label} {bar}/{meter} {percent}/{pct} — e.g. "{label} {bar} {percent}"
# format = "{label} {bar} {percent}"
# on_click = "ghostty -e btop"
# on_right_click = "gnome-system-monitor"

[modules.memory]
# label = "ram"
# format = "{label} {bar} {percent}"
# on_click = "ghostty -e btop"

[modules.gpu]
# label = "gpu"
# format = "{label} {bar} {percent}"
# on_click = "ghostty -e nvtop"

[modules.network]
show_strength = true
# Prefer Wi‑Fi SSID / ethernet connection name over wlan0/eth0
show_name = true
max_chars = 24
# on_click = "nm-connection-editor"
# on_right_click = "ghostty -e nmtui"

[modules.volume]
# Fraction of full scale per scroll notch (0.02 = 2%)
step = 0.02
# Soft ceiling when scrolling up
max_percent = 150.0
# Use headset glyph when the default sink is Bluetooth (bluez)
detect_bluetooth = true
# Append short device name (useful with Bluetooth)
show_device = false
show_percent = true
# Left-click defaults to mute when on_click is unset
# on_click = "pavucontrol"
# on_right_click = "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"

[modules.brightness]
# Percent points per scroll notch
step = 2.0
show_percent = true
# on_click = "wl-gammarelay-rs"
# on_right_click = "brightnessctl set 50%"

# [[modules.custom]]
# id = "weather"
# command = "curl"
# args = ["-s", "https://wttr.in/?format=1"]
# interval_ms = 600000
# on_click = "xdg-open https://wttr.in"
# on_right_click = "notify-send weather refreshed"
# json = false
"#;
