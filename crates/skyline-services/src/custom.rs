use std::process::Command;
use std::sync::atomic::Ordering;
use crate::ServiceTx;
use std::time::Duration;

use skyline_core::{CustomModuleConfig, CustomSnapshot, ServiceEvent};
use tracing::warn;

use crate::live;
use crate::spawn_named;

pub fn spawn(modules: Vec<CustomModuleConfig>, tx: ServiceTx) {
    let gen = live::get().custom_generation.load(Ordering::SeqCst);
    spawn_generation(gen, modules, tx);
}

/// Bump generation (stopping previous custom loops) and start modules from the
/// reloaded config.
pub fn reload(modules: Vec<CustomModuleConfig>, tx: ServiceTx) {
    let gen = live::get()
        .custom_generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    spawn_generation(gen, modules, tx);
}

fn spawn_generation(gen: u64, modules: Vec<CustomModuleConfig>, tx: ServiceTx) {
    for module in modules {
        let tx = tx.clone();
        spawn_named("skyline-custom", move || {
            let id = module.id.clone();
            loop {
                if live::get().custom_generation.load(Ordering::SeqCst) != gen {
                    break;
                }
                match run_command(&module) {
                    Ok(text) => {
                        if tx
                            .send(ServiceEvent::Custom(CustomSnapshot {
                                id: id.clone(),
                                text,
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => warn!("custom module {}: {err}", module.id),
                }
                // Sleep in small slices so generation bumps stop promptly.
                let total = module.interval_ms.max(500);
                let mut slept = 0u64;
                while slept < total {
                    if live::get().custom_generation.load(Ordering::SeqCst) != gen {
                        return;
                    }
                    let step = (total - slept).min(200);
                    std::thread::sleep(Duration::from_millis(step));
                    slept += step;
                }
            }
        });
    }
}

fn run_command(module: &CustomModuleConfig) -> anyhow::Result<String> {
    let output = Command::new(&module.command)
        .args(&module.args)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if module.json {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                return Ok(text.to_string());
            }
        }
    }
    Ok(stdout.lines().next().unwrap_or("").to_string())
}

pub fn run_click(on_click: &str) {
    let _ = Command::new("sh").args(["-c", on_click]).spawn();
}
