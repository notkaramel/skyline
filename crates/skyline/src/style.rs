use std::collections::HashMap;
use std::sync::Mutex;

use iced::widget::button;
use iced::widget::container;
use iced::widget::container::Style as ContainerStyle;
use iced::widget::row;
use iced::widget::text;
use iced::{Background, Border, Color, Element, Font, Padding, Shadow, Theme, Vector};
use skyline_core::ThemeConfig;

use crate::app::Message;

static FONT_NAMES: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);

pub fn rgba(c: [f32; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

/// Resolve a configured font family name to an iced [`Font`].
pub fn named_font(name: &str) -> Font {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("default") {
        return Font::DEFAULT;
    }
    if name.eq_ignore_ascii_case("monospace") || name.eq_ignore_ascii_case("mono") {
        return Font::MONOSPACE;
    }
    if name.eq_ignore_ascii_case("serif") {
        return Font {
            family: iced::font::Family::Serif,
            ..Font::DEFAULT
        };
    }
    let mut guard = FONT_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let static_name: &'static str = map.entry(name.to_string()).or_insert_with(|| {
        let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
        leaked
    });
    Font::with_name(static_name)
}

pub fn ui_font(theme: &ThemeConfig) -> Font {
    named_font(&theme.font)
}

pub fn emoji_font(theme: &ThemeConfig) -> Font {
    if theme.emoji_font.trim().is_empty() {
        ui_font(theme)
    } else {
        named_font(&theme.emoji_font)
    }
}

/// Font for Waybar-style status icons (Font Awesome / Nerd Font private-use glyphs).
/// Prefers a Nerd Font when available so  render; falls back to the UI font.
pub fn icon_font(theme: &ThemeConfig) -> Font {
    static CACHED: Mutex<Option<Option<&'static str>>> = Mutex::new(None);
    let mut guard = CACHED.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(detect_nerd_font());
    }
    match guard.as_ref().and_then(|o| o.as_deref()) {
        Some(name) => named_font(name),
        None => ui_font(theme),
    }
}

fn detect_nerd_font() -> Option<&'static str> {
    const CANDIDATES: &[&str] = &[
        "DejaVuSansM Nerd Font Propo",
        "DejaVuSansM Nerd Font Mono",
        "DejaVuSansM Nerd Font",
        "Symbols Nerd Font Mono",
        "Symbols Nerd Font",
        "FiraCode Nerd Font Mono",
        "FiraCode Nerd Font",
        "JetBrainsMono Nerd Font Mono",
        "JetBrainsMono Nerd Font",
        "MesloLGS NF",
        "Hack Nerd Font Mono",
        "Hack Nerd Font",
    ];
    for name in CANDIDATES {
        let Ok(out) = std::process::Command::new("fc-list")
            .args([*name, "family"])
            .output()
        else {
            continue;
        };
        if out.status.success() && !out.stdout.is_empty() {
            let leaked: &'static str = Box::leak((*name).to_string().into_boxed_str());
            return Some(leaked);
        }
    }
    None
}

pub fn app_style(_app: &crate::app::App, theme: &Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    }
}

pub fn bar_container<'a>(
    content: impl Into<Element<'a, Message>>,
    theme: &ThemeConfig,
) -> Element<'a, Message> {
    let bg = rgba(theme.background);
    container(content)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .style(move |_| ContainerStyle {
            background: Some(Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

pub fn island<'a>(
    content: Element<'a, Message>,
    theme: &ThemeConfig,
) -> Element<'a, Message> {
    let bg = rgba(theme.island_background);
    let radius = theme.island_radius;
    let border_w = theme.island_border_width.max(0.0);
    let border_c = rgba(theme.island_border);
    let shadow = island_shadow(theme);
    let pad_v = theme.island_padding[0];
    let pad_h = theme.island_padding[1];
    let margin_v = f32::from(theme.island_margin[0]);
    let margin_h = f32::from(theme.island_margin[1]);
    // Reserve room so a hard cast shadow is not clipped by neighbors / bar edge.
    let shadow_right = theme.island_shadow_offset[0].max(0.0) + theme.island_shadow_blur.max(0.0);
    let shadow_bottom = theme.island_shadow_offset[1].max(0.0) + theme.island_shadow_blur.max(0.0);
    let island = container(content)
        .padding([pad_v, pad_h])
        .height(iced::Length::Fill)
        .align_y(iced::Alignment::Center)
        .style(move |_| ContainerStyle {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: radius.into(),
                width: border_w,
                color: border_c,
            },
            shadow,
            ..Default::default()
        });
    container(island)
        .padding(Padding {
            top: margin_v,
            right: margin_h + shadow_right,
            bottom: margin_v + shadow_bottom,
            left: margin_h,
        })
        .height(iced::Length::Fill)
        .into()
}

/// Build the configured island drop shadow (hard offset when blur is 0).
pub fn island_shadow(theme: &ThemeConfig) -> Shadow {
    let color = rgba(theme.island_shadow);
    if color.a <= f32::EPSILON
        && theme.island_shadow_offset[0].abs() <= f32::EPSILON
        && theme.island_shadow_offset[1].abs() <= f32::EPSILON
    {
        return Shadow::default();
    }
    Shadow {
        color,
        offset: Vector::new(theme.island_shadow_offset[0], theme.island_shadow_offset[1]),
        blur_radius: theme.island_shadow_blur.max(0.0),
    }
}

/// Fixed-width percent label so 5% → 50% does not resize the island.
pub fn percent_slot<'a>(
    value: f64,
    theme: &'a ThemeConfig,
    size: f32,
    color: Color,
) -> Element<'a, Message> {
    let label = format!("{:>3.0}%", value.round().clamp(0.0, 999.0));
    let width = (theme.font_size * 2.85).ceil();
    container(
        text(label)
            .size(size)
            .color(color)
            .font(ui_font(theme))
            .line_height(iced::widget::text::LineHeight::Relative(1.0)),
    )
    .width(iced::Length::Fixed(width))
    .height(iced::Length::Fill)
    .align_x(iced::Alignment::End)
    .align_y(iced::Alignment::Center)
    .into()
}

pub fn ghost_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let mut style = button::Style::default();
    style.background = None;
    style.text_color = palette.background.base.text;
    style.border = Border {
        radius: 8.0.into(),
        width: 0.0,
        color: Color::TRANSPARENT,
    };
    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.07)));
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.12)));
        }
        _ => {}
    }
    style
}

pub fn workspace_button(
    theme: &ThemeConfig,
    active: bool,
    urgent: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style + '_ {
    move |_t, status| {
        let radius = theme.island_radius;
        let mut style = button::Style::default();
        style.border = Border {
            radius: radius.into(),
            width: if active || urgent {
                theme.island_border_width.max(2.0).min(4.0)
            } else {
                0.0
            },
            color: if urgent {
                rgba(theme.danger)
            } else if active {
                rgba(theme.accent)
            } else {
                Color::TRANSPARENT
            },
        };
        style.text_color = if active {
            // Dark ink on accent fill — hard contrast like wofi selected text
            Color::from_rgb(0.04, 0.03, 0.08)
        } else if urgent {
            rgba(theme.danger)
        } else {
            rgba(theme.text)
        };
        style.background = Some(Background::Color(if active {
            rgba(theme.accent)
        } else {
            match status {
                button::Status::Hovered => Color::from_rgba(
                    theme.accent[0],
                    theme.accent[1],
                    theme.accent[2],
                    0.18,
                ),
                _ => Color::TRANSPARENT,
            }
        }));
        style
    }
}

pub fn taskbar_chip_style(
    theme: &ThemeConfig,
    focused: bool,
    border_width: Option<f32>,
) -> ContainerStyle {
    let radius = theme.island_radius;
    let border_w = if focused {
        border_width
            .unwrap_or(theme.island_border_width)
            .max(0.0)
    } else {
        0.0
    };
    ContainerStyle {
        background: Some(Background::Color(if focused {
            Color::from_rgba(
                theme.accent[0],
                theme.accent[1],
                theme.accent[2],
                0.22,
            )
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.04)
        })),
        border: Border {
            radius: radius.into(),
            width: border_w,
            color: if focused {
                rgba(theme.accent)
            } else {
                Color::TRANSPARENT
            },
        },
        ..Default::default()
    }
}

pub fn _island_row<'a>(
    children: Vec<Element<'a, Message>>,
    gap: u16,
) -> Element<'a, Message> {
    row(children).spacing(f32::from(gap)).into()
}
