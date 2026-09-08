use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, prelude::*, px, rgb,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{SftpFileEntry, SftpFileType};

use std::collections::HashSet;

use crate::features::{
    shell::gpui_code_font_family, transfers::format_file_size, view_widgets::transfer_entry_icon,
};
use crate::models::{TransferBrowserColumnWidths, TransferRenameState};
use crate::theme::ThemePalette;

use super::{format_permissions_octal, format_sftp_modified};
use crate::features::pages::transfers::panel::TransferPanel;

pub(super) fn transfer_browser_parent_entry_row(
    palette: ThemePalette,
    column_widths: TransferBrowserColumnWidths,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    div()
        .id(SharedString::from("transfer-browser-entry-parent"))
        .h(px(30.))
        .flex()
        .items_center()
        .rounded_sm()
        .bg(gpui::rgba(0x00000000))
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)))
        .on_click(cx.listener(|panel, _: &ClickEvent, window, cx| {
            panel.with_app(cx, |this, cx| {
                this.open_transfer_parent_directory(window, cx);
            })
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, _: &MouseDownEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    this.prepare_transfer_browser_parent_context_menu(window, cx);
                })
            }),
        )
        .child(
            div()
                .min_w_0()
                .w(column_widths.name)
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .text_size(px(12.))
                .text_color(rgb(palette.text))
                .child(transfer_entry_icon(palette, "..", true, false, false))
                .child(".."),
        )
        .child(transfer_browser_text_cell(
            palette,
            column_widths.modified,
            "",
        ))
        .child(transfer_browser_text_cell(palette, column_widths.size, "-"))
        .child(transfer_browser_text_cell(
            palette,
            column_widths.permissions,
            "",
        ))
        .child(transfer_browser_text_cell(palette, column_widths.owner, ""))
        .child(transfer_browser_text_cell(palette, column_widths.group, ""))
}

fn transfer_browser_text_cell(
    palette: ThemePalette,
    width: Pixels,
    value: &'static str,
) -> impl IntoElement {
    div()
        .w(width)
        .flex_none()
        .px_2()
        .truncate()
        .text_xs()
        .text_color(rgb(palette.text_muted))
        .child(value)
}

pub(super) fn transfer_browser_entry_row(
    presentation: TransferBrowserEntryRowPresentation<'_>,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let TransferBrowserEntryRowPresentation {
        palette,
        entry,
        selected_remote_path,
        selected_remote_paths,
        column_widths,
        rename_state,
        rename_input,
    } = presentation;
    let entry_identity = entry.identity_key();
    let mouse_down_path = entry_identity.clone();
    let mouse_move_path = entry_identity.clone();
    let context_path = entry_identity.clone();
    let is_selected = selected_remote_path.as_deref() == Some(entry_identity.as_str());
    let is_marked = selected_remote_paths.contains(&entry_identity);
    let inline_rename = rename_state.filter(|state| {
        state.old_path == entry.path && state.raw_path_token == entry.raw_path_token
    });
    let is_renaming = inline_rename.is_some();
    let name_click_path = entry_identity.clone();
    let rename_double_click_path = entry_identity.clone();
    let rename_input_path = entry.path.clone();
    let mut rename_input = rename_input;
    let rename_has_error = inline_rename.as_ref().is_some_and(|state| {
        let trimmed = state.value.trim();
        trimmed.is_empty() || trimmed.contains('/') || trimmed == "." || trimmed == ".."
    });
    let is_directory = entry.is_directory();
    let is_marked_or_selected = is_selected || is_marked;
    let size_display = if is_directory {
        "-".to_string()
    } else {
        format_file_size(entry.size)
    };
    div()
        .id(SharedString::from(format!(
            "transfer-browser-entry-{entry_identity}"
        )))
        .h(px(30.))
        .flex()
        .items_center()
        .bg(if is_marked_or_selected {
            gpui::rgba((palette.primary << 8) | 0x1a)
        } else {
            gpui::rgba(0x00000000)
        })
        .cursor_pointer()
        .when(!is_marked_or_selected, |this| {
            this.hover(|this| this.bg(rgb(palette.hover)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    if !is_renaming {
                        this.handle_transfer_browser_entry_mouse_down(
                            mouse_down_path.clone(),
                            event,
                            window,
                            cx,
                        );
                    }
                })
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, _: &MouseDownEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    this.prepare_transfer_browser_entry_context_menu(
                        context_path.clone(),
                        window,
                        cx,
                    );
                })
            }),
        )
        .on_mouse_move(cx.listener(move |panel, event: &MouseMoveEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                if !is_renaming {
                    this.handle_transfer_browser_entry_mouse_move(
                        mouse_move_path.clone(),
                        event,
                        cx,
                    );
                }
            })
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|panel, event: &MouseUpEvent, _, cx| {
                panel.with_app(cx, |this, cx| {
                    this.finish_transfer_browser_selection_drag(event, cx);
                })
            }),
        )
        .on_click(cx.listener(move |panel, event: &ClickEvent, window, cx| {
            panel.with_app(cx, |this, cx| {
                if !is_renaming {
                    this.select_transfer_browser_entry_from_click(
                        entry_identity.clone(),
                        event,
                        window,
                        cx,
                    );
                }
            })
        }))
        .child(
            div()
                .min_w_0()
                .w(column_widths.name)
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .text_size(px(12.))
                .text_color(if is_marked_or_selected {
                    rgb(palette.primary)
                } else {
                    rgb(palette.text)
                })
                .child(transfer_entry_icon(
                    palette,
                    &entry.name,
                    is_directory,
                    entry.file_type == SftpFileType::Symlink,
                    is_marked_or_selected,
                ))
                .when(is_renaming, |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!(
                                "transfer-browser-rename-input-{rename_input_path}"
                            )))
                            .min_w_0()
                            .flex_1()
                            .font_family(gpui_code_font_family())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                                    panel.with_app(cx, |this, cx| {
                                        if event.click_count >= 2 && !event.modifiers.modified() {
                                            this.open_transfer_browser_entry_from_double_click(
                                                rename_double_click_path.clone(),
                                                window,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    })
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|panel, _, _, cx| {
                                    panel.with_app(cx, |this, cx| {
                                        this.suppress_transfer_browser_context_menu(cx);
                                        cx.stop_propagation();
                                    })
                                }),
                            )
                            .on_click(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // A name that cannot be saved reddens the box.
                            .when(rename_has_error, |this| {
                                this.rounded_sm().border_1().border_color(rgb(0x7f1d1d))
                            })
                            // Escape and Enter belong to the row, which the box
                            // leaves unconsumed.
                            .on_key_down(cx.listener(|panel, event: &KeyDownEvent, window, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.handle_transfer_rename_key_down(event, window, cx);
                                })
                            }))
                            .children(rename_input.take()),
                    )
                })
                .when(!is_renaming, |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!(
                                "transfer-browser-entry-name-{name_click_path}"
                            )))
                            .min_w_0()
                            .flex_1()
                            .on_click(cx.listener(move |panel, event: &ClickEvent, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.schedule_transfer_browser_name_rename(
                                        name_click_path.clone(),
                                        event,
                                        cx,
                                    );
                                })
                            }))
                            .truncate()
                            .child(truncate_preview(&entry.name, 42)),
                    )
                }),
        )
        .child(
            div()
                .w(column_widths.modified)
                .flex_none()
                .px_2()
                .truncate()
                .text_xs()
                .font_family(gpui_code_font_family())
                .text_color(rgb(palette.text_muted))
                .child(format_sftp_modified(entry.modified_at)),
        )
        .child(
            div()
                .w(column_widths.size)
                .flex_none()
                .px_2()
                .truncate()
                .text_right()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(size_display),
        )
        .child(
            div()
                .w(column_widths.permissions)
                .flex_none()
                .px_2()
                .truncate()
                .text_xs()
                .font_family(gpui_code_font_family())
                .text_color(rgb(palette.text_muted))
                .child(
                    entry
                        .permissions
                        .map(format_permissions_octal)
                        .unwrap_or_else(|| "-".to_string()),
                ),
        )
        .child(
            div()
                .w(column_widths.owner)
                .flex_none()
                .px_2()
                .truncate()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(if entry.owner.is_empty() {
                    "-".to_string()
                } else {
                    entry.owner.clone()
                }),
        )
        .child(
            div()
                .w(column_widths.group)
                .flex_none()
                .px_2()
                .truncate()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(if entry.group.is_empty() {
                    "-".to_string()
                } else {
                    entry.group.clone()
                }),
        )
}

pub(super) struct TransferBrowserEntryRowPresentation<'a> {
    pub palette: ThemePalette,
    pub entry: SftpFileEntry,
    pub selected_remote_path: Option<String>,
    pub selected_remote_paths: &'a HashSet<String>,
    pub column_widths: TransferBrowserColumnWidths,
    pub rename_state: Option<TransferRenameState>,
    pub rename_input: Option<AnyElement>,
}
