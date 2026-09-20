//! Pure filename and hierarchy planning for read-only Markdown export.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::models::notes::NotesSnapshot;

pub const MAX_COMPONENT_BYTES: usize = 240;

fn truncate_utf8(value: &str, limit: usize) -> &str {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn sanitize_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut name = truncate_utf8(&cleaned, MAX_COMPONENT_BYTES)
        .trim_end_matches([' ', '.'])
        .to_string();
    if name.is_empty() {
        name = "Untitled".to_string();
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_uppercase();
    let numbered_device = ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|number| {
            matches!(
                number,
                "1" | "2"
                    | "3"
                    | "4"
                    | "5"
                    | "6"
                    | "7"
                    | "8"
                    | "9"
                    | "\u{b9}"
                    | "\u{b2}"
                    | "\u{b3}"
            )
        })
    });
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered_device {
        name.insert(0, '_');
    }
    name
}

/// Files and directories share one case-insensitive sibling namespace.
#[derive(Default)]
pub struct ExportNames(HashSet<String>);

impl ExportNames {
    pub fn reserve(&mut self, name: &str) {
        self.0.insert(name.to_uppercase());
    }

    pub fn unique(&mut self, title: &str, markdown: bool) -> String {
        let extension = if markdown { ".md" } else { "" };
        let base = sanitize_name(title);
        for index in 1_u64.. {
            let suffix = if index == 1 {
                String::new()
            } else {
                format!(" ({index})")
            };
            let stem = truncate_utf8(&base, MAX_COMPONENT_BYTES - extension.len() - suffix.len())
                .trim_end_matches([' ', '.']);
            let name = format!("{stem}{suffix}{extension}");
            if self.0.insert(name.to_uppercase()) {
                return name;
            }
        }
        unreachable!("exhausted export filename suffixes")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NoteExportPlanError {
    #[error("duplicate note node ID")]
    DuplicateId,
    #[error("cyclic or missing note folder parent")]
    InvalidHierarchy,
}

pub struct PlannedNote<'a> {
    pub relative_path: PathBuf,
    pub markdown: &'a str,
}

pub struct NoteExportPlan<'a> {
    pub directories: Vec<PathBuf>,
    pub notes: Vec<PlannedNote<'a>>,
}

pub fn plan_note_export(
    snapshot: &NotesSnapshot,
) -> Result<NoteExportPlan<'_>, NoteExportPlanError> {
    let mut ids = HashSet::new();
    for id in snapshot
        .folders
        .iter()
        .map(|folder| &folder.id)
        .chain(snapshot.notes.iter().map(|note| &note.id))
    {
        if !ids.insert(id) {
            return Err(NoteExportPlanError::DuplicateId);
        }
    }
    let mut paths = HashMap::from([(None, PathBuf::new())]);
    let mut names: HashMap<PathBuf, ExportNames> = HashMap::new();
    let mut pending: Vec<_> = snapshot.folders.iter().collect();
    let mut directories = Vec::new();
    while !pending.is_empty() {
        let previous = pending.len();
        let mut unresolved = Vec::new();
        for folder in pending {
            let Some(parent) = paths.get(&folder.parent_id) else {
                unresolved.push(folder);
                continue;
            };
            let name = names
                .entry(parent.clone())
                .or_default()
                .unique(&folder.name, false);
            let path = parent.join(name);
            directories.push(path.clone());
            paths.insert(Some(folder.id.clone()), path);
        }
        if unresolved.len() == previous {
            return Err(NoteExportPlanError::InvalidHierarchy);
        }
        pending = unresolved;
    }
    let mut notes = Vec::new();
    for note in &snapshot.notes {
        let parent = paths
            .get(&note.parent_id)
            .ok_or(NoteExportPlanError::InvalidHierarchy)?;
        let name = names
            .entry(parent.clone())
            .or_default()
            .unique(&note.title, true);
        notes.push(PlannedNote {
            relative_path: parent.join(name),
            markdown: &note.markdown,
        });
    }
    Ok(NoteExportPlan { directories, notes })
}

#[cfg(test)]
mod tests {
    use super::{
        ExportNames, MAX_COMPONENT_BYTES, NoteExportPlanError, plan_note_export, sanitize_name,
    };
    use crate::models::notes::{NoteDocument, NoteFolder, NotesSnapshot};
    use std::collections::BTreeMap;

    fn folder(id: &str, parent: Option<&str>, name: &str) -> NoteFolder {
        NoteFolder {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            name: name.into(),
            sort_order: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
            extra: BTreeMap::new(),
        }
    }
    fn note(id: &str, parent: Option<&str>, title: &str) -> NoteDocument {
        NoteDocument {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            title: title.into(),
            markdown: "# test\r\n\0".into(),
            revision: 7,
            sort_order: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn filenames_are_portable_and_case_insensitively_unique() {
        for device in ["CON", "prn", "AUX", "NUL", "COM1", "LPT9", "COM\u{b9}"] {
            assert_eq!(
                sanitize_name(&format!("{device}.txt")),
                format!("_{device}.txt")
            );
        }
        assert_eq!(sanitize_name("a/b?c\n"), "a_b_c_");
        assert_eq!(sanitize_name(".. . "), "Untitled");
        let mut names = ExportNames::default();
        assert_eq!(names.unique("foo.md", false), "foo.md");
        assert_eq!(names.unique("FOO", true), "FOO (2).md");
        assert_eq!(names.unique("foo", true), "foo (3).md");
        for _ in 0..3 {
            let long = names.unique(&"\u{4e2d}\u{1f600}".repeat(100), true);
            assert!(long.len() <= MAX_COMPONENT_BYTES);
            assert!(long.ends_with(".md"));
        }
    }

    #[test]
    fn resolves_unordered_parents_and_preserves_markdown() {
        let snapshot = NotesSnapshot {
            folders: vec![folder("b", Some("a"), "Child"), folder("a", None, "Parent")],
            notes: vec![note("n", Some("b"), "Nested")],
            extra: BTreeMap::new(),
        };
        let plan = plan_note_export(&snapshot).expect("plan");
        assert_eq!(plan.directories.len(), 2);
        assert_eq!(
            plan.notes[0].relative_path,
            std::path::Path::new("Parent/Child/Nested.md")
        );
        assert_eq!(plan.notes[0].markdown, snapshot.notes[0].markdown);
    }

    #[test]
    fn rejects_cycles_orphans_and_duplicate_node_ids() {
        for folders in [
            vec![folder("a", Some("a"), "Cycle")],
            vec![folder("a", Some("b"), "A"), folder("b", Some("a"), "B")],
            vec![folder("a", Some("missing"), "Orphan")],
        ] {
            assert!(matches!(
                plan_note_export(&NotesSnapshot {
                    folders,
                    ..NotesSnapshot::default()
                }),
                Err(NoteExportPlanError::InvalidHierarchy)
            ));
        }
        assert!(matches!(
            plan_note_export(&NotesSnapshot {
                notes: vec![note("n", Some("missing"), "Orphan")],
                ..NotesSnapshot::default()
            }),
            Err(NoteExportPlanError::InvalidHierarchy)
        ));
        assert!(matches!(
            plan_note_export(&NotesSnapshot {
                folders: vec![folder("n", None, "F")],
                notes: vec![note("n", None, "N")],
                extra: BTreeMap::new()
            }),
            Err(NoteExportPlanError::DuplicateId)
        ));
    }
}
