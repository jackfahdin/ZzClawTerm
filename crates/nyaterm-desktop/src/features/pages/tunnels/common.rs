use rust_i18n::t;

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{App, ClickEvent, Context, IntoElement, Window, div, px, rgb};
use nyaterm_ui::{NyaDropdownMenu, NyaMenuItem};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::NetworkGroupEditorState;

pub(super) struct NetworkItemMenuConfig {
    pub(super) palette: crate::theme::ThemePalette,
    pub(super) id: String,
    pub(super) more_label: Cow<'static, str>,
    pub(super) edit_label: Cow<'static, str>,
    pub(super) move_label: Cow<'static, str>,
    pub(super) delete_label: Cow<'static, str>,
    pub(super) can_move: bool,
}

pub(super) fn network_item_overflow_menu(
    config: NetworkItemMenuConfig,
    on_edit: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_move: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_delete: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let NetworkItemMenuConfig {
        id,
        more_label,
        edit_label,
        move_label,
        delete_label,
        can_move,
        ..
    } = config;
    let mut items = vec![
        NyaMenuItem::action(edit_label)
            .icon("icons/net/edit.svg")
            .on_click(on_edit),
    ];
    if can_move {
        items.push(
            NyaMenuItem::action(move_label)
                .icon("icons/net/move.svg")
                .on_click(on_move),
        );
    }
    items.extend([
        NyaMenuItem::separator(),
        NyaMenuItem::action(delete_label)
            .icon("icons/net/delete.svg")
            .danger()
            .on_click(on_delete),
    ]);

    NyaDropdownMenu::new(id)
        .icon("icons/session/more.svg")
        .icon_size(px(14.))
        .tooltip(more_label)
        .min_width(px(164.))
        .items(items)
}

pub(super) fn network_group_editor_content(
    app: &mut NyaTermApp,
    editor: NetworkGroupEditorState,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let palette = app.theme_palette();
    let name_input = app
        .text_input_field(
            "network.group-editor.name",
            t!("network.groupName"),
            &editor.name,
            TextInputSetup::default(),
            cx,
        )
        .into_any_element();
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(palette.text_muted))
                .child(t!("network.groupDialogDescription")),
        )
        .child(name_input)
        .when_some(editor.error.clone(), |this, error| {
            this.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xfda4af))
                    .child(error),
            )
        })
}
