---
name: verify-skyline
description: >-
  Build, run, and sanity-check Skyline changes. Use after code edits, when
  debugging the bar, checking CPU/redraw issues, or confirming config
  hot-reload and Wayland behavior.
---

# Verify Skyline

## Always

```bash
make check          # cargo check -p skyline
```

Release run (does **not** rebuild):

```bash
make                # if binary missing / stale
make run
```

Debug:

```bash
make build-debug && make run-debug
```

## Environment

- Needs `WAYLAND_DISPLAY` (or a compositor). Do not expect the binary to stay
  up in a headless SSH session without a nested compositor.
- Logs: `RUST_LOG=skyline=debug,system_tray=warn`
- One output: `SKYLINE_OUTPUT=DP-1`
- Avoid launching a second bar on the same session unless asked — exclusive
  zones stack and look like “double padding”.

## Config

Default: `~/.config/skyline/config.toml`

```bash
make config                 # copy example if missing
# or
./target/release/skyline --write-example-config
./target/release/skyline --config examples/config.toml
```

Hot-reload test: edit the live config, save, confirm theme/modules update
without restart. Invalid TOML should log `config reload failed` and keep the
previous config.

Compositor `backend` changes still need a restart — that is expected.

## Manual checks by area

| Change | Confirm |
| --- | --- |
| Module UI | Appears in the right island; hidden when empty/unavailable |
| Clicks | Left/right run `sh -c` commands; volume mute still works if `on_click` unset |
| Volume scroll | Smooth, no huge jumps on trackpad; bluetooth glyph on bluez sink |
| Brightness | Uses backlight class, not keyboard LEDs |
| Workspaces / taskbar | Correct **per monitor**; click focuses on niri and/or Hyprland |
| Tray | Icons show; left activate; right menu; Esc / outside click dismisses |
| Theme | Island border + hard shadow not clipped; percent column width stable |
| Sys meters | CPU columns = cores; RAM fill; GPU only if AMD/NVIDIA metric exists |
| Custom | Output text; interval; click; reload adds/removes ids |
| CPU usage | Bar idle should stay low; no compositor-frame-rate redraws |

## Performance regressions

Symptoms: fans spin, `skyline` at several % CPU idle.

Check:

1. Niri/Hyprland sending duplicate snapshots? Equality short-circuit in backend.
2. `App::apply_service` still bails on `visually_eq` / `==`.
3. No new `sleep` poll in audio/network/brightness/clock.
4. Sys `refresh_ms` not set extremely low in example config (default 1000).
5. Tray not republishing identical item lists.

## Do not

- Run `cargo test` expecting coverage — there is no suite yet. Adding tests is
  welcome but not required for every change.
- `sudo make install` / write to `/usr`. Use `make` or `make install-user`.
- Commit `target/`, `nohup.out`, or editor swap files.
- “Fix” vendored tray variant errors by aborting the host.
