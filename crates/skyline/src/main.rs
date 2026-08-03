//! Skyline — native Wayland status bar.

mod app;
mod style;
mod widgets;

use std::env;
use std::path::PathBuf;

use anyhow::Result;
use iced_layershell::reexport::Anchor;
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::build_pattern::daemon;
use skyline_core::{CompositorBackendKind, Config, ServiceEvent};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::app::{App, ServiceRxSlot};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("skyline=info".parse()?)
                // Quirky SNI clients spam variant errors; our patched host soft-fails them.
                .add_directive("system_tray=warn".parse()?),
        )
        .init();

    let (config, config_path) = load_config()?;
    info!(
        "config loaded from {} (anchor={}, height={})",
        config_path.display(),
        config.bar.anchor,
        config.bar.height
    );

    let (service_tx, service_rx) = unbounded_channel();
    skyline_services::spawn_all(&config, config_path, service_tx.clone());
    spawn_compositor(&config, service_tx.clone());
    let service_rx = ServiceRxSlot::new(service_rx);

    let start_mode = match config.bar.output.clone().or_else(|| env::var("SKYLINE_OUTPUT").ok()) {
        Some(output) => StartMode::TargetScreen(output),
        None => StartMode::AllScreens,
    };

    let anchor = match config.bar.anchor.as_str() {
        "bottom" => Anchor::Bottom | Anchor::Left | Anchor::Right,
        _ => Anchor::Top | Anchor::Left | Anchor::Right,
    };

    let height = config.bar.height;
    let exclusive = config.bar.exclusive_zone;
    let margin = config.bar.margin;

    let boot_config = config.clone();
    let boot_rx = service_rx.clone();
    let boot_tx = service_tx.clone();
    daemon(
        move || App::new(boot_config.clone(), boot_rx.clone(), boot_tx.clone()),
        App::namespace,
        App::update,
        App::view,
    )
    .subscription(App::subscription)
    .style(style::app_style)
    .settings(Settings {
        default_font: style::named_font(&config.theme.font),
        default_text_size: iced::Pixels(config.theme.font_size),
        layer_settings: LayerShellSettings {
            size: Some((0, height)),
            exclusive_zone: exclusive,
            anchor,
            margin: (margin[0], margin[1], margin[2], margin[3]),
            start_mode,
            ..Default::default()
        },
        ..Default::default()
    })
    .run()
    .map_err(|e| anyhow::anyhow!("iced_layershell: {e}"))?;

    Ok(())
}

fn load_config() -> Result<(Config, PathBuf)> {
    let args: Vec<String> = env::args().collect();
    let mut path: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" | "-c" => {
                if let Some(p) = args.get(i + 1) {
                    path = Some(PathBuf::from(p));
                    i += 2;
                    continue;
                }
            }
            "--write-example-config" => {
                let dest = Config::default_path();
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, Config::example_toml())?;
                println!("wrote {}", dest.display());
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "skyline — Wayland status bar\n\n\
                     Usage: skyline [--config PATH] [--write-example-config]\n\n\
                     Env: SKYLINE_OUTPUT=<wayland-output-name>\n\
                     Config is hot-reloaded when the file is saved."
                );
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    let path = path.unwrap_or_else(Config::default_path);
    let config = if path.exists() {
        Config::load(&path)?
    } else {
        Config::default()
    };
    Ok((config, path))
}

fn spawn_compositor(config: &Config, tx: UnboundedSender<ServiceEvent>) {
    let kind = match config.compositor.backend {
        CompositorBackendKind::Auto => {
            if skyline_niri::is_available() {
                CompositorBackendKind::Niri
            } else if skyline_hyprland::is_available() {
                CompositorBackendKind::Hyprland
            } else {
                warn!("no compositor IPC detected (niri/hyprland); workspaces disabled");
                CompositorBackendKind::None
            }
        }
        other => other,
    };

    match kind {
        CompositorBackendKind::Niri => {
            info!("using niri compositor backend");
            skyline_niri::spawn(tx);
        }
        CompositorBackendKind::Hyprland => {
            info!("using hyprland compositor backend");
            skyline_hyprland::spawn(tx);
        }
        CompositorBackendKind::None | CompositorBackendKind::Auto => {}
    }
}
