use std::path::PathBuf;
use std::process::Command;
use crate::ServiceTx;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use skyline_core::{BrightnessSnapshot, ServiceEvent};
use tracing::{debug, warn};

use crate::spawn_named;

static BRIGHTNESS: OnceLock<Mutex<BrightnessSnapshot>> = OnceLock::new();
static BRIGHTNESS_TX: OnceLock<Mutex<Option<ServiceTx>>> = OnceLock::new();

fn cell() -> &'static Mutex<BrightnessSnapshot> {
    BRIGHTNESS.get_or_init(|| Mutex::new(BrightnessSnapshot::default()))
}

fn store_tx(tx: ServiceTx) {
    let slot = BRIGHTNESS_TX.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(tx);
    }
}

fn publish(snap: BrightnessSnapshot) {
    if let Ok(mut g) = cell().lock() {
        if g.visually_eq(&snap) {
            *g = snap;
            return;
        }
        *g = snap.clone();
    }
    if let Some(Ok(slot)) = BRIGHTNESS_TX.get().map(|m| m.lock()) {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(ServiceEvent::Brightness(snap));
        }
    }
}

pub fn spawn(tx: ServiceTx) {
    store_tx(tx);
    let snap = read_brightness();
    publish(snap);

    spawn_named("skyline-brightness", move || {
        if let Err(err) = run_sysfs_watch() {
            warn!("brightness sysfs watch unavailable ({err}); updates only from bar actions");
            // Stay alive so set_brightness_delta can still publish via BRIGHTNESS_TX.
            loop {
                std::thread::park();
            }
        }
    });
}

fn run_sysfs_watch() -> Result<(), String> {
    let backlight = PathBuf::from("/sys/class/backlight");
    if !backlight.exists() {
        return Err("/sys/class/backlight missing".into());
    }

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) | EventKind::Any
                ) {
                    let _ = notify_tx.send(());
                }
            }
        },
        NotifyConfig::default(),
    )
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&backlight, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    // Also watch each device brightness node explicitly (some kernels only
    // notify on the file, not the directory).
    if let Ok(entries) = std::fs::read_dir(&backlight) {
        for entry in entries.flatten() {
            let brightness = entry.path().join("brightness");
            if brightness.exists() {
                let _ = watcher.watch(&brightness, RecursiveMode::NonRecursive);
            }
        }
    }

    let _watcher = watcher;
    let debounce = Duration::from_millis(50);
    let mut pending_until: Option<Instant> = None;
    let mut last = cell()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();

    loop {
        let timeout = pending_until
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match notify_rx.recv_timeout(timeout) {
            Ok(()) => {
                pending_until = Some(Instant::now() + debounce);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if pending_until.is_some_and(|until| Instant::now() >= until) {
                    pending_until = None;
                    let snap = read_brightness();
                    if (snap.percent - last.percent).abs() > 0.05 || snap.available != last.available
                    {
                        last = snap.clone();
                        publish(snap);
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("watcher closed".into());
            }
        }
    }
}

/// Parse `brightnessctl -c backlight -m`:
/// `device,class,current,percent%,max`
///
/// Class is required — without it brightnessctl picks the first LED
/// (caps/num lock, NIC lights, …) which is not screen brightness.
fn read_brightness() -> BrightnessSnapshot {
    let Ok(output) = Command::new("brightnessctl")
        .args(["-c", "backlight", "-m"])
        .output()
    else {
        return unavailable();
    };
    if !output.status.success() {
        return unavailable();
    }
    parse_machine_readable(&String::from_utf8_lossy(&output.stdout)).unwrap_or_else(unavailable)
}

fn parse_machine_readable(text: &str) -> Option<BrightnessSnapshot> {
    let line = text.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    if let Some(class) = parts.get(1) {
        if !class.eq_ignore_ascii_case("backlight") {
            return None;
        }
    }
    if let Some(pct) = parts.get(3) {
        let pct = pct.trim().trim_end_matches('%');
        if let Ok(v) = pct.parse::<f64>() {
            return Some(BrightnessSnapshot {
                percent: v.clamp(0.0, 100.0),
                available: true,
            });
        }
    }
    if parts.len() >= 5 {
        let cur = parts[2].trim().parse::<f64>().ok()?;
        let max = parts[4].trim().parse::<f64>().ok()?;
        if max > 0.0 {
            return Some(BrightnessSnapshot {
                percent: ((cur / max) * 100.0).clamp(0.0, 100.0),
                available: true,
            });
        }
    }
    None
}

fn unavailable() -> BrightnessSnapshot {
    BrightnessSnapshot {
        percent: 0.0,
        available: false,
    }
}

/// Apply a brightness delta in percent points via `brightnessctl`.
pub fn set_brightness_delta(delta_percent: f64) -> BrightnessSnapshot {
    if delta_percent.abs() < f64::EPSILON {
        return read_brightness();
    }

    let arg = if delta_percent >= 0.0 {
        format!("+{delta_percent}%")
    } else {
        format!("{}%-", delta_percent.abs())
    };

    let ok = Command::new("brightnessctl")
        .args(["-c", "backlight", "set", &arg])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let snap = if ok {
        let snap = read_brightness();
        debug!("brightness -> {:.0}%", snap.percent);
        snap
    } else {
        let current = cell().lock().map(|g| g.percent).unwrap_or(50.0);
        BrightnessSnapshot {
            percent: (current + delta_percent).clamp(1.0, 100.0),
            available: cell().lock().map(|g| g.available).unwrap_or(false),
        }
    };
    publish(snap.clone());
    snap
}
