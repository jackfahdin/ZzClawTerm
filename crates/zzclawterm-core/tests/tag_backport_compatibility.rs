use zzclawterm_core::assets::{
    AssetFilterKey, build_asset_search_text, connection_matches_filters,
};
use zzclawterm_core::models::sessions::SavedConnection;

fn connection(fields: serde_json::Value) -> SavedConnection {
    let mut raw =
        serde_json::json!({"id":"tagged", "name":"Tagged", "type":"ssh", "host":"test.invalid"});
    raw.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(raw).unwrap()
}

#[test]
fn old_asset_tags_load_only_when_top_level_is_empty() {
    for fields in [
        serde_json::json!({"asset":{"tags":["legacy"]}}),
        serde_json::json!({"tags":[], "asset":{"tags":["legacy"]}}),
    ] {
        let loaded = connection(fields);
        assert_eq!(loaded.tags, ["legacy"]);
        let raw = serde_json::to_value(loaded).unwrap();
        assert_eq!(raw["tags"], serde_json::json!(["legacy"]));
        assert_eq!(raw["asset"]["tags"], raw["tags"]);
    }
    let loaded = connection(
        serde_json::json!({"tags":["canonical"], "asset":{"tags":["legacy"], "future_asset":true}, "future_connection":true}),
    );
    assert_eq!(loaded.tags, ["canonical"]);
    let raw = serde_json::to_value(loaded).unwrap();
    assert_eq!(raw["asset"]["tags"], serde_json::json!(["canonical"]));
    assert_eq!(raw["asset"]["future_asset"], true);
    assert_eq!(raw["future_connection"], true);
}

#[test]
fn empty_tags_are_omitted_and_clearing_does_not_resurrect_legacy_tags() {
    let mut loaded =
        connection(serde_json::json!({"asset":{"tags":["legacy"], "os_name":"Linux"}}));
    loaded.tags.clear();
    let raw = serde_json::to_value(loaded).unwrap();
    assert!(raw.get("tags").is_none());
    assert_eq!(raw["asset"]["tags"], serde_json::json!([]));
    assert_eq!(raw["asset"]["os_name"], "Linux");
    let reloaded: SavedConnection = serde_json::from_value(raw).unwrap();
    assert!(reloaded.tags.is_empty());
    let empty = serde_json::to_value(connection(serde_json::json!({"tags":[]}))).unwrap();
    assert!(empty.get("tags").is_none());
    assert!(empty.get("asset").is_none());
}

#[test]
fn new_tags_write_legacy_mirror_without_existing_asset() {
    let loaded = connection(serde_json::json!({"tags":["production", "gpu"]}));
    let raw = serde_json::to_value(loaded).unwrap();
    assert_eq!(raw["asset"]["tags"], raw["tags"]);
    let reloaded: SavedConnection = serde_json::from_value(raw).unwrap();
    assert_eq!(reloaded.tags, ["production", "gpu"]);
}

#[test]
fn dynamic_filters_and_search_use_only_authoritative_tags() {
    let mut loaded =
        connection(serde_json::json!({"tags":["production", "gpu"], "asset":{"tags":["stale"]}}));
    let filters = [
        AssetFilterKey::Tag("production".into()),
        AssetFilterKey::Tag("gpu".into()),
    ];
    assert!(connection_matches_filters(&loaded, &filters));
    assert!(!connection_matches_filters(
        &loaded,
        &[AssetFilterKey::Tag("missing".into())]
    ));
    let search = build_asset_search_text(&loaded, "Root");
    assert!(search.contains("production"));
    assert!(!search.contains("stale"));
    loaded.tags.clear();
    assert!(!connection_matches_filters(&loaded, &filters));
    assert!(!build_asset_search_text(&loaded, "Root").contains("stale"));
}
