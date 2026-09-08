use rust_i18n::t;

use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
    rgb, rgba, svg,
};

use crate::features::{
    NyaTermApp, view_widgets::color_icon, view_widgets::mono_icon, view_widgets::nyaterm_app_icon,
};
use crate::models::ConnectionImportSource;
use crate::theme::ThemePalette;

impl NyaTermApp {
    pub(in crate::features) fn connection_import_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let narrow = self.shell.viewport_size().0 < 520.;
        let docs_url = if self
            .settings
            .summary()
            .language
            .to_ascii_lowercase()
            .starts_with("zh")
        {
            "https://nyaterm.app/docs/guide/ssh-connection#%E5%AF%BC%E5%85%A5%E5%85%B6%E4%BB%96%E5%AE%A2%E6%88%B7%E7%AB%AF%E7%9A%84%E4%BC%9A%E8%AF%9D"
        } else {
            "https://nyaterm.app/docs/guide/ssh-connection#import-sessions-from-other-clients"
        };

        div()
            .id("connection-import-content")
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("savedConnections.importSelectSource")),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(if narrow { 2 } else { 3 })
                    .gap_3()
                    .child(import_source_card(
                        palette,
                        "connection-import-nyaterm",
                        "nyaterm",
                        "NyaTerm",
                        ".nya",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::NyatermBackup,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-xshell",
                        "color/brand/xshell.png",
                        "Xshell",
                        ".xts",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::Xshell,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-mobaxterm",
                        "color/brand/mobaxterm.png",
                        "MobaXterm",
                        ".mxtsessions",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::MobaXterm,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-windterm",
                        "color/brand/windterm.png",
                        "WindTerm",
                        ".sessions",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::WindTerm,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-securecrt",
                        "color/brand/securecrt.png",
                        "SecureCRT",
                        ".xml",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::SecureCrt,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-finalshell",
                        "color/brand/finalshell.png",
                        "FinalShell",
                        "conn directory",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::FinalShell,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-termius",
                        "color/brand/termius.png",
                        "Termius",
                        "local IndexedDB",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::Termius,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-electerm",
                        "color/brand/electerm.png",
                        "Electerm",
                        ".json",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::Electerm,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "connection-import-json",
                        "icons/files.svg",
                        "JSON",
                        ".json",
                        cx.listener(|this, _, window, cx| {
                            this.select_connection_import_source(
                                ConnectionImportSource::NyatermJson,
                                window,
                                cx,
                            );
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                svg()
                                    .size(px(14.))
                                    .flex_none()
                                    .path("icons/conn/terminal.svg")
                                    .text_color(rgb(palette.text_muted)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .line_height(px(16.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(t!("savedConnections.importMergeHint")),
                            ),
                    )
                    .child(import_docs_link(
                        palette,
                        "connection-import-docs",
                        t!("savedConnections.importDocs"),
                        cx.listener(move |this, _, _, cx| {
                            this.open_external_url_for_ui(docs_url, cx);
                        }),
                    )),
            )
    }
}

/// One vendor tile in an import dialog. Shared with the quick command importer:
/// both are Tauri's `min-h-32` card with a 40px logo, a name, and an extension hint.
pub(in crate::features::panels) fn import_source_card(
    palette: ThemePalette,
    id: &'static str,
    icon: &'static str,
    label: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let hint: SharedString = hint.into();
    let hover = rgba((palette.primary << 8) | 0x14);
    div()
        .id(id)
        .min_h(px(128.))
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .p_3()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .text_center()
        .cursor_pointer()
        .hover(move |this| this.border_color(rgb(palette.primary)).bg(hover))
        .on_click(on_click)
        .child(if icon == "nyaterm" {
            nyaterm_app_icon(palette, 40.).into_any_element()
        } else if icon.starts_with("color/") {
            // Vendor logos are full-color rasters; they cannot go through svg().
            color_icon(icon, 40.).into_any_element()
        } else {
            // Tauri tints every non-logo tile icon with `--df-primary` in both import
            // dialogs, so the shared card does the same.
            mono_icon(icon, rgb(palette.primary).into(), 40.).into_any_element()
        })
        .child(
            div()
                .max_w_full()
                .text_xs()
                .font_weight(FontWeight(700.))
                .text_color(rgb(palette.text))
                .child(label),
        )
        .child(
            div()
                .max_w_full()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(hint),
        )
}

/// Tauri renders the docs affordance as a primary-colored link with an
/// external-link glyph, not as a secondary button.
pub(in crate::features::panels) fn import_docs_link(
    palette: ThemePalette,
    id: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .id(id)
        .h(px(28.))
        .px_2()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .rounded_sm()
        .text_size(px(11.))
        .text_color(rgb(palette.link))
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)))
        .child(label)
        .child(
            svg()
                .size(px(12.))
                .flex_none()
                .path("icons/menu/export.svg")
                .text_color(rgb(palette.link)),
        )
        .on_click(on_click)
}
