# Skyline patches to layershellev 0.19.1

Vendored from crates.io for monitor hotplug / DPMS resilience.

1. **`output_destroyed`** — request close while units remain in the list (do not
   `extract_if` before iced can clean up).
2. **`remove_shell`** — destroy AllScreens / TargetScreen bars, not only
   `becreated` popups.
3. **Empty units** — do not stop the event loop for `TargetScreen` (only
   `StartMode::Active` exits).
4. **`NewDisplay` / TargetScreen** — recreate a layer when the target output
   returns and its xdg name is known; avoid duplicate xdg_output cache entries.
5. **`set_unit_binding` / public `get_mut_unit_with_id`** — let iced_layershell
   store the iced window id on AllScreens surfaces.
