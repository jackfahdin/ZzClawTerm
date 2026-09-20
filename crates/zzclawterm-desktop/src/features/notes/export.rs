use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gpui::{Context, PathPromptOptions};
use rust_i18n::t;
use zzclawterm_core::models::notes::NotesSnapshot;
use zzclawterm_core::note_export::{ExportNames, NoteExportPlan, plan_note_export};
use zzclawterm_store::{StoreDomain, store_request};

use crate::features::{ZzClawTermApp, runtime_jobs::await_blocking_job};

struct NoteExportResult {
    output_path: PathBuf,
    folder_count: usize,
    note_count: usize,
}

fn reserve_root(destination: &Path) -> io::Result<PathBuf> {
    let mut names = ExportNames::default();
    for entry in fs::read_dir(destination)? {
        names.reserve(&entry?.file_name().to_string_lossy());
    }
    loop {
        let root = destination.join(names.unique("ZzClawTerm Notes", false));
        match fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

fn write_plan(root: &Path, plan: &NoteExportPlan<'_>) -> io::Result<()> {
    for directory in &plan.directories {
        fs::create_dir(root.join(directory))?;
    }
    for note in &plan.notes {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(&note.relative_path))?;
        file.write_all(note.markdown.as_bytes())?;
        file.flush()?;
    }
    Ok(())
}

fn export_snapshot(
    destination: &Path,
    snapshot: &NotesSnapshot,
) -> Result<NoteExportResult, String> {
    // Validate before creating anything, including a malformed hierarchy's root.
    let plan = plan_note_export(snapshot).map_err(|error| error.to_string())?;
    export_plan(destination, &plan)
}

fn export_plan(destination: &Path, plan: &NoteExportPlan<'_>) -> Result<NoteExportResult, String> {
    let destination = fs::canonicalize(destination).map_err(|error| error.to_string())?;
    let root = reserve_root(&destination).map_err(|error| error.to_string())?;
    if let Err(error) = write_plan(&root, plan) {
        // Only a root exclusively reserved by this operation can be removed.
        if let Err(cleanup) = fs::remove_dir_all(&root) {
            return Err(format!(
                "{error}; incomplete export cleanup failed: {cleanup}"
            ));
        }
        return Err(error.to_string());
    }
    Ok(NoteExportResult {
        output_path: root,
        folder_count: plan.directories.len(),
        note_count: plan.notes.len(),
    })
}

impl ZzClawTermApp {
    pub(super) fn prompt_export_notes(&mut self, cx: &mut Context<Self>) {
        if self.notes.has_unsaved_editors(cx) {
            self.shell
                .set_status(t!("notes.exportSaveFirst").to_string());
            cx.notify();
            return;
        }
        let Some(request) = self.notes.begin_export() else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("notes.exportMarkdown").to_string().into()),
        });
        cx.spawn(async move |this, cx| {
            let selected = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = this.update(cx, |app, cx| {
                let Some(destination) = selected else {
                    app.notes.finish_export(request);
                    cx.notify();
                    return;
                };
                if app.notes.has_unsaved_editors(cx) {
                    app.notes.finish_export(request);
                    app.shell
                        .set_status(t!("notes.exportSaveFirst").to_string());
                    cx.notify();
                    return;
                }
                app.submit_notes_export(request, destination, cx);
            });
        })
        .detach();
    }

    fn submit_notes_export(&mut self, request: u64, destination: PathBuf, cx: &mut Context<Self>) {
        let queued = self.submit_store_request(
            request,
            store_request(StoreDomain::Notes, |store| store.load_notes_snapshot()),
            move |app, event, cx| {
                let snapshot = match event.outcome {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        app.notes.finish_export(request);
                        app.shell.set_status(
                            t!("notes.exportFailed", error = error.to_string()).to_string(),
                        );
                        cx.notify();
                        return;
                    }
                };
                if app.notes.has_unsaved_editors(cx) || app.notes.has_stale_editors(&snapshot, cx) {
                    app.notes.finish_export(request);
                    app.shell
                        .set_status(t!("notes.exportSaveFirst").to_string());
                    cx.notify();
                    return;
                }
                let scheduler = app.blocking_jobs.clone();
                cx.spawn(async move |this, cx| {
                    let task = scheduler.submit_task("notes-markdown-export", move |_| {
                        export_snapshot(&destination, &snapshot)
                    });
                    let result = await_blocking_job(task).await.and_then(|result| result);
                    let _ = this.update(cx, |app, cx| {
                        if !app.notes.finish_export(request) {
                            return;
                        }
                        match result {
                            Ok(result) => app.shell.set_status(
                                t!(
                                    "notes.exportSuccess",
                                    path = result.output_path.display().to_string(),
                                    folders = result.folder_count,
                                    notes = result.note_count
                                )
                                .to_string(),
                            ),
                            Err(error) => app
                                .shell
                                .set_status(t!("notes.exportFailed", error = error).to_string()),
                        }
                        cx.notify();
                    });
                })
                .detach();
            },
            cx,
        );
        if !queued {
            self.notes.finish_export(request);
            self.shell.set_status(
                t!("notes.exportFailed", error = t!("notes.exportUnavailable")).to_string(),
            );
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{export_plan, export_snapshot, write_plan};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use zzclawterm_core::models::notes::{NoteDocument, NotesSnapshot};

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("zzclawterm-export-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).expect("temporary directory");
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn snapshot() -> NotesSnapshot {
        NotesSnapshot {
            notes: vec![NoteDocument {
                id: "n".into(),
                parent_id: None,
                title: "CON".into(),
                markdown: "# test\r\n\0".into(),
                revision: 7,
                sort_order: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
                extra: BTreeMap::new(),
            }],
            ..NotesSnapshot::default()
        }
    }
    #[test]
    fn exports_verbatim_and_never_overwrites_existing_roots() {
        let temp = TestDirectory::new();
        fs::create_dir(temp.0.join("zzclawterm notes")).expect("existing root");
        fs::write(temp.0.join("ZzClawTerm Notes (2)"), "keep").expect("existing file");
        let result = export_snapshot(&temp.0, &snapshot()).expect("export");
        assert_eq!(
            result.output_path.file_name().expect("name"),
            "ZzClawTerm Notes (3)"
        );
        assert_eq!((result.folder_count, result.note_count), (0, 1));
        assert_eq!(
            fs::read(result.output_path.join("_CON.md")).expect("note"),
            b"# test\r\n\0"
        );
        assert_eq!(
            fs::read(temp.0.join("ZzClawTerm Notes (2)")).expect("keep"),
            b"keep"
        );
    }
    #[test]
    fn exclusive_creation_preserves_files_and_invalid_hierarchy_creates_nothing() {
        let temp = TestDirectory::new();
        let mut snapshot = snapshot();
        let plan = zzclawterm_core::note_export::plan_note_export(&snapshot).expect("plan");
        fs::write(temp.0.join("_CON.md"), "keep").expect("existing file");
        assert!(write_plan(&temp.0, &plan).is_err());
        assert_eq!(fs::read(temp.0.join("_CON.md")).expect("keep"), b"keep");
        snapshot.notes[0].parent_id = Some("missing".into());
        assert!(export_snapshot(&temp.0, &snapshot).is_err());
        assert_eq!(fs::read_dir(&temp.0).expect("entries").count(), 1);
    }

    #[test]
    fn partial_write_failure_cleans_only_this_operations_reserved_root() {
        use zzclawterm_core::note_export::{NoteExportPlan, PlannedNote};
        let temp = TestDirectory::new();
        let existing = temp.0.join("ZzClawTerm Notes");
        fs::create_dir(&existing).expect("existing root");
        fs::write(existing.join("keep.md"), "keep").expect("existing note");
        let plan = NoteExportPlan {
            directories: vec![PathBuf::from("blocked.md")],
            notes: vec![
                PlannedNote {
                    relative_path: PathBuf::from("written.md"),
                    markdown: "partial",
                },
                PlannedNote {
                    relative_path: PathBuf::from("blocked.md"),
                    markdown: "cannot write a directory",
                },
            ],
        };
        assert!(export_plan(&temp.0, &plan).is_err());
        assert_eq!(fs::read_dir(&temp.0).expect("entries").count(), 1);
        assert_eq!(fs::read(existing.join("keep.md")).expect("keep"), b"keep");
    }

    #[test]
    fn empty_snapshot_exports_an_empty_root() {
        let temp = TestDirectory::new();
        let result = export_snapshot(&temp.0, &NotesSnapshot::default()).expect("export");
        assert_eq!((result.folder_count, result.note_count), (0, 0));
        assert_eq!(fs::read_dir(result.output_path).expect("root").count(), 0);
    }
}
