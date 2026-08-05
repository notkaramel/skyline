---
name: iced-layershell-ui
description: >-
  Skyline iced 0.14 + iced_layershell UI patterns: App daemon, islands, widgets,
  fonts, popups (tray menu, clock tooltip), scroll handling, and per-monitor
  view. Use when editing app.rs, style.rs, widgets.rs, or layer-shell surfaces.
---

# iced + layershell UI

## Entry

`crates/skyline/src/main.rs` builds an iced_layershell **daemon**:

```text
daemon(App::new, namespace, update, view)
  .subscription .style .settings(LayerShellSettings { … })
  .run()
```

- `StartMode::AllScreens` or `TargetScreen` from `[bar].output` / `SKYLINE_OUTPUT`
- Anchor top or bottom + left + right
- Size `(0, height)` — width is compositor-assigned
- `exclusive_zone`, `margin` from config
- Default font / text size from theme

Namespace: `"skyline"`.

## Messages

`Message` is `#[to_layer_message(multi)]` so iced_layershell injects
`NewLayerShell`, `AnchorChange`, `SizeChange`, `MarginChange`,
`ExclusiveZoneChange`, etc. Keep that attribute. Unknown injected variants are
ignored with `_ => Task::none()`.

Do not block in `update`. Return `Task<Message>` for window/layer ops.

## Layout (`view`)

Non-popup ids are bar surfaces:

1. Pin output (`pin_bar_output` / `output_for_bar`)
2. Build left / center / right **islands**
3. `stack![centered middle, edges row]` so the center island stays geometrically
   centered; the horizontal spacer must not steal clicks

Empty island → zero-width `Space`, not an empty styled island.

Separators: `bar.separators` + `bar.separator` glyph between modules that
actually rendered (`Some`).

## Islands (`style::island`)

Container with theme fill, border, shadow, padding; outer wrapper adds margin
**plus shadow overhang** (right/bottom) so hard-cast shadows are not clipped.

All module groups share one island chrome. Do not invent a second island style
unless theming requires it.

## Hit targets

Prefer `mouse_area` over padded `button` for glyph + percent rows. Button
padding shifts text off the island midline.

`widgets::with_clicks` adds left/right press → `Message::ModuleClick`.
Modules that already attach `mouse_area` (volume, brightness, workspaces,
taskbar, tray, clock, custom) must be excluded from the final wrap in
`module_view`.

Scroll: `widgets::scroll_delta` — discrete wheel notches (±1 line) map to
exactly one configured step; pixel deltas scale by `/ 40`.

Volume/brightness scroll is **previewed** in `displayed_*` then flushed after
70 ms quiet (`ScrollDebounce` subscription every 25 ms while pending).

## Popups (extra layer shells)

| Kind | Layer | Notes |
| --- | --- | --- |
| `TrayDismiss` | Top, fullscreen | click-catcher under menu |
| `TrayMenu` | Overlay, top-right | 240×(rows×28+16) max 400; margin uses `tray_menu_gap` |
| `ClockTooltip` | Overlay, events transparent | hover; width from char count |

Escape or pointer press outside the menu → `close_tray_popups`.
Track ids in `popup_ids`. `view` switches on popup kind **before** drawing the bar.

Hot-reload bar geometry via `layer_tasks_for_bar` on existing bar ids only
(not popups).

## Fonts (`style.rs`)

- `named_font`: empty/`default` → `Font::DEFAULT`; `mono` → monospace; else
  `Font::with_name` with a `'static` leaked family string (cached)
- `ui_font` / `emoji_font` from theme
- `icon_font`: first `fc-list` hit among Nerd Font candidates (cached process-wide)

Volume icons are private-use Waybar glyphs (`  `, bluetooth ``). Brightness
uses emoji `☀` with `emoji_font`.

Fixed-width percents: `style::percent_slot` so `5%` → `100%` does not resize
the island.

## Icons (`widgets.rs`)

Shared caches (`ICON_PATH_CACHE`, `DESKTOP_ICON_CACHE`):

- Theme dirs: hicolor/Adwaita/Papirus/breeze/Tela + user/system/flatpak roots
- `.desktop` `Icon=` / `StartupWMClass` / `Name=`
- Tray: **raster only** (png/xpm/jpeg/webp). SNI pixmap is ARGB32 → RGBA
- Taskbar: svg **or** raster via iced `svg` / `image` widgets

## Performance in view/update

- Do not clone huge tray pixmaps in `view` beyond what widgets need
- `visually_eq` / `==` before assigning state
- Do not log every compositor event at info

## iced versions

Workspace pins `iced = "0.14"` (wgpu, tiny-skia, tokio, wayland, image, svg)
and `iced_layershell = "0.19.1"`. Match existing widget APIs (`space::horizontal`,
`Length::Fill`, `mouse_area::on_scroll`, `container::Style`). Do not upgrade
iced in a drive-by change.
