//! Niri compositor backend using the official EventStream IPC.

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use niri_ipc::socket::Socket;
use niri_ipc::state::{EventStreamState, EventStreamStatePart};
use niri_ipc::{Request, Response};
use skyline_core::{CompositorState, OutputInfo, ServiceEvent, WindowInfo, WorkspaceInfo};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, warn};

/// Re-fetch outputs at least this often so DPMS / hotplug is not stuck behind
/// a long stretch of layout-only events.
const OUTPUT_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Spawn a blocking thread that streams niri events into `tx`.
pub fn spawn(tx: UnboundedSender<ServiceEvent>) {
    thread::Builder::new()
        .name("skyline-niri".into())
        .spawn(move || {
            if let Err(err) = run(tx.clone()) {
                let _ = tx.send(ServiceEvent::Error(format!("niri backend: {err}")));
            }
        })
        .expect("spawn niri backend thread");
}

fn run(tx: UnboundedSender<ServiceEvent>) -> std::io::Result<()> {
    let mut outputs = fetch_outputs(&[]);
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
    let mut last_output_refresh = Instant::now();
    let mut last_snapshot: Option<CompositorState> = None;
    let mut last_workspace_outputs: HashMap<u64, Option<String>> = HashMap::new();

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

        let workspace_outputs_changed = {
            let current: HashMap<u64, Option<String>> = state
                .workspaces
                .workspaces
                .values()
                .map(|ws| (ws.id, ws.output.clone()))
                .collect();
            let changed = current != last_workspace_outputs;
            if changed {
                last_workspace_outputs = current;
            }
            changed
        };

        if workspace_outputs_changed
            || outputs.is_empty()
            || last_output_refresh.elapsed() >= OUTPUT_REFRESH_INTERVAL
        {
            outputs = fetch_outputs(&outputs);
            last_output_refresh = Instant::now();
        }
        let snapshot = snapshot_from_state(&state, &outputs);
        // Layout-only niri events often leave our bar snapshot unchanged; skip
        // those so iced does not redraw at compositor frame rate.
        if last_snapshot.as_ref() == Some(&snapshot) {
            continue;
        }
        last_snapshot = Some(snapshot.clone());
        if tx.send(ServiceEvent::Compositor(snapshot)).is_err() {
            break;
        }
    }
    Ok(())
}

fn fetch_outputs(previous: &[OutputInfo]) -> Vec<OutputInfo> {
    let Ok(mut socket) = Socket::connect() else {
        return previous.to_vec();
    };
    let Ok(reply) = socket.send(Request::Outputs) else {
        return previous.to_vec();
    };
    let Ok(Response::Outputs(map)) = reply else {
        return previous.to_vec();
    };
    let prev_by_name: HashMap<&str, &OutputInfo> =
        previous.iter().map(|o| (o.name.as_str(), o)).collect();
    let mut outs: Vec<OutputInfo> = map
        .into_iter()
        .map(|(name, out)| {
            if let Some(logical) = out.logical {
                OutputInfo {
                    name,
                    x: logical.x,
                    y: logical.y,
                    width: logical.width,
                    height: logical.height,
                }
            } else if let Some(prev) = prev_by_name.get(name.as_str()) {
                // DPMS / temporarily missing logical geometry — keep last size for pinning.
                OutputInfo {
                    name,
                    x: prev.x,
                    y: prev.y,
                    width: prev.width,
                    height: prev.height,
                }
            } else {
                OutputInfo {
                    name,
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }
            }
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
