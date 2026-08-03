//! Network status via `ip`, updated from `ip monitor` / sysfs events (no polling loop).

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use crate::ServiceTx;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use skyline_core::{NetworkSnapshot, ServiceEvent};
use tracing::warn;

use crate::spawn_named;

pub fn spawn(tx: ServiceTx) {
    spawn_named("skyline-network", move || {
        let mut last = probe();
        if tx.send(ServiceEvent::Network(last.clone())).is_err() {
            return;
        }

        if run_ip_monitor(&tx, &mut last).is_err() {
            warn!("ip monitor unavailable; watching /sys/class/net");
            if let Err(err) = run_sysfs_watch(&tx, &mut last) {
                warn!("network watch failed ({err}); network module will not auto-update");
            }
        }
    });
}

fn publish_if_changed(
    tx: &ServiceTx,
    last: &mut NetworkSnapshot,
) -> Result<(), ()> {
    let snap = probe();
    if snap != *last {
        *last = snap.clone();
        tx.send(ServiceEvent::Network(snap)).map_err(|_| ())?;
    }
    Ok(())
}

/// Stream kernel network events from `ip monitor` (iproute2).
fn run_ip_monitor(tx: &ServiceTx, last: &mut NetworkSnapshot) -> Result<(), String> {
    let mut child = Command::new("ip")
        .args(["-o", "monitor", "link", "address", "route"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "no stdout".to_string())?;

    let (line_tx, line_rx) = std::sync::mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("skyline-net-ipmon".into())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if line.is_err() {
                    break;
                }
                if line_tx.send(()).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;

    debounce_loop(tx, last, &line_rx, Duration::from_millis(200))?;
    let _ = child.kill();
    Err("ip monitor ended".into())
}

fn run_sysfs_watch(tx: &ServiceTx, last: &mut NetworkSnapshot) -> Result<(), String> {
    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if res.is_ok() {
                let _ = notify_tx.send(());
            }
        },
        NotifyConfig::default(),
    )
    .map_err(|e| e.to_string())?;

    let net = Path::new("/sys/class/net");
    if net.exists() {
        watcher
            .watch(net, RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;
    } else {
        return Err("/sys/class/net missing".into());
    }

    // Keep watcher alive for the loop lifetime.
    let _watcher = watcher;
    debounce_loop(tx, last, &notify_rx, Duration::from_millis(250))
}

fn debounce_loop(
    tx: &ServiceTx,
    last: &mut NetworkSnapshot,
    rx: &std::sync::mpsc::Receiver<()>,
    debounce: Duration,
) -> Result<(), String> {
    let mut pending_until: Option<Instant> = None;
    loop {
        let timeout = pending_until
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(timeout) {
            Ok(()) => {
                pending_until = Some(Instant::now() + debounce);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if pending_until.is_some_and(|until| Instant::now() >= until) {
                    pending_until = None;
                    if publish_if_changed(tx, last).is_err() {
                        return Err("ui channel closed".into());
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("event source closed".into());
            }
        }
    }
}

fn probe() -> NetworkSnapshot {
    let Some(iface) = primary_iface() else {
        return offline();
    };

    if is_wireless(&iface) {
        let ssid = wifi_ssid(&iface);
        let strength = wifi_signal_percent(&iface);
        return NetworkSnapshot {
            connected: true,
            label: ssid.unwrap_or_else(|| iface.clone()),
            strength,
            interface: Some(iface),
            kind: Some("wifi".into()),
        };
    }

    if is_ethernet(&iface) {
        return NetworkSnapshot {
            connected: true,
            label: "Ethernet".into(),
            strength: None,
            interface: Some(iface),
            kind: Some("ethernet".into()),
        };
    }

    NetworkSnapshot {
        connected: true,
        label: iface.clone(),
        strength: None,
        interface: Some(iface),
        kind: None,
    }
}

/// Default-route device from `ip`, else first non-loopback UP+LOWER_UP link.
fn primary_iface() -> Option<String> {
    if let Some(dev) = default_route_dev() {
        if link_is_up(&dev) {
            return Some(dev);
        }
    }
    for (name, up) in ip_links() {
        if name == "lo" || name.starts_with("br-") || name.starts_with("docker") || name.starts_with("veth")
        {
            continue;
        }
        if up {
            return Some(name);
        }
    }
    None
}

fn default_route_dev() -> Option<String> {
    let output = Command::new("ip")
        .args(["-j", "route", "show", "default"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    #[derive(serde::Deserialize)]
    struct Route {
        dev: Option<String>,
        #[serde(default)]
        metric: u32,
    }
    let mut routes: Vec<Route> = serde_json::from_str(&text).ok()?;
    routes.sort_by_key(|r| r.metric);
    routes.into_iter().find_map(|r| r.dev.filter(|d| !d.is_empty()))
}

fn ip_links() -> Vec<(String, bool)> {
    let output = Command::new("ip").args(["-j", "link"]).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    #[derive(serde::Deserialize)]
    struct Link {
        ifname: String,
        #[serde(default)]
        flags: Vec<String>,
    }
    let links: Vec<Link> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    links
        .into_iter()
        .map(|l| {
            let up = l.flags.iter().any(|f| f == "UP") && l.flags.iter().any(|f| f == "LOWER_UP");
            (l.ifname, up)
        })
        .collect()
}

fn link_is_up(iface: &str) -> bool {
    ip_links()
        .into_iter()
        .find(|(n, _)| n == iface)
        .map(|(_, up)| up)
        .unwrap_or(false)
}

fn is_wireless(iface: &str) -> bool {
    let base = format!("/sys/class/net/{iface}");
    fs::metadata(format!("{base}/wireless")).is_ok() || fs::metadata(format!("{base}/phy80211")).is_ok()
}

fn is_ethernet(iface: &str) -> bool {
    iface.starts_with("eth")
        || iface.starts_with("en")
        || iface.starts_with("em")
        || iface.starts_with("eno")
}

fn wifi_ssid(iface: &str) -> Option<String> {
    if let Some(s) = ssid_from_iw(iface) {
        return Some(s);
    }
    if let Some(s) = ssid_from_iwgetid(iface) {
        return Some(s);
    }
    if let Some(s) = ssid_from_iwd_bus(iface) {
        return Some(s);
    }
    if let Some(s) = ssid_from_wpa_cli(iface) {
        return Some(s);
    }
    None
}

fn ssid_from_iw(iface: &str) -> Option<String> {
    let output = Command::new("iw")
        .args(["dev", iface, "link"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.trim().strip_prefix("SSID:") {
            let s = rest.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn ssid_from_iwgetid(iface: &str) -> Option<String> {
    for args in [vec!["-r", iface], vec!["-r"]] {
        let output = Command::new("iwgetid").args(&args).output().ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn ssid_from_wpa_cli(iface: &str) -> Option<String> {
    let output = Command::new("wpa_cli")
        .args(["-i", iface, "status"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut ssid = None;
    let mut completed = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("ssid=") {
            let s = rest.trim();
            if !s.is_empty() {
                ssid = Some(s.to_string());
            }
        } else if line.contains("wpa_state=COMPLETED") {
            completed = true;
        }
    }
    if completed {
        ssid
    } else {
        None
    }
}

fn ssid_from_iwd_bus(iface: &str) -> Option<String> {
    let tree = Command::new("busctl")
        .args(["tree", "--list", "net.connman.iwd"])
        .output()
        .ok()?;
    if !tree.status.success() {
        return None;
    }
    let paths = String::from_utf8_lossy(&tree.stdout);
    for path in paths.lines().map(str::trim).filter(|p| p.starts_with('/')) {
        let Some(name) =
            busctl_get_string("net.connman.iwd", path, "net.connman.iwd.Device", "Name")
        else {
            continue;
        };
        if name != iface {
            continue;
        }
        let Some(state) =
            busctl_get_string("net.connman.iwd", path, "net.connman.iwd.Station", "State")
        else {
            continue;
        };
        if state != "connected" {
            continue;
        }
        let Some(net_path) = busctl_get_object(
            "net.connman.iwd",
            path,
            "net.connman.iwd.Station",
            "ConnectedNetwork",
        ) else {
            continue;
        };
        return busctl_get_string("net.connman.iwd", &net_path, "net.connman.iwd.Network", "Name");
    }
    None
}

fn busctl_get_string(dest: &str, path: &str, iface: &str, prop: &str) -> Option<String> {
    let output = Command::new("busctl")
        .args(["get-property", dest, path, iface, prop])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_busctl_string(&text)
}

fn busctl_get_object(dest: &str, path: &str, iface: &str, prop: &str) -> Option<String> {
    let output = Command::new("busctl")
        .args(["get-property", dest, path, iface, prop])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    let rest = text.strip_prefix('o')?.trim();
    parse_quoted(rest)
}

fn parse_busctl_string(text: &str) -> Option<String> {
    let text = text.trim();
    let rest = text.strip_prefix('s')?.trim();
    parse_quoted(rest)
}

fn parse_quoted(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        return Some(inner[..end].to_string());
    }
    if !rest.is_empty() {
        return Some(rest.to_string());
    }
    None
}

fn wifi_signal_percent(iface: &str) -> Option<u8> {
    if let Some(pct) = signal_from_iw(iface) {
        return Some(pct);
    }
    signal_from_proc(iface)
}

fn signal_from_iw(iface: &str) -> Option<u8> {
    let output = Command::new("iw")
        .args(["dev", iface, "link"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.trim().strip_prefix("signal:") {
            if let Some(dbm) = rest.split_whitespace().next().and_then(|s| s.parse::<i32>().ok()) {
                return Some(dbm_to_percent(dbm));
            }
        }
    }
    None
}

fn signal_from_proc(iface: &str) -> Option<u8> {
    let text = fs::read_to_string("/proc/net/wireless").ok()?;
    for line in text.lines() {
        let line = line.trim_start();
        if !line.starts_with(iface) {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let level = parts[3].trim_end_matches('.').parse::<i32>().ok()?;
        if level < 0 {
            return Some(dbm_to_percent(level));
        }
        return Some(level.clamp(0, 100) as u8);
    }
    None
}

fn dbm_to_percent(dbm: i32) -> u8 {
    let pct = ((dbm + 100) * 100) / 60;
    pct.clamp(0, 100) as u8
}

fn offline() -> NetworkSnapshot {
    NetworkSnapshot {
        connected: false,
        label: "offline".into(),
        strength: None,
        interface: None,
        kind: None,
    }
}
