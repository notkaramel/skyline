# AGENTS.md

Guidance for humans and coding agents working on Skyline.

Skyline is a **native Wayland status bar** (Rust, iced 0.14 + iced_layershell 0.19).
No GTK, no Electron. Layer-shell surfaces on compositors that implement
`wlr-layer-shell`.

Read this file first. Then open the matching skill under `.cursor/skills/` for
the task you are doing.

## Repo map

```
crates/skyline/              # binary: main, app, style, widgets
crates/skyline-core/         # Config, ModuleKind, snapshots, ServiceEvent
crates/skyline-services/     # background threads → ServiceEvent
crates/skyline-niri/         # niri EventStream backend
crates/skyline-hyprland/     # Hyprland IPC backend
examples/config.toml         # canonical defaults (embedded into the binary)
docs/skyline.1               # man page
vendor/system-tray/          # patched SNI host — touch only for tray bugs
vendor/iced_layershell/      # patched for monitor hotplug / surface cleanup
vendor/layershellev/         # companion to iced_layershell (output destroy / TargetScreen)
```

Workspace root: `Cargo.toml`. Version `0.1.0`, edition 2021, MIT.

`examples/config.toml` is compiled in via
`include_str!("../../../examples/config.toml")` in `skyline-core`. Any schema
change must update that file or `--write-example-config` / serde defaults drift.

## Data flow

```
services / compositor backends
        │  UnboundedSender<ServiceEvent>
        ▼
   iced subscription (8 ms coalesce)
        ▼
   App::apply_services / apply_service   ← drop visually identical snapshots
        ▼
   App::view → islands → widgets
```

- UI crate must stay thin: rendering + input only.
- I/O, polling, D-Bus, and subprocesses live in `skyline-services` or a
  compositor crate.
- Shared types live in `skyline-core` only.

## Skills (read when relevant)

| Skill | When |
| --- | --- |
| [adding-a-module](.cursor/skills/adding-a-module/SKILL.md) | New built-in module or custom-module behavior |
| [compositor-backends](.cursor/skills/compositor-backends/SKILL.md) | niri / Hyprland / workspaces / taskbar / focus |
| [config-and-theme](.cursor/skills/config-and-theme/SKILL.md) | Config structs, TOML, hot-reload, theming |
| [services-pattern](.cursor/skills/services-pattern/SKILL.md) | Background threads, events, live knobs, CPU |
| [iced-layershell-ui](.cursor/skills/iced-layershell-ui/SKILL.md) | `app.rs` / `style.rs` / `widgets.rs` / popups |
| [verify-skyline](.cursor/skills/verify-skyline/SKILL.md) | Build, run, logs, sanity checks |

## Hard rules

1. **Do not poll when the OS can push events.** Clock sleeps until the format
   would change. Network uses `ip monitor`. Audio uses Pulse subscribe /
   `pactl subscribe`. Brightness watches sysfs. Niri uses EventStream.
   Hyprland uses `EventListener`. Config uses `notify`.
2. **Skip no-op UI updates.** Compare snapshots (`PartialEq` or `visually_eq`)
   before `send` and again in `App::apply_service`. Compositor floods: keep only
   the latest snapshot in a batch.
3. **Do not edit `vendor/system-tray` unless fixing SNI parsing / host
   robustness.** Upstream quirks are already patched there. Same for
   `vendor/iced_layershell` and `vendor/layershellev` — only touch for
   Wayland output / layer-shell hotplug bugs (see each `SKYLINE_PATCHES.md`).
4. **Keep `examples/config.toml`, `Config` defaults, README, and `docs/skyline.1`
   in sync** when you add or rename a config key.
5. **Hot-reload must keep working.** New service knobs that should apply without
   restart go through `skyline_services::live` (see `live.rs`). Compositor
   backend switches may still require restart — document that.
6. **Clicks run via `sh -c`.** Use `skyline_services::run_click`. Do not block
   the iced thread on subprocesses.
7. **Per-monitor correctness.** Workspaces, taskbar, and window title are scoped
   to the bar’s pinned output (`App::output_for_bar`), not global focus.
8. **No new heavy deps** without a clear need. Prefer std + existing crates
   (`tokio`, `notify`, `sysinfo`, `zbus`, `libpulse-binding`).

## Coding conventions

- Rust 2021, `anyhow` at boundaries, `thiserror` only if a typed error is shared.
- `tracing` (`info!` / `warn!` / `debug!`), not `println!` in library code.
- Named OS threads: `skyline-clock`, `skyline-sys`, `skyline-niri`, …
  Use `spawn_named` / `spawn_tokio` from `skyline-services`.
- Serde: `#[serde(default)]` on config structs so partial TOML works.
  Click fields use `#[serde(flatten)]` + `ClickActions` (`on_click` /
  `on_left_click` alias, `on_right_click`).
- `ModuleKind` serialize as `"cpu"`, `"custom:weather"`, etc.
- iced widgets: avoid `button` padding on glyph/value rows (it drops them off
  the island midline). Prefer `mouse_area` + `container` Fill + `align_y(Center)`.
- Fonts: `style::named_font` leaks family names to `'static` for iced. Volume
  glyphs go through `style::icon_font` (Nerd Font detect via `fc-list`).
- Colors in config are `[f32; 4]` RGBA 0..=1, converted with `style::rgba`.

## Commands

```bash
make check          # cargo check -p skyline
make                # release → target/release/skyline
make build-debug
make run            # requires an existing release binary; does not build
make run-debug
make config         # copy example config if missing
```

There is no unit-test suite yet. After logic changes run `make check`. After UI
or service changes, run the bar on Wayland (`WAYLAND_DISPLAY` set) and confirm
the affected module. Do not start a second bar on the same outputs without
warning the user — layer-shell exclusive zones stack.

## Common pitfalls

- **`Config::default()` must not parse `EXAMPLE_TOML`.** `#[serde(default)]` on
  `Config` calls `Default` during deserialize; parsing the example there
  recurses forever. Defaults are constructed field-by-field; the example file
  is for `--write-example-config` / `from_example_toml()`.
- **Brightness must use `brightnessctl -c backlight`.** Without the class,
  brightnessctl picks keyboard LEDs.
- **Volume scroll delta is a fraction of full scale** (`0.02` = 2%), not
  percent points. Brightness `step` *is* percent points.
- **Taskbar / workspaces on the wrong monitor:** bar surfaces are pinned by
  matching layer width to `OutputInfo` logical width. Preserve that logic.
- **Tray SVG icons:** tray widget is raster-only; skip SVG paths in
  `tray_handle`. Taskbar may render SVG.
- **Intel GPU:** `sys.rs` does not invent utilization from throttle sysfs.
  Leave it `None` unless a real metric is available.
- **`nmrs` in workspace deps is unused.** Network is `ip`/`iw`/iwd/wpa_cli,
  not NetworkManager D-Bus.

## What “done” looks like

- `make check` passes.
- New public config keys appear in `examples/config.toml` and are documented in
  README (and `docs/skyline.1` if user-visible).
- No new polling loops where an event source exists.
- No redraw storms (snapshot equality / coalesce still in place).
- Compositor actions (focus workspace/window) still work on both niri and
  Hyprland when you touch that path.
