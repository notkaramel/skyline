use serde::{Deserialize, Serialize};

/// Snapshot of compositor-facing state consumed by the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompositorState {
    pub focused_output: Option<String>,
    pub outputs: Vec<OutputInfo>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_window: Option<WindowInfo>,
}

/// Connected monitor / output with logical geometry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: String,
    pub index: u8,
    pub output: Option<String>,
    pub active: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app_id: Option<String>,
    pub workspace_id: Option<u64>,
    pub output: Option<String>,
    pub focused: bool,
    /// Backend focus handle: niri uses decimal id; Hyprland uses client address.
    #[serde(default)]
    pub focus_token: String,
}

impl CompositorState {
    /// Output names known to the compositor (prefer geometry list, else workspaces).
    pub fn output_names(&self) -> Vec<String> {
        if !self.outputs.is_empty() {
            return self.outputs.iter().map(|o| o.name.clone()).collect();
        }
        let mut names: Vec<String> = self
            .workspaces
            .iter()
            .filter_map(|w| w.output.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Find which output contains the given logical desktop point.
    pub fn output_at(&self, x: f32, y: f32) -> Option<&str> {
        let xi = x.round() as i32;
        let yi = y.round() as i32;
        self.outputs
            .iter()
            .find(|o| {
                xi >= o.x
                    && yi >= o.y
                    && xi < o.x.saturating_add(o.width as i32)
                    && yi < o.y.saturating_add(o.height as i32)
            })
            .map(|o| o.name.as_str())
    }

    pub fn workspaces_for_output<'a>(&'a self, output: Option<&str>) -> Vec<&'a WorkspaceInfo> {
        match output {
            Some(name) => self
                .workspaces
                .iter()
                .filter(|ws| ws.output.as_deref() == Some(name))
                .collect(),
            None => {
                if let Some(focused) = self.focused_output.as_deref() {
                    let filtered: Vec<_> = self
                        .workspaces
                        .iter()
                        .filter(|ws| ws.output.as_deref() == Some(focused))
                        .collect();
                    if !filtered.is_empty() {
                        return filtered;
                    }
                }
                self.workspaces.iter().collect()
            }
        }
    }

    pub fn focused_window_for_output(&self, output: Option<&str>) -> Option<&WindowInfo> {
        // Prefer the focused window *on this output*, not a foreign monitor's focus.
        if let Some(name) = output {
            return self
                .windows
                .iter()
                .find(|w| w.focused && w.output.as_deref() == Some(name))
                .or_else(|| {
                    self.focused_window
                        .as_ref()
                        .filter(|w| w.output.as_deref() == Some(name))
                });
        }
        self.focused_window
            .as_ref()
            .or_else(|| self.windows.iter().find(|w| w.focused))
    }

    /// Windows for the taskbar: active workspace on this monitor only.
    pub fn taskbar_windows(&self, output: Option<&str>) -> Vec<&WindowInfo> {
        let workspaces = self.workspaces_for_output(output);
        let active_ids: Vec<u64> = workspaces
            .iter()
            .filter(|ws| ws.active)
            .map(|ws| ws.id)
            .collect();
        let active_ids = if active_ids.is_empty() {
            self.focused_window
                .as_ref()
                .filter(|w| output.is_none() || w.output.as_deref() == output)
                .and_then(|w| w.workspace_id)
                .into_iter()
                .collect()
        } else {
            active_ids
        };

        let mut wins: Vec<&WindowInfo> = self
            .windows
            .iter()
            .filter(|w| {
                let on_active = w
                    .workspace_id
                    .is_some_and(|id| active_ids.iter().any(|ws| *ws == id));
                if !on_active {
                    return false;
                }
                match (output, w.output.as_deref()) {
                    (Some(want), Some(have)) => want == have,
                    (Some(_), None) => true,
                    (None, _) => true,
                }
            })
            .collect();
        wins.sort_by_key(|w| w.id);
        wins
    }
}
