use std::collections::HashMap;

mod editor;
mod panel;
mod runtime;
mod state;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features) enum NotesCatalogEvent {
    NoteUpserted { id: String, revision: u64 },
    NodesDeleted { ids: Vec<String> },
    CatalogReplaced { revisions: HashMap<String, u64> },
}

pub(in crate::features) use panel::NotesPanel;
pub(in crate::features) use state::{NoteTreeRow, NotesFeatureState};
