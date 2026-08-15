---
name: adding-a-module
description: >-
  Add or change a Skyline status-bar module (workspaces, clock, weather, meters,
  network, volume, brightness, tray, custom islands). Use when creating a new ModuleKind,
  wiring clicks, or extending module config/UI/services.
---

# Adding a Skyline module

## Custom island (no Rust)

Prefer this when the user wants a command output on the bar:

```toml
[[modules.custom]]
id = "uptime"
command = "uptime"
args = ["-p"]
interval_ms = 60000
json = false
```

Layout entry: `"custom:uptime"`. JSON mode reads a `text` field from stdout.
Built-in weather is `[modules.weather]` + layout `"weather"` (wttr.in), not a custom island.
Clicks: `sh -c` via `skyline_services::run_click`. Empty stdout hides the module.

## Built-in module checklist

Copy this list and tick as you go:

```
- [ ] ModuleKind variant + serde string in crates/skyline-core/src/config.rs
- [ ] Config struct + ModulesConfig field + Default
- [ ] click_command / clicks_for wiring (or flatten ClickActions)
- [ ] Snapshot type + ServiceEvent variant in modules.rs (if stateful)
- [ ] Service spawn in skyline-services + spawn_all / reload_from_config
- [ ] Live knobs in live.rs if hot-reload should apply without restart
- [ ] App state field + apply_service (skip visually identical updates)
- [ ] App::module_view + widget in widgets.rs if needed
- [ ] examples/config.toml
- [ ] README.md + docs/skyline.1
```

### 1. `ModuleKind`

In `crates/skyline-core/src/config.rs`:

- Add enum variant
- `as_config_str` → kebab name (`"gpu"`, `"taskbar"`)
- `Deserialize`: match that string; unknown strings become `Custom(other)`
- Default layout vectors if it should appear out of the box

### 2. Config

- New `FooConfig` with `#[serde(default)]`
- Flatten clicks:

```rust
#[serde(flatten)]
pub clicks: ClickActions,
```

- `ModulesConfig::click_command` / `clicks_for` must include the new kind
- Tray has no click commands (native SNI)

### 3. Service

- New file under `crates/skyline-services/src/`
- `pub fn spawn(tx: ServiceTx)` using `spawn_named` or `spawn_tokio`
- Emit `ServiceEvent::…` only when the snapshot changed
- Register in `spawn_all`
- If the module list / interval / format should hot-reload, update `live.rs`
  and `reload_from_config`

Do **not** start a timer poll if udev/sysfs/`notify`/D-Bus/Pulse/compositor
events exist. See [services-pattern](../services-pattern/SKILL.md).

### 4. UI

In `crates/skyline/src/app.rs`:

- Store snapshot on `App`
- Handle event in `apply_service`
- Render in `module_view`
- Modules that own hit targets (volume, brightness, workspaces, taskbar, tray,
  clock, custom) must **not** be wrapped again in the final `with_clicks`
  match — they attach clicks themselves

Hide the module (`None`) when there is nothing to show (no tray items, empty
taskbar, brightness unavailable, empty custom text, empty window title).

### 5. Interactions

| Module | Built-in input | Optional config clicks |
| --- | --- | --- |
| workspaces | left-click focuses WS | `on_click` also runs; right-click on strip |
| taskbar | left-click focuses window | flatten clicks on config (rarely used) |
| volume | scroll; left-click mute if `on_click` unset | `on_click` / `on_right_click` |
| brightness | scroll | `on_click` / `on_right_click` |
| tray | SNI activate / menu | none |
| others | none | `on_click` / `on_right_click` |

Volume `step` is a **fraction of full scale**. Brightness `step` is **percent points**.

## Meter modules (cpu / memory / gpu)

Reuse `widgets::usage_meter` + `MeterClickConfig`:

- Format tokens: `{label}`, `{bar}`/`{meter}`, `{percent}`/`{pct}`
- CPU: pass `sys.cpu_per_core`
- RAM: `usage_fill_segments(memory_percent, theme.meter_bars)`
- GPU: `sys.gpu_per_device` (AMD sysfs, else `nvidia-smi`)

Do not fake Intel utilization.

## Verify

```bash
make check
```

Then run on Wayland and confirm the module appears, updates, and clicks.
Update docs. Full verify steps: [verify-skyline](../verify-skyline/SKILL.md).
