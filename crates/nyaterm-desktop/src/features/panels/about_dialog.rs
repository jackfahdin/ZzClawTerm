use rust_i18n::t;

use gpui::{AnyElement, Context, FontWeight, IntoElement, Window, div, prelude::*, px, rgb};
use nyaterm_ui::{NyaButton, NyaDialogWindowExt as _};

use crate::features::{NyaTermApp, view_widgets::nyaterm_app_icon};

impl NyaTermApp {
    pub(in crate::features) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_nya_dialog(cx) {
            return;
        }
        self.open_content_dialog(
            format!("{} NyaTerm", t!("menu.about")),
            360.,
            |app, _, cx| app.about_dialog_content(cx),
            |_, _| {},
            window,
            cx,
        );
    }

    fn about_dialog_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.theme_palette();
        div()
            .id("about-dialog-content")
            .debug_selector(|| "about-dialog-content".to_string())
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(nyaterm_app_icon(palette, 96.))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight(700.))
                    .text_color(rgb(palette.text))
                    .child("NyaTerm"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_dimmed))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .px_3()
                    .text_xs()
                    .line_height(px(18.))
                    .text_center()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("about.description")),
            )
            .child(
                div()
                    .mt_2()
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap_3()
                    .child(
                        NyaButton::new("about-website", t!("about.website")).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.open_external_url_for_ui("https://nyaterm.app", cx);
                            },
                        )),
                    )
                    .child(NyaButton::new("about-issues", t!("about.issues")).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.open_external_url_for_ui(
                                "https://github.com/nyakang/nyaterm/issues",
                                cx,
                            );
                        }),
                    )),
            )
            .into_any_element()
    }
}
