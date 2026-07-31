# Skyline

Native Wayland status bar with modular islands. Built in Rust with `iced` + `iced_layershell` (no GTK, no Electron).

## Features

- Per-monitor layer-shell bars (`AllScreens` or a single `--output`)
- Island layout: left / center / right module groups
- Niri (`EventStream`) and Hyprland compositor backends
- Workspaces + focused window title
- Clock, CPU, RAM, GPU, network, volume, brightness
- Custom command islands (interval + optional click)
- StatusNotifierItem system tray host

## Build

```bash
make            # release build
make run        # build and run
sudo make install   # install to /usr/local/bin
sudo make uninstall
make help
```

Requires a Wayland compositor with `wlr-layer-shell` (niri, Hyprland, Sway, …). Optional runtime helpers: `wpctl` (volume), `brightnessctl`, `nmcli`.

## Run (niri)

```bash
make config     # optional: write starter config if missing
make run

# or pin to one output
SKYLINE_OUTPUT=DP-1 make run
```

Add to your niri config:

```kdl
spawn-at-startup "skyline"
```

Or with a full path:

```kdl
spawn-at-startup "/usr/local/bin/skyline"
```

## Config

Default path: `~/.config/skyline/config.toml`

See [examples/config.toml](examples/config.toml). After `sudo make install`, read the man page with `man skyline`.

Edits to the config file are **hot-reloaded** on save (theme, modules, separators, steps, bar size/margins, clock format, sys refresh, custom modules). Switching the compositor backend still needs a restart.

A full annotated defaults file lives in [examples/config.toml](examples/config.toml) (also written by `skyline --write-example-config`).

### Network names

```toml
[modules.network]
show_name = true      # Wi‑Fi SSID / ethernet connection name (not wlan0)
show_strength = true
max_chars = 24
```

### Tray menu gap

```toml
[bar]
tray_menu_gap = 4     # pixels between bar and tray right-click menu
```

### Volume / Bluetooth

```toml
[modules.volume]
step = 0.02
max_percent = 150.0
detect_bluetooth = true   # 🎧 when default sink is bluez
show_device = false
show_percent = true
```

### Module clicks

Every module section accepts shell commands for mouse buttons:

```toml
[modules.clock]
on_click = "gsimplecal"
on_right_click = "xdg-open https://calendar.google.com"

[modules.volume]
# When unset, left-click still toggles mute
on_click = "pavucontrol"
on_right_click = "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"

[[modules.custom]]
id = "weather"
command = "curl"
args = ["-s", "https://wttr.in/?format=1"]
on_click = "xdg-open https://wttr.in"
on_right_click = "notify-send weather"
```

Commands run via `sh -c`. Tray icons keep their native left/right SNI actions.

### Separators

```toml
[bar]
separators = true
separator = "│"
```

### Scroll sensitivity

```toml
[modules.volume]
step = 0.02            # fraction of full scale per scroll notch

[modules.brightness]
step = 2.0             # percent points per scroll notch
```

### Custom islands

```toml
[[modules.custom]]
id = "weather"
command = "curl"
args = ["-s", "https://wttr.in/?format=1"]
interval_ms = 600000
on_click = "xdg-open https://wttr.in"
json = false
```

Reference a custom module as `custom:<id>` in the module lists.

## Theming

Colors are RGBA floats in `[0, 1]`. `exclusive_zone` reserves compositor space for the bar; set it to `0` for floating-island feel (windows can go under the bar).

## System tray notes

Skyline vendors a patched `system-tray` under `vendor/system-tray` that:
- Fixes `IconPixmap` height field parsing
- Soft-fails malformed pixmap/tooltip properties instead of dropping the item
- Keeps the registration stream alive when one client misbehaves

This avoids the common `ERROR system_tray::client: zbus variant error` abort.
