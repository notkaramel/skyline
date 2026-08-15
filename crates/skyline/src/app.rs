use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Local;
use iced::futures::SinkExt;
use iced::stream;
use iced::widget::{container, row, space, text};
use iced::window::Id;
use iced::{Element, Event, Length, Subscription, Task};
use iced::keyboard::{self, key::Named};
use iced::mouse;
use iced::Rectangle;
use iced_layershell::actions::IcedNewPopupSettings;
use iced_layershell::reexport::{Anchor, PopupAnchor, PopupGravity};
use iced_layershell::to_layer_message;
use skyline_core::{
    BrightnessSnapshot, CompositorState, Config, CustomSnapshot, ModuleKind, NetworkSnapshot,
    OutputInfo, ServiceEvent, SysSnapshot, ThemeConfig, TrayItemSnapshot, TrayMenuAlign,
    TrayMenuSnapshot, VolumeSnapshot, WeatherSnapshot,
};
use skyline_services::ServiceTx;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use crate::style;
use crate::widgets;

/// Last bar surface that saw a pointer event (multi-monitor popup parent).
static LAST_POINTER_WINDOW: Mutex<Option<Id>> = Mutex::new(None);

/// Quiet window before applying coalesced scroll deltas (trackpad-friendly).
const SCROLL_DEBOUNCE: Duration = Duration::from_millis(70);
/// Cap how far one commit can jump so a scroll burst stays controllable.
const VOLUME_SCROLL_BURST: f64 = 0.10; // 10% of full scale
const BRIGHTNESS_SCROLL_BURST: f64 = 10.0; // percent points

/// Service bus receiver, shared with the iced subscription (hashed by Arc address).
#[derive(Clone)]
pub struct ServiceRxSlot(Arc<Mutex<Option<UnboundedReceiver<ServiceEvent>>>>);

impl ServiceRxSlot {
    pub fn new(rx: UnboundedReceiver<ServiceEvent>) -> Self {
        Self(Arc::new(Mutex::new(Some(rx))))
    }

    fn take_rx(&self) -> Option<UnboundedReceiver<ServiceEvent>> {
        self.0.lock().ok().and_then(|mut g| g.take())
    }
}

impl PartialEq for ServiceRxSlot {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ServiceRxSlot {}

impl Hash for ServiceRxSlot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

pub struct App {
    pub config: Config,
    service_rx: ServiceRxSlot,
    service_tx: ServiceTx,
    clock: String,
    compositor: CompositorState,
    sys: SysSnapshot,
    network: NetworkSnapshot,
    volume: VolumeSnapshot,
    brightness: BrightnessSnapshot,
    weather: WeatherSnapshot,
    custom: HashMap<String, String>,
    tray_items: Vec<TrayItemSnapshot>,
    tray_menu: Option<TrayMenuSnapshot>,
    popup_ids: HashMap<Id, PopupKind>,
    /// Right-click that is waiting on DBus menu contents.
    pending_tray: Option<PendingTrayMenu>,
    /// Layer-shell bar surfaces and the output each is pinned to.
    bar_outputs: Mutex<HashMap<Id, String>>,
    /// Logical width of each bar surface (for `tray_menu_align = "end"`).
    bar_widths: HashMap<Id, f32>,
    bound_output: Option<String>,
    errors: Vec<String>,
    volume_pending: f64,
    volume_last_input: Option<Instant>,
    brightness_pending: f64,
    brightness_last_input: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverTipKind {
    Clock,
    Weather,
}

#[derive(Debug, Clone)]
pub enum PopupKind {
    TrayMenu { item_id: String },
    /// Hover tip anchored under a module (clock / weather).
    HoverTooltip { kind: HoverTipKind },
}

#[derive(Debug, Clone)]
struct PendingTrayMenu {
    item_id: String,
    bounds: Rectangle,
    parent: Id,
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    /// One or more service events (pushed instantly; no timer poll).
    Services(Vec<ServiceEvent>),
    ScrollDebounce,
    FocusWorkspace(u64),
    FocusWindow(u64),
    VolumeScroll(f64),
    VolumeToggleMute,
    BrightnessScroll(f64),
    ModuleClick { kind: ModuleKind, right: bool },
    TrayActivate(String),
    TrayOpenMenu { item_id: String, bounds: Rectangle },
    TrayMenuClick { item_id: String, menu_id: i32 },
    DismissTrayMenus,
    HoverTooltip {
        kind: HoverTipKind,
        enter: bool,
        bounds: Rectangle,
    },
    ClosePopup(Id),
    PointerPressed { window: Id },
    BarOpened { id: Id, width: Option<f32> },
    IcedEvent(Event),
    WindowClosed(Id),
}

impl App {
    pub fn new(
        config: Config,
        service_rx: ServiceRxSlot,
        service_tx: ServiceTx,
    ) -> (Self, Task<Message>) {
        let bound_output = config.bar.output.clone();
        (
            Self {
                config,
                service_rx,
                service_tx,
                clock: String::new(),
                compositor: CompositorState::default(),
                sys: SysSnapshot::default(),
                network: NetworkSnapshot::default(),
                volume: VolumeSnapshot::default(),
                brightness: BrightnessSnapshot::default(),
                weather: WeatherSnapshot::default(),
                custom: HashMap::new(),
                tray_items: Vec::new(),
                tray_menu: None,
                popup_ids: HashMap::new(),
                pending_tray: None,
                bar_outputs: Mutex::new(HashMap::new()),
                bar_widths: HashMap::new(),
                bound_output,
                errors: Vec::new(),
                volume_pending: 0.0,
                volume_last_input: None,
                brightness_pending: 0.0,
                brightness_last_input: None,
            },
            Task::none(),
        )
    }

    pub fn namespace() -> String {
        "skyline".into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            service_subscription(self.service_rx.clone()),
            iced::event::listen_with(|event, _status, id| match &event {
                Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                    if matches!(key, keyboard::Key::Named(Named::Escape)) =>
                {
                    Some(Message::DismissTrayMenus)
                }
                Event::Mouse(mouse::Event::CursorMoved { .. })
                | Event::Mouse(mouse::Event::ButtonPressed(_))
                | Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                    if let Ok(mut last) = LAST_POINTER_WINDOW.lock() {
                        *last = Some(id);
                    }
                    match event {
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                            Some(Message::PointerPressed { window: id })
                        }
                        _ => None,
                    }
                }
                Event::Window(iced::window::Event::Opened { size, .. }) => Some(
                    Message::BarOpened {
                        id,
                        width: Some(size.width),
                    },
                ),
                Event::Window(iced::window::Event::Resized(size)) => Some(Message::BarOpened {
                    id,
                    width: Some(size.width),
                }),
                _ => None,
            }),
            iced::window::close_events().map(Message::WindowClosed),
        ];
        if self.volume_pending.abs() > f64::EPSILON
            || self.brightness_pending.abs() > f64::EPSILON
        {
            subs.push(
                iced::time::every(Duration::from_millis(25)).map(|_| Message::ScrollDebounce),
            );
        }
        Subscription::batch(subs)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Services(events) => self.apply_services(events),
            Message::ScrollDebounce => {
                self.flush_volume_scroll(false);
                self.flush_brightness_scroll(false);
                Task::none()
            }
            Message::FocusWorkspace(id) => {
                if skyline_niri::is_available() {
                    if let Err(err) = skyline_niri::focus_workspace(id) {
                        warn!("niri focus workspace: {err}");
                    }
                } else if skyline_hyprland::is_available() {
                    if let Err(err) = skyline_hyprland::focus_workspace(id) {
                        warn!("hyprland focus workspace: {err}");
                    }
                }
                if let Some(cmd) = self
                    .config
                    .modules
                    .click_command(&ModuleKind::Workspaces, false)
                {
                    skyline_services::run_click(cmd);
                }
                Task::none()
            }
            Message::FocusWindow(id) => {
                if let Some(win) = self.compositor.windows.iter().find(|w| w.id == id) {
                    if skyline_niri::is_available() {
                        if let Err(err) = skyline_niri::focus_window(win.id) {
                            warn!("niri focus window: {err}");
                        }
                    } else if skyline_hyprland::is_available() {
                        let token = if win.focus_token.is_empty() {
                            win.id.to_string()
                        } else {
                            win.focus_token.clone()
                        };
                        if let Err(err) = skyline_hyprland::focus_window(&token) {
                            warn!("hyprland focus window: {err}");
                        }
                    }
                }
                Task::none()
            }
            Message::VolumeScroll(delta) => {
                if delta.abs() > f64::EPSILON {
                    self.volume_pending += delta;
                    self.volume_last_input = Some(Instant::now());
                }
                Task::none()
            }
            Message::VolumeToggleMute => {
                self.volume_pending = 0.0;
                self.volume_last_input = None;
                self.volume = skyline_services::set_mute();
                Task::none()
            }
            Message::BrightnessScroll(delta) => {
                if delta.abs() > f64::EPSILON {
                    self.brightness_pending += delta;
                    self.brightness_last_input = Some(Instant::now());
                }
                Task::none()
            }
            Message::ModuleClick { kind, right } => {
                if let Some(cmd) = self.config.modules.click_command(&kind, right) {
                    skyline_services::run_click(cmd);
                }
                Task::none()
            }
            Message::TrayActivate(id) => {
                let dismiss = self.close_tray_popups();
                skyline_services::activate_item(&id);
                dismiss
            }
            Message::TrayOpenMenu { item_id, bounds } => {
                if self
                    .popup_ids
                    .values()
                    .any(|k| matches!(k, PopupKind::TrayMenu { item_id: open } if *open == item_id))
                {
                    return self.close_tray_popups();
                }
                let parent = LAST_POINTER_WINDOW
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .or_else(|| self.bar_widths.keys().copied().next());
                let close = self.close_tray_popups();
                if let Some(parent) = parent {
                    self.pending_tray = Some(PendingTrayMenu {
                        item_id: item_id.clone(),
                        bounds,
                        parent,
                    });
                }
                skyline_services::request_menu(&item_id, self.service_tx.clone());
                close
            }
            Message::TrayMenuClick { item_id, menu_id } => {
                skyline_services::activate_menu(&item_id, menu_id);
                self.close_tray_popups()
            }
            Message::DismissTrayMenus => self.close_tray_popups(),
            Message::HoverTooltip {
                kind,
                enter: true,
                bounds,
            } => self.open_hover_tooltip(kind, bounds),
            Message::HoverTooltip {
                kind,
                enter: false,
                ..
            } => self.close_hover_tooltip(kind),
            Message::ClosePopup(id) => {
                let kind = self.popup_ids.remove(&id);
                if matches!(kind, Some(PopupKind::TrayMenu { .. })) {
                    self.tray_menu = None;
                    self.pending_tray = None;
                }
                iced::window::close(id)
            }
            Message::PointerPressed { window } => {
                // Left-click on the bar (not the menu) dismisses tray menus.
                if self.popup_ids.values().all(|k| !matches!(k, PopupKind::TrayMenu { .. })) {
                    return Task::none();
                }
                if matches!(self.popup_ids.get(&window), Some(PopupKind::TrayMenu { .. })) {
                    Task::none()
                } else {
                    self.close_tray_popups()
                }
            }
            Message::WindowClosed(id) => {
                let kind = self.popup_ids.remove(&id);
                if let Ok(mut map) = self.bar_outputs.lock() {
                    map.remove(&id);
                }
                self.bar_widths.remove(&id);
                if matches!(kind, Some(PopupKind::TrayMenu { .. })) {
                    self.tray_menu = None;
                    self.pending_tray = None;
                }
                Task::none()
            }
            Message::BarOpened { id, width } => {
                if let Some(w) = width {
                    self.bar_widths.insert(id, w);
                }
                if !self.popup_ids.contains_key(&id) {
                    self.pin_bar_output(id, width);
                }
                Task::none()
            }
            Message::IcedEvent(_event) => Task::none(),
            _ => Task::none(),
        }
    }

    /// Pin a bar surface to a monitor so taskbar/workspaces stay on that output
    /// even when keyboard/mouse focus is elsewhere.
    ///
    /// iced_layershell always reports `Opened` with `position: None`, so we match
    /// the layer surface width to each output's logical width when possible.
    fn pin_bar_output(&self, id: Id, width: Option<f32>) {
        if self.bound_output.is_some() {
            return;
        }
        let Ok(mut map) = self.bar_outputs.lock() else {
            return;
        };
        let used: HashSet<String> = map
            .iter()
            .filter(|(k, _)| **k != id)
            .map(|(_, v)| v.clone())
            .collect();

        if let Some(w) = width {
            let mut matches: Vec<&OutputInfo> = self
                .compositor
                .outputs
                .iter()
                .filter(|o| !used.contains(&o.name))
                .filter(|o| (o.width as f32 - w).abs() < 2.0)
                .collect();
            matches.sort_by_key(|o| (o.x, o.y));
            if let Some(out) = matches.first() {
                map.insert(id, out.name.clone());
                return;
            }
            // Soft match: closest unused width (helps fractional scaling quirks).
            let mut by_delta: Vec<_> = self
                .compositor
                .outputs
                .iter()
                .filter(|o| !used.contains(&o.name))
                .map(|o| ((o.width as f32 - w).abs(), o))
                .collect();
            by_delta.sort_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, out)) = by_delta.first() {
                if by_delta[0].0 < 64.0 {
                    map.insert(id, out.name.clone());
                    return;
                }
            }
        }

        if map.contains_key(&id) {
            return;
        }
        let mut available: Vec<String> = self
            .compositor
            .output_names()
            .into_iter()
            .filter(|n| !used.contains(n))
            .collect();
        // Prefer geometry order so multi-monitor assignment is stable.
        if !self.compositor.outputs.is_empty() {
            available = self
                .compositor
                .outputs
                .iter()
                .filter(|o| !used.contains(&o.name))
                .map(|o| o.name.clone())
                .collect();
        } else {
            available.sort();
        }
        if let Some(name) = available.into_iter().next() {
            map.insert(id, name);
        }
    }

    fn refresh_bar_output_pins(&self) {
        if self.bound_output.is_some() {
            return;
        }
        let names = self.compositor.output_names();
        if names.is_empty() {
            return;
        }
        let Ok(mut map) = self.bar_outputs.lock() else {
            return;
        };
        // Drop pins to outputs that disappeared; bars re-pin on next view/open.
        map.retain(|_, out| names.iter().any(|n| n == out));
    }

    fn output_for_bar(&self, id: Id) -> Option<String> {
        if let Some(o) = &self.bound_output {
            return Some(o.clone());
        }
        if let Ok(map) = self.bar_outputs.lock() {
            if let Some(o) = map.get(&id) {
                return Some(o.clone());
            }
        }
        // First paint before Opened: assign an unused output.
        self.pin_bar_output(id, None);
        self.bar_outputs
            .lock()
            .ok()
            .and_then(|map| map.get(&id).cloned())
    }

    fn close_tray_popups(&mut self) -> Task<Message> {
        self.pending_tray = None;
        self.tray_menu = None;
        let ids: Vec<Id> = self
            .popup_ids
            .iter()
            .filter(|(_, kind)| matches!(kind, PopupKind::TrayMenu { .. }))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.popup_ids.remove(id);
        }
        if ids.is_empty() {
            Task::none()
        } else {
            Task::batch(ids.into_iter().map(iced::window::close))
        }
    }

    fn clock_tooltip_text(&self) -> String {
        Local::now()
            .format(&self.config.modules.clock.tooltip_format)
            .to_string()
    }

    fn hover_tooltip_lines(&self, kind: HoverTipKind) -> Vec<String> {
        match kind {
            HoverTipKind::Clock => {
                let fmt = self.config.modules.clock.tooltip_format.trim();
                if fmt.is_empty() {
                    Vec::new()
                } else {
                    vec![self.clock_tooltip_text()]
                }
            }
            HoverTipKind::Weather => {
                widgets::weather_tooltip_lines(&self.weather, self.config.modules.weather.unit)
            }
        }
    }

    fn close_hover_tooltip(&mut self, kind: HoverTipKind) -> Task<Message> {
        let ids: Vec<Id> = self
            .popup_ids
            .iter()
            .filter(|(_, k)| matches!(k, PopupKind::HoverTooltip { kind: open } if *open == kind))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.popup_ids.remove(id);
        }
        if ids.is_empty() {
            Task::none()
        } else {
            Task::batch(ids.into_iter().map(iced::window::close))
        }
    }

    fn close_all_hover_tooltips(&mut self) -> Task<Message> {
        let ids: Vec<Id> = self
            .popup_ids
            .iter()
            .filter(|(_, k)| matches!(k, PopupKind::HoverTooltip { .. }))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.popup_ids.remove(id);
        }
        if ids.is_empty() {
            Task::none()
        } else {
            Task::batch(ids.into_iter().map(iced::window::close))
        }
    }

    fn open_hover_tooltip(&mut self, kind: HoverTipKind, bounds: Rectangle) -> Task<Message> {
        let lines = self.hover_tooltip_lines(kind);
        if lines.is_empty() {
            return Task::none();
        }
        if self
            .popup_ids
            .values()
            .any(|k| matches!(k, PopupKind::HoverTooltip { kind: open } if *open == kind))
        {
            return Task::none();
        }

        let close_other = self.close_all_hover_tooltips();

        let parent = LAST_POINTER_WINDOW
            .lock()
            .ok()
            .and_then(|g| *g)
            .or_else(|| self.bar_widths.keys().copied().next());
        let Some(parent) = parent else {
            return close_other;
        };

        let (width, height) = tooltip_popup_size(&lines, &self.config.theme);
        let gap = self.config.bar.tray_menu_gap.max(0);
        let bar_h = self.config.bar.height as i32;
        let x = bounds.x.round() as i32;
        let w = bounds.width.max(1.0).round() as i32;
        let (anchor_rect, popup_anchor, gravity) = if self.config.bar.anchor == "bottom" {
            (
                (x, -gap, w, bar_h + gap),
                PopupAnchor::TopLeft,
                PopupGravity::TopRight,
            )
        } else {
            (
                (x, 0, w, bar_h + gap),
                PopupAnchor::BottomLeft,
                PopupGravity::BottomRight,
            )
        };

        let id = Id::unique();
        self.popup_ids
            .insert(id, PopupKind::HoverTooltip { kind });
        Task::batch([
            close_other,
            Task::done(Message::NewPopUp {
                settings: IcedNewPopupSettings::new(parent, (width, height), anchor_rect)
                    .anchor(popup_anchor)
                    .gravity(gravity),
                id,
            }),
        ])
    }

    fn apply_config_reload(&mut self, config: Config) -> Task<Message> {
        skyline_services::reload_from_config(&config, self.service_tx.clone());

        let keep: HashSet<&str> = config
            .modules
            .custom
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        self.custom.retain(|id, _| keep.contains(id.as_str()));

        self.bound_output = config.bar.output.clone();
        self.config = config;
        info!("applied hot-reloaded config");
        self.layer_tasks_for_bar()
    }

    fn layer_tasks_for_bar(&self) -> Task<Message> {
        let anchor = match self.config.bar.anchor.as_str() {
            "bottom" => Anchor::Bottom | Anchor::Left | Anchor::Right,
            _ => Anchor::Top | Anchor::Left | Anchor::Right,
        };
        let height = self.config.bar.height;
        let margin = self.config.bar.margin;
        let exclusive = self.config.bar.exclusive_zone;
        let ids: Vec<Id> = self
            .bar_outputs
            .lock()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();

        let mut tasks = Vec::with_capacity(ids.len() * 4);
        for id in ids {
            tasks.push(Task::done(Message::AnchorChange { id, anchor }));
            tasks.push(Task::done(Message::SizeChange {
                id,
                size: (0, height),
            }));
            tasks.push(Task::done(Message::MarginChange {
                id,
                margin: (margin[0], margin[1], margin[2], margin[3]),
            }));
            tasks.push(Task::done(Message::ExclusiveZoneChange {
                id,
                zone_size: exclusive,
            }));
        }
        Task::batch(tasks)
    }

    fn displayed_volume(&self) -> VolumeSnapshot {
        VolumeSnapshot {
            percent: (self.volume.percent + self.volume_pending * 100.0).clamp(
                0.0,
                self.config.modules.volume.max_percent,
            ),
            muted: self.volume.muted,
            bluetooth: self.volume.bluetooth && self.config.modules.volume.detect_bluetooth,
            device: self.volume.device.clone(),
        }
    }

    fn displayed_brightness(&self) -> BrightnessSnapshot {
        BrightnessSnapshot {
            percent: (self.brightness.percent + self.brightness_pending).clamp(0.0, 100.0),
            available: self.brightness.available,
        }
    }

    fn open_tray_menu_popup(&mut self, item_id: String) -> Task<Message> {
        let close = self
            .popup_ids
            .iter()
            .filter(|(_, kind)| matches!(kind, PopupKind::TrayMenu { .. }))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in &close {
            self.popup_ids.remove(id);
        }
        let mut tasks: Vec<Task<Message>> = close.into_iter().map(iced::window::close).collect();

        let entries = self
            .tray_menu
            .as_ref()
            .map(|m| m.entries.iter().filter(|e| e.visible).count())
            .unwrap_or(1)
            .max(1);
        let shadow_x = self.config.theme.island_shadow_offset[0].max(0.0)
            + self.config.theme.island_shadow_blur.max(0.0);
        let shadow_y = self.config.theme.island_shadow_offset[1].max(0.0)
            + self.config.theme.island_shadow_blur.max(0.0);
        let width = (240.0 + shadow_x).ceil() as u32;
        let height = (entries as f32 * 28.0 + 20.0 + shadow_y).ceil() as u32;
        let height = height.clamp(36, 420);

        let pending = self.pending_tray.take();
        let parent = pending
            .as_ref()
            .map(|p| p.parent)
            .or_else(|| LAST_POINTER_WINDOW.lock().ok().and_then(|g| *g))
            .or_else(|| self.bar_widths.keys().copied().next());
        let Some(parent) = parent else {
            return Task::batch(tasks);
        };

        let gap = self.config.bar.tray_menu_gap.max(0);
        let bar_h = self.config.bar.height as i32;
        let bottom_bar = self.config.bar.anchor == "bottom";
        let (anchor_rect, popup_anchor, gravity) = match self.config.bar.tray_menu_align {
            TrayMenuAlign::Icon => {
                let bounds = pending
                    .as_ref()
                    .map(|p| p.bounds)
                    .unwrap_or(Rectangle::new(
                        iced::Point::ORIGIN,
                        iced::Size::new(1.0, bar_h as f32),
                    ));
                let x = bounds.x.round() as i32;
                let w = bounds.width.max(1.0).round() as i32;
                if bottom_bar {
                    (
                        (x, -gap, w, bar_h + gap),
                        PopupAnchor::TopLeft,
                        PopupGravity::TopRight,
                    )
                } else {
                    (
                        (x, 0, w, bar_h + gap),
                        PopupAnchor::BottomLeft,
                        PopupGravity::BottomRight,
                    )
                }
            }
            TrayMenuAlign::End => {
                let bar_w = self
                    .bar_widths
                    .get(&parent)
                    .copied()
                    .or_else(|| pending.as_ref().map(|p| p.bounds.x + p.bounds.width + 8.0))
                    .unwrap_or(1.0)
                    .max(1.0)
                    .round() as i32;
                if bottom_bar {
                    (
                        (bar_w.saturating_sub(1), -gap, 1, bar_h + gap),
                        PopupAnchor::TopRight,
                        PopupGravity::TopLeft,
                    )
                } else {
                    (
                        (bar_w.saturating_sub(1), 0, 1, bar_h + gap),
                        PopupAnchor::BottomRight,
                        PopupGravity::BottomLeft,
                    )
                }
            }
        };

        let menu_id = Id::unique();
        self.popup_ids.insert(menu_id, PopupKind::TrayMenu { item_id });
        tasks.push(Task::done(Message::NewPopUp {
            settings: IcedNewPopupSettings::new(parent, (width, height), anchor_rect)
                .anchor(popup_anchor)
                .gravity(gravity),
            id: menu_id,
        }));
        Task::batch(tasks)
    }

    fn flush_volume_scroll(&mut self, force: bool) {
        if self.volume_pending.abs() <= f64::EPSILON {
            self.volume_last_input = None;
            return;
        }
        let quiet = self
            .volume_last_input
            .is_none_or(|t| t.elapsed() >= SCROLL_DEBOUNCE);
        if !force && !quiet {
            return;
        }
        let pending = std::mem::take(&mut self.volume_pending);
        self.volume_last_input = None;
        let apply = pending.clamp(-VOLUME_SCROLL_BURST, VOLUME_SCROLL_BURST);
        self.volume = skyline_services::set_volume_delta(apply);
        let leftover = pending - apply;
        if leftover.abs() > f64::EPSILON {
            self.volume_pending = leftover;
            // Ready to flush remainder on the next debounce tick.
            self.volume_last_input = Some(Instant::now() - SCROLL_DEBOUNCE);
        }
    }

    fn flush_brightness_scroll(&mut self, force: bool) {
        if self.brightness_pending.abs() <= f64::EPSILON {
            self.brightness_last_input = None;
            return;
        }
        let quiet = self
            .brightness_last_input
            .is_none_or(|t| t.elapsed() >= SCROLL_DEBOUNCE);
        if !force && !quiet {
            return;
        }
        let pending = std::mem::take(&mut self.brightness_pending);
        self.brightness_last_input = None;
        let apply = pending.clamp(-BRIGHTNESS_SCROLL_BURST, BRIGHTNESS_SCROLL_BURST);
        self.brightness = skyline_services::set_brightness_delta(apply);
        let leftover = pending - apply;
        if leftover.abs() > f64::EPSILON {
            self.brightness_pending = leftover;
            self.brightness_last_input = Some(Instant::now() - SCROLL_DEBOUNCE);
        }
    }

    fn apply_services(&mut self, events: Vec<ServiceEvent>) -> Task<Message> {
        let mut tasks = Vec::new();
        // Coalesce compositor floods: keep only the latest snapshot in a burst.
        let mut last_compositor: Option<ServiceEvent> = None;
        for ev in events {
            match ev {
                ServiceEvent::Compositor(_) => last_compositor = Some(ev),
                other => {
                    if let Some(task) = self.apply_service(other) {
                        tasks.push(task);
                    }
                }
            }
        }
        if let Some(ev) = last_compositor {
            if let Some(task) = self.apply_service(ev) {
                tasks.push(task);
            }
        }
        Task::batch(tasks)
    }

    fn apply_service(&mut self, ev: ServiceEvent) -> Option<Task<Message>> {
        match ev {
            ServiceEvent::Tick => None,
            ServiceEvent::Clock(s) => {
                if self.clock == s {
                    return None;
                }
                self.clock = s;
                None
            }
            ServiceEvent::Compositor(state) => {
                if self.compositor == state {
                    return None;
                }
                self.compositor = state;
                self.refresh_bar_output_pins();
                None
            }
            ServiceEvent::Sys(s) => {
                if self.sys.visually_eq(&s) {
                    return None;
                }
                self.sys = s;
                None
            }
            ServiceEvent::Network(n) => {
                if self.network == n {
                    return None;
                }
                self.network = n;
                None
            }
            ServiceEvent::Volume(v) => {
                // Don't clobber an in-flight scroll preview.
                if self.volume_pending.abs() <= f64::EPSILON {
                    if self.volume.visually_eq(&v) {
                        return None;
                    }
                    self.volume = v;
                }
                None
            }
            ServiceEvent::Brightness(b) => {
                if self.brightness_pending.abs() <= f64::EPSILON {
                    if self.brightness.visually_eq(&b) {
                        return None;
                    }
                    self.brightness = b;
                }
                None
            }
            ServiceEvent::Custom(CustomSnapshot { id, text }) => {
                if self.custom.get(&id) == Some(&text) {
                    return None;
                }
                self.custom.insert(id, text);
                None
            }
            ServiceEvent::Weather(snap) => {
                if self.weather == snap {
                    return None;
                }
                self.weather = snap;
                None
            }
            ServiceEvent::TrayItems(items) => {
                if self.tray_items == items {
                    return None;
                }
                self.tray_items = items;
                None
            }
            ServiceEvent::TrayMenu(menu) => {
                let item_id = menu.item_id.clone();
                let already_open = self.popup_ids.values().any(
                    |k| matches!(k, PopupKind::TrayMenu { item_id: open } if *open == item_id),
                );
                if already_open {
                    self.tray_menu = Some(menu);
                    return None;
                }
                if self
                    .pending_tray
                    .as_ref()
                    .is_none_or(|p| p.item_id != item_id)
                {
                    return None;
                }
                self.tray_menu = Some(menu);
                Some(self.open_tray_menu_popup(item_id))
            }
            ServiceEvent::ConfigReloaded(config) => Some(self.apply_config_reload(*config)),
            ServiceEvent::Error(err) => {
                warn!("{err}");
                self.errors.push(err);
                if self.errors.len() > 5 {
                    self.errors.remove(0);
                }
                None
            }
        }
    }

    pub fn view(&self, id: Id) -> Element<'_, Message> {
        match self.popup_ids.get(&id) {
            Some(PopupKind::TrayMenu { item_id }) => {
                return widgets::tray_menu_popup(
                    item_id,
                    self.tray_menu.as_ref(),
                    &self.config.theme,
                );
            }
            Some(PopupKind::HoverTooltip { kind }) => {
                return widgets::hover_tooltip(
                    self.hover_tooltip_lines(*kind),
                    &self.config.theme,
                );
            }
            None => {}
        }

        // Pin this bar to a specific monitor (not whichever has focus).
        self.pin_bar_output(id, None);
        let output = self.output_for_bar(id);
        let output = output.as_deref();

        let left = self.island(&self.config.modules.left, output);
        let center = self.island(&self.config.modules.center, output);
        let right = self.island(&self.config.modules.right, output);

        // Stack keeps the center island geometrically fixed in the middle:
        // base layer = centered clock (etc.), top layer = left/right edges.
        // Horizontal spacer in the top row does not capture clicks, so the
        // center island still receives input under the gap.
        let edges = row![left, space::horizontal(), right,]
            .spacing(f32::from(self.config.bar.island_gap))
            .padding(self.config.bar.padding)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(iced::Alignment::Center);

        let middle = container(center)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .padding(self.config.bar.padding);

        let content = iced::widget::stack![middle, edges]
            .width(Length::Fill)
            .height(Length::Fill);

        style::bar_container(content, &self.config.theme)
    }

    fn island<'a>(
        &'a self,
        modules: &'a [ModuleKind],
        output: Option<&str>,
    ) -> Element<'a, Message> {
        let mut children: Vec<Element<'a, Message>> = Vec::new();
        for kind in modules {
            if let Some(el) = self.module_view(kind, output) {
                if self.config.bar.separators && !children.is_empty() {
                    children.push(widgets::module_separator(
                        &self.config.bar.separator,
                        &self.config.theme,
                    ));
                }
                children.push(el);
            }
        }
        if children.is_empty() {
            return space::Space::new().width(0).into();
        }
        style::island(
            row(children)
                .spacing(8.0)
                .height(Length::Fill)
                .align_y(iced::Alignment::Center)
                .into(),
            &self.config.theme,
        )
    }

    fn module_view<'a>(
        &'a self,
        kind: &'a ModuleKind,
        output: Option<&str>,
    ) -> Option<Element<'a, Message>> {
        let clicks = self.config.modules.clicks_for(kind);
        let el = match kind {
            ModuleKind::Workspaces => {
                let ws = widgets::workspaces(&self.compositor, output, &self.config.theme);
                // Left-click stays on workspace buttons (focus + optional on_click).
                // Only attach right-click on the strip here.
                let mut strip = clicks.clone();
                strip.on_click = None;
                Some(widgets::with_clicks(ws, ModuleKind::Workspaces, &strip))
            }
            ModuleKind::Taskbar => {
                let wins = self
                    .compositor
                    .taskbar_windows(output);
                if wins.is_empty() {
                    None
                } else {
                    Some(widgets::taskbar(
                        &self.compositor,
                        output,
                        &self.config.theme,
                        &self.config.modules.taskbar,
                    ))
                }
            }
            ModuleKind::Window => {
                let title = self
                    .compositor
                    .focused_window_for_output(output)
                    .map(|w| truncate(&w.title, self.config.modules.window.max_chars))
                    .unwrap_or_default();
                if title.is_empty() {
                    None
                } else {
                    Some(
                        text(title)
                            .size(self.config.theme.font_size)
                            .font(style::ui_font(&self.config.theme))
                            .color(style::rgba(self.config.theme.muted))
                            .into(),
                    )
                }
            }
            ModuleKind::Clock => {
                let label = text(&self.clock)
                    .size(self.config.theme.font_size)
                    .font(style::ui_font(&self.config.theme))
                    .color(style::rgba(self.config.theme.text));
                let clickable = widgets::with_clicks(label.into(), ModuleKind::Clock, &clicks);
                if self
                    .config
                    .modules
                    .clock
                    .tooltip_format
                    .trim()
                    .is_empty()
                {
                    Some(clickable)
                } else {
                    Some(widgets::hover_area(
                        clickable,
                        |bounds| Message::HoverTooltip {
                            kind: HoverTipKind::Clock,
                            enter: true,
                            bounds,
                        },
                        |bounds| Message::HoverTooltip {
                            kind: HoverTipKind::Clock,
                            enter: false,
                            bounds,
                        },
                    ))
                }
            }
            ModuleKind::Weather => {
                if self.weather.emoji.is_empty() && self.weather.condition.is_empty() {
                    None
                } else {
                    Some(widgets::weather(
                        &self.weather,
                        self.config.modules.weather.unit,
                        &self.config.theme,
                        &clicks,
                    ))
                }
            }
            ModuleKind::Cpu => Some(widgets::usage_meter(
                "cpu",
                &self.sys.cpu_per_core,
                self.sys.cpu_percent,
                &self.config.theme,
                &self.config.modules.cpu,
            )),
            ModuleKind::Memory => {
                let bars = self.config.theme.meter_bars.max(1) as usize;
                let segments =
                    widgets::usage_fill_segments(self.sys.memory_percent, bars);
                Some(widgets::usage_meter(
                    "ram",
                    &segments,
                    self.sys.memory_percent,
                    &self.config.theme,
                    &self.config.modules.memory,
                ))
            }
            ModuleKind::Gpu => {
                let value = self.sys.gpu_percent.unwrap_or(0.0);
                Some(widgets::usage_meter(
                    "gpu",
                    &self.sys.gpu_per_device,
                    value,
                    &self.config.theme,
                    &self.config.modules.gpu,
                ))
            }
            ModuleKind::Network => {
                let cfg = &self.config.modules.network;
                let raw = if self.network.connected {
                    if cfg.show_name {
                        self.network.label.clone()
                    } else {
                        self.network
                            .interface
                            .clone()
                            .unwrap_or_else(|| self.network.label.clone())
                    }
                } else {
                    "offline".into()
                };
                let name = truncate(&raw, cfg.max_chars.max(1));
                let color = style::rgba(if self.network.connected {
                    self.config.theme.text
                } else {
                    self.config.theme.danger
                });
                if self.network.connected {
                    match (cfg.show_strength, self.network.strength) {
                        (true, Some(s)) => Some(
                            row![
                                text(name)
                                    .size(self.config.theme.font_size)
                                    .font(style::ui_font(&self.config.theme))
                                    .color(color),
                                style::percent_slot(
                                    f64::from(s),
                                    &self.config.theme,
                                    self.config.theme.font_size,
                                    color,
                                ),
                            ]
                            .spacing(4)
                            .align_y(iced::Alignment::Center)
                            .into(),
                        ),
                        _ => Some(
                            text(name)
                                .size(self.config.theme.font_size)
                                .font(style::ui_font(&self.config.theme))
                                .color(color)
                                .into(),
                        ),
                    }
                } else {
                    Some(
                        text(name)
                            .size(self.config.theme.font_size)
                            .font(style::ui_font(&self.config.theme))
                            .color(color)
                            .into(),
                    )
                }
            }
            ModuleKind::Volume => {
                let snap = self.displayed_volume();
                Some(widgets::volume(
                    snap.percent,
                    snap.muted,
                    snap.bluetooth,
                    snap.device.as_deref(),
                    &self.config.theme,
                    self.config.modules.volume.step,
                    self.config.modules.volume.show_percent,
                    self.config.modules.volume.show_device,
                    &clicks,
                ))
            }
            ModuleKind::Brightness => {
                let snap = self.displayed_brightness();
                if snap.available {
                    Some(widgets::brightness(
                        snap.percent,
                        &self.config.theme,
                        self.config.modules.brightness.step,
                        self.config.modules.brightness.show_percent,
                        &clicks,
                    ))
                } else {
                    None
                }
            }
            ModuleKind::Tray => {
                if self.tray_items.is_empty() {
                    None
                } else {
                    Some(widgets::tray_icons(&self.tray_items, &self.config.theme))
                }
            }
            ModuleKind::Custom(id) => {
                let text_val = self.custom.get(id).cloned().unwrap_or_default();
                if text_val.is_empty() {
                    return None;
                }
                let content = text(text_val)
                    .size(self.config.theme.font_size)
                    .font(style::ui_font(&self.config.theme))
                    .color(style::rgba(self.config.theme.text));
                Some(widgets::with_clicks(
                    content.into(),
                    ModuleKind::Custom(id.clone()),
                    &clicks,
                ))
            }
        }?;

        // Volume / brightness / workspaces / custom / tray / clock manage their own hit targets.
        match kind {
            ModuleKind::Volume
            | ModuleKind::Brightness
            | ModuleKind::Workspaces
            | ModuleKind::Taskbar
            | ModuleKind::Tray
            | ModuleKind::Clock
            | ModuleKind::Weather
            | ModuleKind::Custom(_) => Some(el),
            other => Some(widgets::with_clicks(el, other.clone(), &clicks)),
        }
    }
}

/// Push service events into iced as they arrive (no polling timer).
fn service_subscription(slot: ServiceRxSlot) -> Subscription<Message> {
    Subscription::run_with(slot, |slot| {
        let slot = slot.clone();
        stream::channel(64, async move |mut output| {
            let mut rx = loop {
                if let Some(rx) = slot.take_rx() {
                    break rx;
                }
                // Subscription rebuilt before rx was installed — wait briefly.
                tokio::time::sleep(Duration::from_millis(10)).await;
            };
            loop {
                let Some(first) = rx.recv().await else {
                    break;
                };
                let mut batch = vec![first];
                // Brief coalesce so a burst (niri + sys + tray) is one redraw.
                tokio::time::sleep(Duration::from_millis(8)).await;
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                if output.send(Message::Services(batch)).await.is_err() {
                    break;
                }
            }
        })
    })
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

fn tooltip_popup_size(lines: &[String], theme: &ThemeConfig) -> (u32, u32) {
    let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(8);
    let shadow_x = theme.island_shadow_offset[0].max(0.0) + theme.island_shadow_blur.max(0.0);
    let shadow_y = theme.island_shadow_offset[1].max(0.0) + theme.island_shadow_blur.max(0.0);
    let width = ((max_chars as f32) * 8.5 + 28.0 + shadow_x).ceil() as u32;
    let line_h = theme.font_size + 4.0;
    let height = (lines.len() as f32 * line_h + 16.0 + shadow_y).ceil() as u32;
    (width.clamp(80, 560), height.clamp(28, 420))
}
