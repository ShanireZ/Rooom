use rooom_core::{Edge, Settings, SettingsIssue, Snapshot, Surface};
use serde_json::json;

#[test]
fn settings_default_to_a_right_edge_strip_with_capture_exclusion() {
    let settings = Settings::from_json("{}").unwrap();
    assert_eq!(
        settings,
        Settings {
            edge: Edge::Right,
            display: None,
            shortcut: None,
            capture_exclusion: true,
        }
    );
}

#[test]
fn settings_round_trip_preserves_display_edge_shortcut_and_capture_preference() {
    let settings = Settings {
        edge: Edge::Bottom,
        display: Some("Fixture display".into()),
        shortcut: Some("Ctrl+Alt+R".into()),
        capture_exclusion: false,
    };
    let text = serde_json::to_string(&settings).unwrap();
    assert_eq!(Settings::from_json(&text).unwrap(), settings);
}

#[test]
fn malformed_settings_return_an_error() {
    for text in [
        "{",
        "null",
        "[]",
        r#"{"edge":"middle"}"#,
        r#"{"captureExclusion":"true"}"#,
        r#"{"display":" "}"#,
        r#"{"shortcut":""}"#,
    ] {
        assert!(Settings::from_json(text).is_err(), "{text}");
    }
}

#[test]
fn the_snapshot_orders_surface_changes_and_serializes_the_settings_issue() {
    let mut snapshot = Snapshot::new(Settings::default(), Some(SettingsIssue::Invalid));
    assert_eq!((snapshot.revision, snapshot.surface), (0, Surface::Strip));
    snapshot.set_surface(Surface::Panel);
    snapshot.set_surface(Surface::Panel);
    assert_eq!((snapshot.revision, snapshot.surface), (1, Surface::Panel));
    snapshot.set_surface(Surface::Strip);
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({
            "revision": 2,
            "surface": "strip",
            "settings": {
                "edge": "right",
                "display": null,
                "shortcut": null,
                "captureExclusion": true
            },
            "settingsIssue": "invalid",
            "pinned": false,
            "shortcutIssue": null
        })
    );
}
