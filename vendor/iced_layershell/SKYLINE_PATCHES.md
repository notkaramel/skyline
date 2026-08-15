# Skyline patches to iced_layershell 0.19.1

Vendored from crates.io; depends on `../layershellev`.

1. **`handle_closed_event`** — resolve iced id via window-manager aliases (not only
   unit binding), so AllScreens bars clean up on monitor unplug.
2. **`OutOfMemory` on present** — drop the window instead of `panic!`.
3. **`set_unit_binding`** on first refresh — store iced id on the layershell unit.
