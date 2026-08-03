use std::path::{Path, PathBuf};
use crate::ServiceTx;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use skyline_core::{Config, ServiceEvent};
use tracing::{info, warn};

use crate::spawn_named;

/// Watch `path` (and its parent directory) and emit [`ServiceEvent::ConfigReloaded`]
/// after successful saves. Debounces bursty editor write/rename sequences.
pub fn spawn(path: PathBuf, tx: ServiceTx) {
    spawn_named("skyline-config", move || {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let (notify_tx, notify_rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                let _ = notify_tx.send(res);
            },
            NotifyConfig::default(),
        ) {
            Ok(w) => w,
            Err(err) => {
                warn!("config watcher unavailable: {err}");
                return;
            }
        };

        // Prefer watching the parent so atomic replace (tmp + rename) is visible.
        let watch_target = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        if let Err(err) = watcher.watch(&watch_target, RecursiveMode::NonRecursive) {
            warn!("config watch {}: {err}", watch_target.display());
            return;
        }
        // Also watch the file itself when it already exists.
        if path.exists() {
            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        }

        info!("watching config {}", path.display());

        let file_name = path.file_name().map(|s| s.to_os_string());
        let debounce = Duration::from_millis(250);
        let mut pending_until: Option<Instant> = None;

        loop {
            let timeout = pending_until
                .map(|until| until.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(3600));

            match notify_rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    if !is_relevant(&event, &path, file_name.as_deref()) {
                        continue;
                    }
                    pending_until = Some(Instant::now() + debounce);
                }
                Ok(Err(err)) => {
                    warn!("config watcher error: {err}");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending_until.is_some_and(|until| Instant::now() >= until) {
                        pending_until = None;
                        // Re-attach watch if the inode was replaced.
                        if path.exists() {
                            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
                        }
                        match Config::load(&path) {
                            Ok(config) => {
                                info!("config reloaded from {}", path.display());
                                if tx
                                    .send(ServiceEvent::ConfigReloaded(Box::new(config)))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!("config reload failed: {err:#}");
                                let _ = tx.send(ServiceEvent::Error(format!(
                                    "config reload failed: {err:#}"
                                )));
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn is_relevant(
    event: &notify::Event,
    path: &Path,
    file_name: Option<&std::ffi::OsStr>,
) -> bool {
    match event.kind {
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) | EventKind::Any => {}
        EventKind::Access(_) | EventKind::Other => return false,
    }
    event.paths.iter().any(|p| {
        p == path
            || file_name.is_some_and(|name| p.file_name() == Some(name))
            || p.ends_with(path.file_name().unwrap_or_default())
    })
}
