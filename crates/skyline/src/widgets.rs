use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};

use iced::widget::{button, container, image, mouse_area, row, space, text, Column};
use iced::widget::container::Style as ContainerStyle;
use iced::{Background, Border, Color, Element, Length};
use skyline_core::{
    ClickActions, CompositorState, ModuleKind, ThemeConfig, TrayItemSnapshot, TrayMenuSnapshot,
    TrayPixmap,
};

use crate::app::Message;
use crate::style;

pub fn workspaces<'a>(
    state: &'a CompositorState,
    output: Option<&str>,
    theme: &'a ThemeConfig,
) -> Element<'a, Message> {
    let list = state.workspaces_for_output(output);
    if list.is_empty() {
        return text("—")
            .size(theme.font_size)
            .font(style::ui_font(theme))
            .color(style::rgba(theme.muted))
            .into();
    }
    let buttons = list.into_iter().map(|ws| {
        let label = if ws.name.chars().all(|c| c.is_ascii_digit()) {
            ws.name.clone()
        } else if ws.name.len() <= 3 {
            ws.name.clone()
        } else {
            ws.index.to_string()
        };
        button(text(label).size(theme.font_size).font(style::ui_font(theme)))
            .padding([0, 6])
            .style(style::workspace_button(theme, ws.active, ws.urgent))
            .on_press(Message::FocusWorkspace(ws.id))
            .into()
    });
    row(buttons)
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into()
}

/// Cava-style vertical meter from a history of 0..=100 samples.
pub fn cava_meter<'a>(
    default_label: &'a str,
    history: &'a VecDeque<f32>,
    current: f32,
    theme: &'a ThemeConfig,
    meter: &'a skyline_core::MeterClickConfig,
) -> Element<'a, Message> {
    let label = if meter.label.trim().is_empty() {
        default_label
    } else {
        meter.label.as_str()
    };
    let format = if meter.format.trim().is_empty() {
        "{label} {bar} {percent}"
    } else {
        meter.format.as_str()
    };

    let bars = theme.meter_bars.max(1) as usize;
    let height = theme.meter_height.max(4.0);
    let bar_w = theme.meter_width.max(1.0);
    let gap = theme.meter_gap.max(0.0);
    let accent = style::rgba(theme.accent);
    let track = Color::from_rgba(1.0, 1.0, 1.0, 0.10);
    let ui_font = style::ui_font(theme);

    let mut samples: Vec<f32> = history.iter().copied().collect();
    while samples.len() < bars {
        samples.insert(0, 0.0);
    }
    if samples.len() > bars {
        samples = samples[samples.len() - bars..].to_vec();
    }
    if let Some(last) = samples.last_mut() {
        *last = current.clamp(0.0, 100.0);
    }

    let bar_row = |samples: &[f32]| -> Element<'a, Message> {
        let columns = samples.iter().copied().enumerate().map(|(i, v)| {
            let t = (v / 100.0).clamp(0.05, 1.0);
            let age = 1.0 - (bars.saturating_sub(i + 1) as f32) * 0.04;
            let fill_h = (height * t * age).max(2.0);
            let bar = container(space::Space::new())
                .width(bar_w)
                .height(fill_h)
                .style(move |_| ContainerStyle {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 1.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                });
            container(bar)
                .width(bar_w)
                .height(height)
                .align_y(iced::Alignment::End)
                .style(move |_| ContainerStyle {
                    background: Some(Background::Color(track)),
                    border: Border {
                        radius: 1.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                })
                .into()
        });
        row(columns)
            .spacing(gap)
            .align_y(iced::Alignment::Center)
            .into()
    };

    let mut children: Vec<Element<'a, Message>> = Vec::new();
    for part in parse_meter_format(format) {
        match part {
            MeterPart::Label => children.push(
                text(label.to_string())
                    .size(theme.font_size)
                    .font(ui_font)
                    .color(style::rgba(theme.muted))
                    .into(),
            ),
            MeterPart::Bar => children.push(bar_row(&samples)),
            MeterPart::Percent => children.push(style::percent_slot(
                f64::from(current),
                theme,
                theme.font_size * 0.9,
                style::rgba(theme.text),
            )),
            MeterPart::Text(s) => {
                let s = s.trim();
                if !s.is_empty() {
                    children.push(
                        text(s.to_string())
                            .size(theme.font_size)
                            .font(ui_font)
                            .color(style::rgba(theme.muted))
                            .into(),
                    );
                }
            }
        }
    }

    row(children)
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

#[derive(Debug, Clone, Copy)]
enum MeterPart<'a> {
    Label,
    Bar,
    Percent,
    Text(&'a str),
}

fn parse_meter_format(fmt: &str) -> Vec<MeterPart<'_>> {
    let mut out = Vec::new();
    let bytes = fmt.as_bytes();
    let mut i = 0;
    let mut lit_start = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(rel_end) = fmt[i + 1..].find('}') {
                let end = i + 1 + rel_end;
                if i > lit_start {
                    out.push(MeterPart::Text(&fmt[lit_start..i]));
                }
                match &fmt[i + 1..end] {
                    "label" => out.push(MeterPart::Label),
                    "bar" | "meter" => out.push(MeterPart::Bar),
                    "percent" | "pct" => out.push(MeterPart::Percent),
                    _ => out.push(MeterPart::Text(&fmt[i..=end])),
                }
                i = end + 1;
                lit_start = i;
                continue;
            }
        }
        i += 1;
    }
    if lit_start < fmt.len() {
        out.push(MeterPart::Text(&fmt[lit_start..]));
    }
    if out.is_empty() {
        out.push(MeterPart::Label);
        out.push(MeterPart::Bar);
        out.push(MeterPart::Percent);
    }
    out
}

pub fn module_separator<'a>(glyph: &'a str, theme: &'a ThemeConfig) -> Element<'a, Message> {
    text(glyph)
        .size(theme.font_size)
        .font(style::ui_font(theme))
        .color(style::rgba(theme.separator))
        .into()
}

/// Wrap a module element with configured left / right click handlers.
pub fn with_clicks<'a>(
    content: Element<'a, Message>,
    kind: ModuleKind,
    clicks: &ClickActions,
) -> Element<'a, Message> {
    let left = clicks.on_click.as_ref().is_some_and(|s| !s.is_empty());
    let right = clicks
        .on_right_click
        .as_ref()
        .is_some_and(|s| !s.is_empty());
    if !left && !right {
        return content;
    }
    let mut area = mouse_area(content);
    if left {
        let kind = kind.clone();
        area = area.on_press(Message::ModuleClick {
            kind,
            right: false,
        });
    }
    if right {
        let kind = kind.clone();
        area = area.on_right_press(Message::ModuleClick {
            kind,
            right: true,
        });
    }
    area.into()
}

pub fn volume<'a>(
    percent: f64,
    muted: bool,
    bluetooth: bool,
    device: Option<&str>,
    theme: &'a ThemeConfig,
    step: f64,
    show_percent: bool,
    show_device: bool,
    clicks: &ClickActions,
) -> Element<'a, Message> {
    let emoji = volume_emoji(muted, percent, bluetooth);
    let mut children: Vec<Element<'a, Message>> = vec![
        container(
            text(emoji)
                .size(theme.font_size)
                .font(style::emoji_font(theme))
                .color(style::rgba(theme.text))
                .line_height(iced::widget::text::LineHeight::Relative(1.0)),
        )
        .height(Length::Fill)
        .align_y(iced::Alignment::Center)
        .into(),
    ];
    if muted {
        let width = (theme.font_size * 2.85).ceil();
        children.push(
            container(
                text("mute")
                    .size(theme.font_size)
                    .color(style::rgba(theme.text))
                    .font(iced::Font::MONOSPACE)
                    .line_height(iced::widget::text::LineHeight::Relative(1.0)),
            )
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .align_x(iced::Alignment::End)
            .align_y(iced::Alignment::Center)
            .into(),
        );
    } else if show_percent {
        children.push(style::percent_slot(
            percent,
            theme,
            theme.font_size,
            style::rgba(theme.text),
        ));
    }
    if show_device {
        if let Some(dev) = device.filter(|d| !d.is_empty()) {
            children.push(
                container(
                    text(dev.to_string())
                        .size(theme.font_size)
                        .font(style::ui_font(theme))
                        .color(style::rgba(theme.muted))
                        .line_height(iced::widget::text::LineHeight::Relative(1.0)),
                )
                .height(Length::Fill)
                .align_y(iced::Alignment::Center)
                .into(),
            );
        }
    }
    let left = if clicks.on_click.as_ref().is_some_and(|s| !s.is_empty()) {
        Message::ModuleClick {
            kind: ModuleKind::Volume,
            right: false,
        }
    } else {
        Message::VolumeToggleMute
    };
    // Avoid `button` padding — it drops the glyph/value below the island midline.
    let content = container(
        row(children)
            .spacing(4)
            .height(Length::Fill)
            .align_y(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .align_y(iced::Alignment::Center)
    .padding(0);

    let mut area = mouse_area(content)
        .on_press(left)
        .on_scroll(move |delta| Message::VolumeScroll(scroll_delta(delta, step)));
    if clicks
        .on_right_click
        .as_ref()
        .is_some_and(|s| !s.is_empty())
    {
        area = area.on_right_press(Message::ModuleClick {
            kind: ModuleKind::Volume,
            right: true,
        });
    }
    area.into()
}

pub fn brightness<'a>(
    percent: f64,
    theme: &'a ThemeConfig,
    step: f64,
    show_percent: bool,
    clicks: &ClickActions,
) -> Element<'a, Message> {
    let mut children: Vec<Element<'a, Message>> = vec![
        container(
            text("☀")
                .size(theme.font_size)
                .font(style::emoji_font(theme))
                .color(style::rgba(theme.text))
                .line_height(iced::widget::text::LineHeight::Relative(1.0)),
        )
        .height(Length::Fill)
        .align_y(iced::Alignment::Center)
        .into(),
    ];
    if show_percent {
        children.push(style::percent_slot(
            percent,
            theme,
            theme.font_size,
            style::rgba(theme.text),
        ));
    }
    // Match volume: Fill + centered so the glyph/value sit on the island midline.
    let content = container(
        row(children)
            .spacing(4)
            .height(Length::Fill)
            .align_y(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .align_y(iced::Alignment::Center)
    .padding(0);
    let clickable = with_clicks(content.into(), ModuleKind::Brightness, clicks);
    mouse_area(clickable)
        .on_scroll(move |delta| Message::BrightnessScroll(scroll_delta(delta, step)))
        .into()
}

fn volume_emoji(muted: bool, percent: f64, bluetooth: bool) -> &'static str {
    if muted || percent < 0.5 {
        return "🔇";
    }
    if bluetooth {
        return "🎧";
    }
    if percent < 34.0 {
        "🔈"
    } else if percent < 67.0 {
        "🔉"
    } else {
        "🔊"
    }
}

/// Normalize wheel/trackpad scroll into a signed fraction/percent step contribution.
fn scroll_delta(delta: iced::mouse::ScrollDelta, step: f64) -> f64 {
    let y = match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y as f64,
        // ~40px ≈ one notch; keep proportional so trackpads can accumulate.
        iced::mouse::ScrollDelta::Pixels { y, .. } => (y as f64) / 40.0,
    };
    if y.abs() < f64::EPSILON {
        return 0.0;
    }
    // Discrete wheel notches (±1 line) map to exactly one configured step.
    if y.abs() >= 0.9 {
        y.signum() * step * y.abs().round()
    } else {
        y * step
    }
}

pub fn tray_icons<'a>(
    items: &'a [TrayItemSnapshot],
    theme: &'a ThemeConfig,
) -> Element<'a, Message> {
    let icons = items.iter().map(|item| {
        let content: Element<'a, Message> = if let Some(handle) = tray_handle(item) {
            image(handle)
                .width(Length::Fixed(18.0))
                .height(Length::Fixed(18.0))
                .into()
        } else {
            let label = item
                .title
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".into());
            text(label)
                .size(theme.font_size)
                .font(style::ui_font(theme))
                .color(style::rgba(theme.text))
                .into()
        };
        let id = item.id.clone();
        let id_menu = item.id.clone();
        mouse_area(
            button(content)
                .padding([0, 4])
                .style(style::ghost_button)
                .on_press(Message::TrayActivate(id)),
        )
        .on_right_press(Message::TrayOpenMenu(id_menu))
        .into()
    });
    row(icons).spacing(4).align_y(iced::Alignment::Center).into()
}

fn tray_handle(item: &TrayItemSnapshot) -> Option<iced::widget::image::Handle> {
    // Prefer named icons (SNI guidance), then pixmap, then application desktop icons.
    let mut names: Vec<String> = Vec::new();
    if item.needs_attention {
        if let Some(n) = item.attention_icon_name.as_deref() {
            names.push(n.to_string());
        }
    }
    if let Some(n) = item.icon_name.as_deref() {
        names.push(n.to_string());
    }
    if !item.app_id.is_empty() {
        names.push(item.app_id.clone());
        // org.foo.Bar → Bar
        if let Some(short) = item.app_id.rsplit('.').next() {
            if short.len() > 1 {
                names.push(short.to_string());
            }
        }
    }
    let title_key = item.title.to_lowercase().replace(' ', "-");
    if title_key.len() > 1 {
        names.push(title_key);
    }

    for name in names.iter().filter(|n| !n.is_empty()) {
        if let Some(path) = resolve_icon_path(name, item.icon_theme_path.as_deref()) {
            return Some(iced::widget::image::Handle::from_path(path));
        }
    }

    if let Some(px) = &item.icon_pixmap {
        if let Some(handle) = argb_to_handle(px) {
            return Some(handle);
        }
    }

    // Desktop-file Icon= lookup as last graphical fallback.
    for key in [&item.app_id, &item.title] {
        if let Some(icon) = desktop_icon_name(key) {
            if let Some(path) = resolve_icon_path(&icon, item.icon_theme_path.as_deref()) {
                return Some(iced::widget::image::Handle::from_path(path));
            }
        }
    }
    None
}

fn argb_to_handle(px: &TrayPixmap) -> Option<iced::widget::image::Handle> {
    let w = px.width.max(0) as u32;
    let h = px.height.max(0) as u32;
    if w == 0 || h == 0 {
        return None;
    }
    let expected = (w as usize).saturating_mul(h as usize).saturating_mul(4);
    if px.bytes.len() < expected {
        return None;
    }
    // SNI IconPixmap is ARGB32 network byte order (big-endian).
    let mut rgba = Vec::with_capacity(expected);
    for chunk in px.bytes[..expected].chunks_exact(4) {
        let a = chunk[0];
        let r = chunk[1];
        let g = chunk[2];
        let b = chunk[3];
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    Some(iced::widget::image::Handle::from_rgba(w, h, rgba))
}

fn resolve_icon_path(name: &str, theme_path: Option<&str>) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    // Absolute / relative file path.
    if name.contains('/') {
        let p = PathBuf::from(name);
        if is_raster(&p) && p.exists() {
            return Some(p);
        }
        if let Some(theme) = theme_path {
            let joined = PathBuf::from(theme).join(name);
            if is_raster(&joined) && joined.exists() {
                return Some(joined);
            }
        }
    }

    // App-provided theme directory (common for Electron / flatpak trays).
    if let Some(theme) = theme_path {
        if let Some(p) = find_in_theme_root(Path::new(theme), name) {
            return Some(p);
        }
        // Sometimes IconThemePath is a single flat directory of icons.
        for ext in RASTER_EXTS {
            let p = Path::new(theme).join(format!("{name}.{ext}"));
            if p.exists() {
                return Some(p);
            }
        }
        let bare = Path::new(theme).join(name);
        if is_raster(&bare) && bare.exists() {
            return Some(bare);
        }
    }

    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let roots = [
        home.join(".local/share/icons"),
        home.join(".icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons"),
        home.join(".local/share/flatpak/exports/share/icons"),
    ];

    for root in &roots {
        if root.ends_with("pixmaps") {
            for ext in RASTER_EXTS {
                let p = root.join(format!("{name}.{ext}"));
                if p.exists() {
                    return Some(p);
                }
            }
            continue;
        }
        if let Some(p) = find_in_icons_dir(root, name) {
            return Some(p);
        }
    }
    None
}

const RASTER_EXTS: &[&str] = &["png", "xpm", "jpg", "jpeg", "webp"];
const THEME_NAMES: &[&str] = &[
    "hicolor",
    "Adwaita",
    "Papirus",
    "Papirus-Dark",
    "breeze",
    "Breeze",
    "Tela",
    "Tela-dark",
];
const SIZES: &[&str] = &[
    "24x24", "22x22", "32x32", "48x48", "16x16", "64x64", "128x128", "scalable",
];
const CATS: &[&str] = &[
    "status",
    "apps",
    "devices",
    "panel",
    "categories",
    "actions",
    "places",
    "emblems",
];

fn is_raster(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            RASTER_EXTS
                .iter()
                .any(|r| ext.eq_ignore_ascii_case(r))
        })
}

fn find_in_icons_dir(root: &Path, name: &str) -> Option<PathBuf> {
    // Direct theme layout under this root.
    if let Some(p) = find_in_theme_root(root, name) {
        return Some(p);
    }
    for theme in THEME_NAMES {
        let theme_root = root.join(theme);
        if let Some(p) = find_in_theme_root(&theme_root, name) {
            return Some(p);
        }
    }
    None
}

fn find_in_theme_root(theme_root: &Path, name: &str) -> Option<PathBuf> {
    if !theme_root.exists() {
        return None;
    }
    for size in SIZES {
        for cat in CATS {
            for ext in RASTER_EXTS {
                let p = theme_root
                    .join(size)
                    .join(cat)
                    .join(format!("{name}.{ext}"));
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    // index.theme-less flat / recursive shallow search for raster files.
    for ext in RASTER_EXTS {
        let p = theme_root.join(format!("{name}.{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn desktop_icon_name(key: &str) -> Option<String> {
    let key_l = key.to_lowercase();
    let key_short = key_l.rsplit('.').next().unwrap_or(&key_l).to_string();
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let dirs = [
        home.join(".local/share/applications"),
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let stem_short = stem.rsplit('.').next().unwrap_or(&stem).to_string();
            let name_match = stem == key_l
                || stem_short == key_short
                || stem.contains(&key_short)
                || key_l.contains(&stem_short);
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !name_match {
                // Also match Name= / StartupWMClass=
                let soft = contents.lines().any(|line| {
                    let l = line.to_lowercase();
                    (l.starts_with("name=") || l.starts_with("startupwmclass="))
                        && (l.contains(&key_l) || l.contains(&key_short))
                });
                if !soft {
                    continue;
                }
            }
            for line in contents.lines() {
                let line = line.trim();
                if let Some(icon) = line.strip_prefix("Icon=") {
                    let icon = icon.trim();
                    if !icon.is_empty() {
                        return Some(icon.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn clock_tooltip<'a>(tip: String, theme: &'a ThemeConfig) -> Element<'a, Message> {
    let bg = style::rgba(theme.island_background);
    let radius = theme.island_radius;
    container(
        text(tip)
            .size(theme.font_size)
            .font(style::ui_font(theme))
            .color(style::rgba(theme.text)),
    )
    .padding([6, 12])
    .center_y(Length::Fill)
    .style(move |_| ContainerStyle {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
        },
        ..Default::default()
    })
    .into()
}

pub fn tray_menu_popup<'a>(
    item_id: &'a str,
    menu: Option<&'a TrayMenuSnapshot>,
    theme: &'a ThemeConfig,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(2).padding(8);
    if let Some(menu) = menu {
        for entry in &menu.entries {
            if !entry.visible {
                continue;
            }
            if entry.separator {
                col = col.push(
                    text("────────")
                        .size(10.0)
                        .font(style::ui_font(theme))
                        .color(style::rgba(theme.muted)),
                );
                continue;
            }
            let label = entry.label.replace('_', "");
            let menu_id = entry.id;
            let item_id = item_id.to_string();
            col = col.push(
                button(
                    text(label)
                        .size(theme.font_size)
                        .font(style::ui_font(theme))
                        .color(style::rgba(theme.text)),
                )
                .width(Length::Fill)
                .style(style::ghost_button)
                .on_press_maybe(entry.enabled.then_some(Message::TrayMenuClick {
                    item_id,
                    menu_id,
                })),
            );
        }
    } else {
        col = col.push(
            text("Loading…")
                .size(theme.font_size)
                .font(style::ui_font(theme))
                .color(style::rgba(theme.muted)),
        );
    }
    style::island(col.into(), theme)
}
