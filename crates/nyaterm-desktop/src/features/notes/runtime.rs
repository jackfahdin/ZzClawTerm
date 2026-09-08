use gpui::{Context, ParentElement as _, Window, div, prelude::*};
use nyaterm_core::{NoteDocument, NoteNodeKind, NoteSummary};
use nyaterm_store::{StoreDomain, store_request};
use nyaterm_ui::{NyaConfirmDialog, NyaDialogFooter, NyaDialogWindowExt as _};

use crate::{
    features::{NyaTermApp, notes::NotesCatalogEvent},
    models::NavItem,
};

impl NyaTermApp {
    pub(in crate::features) fn ensure_visible_notes_loaded(&mut self, cx: &mut Context<Self>) {
        if self.panel_is_rendered(NavItem::Notes) {
            self.load_notes_if_needed(cx);
        }
    }

    pub(in crate::features) fn load_notes_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.notes.loaded() || self.notes.loading() {
            return;
        }
        let Some(generation) = self.notes.begin_load() else {
            return;
        };
        self.submit_notes_load(generation, cx);
    }

    pub(in crate::features) fn refresh_notes(&mut self, cx: &mut Context<Self>) {
        let generation = self.notes.begin_refresh();
        self.submit_notes_load(generation, cx);
        cx.notify();
    }

    fn submit_notes_load(&mut self, generation: u64, cx: &mut Context<Self>) {
        let queued = self.submit_store_request(
            generation,
            store_request(StoreDomain::Notes, |store| store.list_note_tree()),
            move |app, event, cx| {
                match event.outcome {
                    Ok(payload) => {
                        if app.notes.apply_load(generation, payload) {
                            cx.emit(NotesCatalogEvent::CatalogReplaced {
                                revisions: app.notes.revisions(),
                            });
                        }
                    }
                    Err(error) => {
                        app.notes.fail_load(generation, error.to_string());
                    }
                }
                cx.notify();
            },
            cx,
        );
        if !queued {
            self.notes
                .fail_load(generation, "Notes storage is unavailable".to_string());
        }
    }

    pub(in crate::features) fn select_note_node(
        &mut self,
        node_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.notes.set_selected(node_id) {
            self.persist_notes_ui_state(cx);
            cx.notify();
        }
    }

    pub(in crate::features) fn toggle_note_folder(
        &mut self,
        folder_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.notes.toggle_folder(folder_id) {
            self.persist_notes_ui_state(cx);
            cx.notify();
        }
    }

    pub(in crate::features) fn set_all_notes_expanded(
        &mut self,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.notes.set_all_expanded(expanded);
        self.persist_notes_ui_state(cx);
        cx.notify();
    }

    fn persist_notes_ui_state(&mut self, cx: &mut Context<Self>) {
        let state = self.notes.ui_state();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.save_notes_ui_state(&state).map(|_| ())
            }),
            |app, event, cx| {
                if let Err(error) = event.outcome {
                    app.shell
                        .set_status(format!("failed to save Notes panel state: {error}"));
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn create_note_in_selected_folder(&mut self, cx: &mut Context<Self>) {
        let parent_id = self.selected_note_parent_for_creation();
        self.create_note_in_folder(parent_id, cx);
    }

    pub(in crate::features) fn create_root_note(&mut self, cx: &mut Context<Self>) {
        self.create_note_in_folder(None, cx);
    }

    pub(in crate::features) fn create_note_in_folder(
        &mut self,
        parent_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let default_title = self
            .notes
            .unique_name_for_parent(parent_id.as_deref(), &rust_i18n::t!("notes.newNote"));
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.create_note(parent_id, Some(default_title), None)
            }),
            |app, event, cx| match event.outcome {
                Ok(note) => {
                    let note_id = note.id.clone();
                    let parent_id = note.parent_id.clone();
                    let revision = note.revision;
                    app.notes.upsert_note(NoteSummary::from(&note));
                    cx.emit(NotesCatalogEvent::NoteUpserted {
                        id: note_id.clone(),
                        revision,
                    });
                    app.notes.set_selected(Some(note_id.clone()));
                    if let Some(parent_id) = parent_id {
                        app.notes.set_folder_expanded(&parent_id, true);
                    }
                    app.persist_notes_ui_state(cx);
                    app.open_note_editor(note_id, cx);
                    cx.notify();
                }
                Err(error) => {
                    app.shell.set_status(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn create_note_folder_in_selected_folder(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let parent_id = self.selected_note_parent_for_creation();
        self.create_note_folder_in_folder(parent_id, cx);
    }

    pub(in crate::features) fn create_root_note_folder(&mut self, cx: &mut Context<Self>) {
        self.create_note_folder_in_folder(None, cx);
    }

    pub(in crate::features) fn create_note_folder_in_folder(
        &mut self,
        parent_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let default_name = self
            .notes
            .unique_name_for_parent(parent_id.as_deref(), &rust_i18n::t!("notes.newFolder"));
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.create_note_folder(parent_id, Some(default_name))
            }),
            |app, event, cx| match event.outcome {
                Ok(folder) => {
                    let id = folder.id.clone();
                    let parent_id = folder.parent_id.clone();
                    app.notes.upsert_folder(folder);
                    app.notes.set_selected(Some(id));
                    if let Some(parent_id) = parent_id {
                        app.notes.set_folder_expanded(&parent_id, true);
                    }
                    app.persist_notes_ui_state(cx);
                    cx.notify();
                }
                Err(error) => {
                    app.shell.set_status(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn selected_note_parent_for_creation(&self) -> Option<String> {
        self.notes
            .selected_node_id()
            .and_then(|id| self.notes.node(id))
            .and_then(|(kind, parent_id, _)| match kind {
                NoteNodeKind::Folder => self.notes.selected_node_id().map(str::to_string),
                NoteNodeKind::Note => parent_id,
            })
    }

    pub(in crate::features) fn delete_note_node(
        &mut self,
        node_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some((kind, _, _)) = self.notes.node(&node_id) else {
            return;
        };
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.delete_note_node(kind, &node_id)
            }),
            |app, event, cx| match event.outcome {
                Ok(result) => {
                    let ids = result.ids.clone();
                    app.notes.apply_delete(&result);
                    cx.emit(NotesCatalogEvent::NodesDeleted { ids });
                    app.persist_notes_ui_state(cx);
                    cx.notify();
                }
                Err(error) => {
                    app.shell.set_status(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn request_delete_selected_note_node(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.notes.selected_node_id().map(str::to_string) else {
            return;
        };
        self.request_delete_note_node(node_id, window, cx);
    }

    pub(in crate::features) fn request_delete_note_node(
        &mut self,
        node_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((kind, _, name)) = self.notes.node(&node_id) else {
            return;
        };
        let (folder_count, note_count) = self.notes.delete_counts(&node_id).unwrap_or_default();
        let message = match kind {
            NoteNodeKind::Folder => rust_i18n::t!(
                "notes.deleteFolderDescription",
                name = name,
                folders = folder_count.saturating_sub(1),
                notes = note_count
            )
            .to_string(),
            NoteNodeKind::Note => {
                rust_i18n::t!("notes.deleteNoteDescription", name = name).to_string()
            }
        };
        let app = cx.weak_entity();
        window.open_nya_dialog(cx, move |dialog, _, _| {
            let confirm_app = app.clone();
            let confirm_node_id = node_id.clone();
            NyaConfirmDialog::new(
                dialog
                    .title(
                        div()
                            .debug_selector(|| "notes-delete-dialog-title".to_string())
                            .child(rust_i18n::t!("notes.deleteTitle").to_string()),
                    )
                    .width(420.),
                NyaDialogFooter::new(
                    rust_i18n::t!("common.cancel").to_string(),
                    rust_i18n::t!("common.delete").to_string(),
                )
                .danger(),
            )
            .content(
                div()
                    .debug_selector(|| "notes-delete-dialog-content".to_string())
                    .child(message.clone()),
            )
            .on_confirm(move |_, _, cx| {
                confirm_app
                    .update(cx, |app, cx| {
                        app.delete_note_node(confirm_node_id.clone(), cx)
                    })
                    .is_ok()
            })
            .on_cancel(|_, _, _| true)
            .into_dialog()
        });
    }

    pub(in crate::features) fn rename_note_node(
        &mut self,
        node_id: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some((kind, _, _)) = self.notes.node(&node_id) else {
            return;
        };
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.rename_note_node(kind, &node_id, name)
            }),
            |app, event, cx| match event.outcome {
                Ok(change) => {
                    let note_version = change
                        .note
                        .as_ref()
                        .map(|note| (note.id.clone(), note.revision));
                    app.notes.apply_node_change(change);
                    if let Some((id, revision)) = note_version {
                        cx.emit(NotesCatalogEvent::NoteUpserted { id, revision });
                    }
                    cx.notify();
                }
                Err(error) => {
                    app.shell.set_status(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn move_note_node_to(
        &mut self,
        node_id: String,
        target_parent_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .notes
            .can_move_to(&node_id, target_parent_id.as_deref())
        {
            self.shell
                .set_status("invalid Notes move target".to_string());
            cx.notify();
            return;
        }
        let Some((kind, _, _)) = self.notes.node(&node_id) else {
            return;
        };
        let sort_order = self.notes.next_sort_order(target_parent_id.as_deref());
        let expanded_parent_id = target_parent_id.clone();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.move_note_node(kind, &node_id, target_parent_id, sort_order)
            }),
            move |app, event, cx| match event.outcome {
                Ok(change) => {
                    let note_version = change
                        .note
                        .as_ref()
                        .map(|note| (note.id.clone(), note.revision));
                    app.notes.apply_node_change(change);
                    if let Some(parent_id) = expanded_parent_id.as_deref()
                        && app.notes.set_folder_expanded(parent_id, true)
                    {
                        app.persist_notes_ui_state(cx);
                    }
                    if let Some((id, revision)) = note_version {
                        cx.emit(NotesCatalogEvent::NoteUpserted { id, revision });
                    }
                    cx.notify();
                }
                Err(error) => {
                    app.shell.set_status(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn save_note_document(
        &mut self,
        note: NoteDocument,
        force: bool,
        apply: impl FnOnce(&mut Self, Result<NoteDocument, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let id = note.id.clone();
        let title = note.title.clone();
        let markdown = note.markdown.clone();
        let revision = note.revision;
        self.submit_store_request(
            0,
            store_request(StoreDomain::Notes, move |store| {
                store.update_note(&id, title, markdown, revision, force)
            }),
            move |app, event, cx| match event.outcome {
                Ok(result) => {
                    let id = result.note.id.clone();
                    let revision = result.note.revision;
                    app.notes.upsert_note(NoteSummary::from(&result.note));
                    apply(app, Ok(result.note), cx);
                    cx.emit(NotesCatalogEvent::NoteUpserted { id, revision });
                    cx.notify();
                }
                Err(error) => {
                    apply(app, Err(error.to_string()), cx);
                    cx.notify();
                }
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{AppContext as _, Context, Entity, Subscription, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::{NyaTermApp, notes::NotesCatalogEvent};
    use crate::models::NavItem;

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "nyaterm-notes-runtime-{}-{}",
            std::process::id(),
            uuid()
        ))
    }

    fn test_app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        let root = unique_test_dir();
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    struct NotesEventSink {
        events: Vec<NotesCatalogEvent>,
        _subscription: Subscription,
    }

    impl NotesEventSink {
        fn new(app: &Entity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
            let subscription = cx.subscribe(app, |this, _, event: &NotesCatalogEvent, _| {
                this.events.push(event.clone());
            });
            Self {
                events: Vec::new(),
                _subscription: subscription,
            }
        }
    }

    #[test]
    fn notes_catalog_event_is_delivered_after_app_update_without_reentry() {
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx);
        let sink = cx.new(|cx| NotesEventSink::new(&app, cx));

        cx.update_entity(&app, |_, cx| {
            cx.emit(NotesCatalogEvent::NoteUpserted {
                id: "note-1".to_string(),
                revision: 2,
            });
            cx.notify();
        });

        assert_eq!(
            cx.read_entity(&sink, |sink, _| sink.events.clone()),
            vec![NotesCatalogEvent::NoteUpserted {
                id: "note-1".to_string(),
                revision: 2,
            }]
        );
    }

    #[test]
    fn restored_visible_notes_panel_starts_lazy_load() {
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            if app.shell.panel_multi_open() {
                app.toggle_panel_multi_open(cx);
            }
            app.ensure_panel_open(NavItem::Notes);

            app.ensure_visible_notes_loaded(cx);

            assert!(app.notes.loading());
        });
    }

    #[test]
    fn collapsed_restored_notes_panel_stays_lazy() {
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            if app.shell.panel_multi_open() {
                app.toggle_panel_multi_open(cx);
            }
            app.ensure_panel_open(NavItem::Notes);
            app.toggle_left_sidebar(cx);

            app.ensure_visible_notes_loaded(cx);

            assert!(!app.notes.loading());
            assert!(!app.notes.loaded());
        });
    }

    #[test]
    fn restored_multi_open_notes_stack_starts_lazy_load() {
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            if !app.shell.panel_multi_open() {
                app.toggle_panel_multi_open(cx);
            }
            app.open_or_toggle_panel(NavItem::Notes, cx);

            app.ensure_visible_notes_loaded(cx);

            assert!(app.notes.loading());
        });
    }
}
