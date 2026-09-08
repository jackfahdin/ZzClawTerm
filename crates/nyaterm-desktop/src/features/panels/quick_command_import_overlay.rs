use rust_i18n::t;

use gpui::{Context, IntoElement, div, prelude::*, px, rgb, svg};

use super::connection_import_overlay::{import_docs_link, import_source_card};
use crate::features::NyaTermApp;
use crate::models::QuickCommandImportPathPromptKind;

impl NyaTermApp {
    pub(in crate::features) fn quick_command_import_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let docs_url = if self
            .settings
            .summary()
            .language
            .to_ascii_lowercase()
            .starts_with("zh")
        {
            "https://nyaterm.app/docs/guide/quick-commands#%E5%AF%BC%E5%85%A5%E5%BF%AB%E6%8D%B7%E5%91%BD%E4%BB%A4"
        } else {
            "https://nyaterm.app/docs/guide/quick-commands#import-quick-commands"
        };
        div()
            .id("quick-command-import-content")
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("quickCommands.importSelectSource")),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_3()
                    // Real vendor logos, as in Tauri. They are full-color rasters, so
                    // the shared card routes them through `color_icon` / `img()`.
                    .child(import_source_card(
                        palette,
                        "quick-command-import-windterm-card",
                        "color/brand/windterm.png",
                        t!("quickCommands.importWindTerm"),
                        t!("quickCommands.importWindTermHint"),
                        cx.listener(|this, _, window, cx| {
                            this.select_quick_command_import_source(
                                QuickCommandImportPathPromptKind::WindTermQuickbar,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "quick-command-import-xshell-card",
                        "color/brand/xshell.png",
                        t!("quickCommands.importXshell"),
                        t!("quickCommands.importXshellHint"),
                        cx.listener(|this, _, window, cx| {
                            this.select_quick_command_import_source(
                                QuickCommandImportPathPromptKind::XshellXts,
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(import_source_card(
                        palette,
                        "quick-command-import-json-card",
                        "icons/file/data-object.svg",
                        t!("quickCommands.importNyaTermJson"),
                        t!("quickCommands.importNyaTermJsonHint"),
                        cx.listener(|this, _, window, cx| {
                            this.select_quick_command_import_source(
                                QuickCommandImportPathPromptKind::NyatermJson,
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
                                    .child(t!("quickCommands.importMergeHint")),
                            ),
                    )
                    .child(import_docs_link(
                        palette,
                        "quick-command-import-docs",
                        t!("quickCommands.importDocs"),
                        cx.listener(move |this, _, _, cx| {
                            this.open_external_url_for_ui(docs_url, cx);
                        }),
                    )),
            )
    }
}
