use serde::{Deserialize, Serialize};

/// Snapshot of compositor-facing state consumed by the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompositorState {
    pub focused_output: Option<String>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_window: Option<WindowInfo>,
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
}

impl CompositorState {
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
        if let Some(win) = &self.focused_window {
            if output.is_none() || win.output.as_deref() == output || win.focused {
                return Some(win);
            }
        }
        self.windows.iter().find(|w| {
            w.focused && (output.is_none() || w.output.as_deref() == output)
        })
    }
}
