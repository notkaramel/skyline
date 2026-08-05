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
        // Must not parse EXAMPLE_TOML here: `#[serde(default)]` on Config calls
        // `Config::default()` while deserializing, which would recurse forever.
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
    /// Extra pixels between the bar and a tray context menu.
    pub tray_menu_gap: i32,
    /// Where tray context menus are anchored.
    pub tray_menu_align: TrayMenuAlign,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: 42,
            margin: [6, 10, 0, 10],
            exclusive_zone: 48,
            anchor: "top".into(),
            output: None,
            padding: 4,
            island_gap: 12,
            separators: true,
            separator: "│".into(),
            tray_menu_gap: 4,
            tray_menu_align: TrayMenuAlign::Icon,
        }
    }
}

/// Horizontal placement of StatusNotifierItem context menus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayMenuAlign {
    /// Draw the menu directly below (top bar) or above (bottom bar) the icon.
    #[default]
    Icon,
    /// Draw the menu at the trailing edge of the bar (right on LTR layouts).
    End,
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
    /// Island outline color (neobrutalism: hard accent border).
    pub island_border: [f32; 4],
    /// Island outline width in logical pixels (`0` = hairline / none).
    pub island_border_width: f32,
    /// Drop-shadow color for islands (neobrutalism: hard dark offset).
    pub island_shadow: [f32; 4],
    /// Shadow offset in logical pixels `[x, y]` (wofi-style hard cast).
    pub island_shadow_offset: [f32; 2],
    /// Shadow blur radius (`0` = hard edge).
    pub island_shadow_blur: f32,
    /// Vertical and horizontal padding inside every module island.
    pub island_padding: [u16; 2],
    /// Outer margin around every module island (vertical, horizontal).
    pub island_margin: [u16; 2],
    pub font_size: f32,
    /// UI font family (empty = system sans-serif). Example: `"JetBrains Mono"`.
    pub font: String,
    /// Font for emoji glyphs (brightness, clock). Empty = same as `font`.
    /// Volume uses Waybar-style Nerd Font icons when a Nerd Font is available.
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
            // Transparent chrome — islands carry the look.
            background: [0.0, 0.0, 0.0, 0.0],
            // Deep pastel purple (#322c4a) — matches niri/wofi neobrutalism
            island_background: [0.196, 0.173, 0.290, 1.0],
            // #ede8fa
            text: [0.929, 0.910, 0.980, 1.0],
            // #b4accf
            muted: [0.706, 0.675, 0.812, 1.0],
            // #a894e0
            accent: [0.659, 0.580, 0.878, 1.0],
            // #e08eb8
            danger: [0.878, 0.557, 0.722, 1.0],
            // #8a7bc4 @ 55%
            separator: [0.541, 0.482, 0.769, 0.55],
            island_radius: 0.0,
            island_border: [0.659, 0.580, 0.878, 1.0],
            island_border_width: 3.0,
            // #0a0814 — wofi hard drop shadow
            island_shadow: [0.039, 0.031, 0.078, 1.0],
            island_shadow_offset: [6.0, 6.0],
            island_shadow_blur: 0.0,
            island_padding: [4, 10],
            island_margin: [0, 2],
            font_size: 14.0,
            font: "Fira Sans".into(),
            emoji_font: "Noto Color Emoji".into(),
            meter_height: 16.0,
            meter_width: 4.0,
            meter_gap: 2.0,
            meter_bars: 12,
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
                ModuleKind::Tray,
                ModuleKind::Cpu,
                ModuleKind::Memory,
                ModuleKind::Gpu,
                ModuleKind::Network,
                ModuleKind::Brightness,
                ModuleKind::Volume,
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
            cpu: MeterClickConfig {
                label: "CPU".into(),
                format: "{label} {bar}".into(),
                clicks: ClickActions {
                    on_click: Some("alacritty -e btop".into()),
                    on_right_click: None,
                },
            },
            memory: MeterClickConfig {
                label: "RAM".into(),
                format: "{label} {bar}".into(),
                clicks: ClickActions::default(),
            },
            gpu: MeterClickConfig {
                label: "GPU".into(),
                format: "{label} {bar}".into(),
                clicks: ClickActions {
                    on_click: Some("alacritty -e amdgpu_top".into()),
                    on_right_click: None,
                },
            },
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
            clicks: ClickActions {
                on_click: Some("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".into()),
                on_right_click: Some("pwvucontrol".into()),
            },
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
            max_chars: 120,
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
            show_strength: false,
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
            format: "%a %b %d ☀️ %I:%M %p".into(),
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
    /// Focused chip background border width. When unset, uses theme `island_border_width`.
    #[serde(default)]
    pub border_width: Option<f32>,
    /// Max icons shown (0 = unlimited).
    pub max_items: usize,
    #[serde(flatten)]
    pub clicks: ClickActions,
}

impl Default for TaskbarConfig {
    fn default() -> Self {
        Self {
            width: 24.0,
            padding: 2.0,
            gap: 4.0,
            border_width: None,
            max_items: 12,
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
            format: "{label} {bar}".into(),
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
        Self { refresh_ms: 1000 }
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
            Ok(Self::from_example_toml())
        }
    }

    /// Full annotated defaults from `examples/config.toml`.
    pub fn from_example_toml() -> Self {
        toml::from_str(EXAMPLE_TOML).expect("embedded default config is valid TOML")
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

const EXAMPLE_TOML: &str = include_str!("../../../examples/config.toml");
