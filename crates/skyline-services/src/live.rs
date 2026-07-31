use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use skyline_core::Config;

/// Settings that background threads re-read so a config hot-reload takes effect
/// without restarting every service.
pub struct LiveServiceConfig {
    pub clock_format: RwLock<String>,
    pub sys_refresh_ms: AtomicU64,
    pub custom_generation: AtomicU64,
    pub tray_started: AtomicBool,
    pub volume_detect_bluetooth: AtomicBool,
    /// Stored as centi-percent (150.0% → 15000) for atomic updates.
    volume_max_centi: AtomicU64,
}

static LIVE: OnceLock<Arc<LiveServiceConfig>> = OnceLock::new();

pub fn init(config: &Config) -> Arc<LiveServiceConfig> {
    let live = Arc::new(LiveServiceConfig {
        clock_format: RwLock::new(config.modules.clock.format.clone()),
        sys_refresh_ms: AtomicU64::new(config.modules.sys.refresh_ms.max(50)),
        custom_generation: AtomicU64::new(0),
        tray_started: AtomicBool::new(false),
        volume_detect_bluetooth: AtomicBool::new(config.modules.volume.detect_bluetooth),
        volume_max_centi: AtomicU64::new(percent_to_centi(config.modules.volume.max_percent)),
    });
    let _ = LIVE.set(live.clone());
    live
}

pub fn get() -> Arc<LiveServiceConfig> {
    LIVE.get()
        .cloned()
        .expect("live service config initialized")
}

pub fn apply(config: &Config) {
    let live = get();
    if let Ok(mut fmt) = live.clock_format.write() {
        *fmt = config.modules.clock.format.clone();
    }
    live.sys_refresh_ms
        .store(config.modules.sys.refresh_ms.max(50), Ordering::Relaxed);
    live.volume_detect_bluetooth
        .store(config.modules.volume.detect_bluetooth, Ordering::Relaxed);
    live.volume_max_centi.store(
        percent_to_centi(config.modules.volume.max_percent),
        Ordering::Relaxed,
    );
}

impl LiveServiceConfig {
    pub fn volume_max_percent(&self) -> f64 {
        self.volume_max_centi.load(Ordering::Relaxed) as f64 / 100.0
    }
}

fn percent_to_centi(p: f64) -> u64 {
    (p.clamp(1.0, 300.0) * 100.0).round() as u64
}
