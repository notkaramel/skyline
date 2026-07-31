use std::collections::HashMap;
use std::sync::Mutex;

use iced::widget::button;
use iced::widget::container;
use iced::widget::container::Style as ContainerStyle;
use iced::widget::row;
use iced::widget::text;
use iced::{Background, Border, Color, Element, Font, Theme};
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
    let pad_v = theme.island_padding[0];
    let pad_h = theme.island_padding[1];
    let margin_v = theme.island_margin[0];
    let margin_h = theme.island_margin[1];
    let island = container(content)
        .padding([pad_v, pad_h])
        .height(iced::Length::Fill)
        .align_y(iced::Alignment::Center)
        .style(move |_| ContainerStyle {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: radius.into(),
                width: 1.0,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            },
            ..Default::default()
        });
    // Outer margin keeps islands from shifting neighbors when content width changes.
    container(island)
        .padding([margin_v, margin_h])
        .height(iced::Length::Fill)
        .into()
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
            .font(Font::MONOSPACE)
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
        let mut style = button::Style::default();
        style.border = Border {
            radius: 9.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        };
        style.text_color = if active {
            Color::from_rgb(0.14, 0.12, 0.16)
        } else if urgent {
            rgba(theme.danger)
        } else {
            rgba(theme.text)
        };
        style.background = Some(Background::Color(if active {
            rgba(theme.accent)
        } else {
            match status {
                button::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.08),
                _ => Color::TRANSPARENT,
            }
        }));
        style
    }
}

pub fn taskbar_chip_style(theme: &ThemeConfig, focused: bool) -> ContainerStyle {
    let radius = (theme.island_radius * 0.45).clamp(4.0, 8.0);
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
            width: if focused { 1.0 } else { 0.0 },
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
