use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{self, Facility, InterestMaskSet};
use libpulse_binding::context::{Context, FlagSet as ContextFlagSet, State};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use libpulse_binding::volume::Volume;
use skyline_core::{ServiceEvent, VolumeSnapshot};
use tracing::{debug, warn};

use crate::live;
use crate::spawn_named;

static VOLUME: OnceLock<Mutex<VolumeSnapshot>> = OnceLock::new();
static VOLUME_TX: OnceLock<Mutex<Option<Sender<ServiceEvent>>>> = OnceLock::new();

fn volume_cell() -> &'static Mutex<VolumeSnapshot> {
    VOLUME.get_or_init(|| Mutex::new(VolumeSnapshot::default()))
}

fn store_tx(tx: Sender<ServiceEvent>) {
    let slot = VOLUME_TX.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(tx);
    }
}

fn publish(snap: VolumeSnapshot) {
    if let Ok(mut g) = volume_cell().lock() {
        *g = snap.clone();
    }
    if let Some(Ok(slot)) = VOLUME_TX.get().map(|m| m.lock()) {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(ServiceEvent::Volume(snap));
        }
    }
}

pub fn spawn(tx: Sender<ServiceEvent>) {
    store_tx(tx.clone());
    spawn_named("skyline-audio", move || {
        if let Err(err) = run_pulse(tx.clone()) {
            warn!("pulse audio unavailable ({err}); falling back to wpctl");
            run_wpctl_loop(&tx);
        }
    });
}

fn run_wpctl_loop(_tx: &Sender<ServiceEvent>) {
    let snap = read_wpctl();
    publish(snap);

    if let Err(err) = run_pactl_subscribe() {
        warn!("pactl subscribe unavailable ({err}); volume updates only from bar actions / Pulse");
        loop {
            std::thread::park();
        }
    }
}

/// Stream PulseAudio / PipeWire events via `pactl subscribe` (no polling).
fn run_pactl_subscribe() -> Result<(), String> {
    let mut child = std::process::Command::new("pactl")
        .arg("subscribe")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "no stdout".to_string())?;
    let reader = std::io::BufReader::new(stdout);
    use std::io::BufRead;
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        let lower = line.to_lowercase();
        // Event 'change' on sink #0 / server / sink-input …
        if lower.contains(" on sink") || lower.contains(" on server") {
            publish(read_wpctl());
        }
    }
    let _ = child.kill();
    Err("pactl subscribe ended".into())
}

fn read_wpctl() -> VolumeSnapshot {
    let output = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output();
    let Ok(output) = output else {
        return VolumeSnapshot::default();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut percent = 0.0;
    let muted = text.contains("MUTED");
    if let Some(part) = text.split_whitespace().nth(1) {
        if let Ok(v) = part.parse::<f64>() {
            percent = v * 100.0;
        }
    }
    let (bluetooth, device) = detect_bluetooth_fallback();
    VolumeSnapshot {
        percent,
        muted,
        bluetooth,
        device,
    }
}

fn detect_bluetooth_fallback() -> (bool, Option<String>) {
    if !live::get()
        .volume_detect_bluetooth
        .load(Ordering::Relaxed)
    {
        return (false, None);
    }
    // pactl get-default-sink → bluez_output....
    if let Ok(out) = std::process::Command::new("pactl")
        .args(["get-default-sink"])
        .output()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if sink_name_is_bluetooth(&name) {
            return (true, short_device_label(&name));
        }
    }
    // wpctl status: look for * in Sinks block with bluez
    if let Ok(out) = std::process::Command::new("wpctl").arg("status").output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('*') || t.contains('*') {
                let lower = t.to_lowercase();
                if lower.contains("bluez") || lower.contains("bluetooth") {
                    let label = t
                        .trim_start_matches(|c: char| c == '*' || c.is_whitespace())
                        .split('.')
                        .nth(1)
                        .unwrap_or(t)
                        .trim()
                        .to_string();
                    return (true, short_device_label(&label));
                }
            }
        }
    }
    (false, None)
}

fn sink_name_is_bluetooth(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("bluez") || lower.contains("bluetooth")
}

fn short_device_label(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_start_matches("bluez_output.")
        .trim_start_matches("bluez_sink.");
    if cleaned.is_empty() {
        return None;
    }
    // Prefer a human-ish tail; strip MAC-like prefixes.
    let parts: Vec<&str> = cleaned.split(['.', '_', '-']).collect();
    let label = parts
        .iter()
        .rev()
        .find(|p| p.chars().any(|c| c.is_ascii_alphabetic()) && p.len() > 2)
        .copied()
        .unwrap_or(cleaned);
    let mut s = label.replace('_', " ");
    if s.len() > 18 {
        s.truncate(17);
        s.push('…');
    }
    Some(s)
}

fn run_pulse(tx: Sender<ServiceEvent>) -> Result<(), String> {
    let mut mainloop = Mainloop::new().ok_or("pulse mainloop")?;
    let mut context = Context::new(&mainloop, "skyline").ok_or("pulse context")?;
    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|e| format!("pulse connect: {e}"))?;

    loop {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                return Err("pulse iterate failed".into());
            }
            IterateResult::Success(_) => {}
        }
        match context.get_state() {
            State::Ready => break,
            State::Failed | State::Terminated => return Err("pulse failed".into()),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }

    let snap = fetch_sink_volume(&mut mainloop, &mut context);
    publish(snap);

    let dirty = Arc::new(Mutex::new(true));
    let dirty_cb = dirty.clone();
    context.set_subscribe_callback(Some(Box::new(move |facility, _op, _idx| {
        if matches!(facility, Some(Facility::Sink) | Some(Facility::Server) | None) {
            if let Ok(mut g) = dirty_cb.lock() {
                *g = true;
            }
        }
    })));
    context.subscribe(InterestMaskSet::SINK | InterestMaskSet::SERVER, |_| {});
    let _ = subscribe::Facility::Sink;

    loop {
        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => break,
            IterateResult::Success(_) => {}
        }

        let should_refresh = dirty.lock().map(|g| *g).unwrap_or(false);
        if should_refresh {
            if let Ok(mut g) = dirty.lock() {
                *g = false;
            }
            let snap = fetch_sink_volume(&mut mainloop, &mut context);
            publish(snap);
        }
    }
    let _ = tx;
    Ok(())
}

fn fetch_sink_volume(mainloop: &mut Mainloop, context: &mut Context) -> VolumeSnapshot {
    let interest = Arc::new(Mutex::new(VolumeSnapshot::default()));
    let interest2 = interest.clone();
    let detect_bt = live::get()
        .volume_detect_bluetooth
        .load(Ordering::Relaxed);
    let introspect = context.introspect();
    let _op = introspect.get_sink_info_by_name("@DEFAULT_SINK@", move |list| {
        if let ListResult::Item(sink) = list {
            let avg = sink.volume.avg().0 as f64 / Volume::NORMAL.0 as f64;
            let name = sink.name.as_deref().unwrap_or("");
            let desc = sink.description.as_deref().unwrap_or("");
            let bluetooth = detect_bt
                && (sink_name_is_bluetooth(name)
                    || desc.to_lowercase().contains("bluetooth")
                    || sink.ports.iter().any(|p| {
                        let n = p.name.as_deref().unwrap_or("").to_lowercase();
                        let d = p.description.as_deref().unwrap_or("").to_lowercase();
                        n.contains("bluetooth") || d.contains("bluetooth") || n.contains("bluez")
                    }));
            let device = if bluetooth {
                short_device_label(if desc.is_empty() { name } else { desc })
            } else {
                None
            };
            if let Ok(mut g) = interest2.lock() {
                g.percent = avg * 100.0;
                g.muted = sink.mute;
                g.bluetooth = bluetooth;
                g.device = device;
            }
        }
    });
    for _ in 0..40 {
        match mainloop.iterate(false) {
            IterateResult::Success(_) => {}
            _ => break,
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    interest.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Apply a scroll delta as a fraction of full scale (e.g. 0.02 = +2%).
pub fn set_volume_delta(delta_fraction: f64) -> VolumeSnapshot {
    let current = volume_cell()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let max = live::get().volume_max_percent();
    let next = (current.percent + delta_fraction * 100.0).clamp(0.0, max);
    let _ = std::process::Command::new("wpctl")
        .args([
            "set-volume",
            "@DEFAULT_AUDIO_SINK@",
            &format!("{:.0}%", next),
        ])
        .status();
    let snap = VolumeSnapshot {
        percent: next,
        muted: current.muted,
        bluetooth: current.bluetooth,
        device: current.device,
    };
    publish(snap.clone());
    debug!("volume -> {next:.0}%");
    snap
}

pub fn set_mute() -> VolumeSnapshot {
    let prev = volume_cell()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let _ = std::process::Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .status();
    let mut snap = read_wpctl();
    // Prefer previous BT metadata if wpctl fallback didn't resolve it yet.
    if !snap.bluetooth && prev.bluetooth {
        snap.bluetooth = true;
        snap.device = prev.device;
    }
    publish(snap.clone());
    snap
}
