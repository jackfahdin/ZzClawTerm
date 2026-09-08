use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A folder in the Notes tree.
///
/// The flattened map deliberately retains fields written by newer NyaTerm
/// versions. Notes are part of portable backups, so silently dropping an
/// unknown field during an otherwise unrelated rename would be a compatibility
/// regression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// The complete persisted Markdown document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDocument {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub markdown: String,
    pub sort_order: i64,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Lightweight tree index record. The Markdown body intentionally lives only
/// in [`NoteDocument`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub sort_order: i64,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

impl From<&NoteDocument> for NoteSummary {
    fn from(note: &NoteDocument) -> Self {
        Self {
            id: note.id.clone(),
            parent_id: note.parent_id.clone(),
            title: note.title.clone(),
            sort_order: note.sort_order,
            revision: note.revision,
            created_at_ms: note.created_at_ms,
            updated_at_ms: note.updated_at_ms,
            extra: BTreeMap::new(),
        }
    }
}

impl From<NoteDocument> for NoteSummary {
    fn from(note: NoteDocument) -> Self {
        Self::from(&note)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NotesSnapshot {
    #[serde(default)]
    pub folders: Vec<NoteFolder>,
    #[serde(default)]
    pub notes: Vec<NoteDocument>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NotesUiState {
    #[serde(default)]
    pub expanded_folder_ids: Vec<String>,
    #[serde(default)]
    pub last_selected_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteTreePayload {
    pub folders: Vec<NoteFolder>,
    pub notes: Vec<NoteSummary>,
    #[serde(default)]
    pub ui: NotesUiState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NoteNodeKind {
    Folder,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteUpdateResult {
    pub note: NoteDocument,
    pub changed: bool,
    pub tree_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteNodeChange {
    pub changed: bool,
    pub tree_changed: bool,
    pub folder: Option<NoteFolder>,
    pub note: Option<NoteSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteNoteNodeResult {
    pub folder_count: usize,
    pub note_count: usize,
    pub ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{NoteDocument, NotesSnapshot};

    #[test]
    fn tauri_note_fixture_round_trips_without_renaming_fields() {
        let raw = r##"{"id":"note-1","parent_id":null,"title":"Runbook","markdown":"# Deploy","sort_order":7,"revision":3,"created_at_ms":10,"updated_at_ms":20}"##;
        let note: NoteDocument = serde_json::from_str(raw).expect("parse Tauri note");
        let value = serde_json::to_value(note).expect("serialize note");

        assert_eq!(value["parent_id"], serde_json::Value::Null);
        assert_eq!(value["markdown"], "# Deploy");
        assert_eq!(value["revision"], 3);
    }

    #[test]
    fn note_snapshots_preserve_unknown_fields() {
        let raw = serde_json::json!({
            "folders": [{
                "id": "folder-1",
                "parent_id": null,
                "name": "Ops",
                "sort_order": 0,
                "created_at_ms": 1,
                "updated_at_ms": 2,
                "future_folder_flag": true
            }],
            "notes": [{
                "id": "note-1",
                "parent_id": "folder-1",
                "title": "Runbook",
                "markdown": "body",
                "sort_order": 0,
                "revision": 1,
                "created_at_ms": 1,
                "updated_at_ms": 2,
                "future_document": { "version": 4 }
            }],
            "future_snapshot": "kept"
        });
        let snapshot: NotesSnapshot =
            serde_json::from_value(raw.clone()).expect("parse future snapshot");
        let encoded = serde_json::to_value(snapshot).expect("serialize future snapshot");

        assert_eq!(encoded, raw);
    }
}
