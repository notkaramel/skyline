//! Background services that feed [`skyline_core::ServiceEvent`]s to the UI.

mod audio;
mod brightness;
mod clock;
mod config_watch;
mod custom;
mod live;
mod network;
mod sys;
mod tray;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;

use skyline_core::{Config, CustomModuleConfig, ServiceEvent};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

pub use audio::{set_mute, set_volume_delta};
pub use brightness::set_brightness_delta;
pub use custom::run_click;
pub use live::apply as apply_live;
pub use tray::{activate_item, activate_menu, request_menu};

/// Channel used by background services to push UI events.
pub type ServiceTx = UnboundedSender<ServiceEvent>;

/// Start all background service threads.
pub fn spawn_all(config: &Config, config_path: PathBuf, tx: ServiceTx) {
    live::init(config);

    clock::spawn(tx.clone());
    sys::spawn(tx.clone());
    network::spawn(tx.clone());
    audio::spawn(tx.clone());
    brightness::spawn(tx.clone());
    custom::spawn(config.modules.custom.clone(), tx.clone());
    config_watch::spawn(config_path, tx.clone());

    if config.modules.tray {
        ensure_tray(tx.clone());
    }

    info!("skyline services started");
}

/// Apply a hot-reloaded config to live service knobs and custom modules.
pub fn reload_from_config(config: &Config, tx: ServiceTx) {
    live::apply(config);
    custom::reload(config.modules.custom.clone(), tx.clone());
    if config.modules.tray {
        ensure_tray(tx);
    }
}

pub fn ensure_tray(tx: ServiceTx) {
    let live = live::get();
    if live.tray_started.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(feature = "tray")]
    tray::spawn(tx);
    #[cfg(not(feature = "tray"))]
    let _ = tx;
}

/// Tiny helper used by modules that prefer a dedicated OS thread + tokio runtime.
pub(crate) fn spawn_named<F>(name: &'static str, f: F)
where
    F: FnOnce() + Send + 'static,
{
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .unwrap_or_else(|e| panic!("failed to spawn {name}: {e}"));
}

pub(crate) fn spawn_tokio(name: &'static str, fut: impl std::future::Future<Output = ()> + Send + 'static) {
    spawn_named(name, move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(fut);
    });
}

pub fn custom_configs(config: &Config) -> &[CustomModuleConfig] {
    &config.modules.custom
}
