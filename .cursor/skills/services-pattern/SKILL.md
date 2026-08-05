---
name: services-pattern
description: >-
  Skyline background service pattern: spawn threads, ServiceEvent bus, snapshot
  equality, live config knobs, and event-driven I/O (no polling). Use when
  editing skyline-services, adding sys/audio/network/tray/clock/custom logic,
  or fixing high CPU / redraw storms.
---

# Services pattern

## Spawn

`skyline_services::spawn_all` starts:

| Thread | Source | Event |
| --- | --- | --- |
| clock | sleep until format would change | `Clock(String)` |
| sys | `sysinfo` + GPU sysfs/`nvidia-smi` on `refresh_ms` | `Sys` |
| network | `ip monitor` → else `/sys/class/net` watch | `Network` |
| audio | Pulse subscribe → else `pactl subscribe` + `wpctl` | `Volume` |
| brightness | `/sys/class/backlight` watch; read/set via `brightnessctl -c backlight` | `Brightness` |
| custom | per-module command loop | `Custom` |
| config | `notify` on config dir | `ConfigReloaded` / `Error` |
| tray | tokio SNI client (`vendor/system-tray`) | `TrayItems` / `TrayMenu` |

Helpers in `lib.rs`:

- `spawn_named` — OS thread with a name
- `spawn_tokio` — current-thread tokio runtime on a named thread (tray)

Never do blocking I/O on the iced update/view path except tiny sync focus
IPCs on click.

## Event bus

```rust
pub type ServiceTx = UnboundedSender<ServiceEvent>;
```

UI side: `ServiceRxSlot` + `service_subscription` coalesces bursts with an
**8 ms** sleep + `try_recv`, then `Message::Services(Vec<…>)`.

`App::apply_services` keeps only the **latest** `Compositor` event in a batch.

## Skip redundant work

Before `tx.send`:

- Clock: string changed
- Sys / volume / brightness: `visually_eq` (epsilon, not bit-identical floats)
- Network / compositor / tray items: `PartialEq`
- Custom: text changed per id

In `apply_service`, compare again before mutating `App` (subscription may
still deliver dupes).

Volume/brightness: do **not** apply incoming snapshots while a scroll preview
(`*_pending`) is in flight.

## Live config

`live::init` at spawn; `live::apply` on reload. Threads read atomics / `RwLock`
each loop. Custom modules: bump `custom_generation` to stop old loops.

Tray starts at most once (`tray_started` swap). Enabling tray later on reload
calls `ensure_tray`; disabling mid-run does not tear it down.

## Event-driven I/O (required)

| Bad | Good |
| --- | --- |
| `loop { sleep(1s); poll wpctl }` | Pulse `subscribe` / `pactl subscribe` |
| poll `ip` every second | `ip -o monitor link address route` + debounce |
| poll brightnessctl | inotify on `/sys/class/backlight` |
| clock `sleep(1s)` always | sleep until next second/minute/hour based on format tokens |
| niri `Request::Workspaces` on a timer | `EventStream` |

Debounce noisy sources (network ~200–250 ms, brightness ~50 ms, config 250 ms).

Sys **is** a poll (`refresh_ms`, default 1000, min 50) — that is the exception,
because `sysinfo` CPU usage is sample-based. Do not lower the default without
measuring bar CPU.

## Audio details

- Prefer `libpulse-binding` mainloop on `skyline-audio` thread
- Fallback: `wpctl get-volume` + `pactl subscribe`
- Mutate via `wpctl set-volume` / `set-mute` (fraction → percent string)
- Bluetooth: sink name/desc/ports contain `bluez` / `bluetooth`
- Scroll: UI accumulates deltas, debounce **70 ms**, cap burst 10% volume /
  10 brightness points per flush

## Network details

Probe path: default-route dev from `ip -j route` → else first UP+LOWER_UP
non-lo/br/docker/veth link.

Wi-Fi SSID order: `iw` → `iwgetid` → iwd `busctl` → `wpa_cli`.
Do not add NetworkManager/`nmrs` unless replacing this stack deliberately
(workspace currently lists unused `nmrs`).

## GPU details (`sys.rs`)

1. AMD: `/sys/class/drm/cardN/device/gpu_busy_percent` (skip `cardN-*`, `renderD*`)
2. Else NVIDIA: `nvidia-smi --query-gpu=utilization.gpu`
3. Intel: **leave empty** (no fake metric)

## Custom modules

- `Command::new(command).args(args)` — not a shell, unless the user puts
  `sh` as the command
- `json = true`: parse stdout JSON, use `.text`
- Else first line of stdout, trimmed
- Interval sleep in ≤200 ms slices so reload stops promptly
- Min interval 500 ms

Clicks always: `sh -c` via `run_click` (fire-and-forget `spawn`).

## Tray

- `Client::new().await` + `subscribe`
- Soft-fail lives in **vendored** `system-tray` — do not “fix” variant errors
  by restarting the host on one bad client
- Activate / menu on short-lived `spawn_tokio` tasks
- Publish items only when the snapshot vec changed
