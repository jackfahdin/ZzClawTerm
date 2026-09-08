use rust_i18n::t;

use futures::StreamExt as _;
use gpui::{ClipboardItem, Context, IntoElement, SharedString, Window, div, prelude::*, px, rgb};
use nyaterm_core::TranslationSettings;
use nyaterm_store::{StoreDomain, store_request};
use nyaterm_ui::NyaDialogWindowExt as _;
use nyaterm_ui::NyaScrollable;

use crate::features::NyaTermApp;
use crate::http::translation::translate_text;
use crate::models::TranslateInputField;
use crate::widgets::small_button;

use super::state::TranslateJobResult;

impl NyaTermApp {
    pub(in crate::features) fn run_translation(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.translation.begin_run() else {
            cx.notify();
            return;
        };
        let (tx, provider, target_language, text, settings) = request.into_parts();
        let rejected_tx = tx.clone();
        if let Err(error) = self.blocking_jobs.submit_detached("translation", move |_| {
            let result = translate_text(&provider, &text, &target_language, &settings);
            let _ = tx.unbounded_send(TranslateJobResult::new(result));
        }) {
            let _ = rejected_tx.unbounded_send(TranslateJobResult::new(Err(error.to_string())));
        }
        cx.notify();
    }

    pub(in crate::features) fn save_translation_settings(&mut self, cx: &mut Context<Self>) {
        let next = self.translation.pending_settings();
        if self.defer_settings_persistence(cx) {
            self.translation.settings_staged(next);
            return;
        }
        if let Some((generation, snapshot)) = self.translation.queue_settings_persistence() {
            self.submit_translation_settings_save(generation, snapshot, cx);
        }
        cx.notify();
    }

    fn submit_translation_settings_save(
        &mut self,
        generation: u64,
        snapshot: TranslationSettings,
        cx: &mut Context<Self>,
    ) {
        let request = store_request(StoreDomain::Settings, move |store| {
            store.save_translation_settings(snapshot)
        });
        let task = match self.store_ui.try_submit(generation, request) {
            Ok(task) => task,
            Err(error) => {
                self.translation
                    .finish_settings_persistence(generation, false);
                self.translation.settings_save_failed(error);
                self.settings
                    .update_store_status(self.translation.status().to_string(), false);
                cx.notify();
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let event = task.await;
            let _ = this.update(cx, |this, cx| {
                let mut completion = this
                    .translation
                    .finish_settings_persistence(event.generation, event.outcome.is_ok());
                if completion.apply_result()
                    && let Ok(saved) = event.outcome.as_ref()
                {
                    this.translation.settings_saved(saved.clone());
                }
                if completion.report_result() {
                    if let Err(error) = event.outcome {
                        this.translation.settings_save_failed(error);
                    }
                    this.settings.update_store_status(
                        this.translation.status().to_string(),
                        completion.apply_result(),
                    );
                }
                if let Some((generation, snapshot)) = completion.take_next() {
                    this.submit_translation_settings_save(generation, snapshot, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::features) fn clear_translation_secret(
        &mut self,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.translation.clear_secret(provider);
        cx.notify();
    }

    /// Apply an edit from one of the translation inputs.
    pub(in crate::features) fn apply_translate_input(
        &mut self,
        field: TranslateInputField,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.translation.edit_input(field, text);
        cx.notify();
    }

    pub(in crate::features) fn open_translation_dialog(
        &mut self,
        text: String,
        provider: String,
        provider_label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.translation.open_dialog(text, provider, provider_label) {
            cx.notify();
            return;
        }
        self.open_content_dialog(
            t!("translation.title").to_string(),
            540.,
            |app, _, cx| app.translation_dialog_content(cx).into_any_element(),
            |app, cx| {
                app.translation.close_dialog();
                cx.notify();
            },
            window,
            cx,
        );
        // Kick off immediately (Tauri TranslationDialog behavior).
        if !self.translation.is_pending() {
            self.run_translation(window, cx);
        } else {
            cx.notify();
        }
    }

    pub(in crate::features) fn close_translation_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.translation.close_dialog() {
            window.close_nya_dialog(cx);
            cx.notify();
        }
    }

    pub(in crate::features) fn translation_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(dialog) = self.translation.dialog_snapshot() else {
            return div().into_any_element();
        };
        let provider_label = dialog.provider_label.clone();
        let source = dialog.source_text.clone();
        let pending = self.translation.is_pending();
        let status = self.translation.status().to_string();
        let source_label = t!("translation.sourceText");
        let translated_label = t!("translation.translatedText");
        let loading_label = t!("translation.loading");
        let error_label = t!("translation.error");
        let copy_label = t!("translation.copy");
        let close_label = t!("translation.close");
        let copied_label = t!("translation.copied");
        let result = self.translation.result_snapshot();
        let detected = result
            .as_ref()
            .map(|item| item.detected_language.clone())
            .filter(|s| !s.trim().is_empty());
        let translated = result
            .as_ref()
            .map(|item| item.translated.clone())
            .unwrap_or_default();
        let can_copy = !translated.trim().is_empty();
        let error_detail = status
            .strip_prefix("translation failed:")
            .map(str::trim)
            .filter(|detail| !detail.is_empty());
        let detected_label = detected
            .as_ref()
            .map(|language| t!("translation.detectedLang", lang = language));

        let source_box = div()
            .id(SharedString::from("translation-dialog-source"))
            .rounded_sm()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.input))
            .p_3()
            .max_h(px(120.))
            .overflow_y_scrollbar()
            .text_sm()
            .line_height(px(20.))
            .whitespace_normal()
            .text_color(rgb(palette.text))
            .child(source.clone());

        let mut result_box = div()
            .id(SharedString::from("translation-dialog-result"))
            .rounded_sm()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.input))
            .p_3()
            .min_h(px(60.))
            .max_h(px(200.))
            .overflow_y_scrollbar()
            .text_sm()
            .line_height(px(20.))
            .whitespace_normal()
            .text_color(rgb(palette.text));
        if pending {
            result_box = result_box.child(
                div()
                    .text_color(rgb(palette.text_muted))
                    .child(loading_label),
            );
        } else if let Some(detail) = error_detail {
            result_box = result_box.child(
                div()
                    .text_color(rgb(palette.danger))
                    .child(format!("{error_label}: {detail}")),
            );
        } else if !translated.is_empty() {
            result_box = result_box.child(translated.clone());
        }

        div()
            .id(SharedString::from("translation-dialog-content"))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div().flex().items_center().gap_2().child(
                    div()
                        .px_2()
                        .py(px(2.))
                        .rounded_sm()
                        .bg(rgb(palette.surface))
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_muted))
                        .child(provider_label),
                ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .child(source_label),
            )
            .child(source_box)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .child(translated_label),
                    )
                    .when_some(detected_label, |this, label| {
                        this.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.text_dimmed))
                                .child(label),
                        )
                    }),
            )
            .child(result_box)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .when(!can_copy, |this| this.opacity(0.45))
                            .child(small_button(
                                palette,
                                "translation-dialog-copy",
                                copy_label,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(result) = this.translation.result_snapshot() {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            result.translated,
                                        ));
                                        this.translation
                                            .mark_result_copied(copied_label.to_string());
                                        cx.notify();
                                    }
                                }),
                            )),
                    )
                    .child(small_button(
                        palette,
                        "translation-dialog-close",
                        close_label,
                        cx.listener(|this, _, window, cx| {
                            this.close_translation_dialog(window, cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    /// Deliver translation results as they arrive.
    ///
    /// Started once at window open. Before this the runtime tick polled
    /// `rx.try_recv`, which meant a result waited for the next tick and forced
    /// `runtime_quiet_tick_allowed` to carry a `translation` term to keep that
    /// wait short.
    pub(in crate::features) fn start_translation_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut rx) = self.translation.take_event_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        if this.translation.apply_event(event) {
                            this.shell.set_status(this.translation.status().to_string());
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
