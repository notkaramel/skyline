//! Niri compositor backend using the official EventStream IPC.

use std::collections::HashMap;
use std::thread;

use niri_ipc::socket::Socket;
use niri_ipc::state::{EventStreamState, EventStreamStatePart};
use niri_ipc::{Request, Response};
use skyline_core::{CompositorState, OutputInfo, ServiceEvent, WindowInfo, WorkspaceInfo};
use tracing::{debug, error, warn};

/// Spawn a blocking thread that streams niri events into `tx`.
pub fn spawn(tx: std::sync::mpsc::Sender<ServiceEvent>) {
    thread::Builder::new()
        .name("skyline-niri".into())
        .spawn(move || {
            if let Err(err) = run(tx.clone()) {
                let _ = tx.send(ServiceEvent::Error(format!("niri backend: {err}")));
            }
        })
        .expect("spawn niri backend thread");
}

fn run(tx: std::sync::mpsc::Sender<ServiceEvent>) -> std::io::Result<()> {
    let mut outputs = fetch_outputs();
    let mut socket = Socket::connect()?;
    let reply = socket.send(Request::EventStream)?;
    match reply {
        Ok(Response::Handled) => {}
        Ok(other) => {
            warn!("unexpected niri EventStream reply: {other:?}");
            return Ok(());
        }
        Err(err) => {
            return Err(std::io::Error::other(err));
        }
    }

    let mut state = EventStreamState::default();
    let mut read_event = socket.read_events();
    let mut events_since_output_refresh = 0u32;

    loop {
        let event = match read_event() {
            Ok(ev) => ev,
            Err(err) => {
                error!("niri event stream ended: {err}");
                break;
            }
        };
        debug!(?event, "niri event");
        state.apply(event);
        events_since_output_refresh += 1;
        // Outputs rarely change; refresh occasionally so hotplug is picked up.
        if events_since_output_refresh >= 64 || outputs.is_empty() {
            outputs = fetch_outputs();
            events_since_output_refresh = 0;
        }
        let snapshot = snapshot_from_state(&state, &outputs);
        if tx.send(ServiceEvent::Compositor(snapshot)).is_err() {
            break;
        }
    }
    Ok(())
}

fn fetch_outputs() -> Vec<OutputInfo> {
    let Ok(mut socket) = Socket::connect() else {
        return Vec::new();
    };
    let Ok(reply) = socket.send(Request::Outputs) else {
        return Vec::new();
    };
    let Ok(Response::Outputs(map)) = reply else {
        return Vec::new();
    };
    let mut outs: Vec<OutputInfo> = map
        .into_iter()
        .filter_map(|(name, out)| {
            let logical = out.logical?;
            Some(OutputInfo {
                name,
                x: logical.x,
                y: logical.y,
                width: logical.width,
                height: logical.height,
            })
        })
        .collect();
    outs.sort_by(|a, b| (a.x, a.y).cmp(&(b.x, b.y)));
    outs
}

fn snapshot_from_state(state: &EventStreamState, outputs: &[OutputInfo]) -> CompositorState {
    let mut workspaces: Vec<WorkspaceInfo> = state
        .workspaces
        .workspaces
        .values()
        .map(|ws| WorkspaceInfo {
            id: ws.id,
            name: ws.name.clone().unwrap_or_else(|| ws.idx.to_string()),
            index: ws.idx,
            output: ws.output.clone(),
            active: ws.is_active,
            urgent: ws.is_urgent,
        })
        .collect();
    workspaces.sort_by_key(|w| (w.output.clone().unwrap_or_default(), w.index));

    let workspace_output: HashMap<u64, Option<String>> = state
        .workspaces
        .workspaces
        .values()
        .map(|ws| (ws.id, ws.output.clone()))
        .collect();

    let mut windows: Vec<WindowInfo> = state
        .windows
        .windows
        .values()
        .map(|win| {
            let output = win
                .workspace_id
                .and_then(|id| workspace_output.get(&id).cloned())
                .flatten();
            WindowInfo {
                id: win.id,
                title: win.title.clone().unwrap_or_default(),
                app_id: win.app_id.clone(),
                workspace_id: win.workspace_id,
                output,
                focused: win.is_focused,
                focus_token: win.id.to_string(),
            }
        })
        .collect();
    windows.sort_by_key(|w| w.id);

    let focused_window = windows.iter().find(|w| w.focused).cloned();
    let focused_output = state
        .workspaces
        .workspaces
        .values()
        .find(|ws| ws.is_focused)
        .and_then(|ws| ws.output.clone())
        .or_else(|| focused_window.as_ref().and_then(|w| w.output.clone()));

    CompositorState {
        focused_output,
        outputs: outputs.to_vec(),
        workspaces,
        windows,
        focused_window,
    }
}

/// Focus a niri workspace by id (uses a separate socket).
pub fn focus_workspace(id: u64) -> std::io::Result<()> {
    let mut socket = Socket::connect()?;
    let _ = socket.send(Request::Action(niri_ipc::Action::FocusWorkspace {
        reference: niri_ipc::WorkspaceReferenceArg::Id(id),
    }))?;
    Ok(())
}

/// Focus a niri window by id.
pub fn focus_window(id: u64) -> std::io::Result<()> {
    let mut socket = Socket::connect()?;
    let _ = socket.send(Request::Action(niri_ipc::Action::FocusWindow { id }))?;
    Ok(())
}

/// Detect whether we appear to be running under niri.
pub fn is_available() -> bool {
    std::env::var_os(niri_ipc::socket::SOCKET_PATH_ENV).is_some()
}
