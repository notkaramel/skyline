---
name: compositor-backends
description: >-
  Work on Skyline compositor backends (niri EventStream, Hyprland IPC),
  workspaces, taskbar, focused window, multi-monitor pinning, and focus
  actions. Use when editing skyline-niri, skyline-hyprland, CompositorState,
  or workspace/taskbar/window modules.
---

# Compositor backends

## Detection (`crates/skyline/src/main.rs`)

`[compositor] backend`:

| Value | Behavior |
| --- | --- |
| `auto` (default) | niri if `NIRI_SOCKET` / `niri_ipc::socket::SOCKET_PATH_ENV`, else Hyprland if `HYPRLAND_INSTANCE_SIGNATURE`, else none |
| `niri` / `hyprland` / `none` | force |

Backend switch is **not** hot-reloaded — requires restart.

Each backend: `spawn(tx)`, `is_available()`, `focus_workspace`, `focus_window`.

## Snapshot contract (`skyline-core::CompositorState`)

```
focused_output, outputs[], workspaces[], windows[], focused_window
```

`WindowInfo.focus_token`:

- niri: decimal window id (same as `id`)
- Hyprland: client address string (`0x…`); `id` is parsed hex or a hash

Always fill `output` on workspaces and windows when the compositor knows it.
UI filters with `workspaces_for_output`, `focused_window_for_output`,
`taskbar_windows`.

**Send a snapshot only when it differs** from the last one (`PartialEq`).
Niri layout events otherwise redraw at compositor frame rate.

## Niri (`crates/skyline-niri`)

- `Socket::connect` + `Request::EventStream`
- Maintain `niri_ipc::state::EventStreamState`
- Refresh `Request::Outputs` on workspace-output changes, when empty, or at least
  every 2s so hotplug / DPMS is picked up. Keep last-known geometry if `logical`
  is temporarily missing.
- Focus: `Action::FocusWorkspace { Id }` / `Action::FocusWindow { id }`
- Pin `niri-ipc = "=26.4.0"` — keep in lockstep with the running niri version

## Hyprland (`crates/skyline-hyprland`)

- Initial `push_snapshot`, then `EventListener` handlers (workspace / window /
  monitor / title)
- Collect via `Workspaces`, `Monitors`, `Clients`, `Client::get_active`
- Mark active workspace from active monitor’s `active_workspace`, not only
  the globally active workspace
- Focus: `DispatchType::Workspace(Id)` / `FocusWindow(Address)`
- `urgent` is currently always `false` (Hyprland crate path does not expose it)

## Multi-monitor bar pinning (`App`)

iced_layershell opens one surface per output but `Opened` has `position: None`.
`pin_bar_output` matches surface width to `OutputInfo.width` (±2 px, then
closest unused < 64 px), else geometry order.

- Workspaces / taskbar / window title **must** use `output_for_bar(id)`
- Do not use global `focused_output` for those widgets on multi-head
- `bound_output` (`[bar] output` or `SKYLINE_OUTPUT`) skips pinning

## Taskbar

`CompositorState::taskbar_windows(output)`:

- Windows on the **active workspace(s) of that output only**
- Sorted by `id`
- Icons: app id → `.desktop` `Icon=` → letter fallback (`widgets.rs`)
- Left-click → `Message::FocusWindow` → backend `focus_window`
- Config: `width` (alias `icon_size`), `padding`, `gap`, `border_width`, `max_items`

## Workspaces UI

- Digit / short names shown as-is; longer names fall back to `index`
- Active: accent fill + border; urgent: danger border
- Left-click: focus + optional `[modules.workspaces] on_click`

## Adding a backend

1. New crate `crates/skyline-<name>` + workspace member + `skyline` dep
2. Implement spawn / is_available / focus_* emitting `ServiceEvent::Compositor`
3. Add `CompositorBackendKind` variant (`snake_case` serde)
4. Wire `spawn_compositor` and `App` focus messages
5. Dedup snapshots; map outputs/workspaces/windows completely
6. Document env detection in README + `docs/skyline.1`

Do not block the iced thread on IPC. Focus helpers may be sync on a click
path (current niri/Hyprland style) but keep them fast and log errors with
`tracing::warn`.
