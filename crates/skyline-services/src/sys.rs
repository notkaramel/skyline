use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

use skyline_core::{ServiceEvent, SysSnapshot};
use sysinfo::System;

use crate::live;
use crate::spawn_named;

pub fn spawn(tx: Sender<ServiceEvent>) {
    spawn_named("skyline-sys", move || {
        let mut sys = System::new();
        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let cpu = sys.global_cpu_usage();
            let total = sys.total_memory() as f32;
            let used = sys.used_memory() as f32;
            let (gpu_percent, gpu_label) = probe_gpu();
            let snap = SysSnapshot {
                cpu_percent: cpu,
                memory_percent: if total > 0.0 {
                    (used / total) * 100.0
                } else {
                    0.0
                },
                memory_used_gb: used / (1024.0 * 1024.0 * 1024.0),
                memory_total_gb: total / (1024.0 * 1024.0 * 1024.0),
                gpu_percent,
                gpu_label,
            };
            if tx.send(ServiceEvent::Sys(snap)).is_err() {
                break;
            }
            let refresh_ms = live::get().sys_refresh_ms.load(Ordering::Relaxed).max(50);
            std::thread::sleep(Duration::from_millis(refresh_ms));
        }
    });
}

fn probe_gpu() -> (Option<f32>, Option<String>) {
    if let Some(v) = read_amd_gpu() {
        return (Some(v), Some("AMD".into()));
    }
    if let Some(v) = read_nvidia_gpu() {
        return (Some(v), Some("NVIDIA".into()));
    }
    if let Some(v) = read_intel_gpu() {
        return (Some(v), Some("Intel".into()));
    }
    (None, None)
}

fn read_amd_gpu() -> Option<f32> {
    let entries = fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("device/gpu_busy_percent");
        if path.exists() {
            if let Ok(s) = fs::read_to_string(&path) {
                if let Ok(v) = s.trim().parse::<f32>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn read_nvidia_gpu() -> Option<f32> {
    // Prefer nvidia-smi when NVML isn't linked; keep it optional.
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.lines().next()?.trim().parse().ok()
}

fn read_intel_gpu() -> Option<f32> {
    let path = PathBuf::from("/sys/class/drm/card0/gt/gt0/throttle_reason_status");
    let _ = path; // presence alone isn't utilization; skip speculative reads
    None
}
