use gpui::{Context, IntoElement, div, prelude::*, rgb};
use rust_i18n::t;
use zzclawterm_ui::ZzClawIconButton;

use crate::features::ZzClawTermApp;
use crate::features::pages::connections::list::{ConnectionEditorFields, editor_field};
use crate::models::{ConnectionEditorField, ConnectionEditorState};
use crate::theme::ThemePalette;

pub(super) fn connection_tags_field(
    palette: ThemePalette,
    editor: &ConnectionEditorState,
    fields: &ConnectionEditorFields,
    cx: &mut Context<ZzClawTermApp>,
) -> impl IntoElement {
    let mut tags = div().flex().flex_wrap().gap_2();
    for (index, tag) in editor.tags.iter().enumerate() {
        let remove = tag.clone();
        tags = tags.child(
            div()
                .max_w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(rgb(palette.text))
                        .truncate()
                        .child(tag.clone()),
                )
                .child(
                    ZzClawIconButton::new(
                        format!("connection-remove-tag-{index}"),
                        "icons/close.svg",
                    )
                    .tooltip(t!("dialog.removeTag", tag = tag))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.remove_connection_editor_tag(&remove, cx)
                    })),
                ),
        );
    }
    let tag = editor.new_tag.trim();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_end()
                .gap_2()
                .child(div().min_w_0().flex_1().child(editor_field(
                    palette,
                    t!("dialog.tags"),
                    ConnectionEditorField::NewTag,
                    fields,
                    cx,
                )))
                .child(
                    ZzClawIconButton::new("connection-add-tag", "icons/plus.svg")
                        .tooltip(t!("dialog.addTag"))
                        .disabled(
                            tag.is_empty() || editor.tags.iter().any(|existing| existing == tag),
                        )
                        .on_click(cx.listener(|app, _, _, cx| app.add_connection_editor_tag(cx))),
                ),
        )
        .child(tags)
}
