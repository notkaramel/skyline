# Skyline

Native Wayland status bar with modular islands. Built in Rust with
[`iced`](https://github.com/iced-rs/iced) +
[`iced_layershell`](https://github.com/waycrate/exwlshelleventloop) — no GTK, no Electron.

Requires a compositor that implements `wlr-layer-shell` (niri, Hyprland, Sway, …).

## Features

- Per-monitor layer-shell bars (`AllScreens`, or pin one with `--output` / `SKYLINE_OUTPUT`)
- Island layout: left / center / right module groups
- Niri (`EventStream`) and Hyprland compositor backends (auto-detect)
- Workspaces, icon taskbar, focused window title (per-monitor)
- Clock with hover tooltip
- CPU / RAM / GPU meters (per-core CPU, AMD sysfs / NVIDIA `nvidia-smi`)
- Network (SSID / ethernet via `ip` + `iw` / iwd / wpa_cli — event-driven)
- Volume (Pulse/PipeWire subscribe, `wpctl` fallback) and brightness (`brightnessctl`)
- Custom command islands (interval + optional JSON `text` field)
- StatusNotifierItem system tray host with DBusMenu popups
- Config hot-reload on save

## Quick start

```bash
make            # release build → target/release/skyline
make config     # write ~/.config/skyline/config.toml if missing
make run        # run from target/ (does not rebuild)
```

Optional user install (no sudo):

```bash
make install-user   # ~/.local/bin/skyline + man page
make help
```

Runtime helpers (optional, feature-dependent):

| Helper | Used for |
| --- | --- |
| `wpctl` / Pulse | volume read/write |
| `brightnessctl` | screen backlight (`-c backlight`) |
| `ip`, `iw` / `iwgetid` / `wpa_cli` / `busctl` (iwd) | network label + signal |
| `fc-list` | detect a Nerd Font for volume glyphs |
| `nvidia-smi` | NVIDIA GPU utilization |

## Run

```bash
make run
SKYLINE_OUTPUT=DP-1 make run    # one output only
RUST_LOG=skyline=debug,system_tray=warn make run
```

### niri

```kdl
spawn-at-startup "skyline"
# or a local build:
spawn-at-startup "/absolute/path/to/skyline/target/release/skyline"
```

### Hyprland

```conf
exec-once = skyline
```

Detection: `NIRI_SOCKET` wins over `HYPRLAND_INSTANCE_SIGNATURE`. Override with
`[compositor] backend = "niri" | "hyprland" | "none" | "auto"`.

## Config

Default path: `~/.config/skyline/config.toml`

Full annotated defaults: [examples/config.toml](examples/config.toml)
(also written by `skyline --write-example-config` / `make example-config`).

Edits are **hot-reloaded** on save: theme, module lists, separators, scroll steps,
bar size/margins/exclusive zone, clock format, sys refresh, custom modules, click
commands. Switching the compositor backend still needs a restart.

CLI:

```text
skyline [--config PATH] [--write-example-config]
```

After `make install-user`, see `man skyline` if `~/.local/share/man` is on your manpath.

### Layout

```toml
[modules]
left = ["workspaces", "taskbar", "window"]
center = ["clock"]
right = ["tray", "cpu", "memory", "gpu", "network", "brightness", "volume"]
```

Built-in names: `workspaces`, `taskbar`, `window`, `clock`, `cpu`, `memory`,
`gpu`, `network`, `volume`, `brightness`, `tray`. Custom islands are
`custom:<id>` (or a bare id that is treated as custom).

### Module clicks

Most module sections accept shell commands (`sh -c`):

```toml
[modules.clock]
on_click = "gsimplecal"
on_right_click = "xdg-open https://calendar.google.com"

[modules.volume]
# When unset, left-click still toggles mute
on_click = "pavucontrol"
on_right_click = "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"
```

`on_left_click` is an alias for `on_click`. Tray icons keep native SNI actions.

### Volume / brightness

```toml
[modules.volume]
step = 0.02              # fraction of full scale per scroll notch
max_percent = 150.0
detect_bluetooth = true  # headset glyph +  when default sink is bluez
show_device = false
show_percent = true

[modules.brightness]
step = 2.0               # percent points per scroll notch
show_percent = true
```

Scroll input is debounced (~70 ms) so trackpads stay controllable.

### Meters

```toml
[modules.sys]
refresh_ms = 1000

[modules.cpu]
label = "CPU"
format = "{label} {bar}"   # tokens: {label} {bar}/{meter} {percent}/{pct}
on_click = "alacritty -e btop"
```

CPU draws one column per logical core. RAM is a fill meter (`meter_bars` columns).
GPU is one column per detected device (AMD `gpu_busy_percent`, else NVIDIA).

### Network

```toml
[modules.network]
show_name = true       # SSID / "Ethernet" (not wlan0)
show_strength = false
max_chars = 24
```

### Taskbar

Icon strip of windows on the **active workspace of that monitor**.

```toml
[modules.taskbar]
width = 22.0           # icon square (alias: icon_size)
padding = 3.0
gap = 4.0
border_width = 3.0     # focused chip outline; unset → theme island_border_width
max_items = 12
```

Left-click focuses the window (niri / Hyprland). Icons resolve from app id,
`.desktop` `Icon=`, then a letter fallback.

### Custom islands

```toml
[[modules.custom]]
id = "weather"
command = "curl"
args = ["-s", "https://wttr.in/?format=1"]
interval_ms = 600000
on_click = "xdg-open https://wttr.in"
json = false           # true → parse stdout JSON and use the `text` field
```

Reference as `custom:weather` in `left` / `center` / `right`.

### Separators and tray menu

```toml
[bar]
separators = true
separator = "│"
tray_menu_gap = 4         # px between bar and tray menu / clock tooltip
tray_menu_align = "icon"  # "icon" = under the clicked tray icon; "end" = bar edge
```

## Theming

Colors are RGBA floats in `[0, 1]`. `exclusive_zone` reserves compositor space
for the bar; set it to `0` for floating islands (windows can go under the bar).

```toml
[theme]
background = [0.039, 0.031, 0.078, 0.0]
island_background = [0.196, 0.173, 0.290, 1.0]
text = [0.929, 0.910, 0.980, 1.0]
muted = [0.706, 0.675, 0.812, 1.0]
accent = [0.659, 0.580, 0.878, 1.0]
danger = [0.878, 0.557, 0.722, 1.0]
separator = [0.541, 0.482, 0.769, 0.55]
island_radius = 0.0
island_border = [0.659, 0.580, 0.878, 1.0]
island_border_width = 3.0
island_shadow = [0.039, 0.031, 0.078, 1.0]
island_shadow_offset = [6.0, 6.0]
island_shadow_blur = 0.0
island_padding = [4, 12]     # vertical, horizontal
island_margin = [0, 2]
font = "Fira Sans"
emoji_font = "Noto Color Emoji"
font_size = 14.0
meter_height = 16.0
meter_width = 4.0
meter_gap = 2.0
meter_bars = 12
```

Volume icons use a detected Nerd Font (Waybar-style `  `) when `fc-list`
finds one; otherwise they fall back to the UI font.

## Architecture

| Crate | Role |
| --- | --- |
| `skyline` | Binary: iced daemon, widgets, layer-shell popups |
| `skyline-core` | Config, `ModuleKind`, snapshots, `ServiceEvent` |
| `skyline-services` | Clock, sys, net, audio, brightness, tray, custom, config watch |
| `skyline-niri` | Niri EventStream → `CompositorState` |
| `skyline-hyprland` | Hyprland IPC → `CompositorState` |
| `vendor/system-tray` | Patched SNI host (see below) |

Background threads push `ServiceEvent`s into iced. Identical snapshots are
dropped before redraw. See [AGENTS.md](AGENTS.md) for contributor / agent notes.

## System tray

Skyline vendors a patched `system-tray` under `vendor/system-tray` that:

- Fixes `IconPixmap` height field parsing
- Soft-fails malformed pixmap/tooltip properties instead of dropping the item
- Keeps the registration stream alive when one client misbehaves

This avoids the common `ERROR system_tray::client: zbus variant error` abort.

Left-click activates the item; right-click opens its DBusMenu as an overlay.
Escape or clicking outside dismisses the menu.

## Environment

| Variable | Effect |
| --- | --- |
| `SKYLINE_OUTPUT` | Bind to one Wayland output (e.g. `DP-1`) |
| `NIRI_SOCKET` | Niri backend available |
| `HYPRLAND_INSTANCE_SIGNATURE` | Hyprland backend available |
| `RUST_LOG` | `tracing-subscriber` filter |

## Development

```bash
make check          # cargo check -p skyline
make build-debug
make run-debug
```

Agent / contributor guide: [AGENTS.md](AGENTS.md).

## License

MIT
