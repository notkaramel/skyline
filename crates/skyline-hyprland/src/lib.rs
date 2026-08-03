//! Hyprland compositor backend.

use std::thread;

use hyprland::data::{Client, Clients, Monitor, Monitors, Workspace, Workspaces};
use hyprland::dispatch::{Dispatch, DispatchType, WindowIdentifier, WorkspaceIdentifierWithSpecial};
use hyprland::event_listener::EventListener;
use hyprland::prelude::*;
use hyprland::shared::{Address, HyprData, HyprDataActive, HyprDataActiveOptional};
use skyline_core::{CompositorState, OutputInfo, ServiceEvent, WindowInfo, WorkspaceInfo};
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

/// Spawn a thread that listens for Hyprland events and pushes snapshots.
pub fn spawn(tx: UnboundedSender<ServiceEvent>) {
    thread::Builder::new()
        .name("skyline-hyprland".into())
        .spawn(move || {
            if let Err(err) = push_snapshot(&tx) {
                let _ = tx.send(ServiceEvent::Error(format!("hyprland initial: {err}")));
            }
            if let Err(err) = run(tx.clone()) {
                let _ = tx.send(ServiceEvent::Error(format!("hyprland backend: {err}")));
            }
        })
        .expect("spawn hyprland backend thread");
}

fn run(tx: UnboundedSender<ServiceEvent>) -> hyprland::Result<()> {
    let mut listener = EventListener::new();
    // Fresh closures per handler so each gets the right event-data type.
    {
        let tx = tx.clone();
        listener.add_workspace_changed_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_workspace_added_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_workspace_deleted_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_active_window_changed_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_active_monitor_changed_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_window_opened_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_window_closed_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_window_moved_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }
    {
        let tx = tx.clone();
        listener.add_window_title_changed_handler(move |_| {
            let _ = push_snapshot(&tx);
        });
    }

    listener.start_listener()
}

fn push_snapshot(tx: &UnboundedSender<ServiceEvent>) -> hyprland::Result<()> {
    let snapshot = collect_state()?;
    static LAST: std::sync::Mutex<Option<CompositorState>> = std::sync::Mutex::new(None);
    if let Ok(mut last) = LAST.lock() {
        if last.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        *last = Some(snapshot.clone());
    }
    if tx.send(ServiceEvent::Compositor(snapshot)).is_err() {
        warn!("hyprland: UI channel closed");
    }
    Ok(())
}

fn collect_state() -> hyprland::Result<CompositorState> {
    let active_ws = Workspace::get_active().ok();
    let active_monitor = Monitor::get_active().ok();
    let focused_output = active_monitor.as_ref().map(|m| m.name.clone());

    let workspaces_raw = Workspaces::get()?.to_vec();
    let mut workspaces: Vec<WorkspaceInfo> = workspaces_raw
        .iter()
        .map(|ws| {
            let active = active_ws
                .as_ref()
                .map(|a| a.id == ws.id)
                .unwrap_or(false)
                || active_monitor
                    .as_ref()
                    .map(|m| m.active_workspace.id == ws.id)
                    .unwrap_or(false);
            WorkspaceInfo {
                id: ws.id as u64,
                name: ws.name.clone(),
                index: ws.id.unsigned_abs() as u8,
                output: Some(ws.monitor.clone()),
                active,
                urgent: false,
            }
        })
        .collect();
    workspaces.sort_by_key(|w| (w.output.clone().unwrap_or_default(), w.index));

    let mut outputs: Vec<OutputInfo> = Vec::new();
    if let Ok(monitors) = Monitors::get() {
        for mon in monitors.to_vec() {
            for ws in &mut workspaces {
                if ws.output.as_deref() == Some(mon.name.as_str())
                    && ws.id == mon.active_workspace.id as u64
                {
                    ws.active = true;
                }
            }
            outputs.push(OutputInfo {
                name: mon.name.clone(),
                x: mon.x,
                y: mon.y,
                width: mon.width as u32,
                height: mon.height as u32,
            });
        }
    }
    outputs.sort_by(|a, b| (a.x, a.y).cmp(&(b.x, b.y)));

    let clients = Clients::get()?.to_vec();
    let focused = Client::get_active().ok().flatten();
    let focused_addr = focused.as_ref().map(|c| c.address.clone());

    let windows: Vec<WindowInfo> = clients
        .into_iter()
        .map(|c| {
            let focused = focused_addr
                .as_ref()
                .map(|a| a == &c.address)
                .unwrap_or(false);
            let addr = c.address.to_string();
            WindowInfo {
                id: address_to_id(&addr),
                title: c.title,
                app_id: Some(c.class),
                workspace_id: Some(c.workspace.id as u64),
                output: workspaces
                    .iter()
                    .find(|w| w.id == c.workspace.id as u64)
                    .and_then(|w| w.output.clone()),
                focused,
                focus_token: addr,
            }
        })
        .collect();

    let focused_window = windows.iter().find(|w| w.focused).cloned().or_else(|| {
        focused.map(|c| {
            let addr = c.address.to_string();
            WindowInfo {
                id: address_to_id(&addr),
                title: c.title,
                app_id: Some(c.class),
                workspace_id: Some(c.workspace.id as u64),
                output: focused_output.clone(),
                focused: true,
                focus_token: addr,
            }
        })
    });

    Ok(CompositorState {
        focused_output,
        outputs,
        workspaces,
        windows,
        focused_window,
    })
}

fn address_to_id(addr: &str) -> u64 {
    let hex = addr.trim_start_matches("0x");
    u64::from_str_radix(hex, 16).unwrap_or_else(|_| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        addr.hash(&mut h);
        h.finish()
    })
}

pub fn focus_workspace(id: u64) -> hyprland::Result<()> {
    Dispatch::call(DispatchType::Workspace(WorkspaceIdentifierWithSpecial::Id(
        id as i32,
    )))
}

pub fn focus_window(address: &str) -> hyprland::Result<()> {
    Dispatch::call(DispatchType::FocusWindow(WindowIdentifier::Address(
        Address::new(address),
    )))
}

pub fn is_available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}
