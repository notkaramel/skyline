use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use skyline_core::{BrightnessSnapshot, ServiceEvent};
use tracing::debug;

use crate::spawn_named;

static BRIGHTNESS: OnceLock<Mutex<BrightnessSnapshot>> = OnceLock::new();
static BRIGHTNESS_TX: OnceLock<Mutex<Option<Sender<ServiceEvent>>>> = OnceLock::new();

fn cell() -> &'static Mutex<BrightnessSnapshot> {
    BRIGHTNESS.get_or_init(|| Mutex::new(BrightnessSnapshot::default()))
}

fn store_tx(tx: Sender<ServiceEvent>) {
    let slot = BRIGHTNESS_TX.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(tx);
    }
}

fn publish(snap: BrightnessSnapshot) {
    if let Ok(mut g) = cell().lock() {
        *g = snap.clone();
    }
    if let Some(Ok(slot)) = BRIGHTNESS_TX.get().map(|m| m.lock()) {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(ServiceEvent::Brightness(snap));
        }
    }
}

pub fn spawn(tx: Sender<ServiceEvent>) {
    store_tx(tx.clone());
    spawn_named("skyline-brightness", move || {
        let mut last = BrightnessSnapshot {
            percent: -1.0,
            available: false,
        };
        loop {
            let snap = read_brightness();
            if (snap.percent - last.percent).abs() > 0.05 || snap.available != last.available {
                last = snap.clone();
                publish(snap);
            }
            std::thread::sleep(Duration::from_millis(150));
            let _ = &tx;
        }
    });
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
    // Reject non-backlight rows if -c was ignored / listing leaked through.
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

    // Docs: `+10%` or `50%-`
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
