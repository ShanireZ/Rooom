use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub edge: Edge,
    pub display: Option<String>,
    pub shortcut: Option<String>,
    pub capture_exclusion: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            display: None,
            shortcut: None,
            capture_exclusion: true,
        }
    }
}

impl Settings {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if !text.trim_start().starts_with('{') {
            return Err("Settings must be a JSON object".into());
        }
        let settings: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        for (field, value) in [
            ("display", settings.display.as_deref()),
            ("shortcut", settings.shortcut.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("{field} must be null or a nonempty string"));
            }
        }
        Ok(settings)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Surface {
    Strip,
    Panel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsIssue {
    Invalid,
    Unreadable,
    Unwritable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutIssue {
    Invalid,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u32,
    pub surface: Surface,
    pub settings: Settings,
    pub settings_issue: Option<SettingsIssue>,
    pub pinned: bool,
    pub shortcut_issue: Option<ShortcutIssue>,
}

impl Snapshot {
    pub fn new(settings: Settings, settings_issue: Option<SettingsIssue>) -> Self {
        Self {
            revision: 0,
            surface: Surface::Strip,
            settings,
            settings_issue,
            pinned: false,
            shortcut_issue: None,
        }
    }

    pub fn set_surface(&mut self, surface: Surface) {
        if self.surface != surface {
            self.surface = surface;
            self.revision += 1;
        }
    }

    pub fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        self.revision += 1;
    }
}

#[derive(Default)]
pub struct PanelTimer {
    outside_since: Option<Instant>,
    wait_for_leave: bool,
}

impl PanelTimer {
    pub const RETRACT_AFTER: Duration = Duration::from_secs(3);

    pub fn surface_changed(&mut self, surface: Surface) {
        self.outside_since = None;
        self.wait_for_leave = surface == Surface::Strip;
    }

    pub fn observe(
        &mut self,
        surface: Surface,
        near: bool,
        held: bool,
        now: Instant,
    ) -> Option<Surface> {
        match surface {
            Surface::Strip => {
                self.wait_for_leave &= near;
                (near && !self.wait_for_leave).then_some(Surface::Panel)
            }
            Surface::Panel if near || held => {
                self.outside_since = None;
                None
            }
            Surface::Panel => {
                let since = *self.outside_since.get_or_insert(now);
                (now.saturating_duration_since(since) >= Self::RETRACT_AFTER)
                    .then_some(Surface::Strip)
            }
        }
    }
}
