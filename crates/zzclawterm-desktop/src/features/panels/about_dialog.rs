use rust_i18n::t;

use gpui::{AnyElement, Context, FontWeight, IntoElement, Window, div, prelude::*, px, rgb};
use zzclawterm_ui::{ZzClawButton, ZzClawDialogWindowExt as _};

use crate::features::{ZzClawTermApp, view_widgets::zzclawterm_app_icon};

impl ZzClawTermApp {
    pub(in crate::features) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_nya_dialog(cx) {
            return;
        }
        self.open_content_dialog(
            format!("{} ZzClawTerm", t!("menu.about")),
            360.,
            |app, _, cx| app.about_dialog_content(cx),
            |_, _| {},
            window,
            cx,
        );
    }

    fn about_dialog_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.theme_palette();
        let support_info = format!(
            "ZzClawTerm {}\nOS: {}\nArchitecture: {}\nMode: {:?}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            self.runtime.mode()
        );
        div()
            .id("about-dialog-content")
            .debug_selector(|| "about-dialog-content".to_string())
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(zzclawterm_app_icon(palette, 96.))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight(700.))
                    .text_color(rgb(palette.text))
                    .child("ZzClawTerm"),
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
            .child(div().text_xs().child(support_info.clone()))
            .child(
                ZzClawButton::new("about-copy-support", t!("about.copySupportInfo")).on_click(
                    cx.listener(move |_, _, window, cx| {
                        use zzclawterm_ui::notification::{
                            ZzClawNotificationKind, ZzClawNotificationWindowExt as _,
                        };
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            support_info.clone(),
                        ));
                        window.notify_operation(
                            "support-info-copied",
                            ZzClawNotificationKind::Success,
                            t!("about.supportInfoCopied"),
                            cx,
                        );
                    }),
                ),
            )
            .child(
                div()
                    .mt_2()
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap_3()
                    .child(
                        ZzClawButton::new("about-website", t!("about.website")).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.open_external_url_for_ui(
                                    "https://github.com/jackfahdin/ZzClawTerm",
                                    cx,
                                );
                            }),
                        ),
                    )
                    .child(
                        ZzClawButton::new("about-issues", t!("about.issues")).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.open_external_url_for_ui(
                                    "https://github.com/jackfahdin/ZzClawTerm/issues",
                                    cx,
                                );
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }
}
