//! Conversion of legacy feature switches into activity-bar visibility.

use serde_json::{Map, Value};

const LEGACY_PANELS: [(&str, &str, bool); 6] = [
    ("show_notes_panel", "notes", true),
    ("show_remote_stats", "resourceMonitor", true),
    ("show_gpu_monitor", "gpuMonitor", false),
    ("show_ascend_npu_monitor", "ascendNpuMonitor", false),
    ("show_process_manager", "processManager", true),
    ("show_docker_manager", "dockerManager", true),
];

pub fn default_hidden_items() -> Vec<String> {
    vec!["gpuMonitor".into(), "ascendNpuMonitor".into()]
}

/// Validate before changing the document. The marker travels with the layout so
/// restoring an older backup cannot inherit the destination's migration state.
pub fn migrate_legacy_panel_visibility(document: &mut Value) -> Result<bool, String> {
    let root = document.as_object().ok_or("settings must be an object")?;
    let empty = Map::new();
    let ui = match root.get("ui") {
        None => &empty,
        Some(value) => value.as_object().ok_or("ui must be an object")?,
    };
    let mut layout = match ui.get("activity_bar_layout") {
        None => Map::new(),
        Some(Value::String(value)) if value == "default" => Map::new(),
        Some(Value::Object(value)) => value.clone(),
        Some(_) => return Err("unsupported activity bar layout".into()),
    };
    for field in [
        "left_top",
        "left_bottom",
        "right_top",
        "right_bottom",
        "hidden_items",
    ] {
        if let Some(value) = layout.get(field)
            && !value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string))
        {
            return Err(format!("activity bar {field} must be an array of strings"));
        }
    }
    if layout
        .get("show_labels")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err("activity bar show_labels must be a boolean".into());
    }
    match layout.get("legacy_visibility_migrated") {
        Some(Value::Bool(true)) => return Ok(false),
        None | Some(Value::Bool(false)) => {}
        Some(_) => return Err("legacy visibility marker must be a boolean".into()),
    }
    let mut hidden = layout
        .get("hidden_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut disabled = Vec::new();
    for (key, id, default) in LEGACY_PANELS {
        let enabled = match ui.get(key) {
            None => default,
            Some(Value::Bool(enabled)) => *enabled,
            Some(_) => return Err(format!("ui.{key} must be a boolean")),
        };
        if !enabled {
            disabled.push(id);
        }
        if !enabled && !hidden.iter().any(|value| value.as_str() == Some(id)) {
            hidden.push(Value::String(id.into()));
        }
    }
    layout.insert("hidden_items".into(), Value::Array(hidden));
    layout.insert("legacy_visibility_migrated".into(), Value::Bool(true));
    let root = document.as_object_mut().expect("validated settings object");
    let ui = root
        .entry("ui")
        .or_insert_with(|| Value::Object(Map::new()));
    let ui = ui.as_object_mut().expect("validated ui object");
    // Old switches could leave remembered open panels behind. Preserve their
    // effective disabled state once, without making rail visibility a runtime
    // restriction on panels explicitly opened after migration.
    clear_disabled_panels(ui, &disabled);
    if let Some(workspaces) = ui
        .get_mut("workspaces")
        .and_then(|manifest| manifest.get_mut("workspaces"))
        .and_then(Value::as_array_mut)
    {
        for workspace in workspaces {
            if let Some(workspace_ui) = workspace.get_mut("ui").and_then(Value::as_object_mut) {
                clear_disabled_panels(workspace_ui, &disabled);
            }
        }
    }
    ui.insert("activity_bar_layout".into(), Value::Object(layout));
    Ok(true)
}

fn clear_disabled_panels(ui: &mut Map<String, Value>, disabled: &[&str]) {
    for key in ["active_left_panel", "active_right_panel"] {
        if ui
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|id| disabled.contains(&id))
        {
            ui.insert(key.into(), Value::Null);
        }
    }
    for key in ["left_open_panels", "right_open_panels"] {
        if let Some(Value::Array(open)) = ui.get_mut(key) {
            open.retain(|value| !value.as_str().is_some_and(|id| disabled.contains(&id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::migrate_legacy_panel_visibility;
    use serde_json::json;

    #[test]
    fn migration_preserves_layout_unknown_fields_and_is_not_reapplied() {
        let mut value = json!({"future": 42, "ui": {
            "show_notes_panel": false, "show_gpu_monitor": true,
            "activity_bar_layout": {"left_top": ["gpuMonitor", "notes"], "hidden_items": ["futurePanel", "gpuMonitor"], "future": {"x": 1}}
        }});
        assert!(migrate_legacy_panel_visibility(&mut value).unwrap());
        assert_eq!(
            value["ui"]["activity_bar_layout"]["hidden_items"],
            json!(["futurePanel", "gpuMonitor", "notes", "ascendNpuMonitor"])
        );
        assert_eq!(
            value["ui"]["activity_bar_layout"]["left_top"],
            json!(["gpuMonitor", "notes"])
        );
        assert_eq!(
            value["ui"]["activity_bar_layout"]["future"],
            json!({"x": 1})
        );
        assert_eq!(value["future"], 42);
        value["ui"]["activity_bar_layout"]["hidden_items"] = json!([]);
        let before = value.clone();
        assert!(!migrate_legacy_panel_visibility(&mut value).unwrap());
        assert_eq!(value, before);
    }

    #[test]
    fn all_switch_combinations_preserve_effective_visibility() {
        for bits in 0..64 {
            let mut value = json!({"ui": {"activity_bar_layout": "default"}});
            for (index, (key, _, _)) in super::LEGACY_PANELS.iter().enumerate() {
                value["ui"][*key] = json!(bits & (1 << index) != 0);
            }
            migrate_legacy_panel_visibility(&mut value).unwrap();
            let hidden = value["ui"]["activity_bar_layout"]["hidden_items"]
                .as_array()
                .unwrap();
            for (index, (_, id, _)) in super::LEGACY_PANELS.iter().enumerate() {
                assert_eq!(hidden.contains(&json!(id)), bits & (1 << index) == 0);
            }
        }
        let mut value = json!({});
        migrate_legacy_panel_visibility(&mut value).unwrap();
        assert_eq!(
            value["ui"]["activity_bar_layout"]["hidden_items"],
            json!(["gpuMonitor", "ascendNpuMonitor"])
        );
    }

    #[test]
    fn malformed_documents_are_not_modified() {
        for mut value in [
            json!([]),
            json!({"ui": null}),
            json!({"ui": {"activity_bar_layout": "future"}}),
            json!({"ui": {"activity_bar_layout": {"hidden_items": [3]}}}),
            json!({"ui": {"show_gpu_monitor": "false"}}),
            json!({"ui": {"activity_bar_layout": {"legacy_visibility_migrated": 1}}}),
        ] {
            let before = value.clone();
            assert!(migrate_legacy_panel_visibility(&mut value).is_err());
            assert_eq!(value, before);
        }
    }

    #[test]
    fn disabled_legacy_panels_do_not_reopen_from_remembered_selections() {
        let mut value = json!({"ui": {
            "active_left_panel": "notes", "active_right_panel": "gpuMonitor",
            "show_notes_panel": true, "activity_bar_layout": {"hidden_items": ["notes"]},
            "right_open_panels": ["gpuMonitor", "futurePanel", "processManager"],
            "workspaces": {"workspaces": [{"ui": {"active_right_panel": "gpuMonitor", "right_open_panels": ["gpuMonitor", "notes"], "future": 7}}]}
        }});
        migrate_legacy_panel_visibility(&mut value).unwrap();
        assert_eq!(value["ui"]["active_left_panel"], "notes");
        assert!(value["ui"]["active_right_panel"].is_null());
        assert_eq!(
            value["ui"]["right_open_panels"],
            json!(["futurePanel", "processManager"])
        );
        assert_eq!(
            value["ui"]["workspaces"]["workspaces"][0]["ui"],
            json!({"active_right_panel": null, "right_open_panels": ["notes"], "future": 7})
        );
    }
}
