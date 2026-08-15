---
name: config-and-theme
description: >-
  Skyline TOML config schema, serde defaults, hot-reload, live service knobs,
  and neobrutalism theming (islands, borders, shadows, fonts, meters). Use when
  editing config.rs, examples/config.toml, theme keys, or style.rs.
---

# Config and theme

## Canonical files

| File | Role |
| --- | --- |
| `crates/skyline-core/src/config.rs` | structs + `Default` + load/migrate |
| `examples/config.toml` | annotated defaults; **embedded** via `include_str!` |
| `docs/skyline.1` | user man page |
| `README.md` | user docs |

Changing a key? Update **all four** (man page if user-visible).

`skyline --write-example-config` writes `Config::example_toml()` (the embedded
file) to `~/.config/skyline/config.toml`.

## Load path

1. `--config` / `-c`, else `dirs::config_dir()/skyline/config.toml`
2. Missing file → in-memory `Config::default()` (not the example file)
3. Present file → `migrate_legacy_module_keys` then `toml::from_str`

**Never** implement `Config::default` by parsing `EXAMPLE_TOML`.
`#[serde(default)]` on `Config` calls `Default` while deserializing → infinite
recursion.

## Serde rules

- Every config struct: `#[serde(default)]` so partial TOML works
- Clicks: flatten `ClickActions` (`on_click`, alias `on_left_click`,
  `on_right_click`)
- `ModuleKind` strings: `"workspaces"`, `"custom:id"`; unknown → `Custom`
- `CompositorBackendKind`: `snake_case` (`auto`, `niri`, `hyprland`, `none`)
- Taskbar `width` accepts alias `icon_size`
- Legacy `[modules] volume_step` / `brightness_step` are rewritten into nested
  tables if those sections are missing

## Hot-reload

`skyline-services/src/config_watch.rs` watches the config **parent directory**
(atomic save/rename) + the file, debounce **250 ms**, then
`ServiceEvent::ConfigReloaded`.

`App::apply_config_reload`:

1. `skyline_services::reload_from_config` → `live::apply` + custom regen + tray
2. Drop custom snapshot ids no longer in config
3. Update `bound_output`
4. `layer_tasks_for_bar` — anchor / size / margin / exclusive zone on existing
   bar window ids

**Not live:** compositor backend selection (restart).

### Live knobs (`live.rs`)

Background threads re-read:

- `clock_format`
- `weather_location`, `weather_interval_ms` (min 60s)
- `sys_refresh_ms` (min 50)
- `volume_detect_bluetooth`, `volume_max_percent`
- `custom_generation` (bump stops old custom loops)

New setting that services must see without restart → add it here, not a
restart-only global.

## Theme (`ThemeConfig` → `style.rs`)

Colors: `[f32; 4]` RGBA 0..=1 → `style::rgba`.

| Key | Meaning |
| --- | --- |
| `background` | bar chrome (often alpha 0) |
| `island_background` | island fill |
| `text` / `muted` / `accent` / `danger` / `separator` | UI colors |
| `island_radius` | corner radius (0 = sharp / neobrutalism) |
| `island_border` + `island_border_width` | hard outline |
| `island_shadow` + `offset` + `blur` | drop shadow (`blur` 0 = hard cast) |
| `island_padding` / `island_margin` | `[vertical, horizontal]` |
| `font` / `emoji_font` / `font_size` | UI vs emoji (brightness ☀) |
| `meter_*` | cava-style usage meters |

Island containers reserve padding for shadow offset so neighbors do not clip
the cast. Keep that when changing `style::island`.

Fonts:

- `named_font` / `ui_font` / `emoji_font`
- Volume Waybar glyphs: `icon_font` (Nerd Font via `fc-list`, cached)

Workspace active text is hard dark ink on accent fill — keep contrast if you
retheme `workspace_button`.

## Bar geometry (`BarConfig`)

- `height`, `margin` `[top, right, bottom, left]`, `exclusive_zone`
- `anchor`: `"top"` | `"bottom"`
- `output`: optional Wayland output name
- `padding`, `island_gap`, `separators`, `separator`, `tray_menu_gap`

`exclusive_zone = 0` → floating islands (windows can go underneath).

## After editing config types

```bash
make check
# if example toml changed, sanity-parse:
#   target/release/skyline --config examples/config.toml
# (needs Wayland to stay up; parse errors log on startup / reload)
```
