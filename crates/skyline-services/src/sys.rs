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
            let cpu_per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
            let cpu = sys.global_cpu_usage();
            let total = sys.total_memory() as f32;
            let used = sys.used_memory() as f32;
            let (gpu_per_device, gpu_label) = probe_gpus();
            let gpu_percent = gpu_per_device.first().copied();
            let snap = SysSnapshot {
                cpu_percent: cpu,
                cpu_per_core,
                memory_percent: if total > 0.0 {
                    (used / total) * 100.0
                } else {
                    0.0
                },
                memory_used_gb: used / (1024.0 * 1024.0 * 1024.0),
                memory_total_gb: total / (1024.0 * 1024.0 * 1024.0),
                gpu_percent,
                gpu_per_device,
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

fn probe_gpus() -> (Vec<f32>, Option<String>) {
    let amd = read_amd_gpus();
    if !amd.is_empty() {
        return (amd, Some("AMD".into()));
    }
    let nvidia = read_nvidia_gpus();
    if !nvidia.is_empty() {
        return (nvidia, Some("NVIDIA".into()));
    }
    let intel = read_intel_gpu();
    if let Some(v) = intel {
        return (vec![v], Some("Intel".into()));
    }
    (Vec::new(), None)
}

fn read_amd_gpus() -> Vec<f32> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Prefer cardN (skip renderD* / cardN-*)
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let path = entry.path().join("device/gpu_busy_percent");
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(v) = s.trim().parse::<f32>() {
                values.push(v);
            }
        }
    }
    values
}

fn read_nvidia_gpus() -> Vec<f32> {
    // Prefer nvidia-smi when NVML isn't linked; keep it optional.
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok();
    let Some(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.lines()
        .filter_map(|line| line.trim().parse::<f32>().ok())
        .collect()
}

fn read_intel_gpu() -> Option<f32> {
    let path = PathBuf::from("/sys/class/drm/card0/gt/gt0/throttle_reason_status");
    let _ = path; // presence alone isn't utilization; skip speculative reads
    None
}
