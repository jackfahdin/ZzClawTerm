use rust_i18n::t;

use gpui::{AnyElement, Context, FontWeight, IntoElement, Window, div, prelude::*, px, rgb};
use zzclawterm_core::runtime::RuntimeMode;
use zzclawterm_ui::{ZzClawButton, ZzClawButtonVariant, ZzClawDialogWindowExt as _};

use crate::features::ZzClawTermApp;
use crate::features::shell::gpui_code_font_family;
use crate::features::view_widgets::zzclawterm_app_icon;

impl ZzClawTermApp {
    pub(in crate::features) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_nya_dialog(cx) {
            return;
        }
        self.open_content_dialog(
            String::new(),
            440.,
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
        let runtime_label = match self.runtime.mode() {
            RuntimeMode::Portable => t!("about.portable"),
            RuntimeMode::Installed => t!("about.installed"),
        };
        let support_rows = [
            (t!("about.version"), env!("CARGO_PKG_VERSION").to_string()),
            (
                t!("about.operatingSystem"),
                std::env::consts::OS.to_string(),
            ),
            (t!("about.architecture"), std::env::consts::ARCH.to_string()),
            (t!("about.runtime"), runtime_label.to_string()),
        ];
        div()
            .id("about-dialog-content")
            .debug_selector(|| "about-dialog-content".to_string())
            .w_full()
            .p_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(zzclawterm_app_icon(palette, 96.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(
                                zzclawterm_core::app_identity::AppFlavor::current().display_name(),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
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
                    .w_full()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight(500.))
                                    .text_color(rgb(palette.text))
                                    .child(t!("about.supportInfo")),
                            )
                            .child(
                                ZzClawButton::new(
                                    "about-copy-support",
                                    t!("about.copySupportInfo"),
                                )
                                .icon("icons/copy.svg")
                                .small()
                                .variant(ZzClawButtonVariant::Ghost)
                                .tooltip(t!("about.copySupportInfo"))
                                .on_click(cx.listener(
                                    move |_, _, window, cx| {
                                        use zzclawterm_ui::notification::{
                                            ZzClawNotificationKind,
                                            ZzClawNotificationWindowExt as _,
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
                                    },
                                )),
                            ),
                    )
                    .child(div().flex().flex_col().gap_1p5().children(
                        support_rows.into_iter().map(|(label, value)| {
                            div()
                                .w_full()
                                .flex()
                                .items_start()
                                .gap_4()
                                .text_xs()
                                .line_height(px(18.))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(rgb(palette.text_muted))
                                        .child(label),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_right()
                                        .font_family(gpui_code_font_family())
                                        .text_color(rgb(palette.text))
                                        .child(value),
                                )
                        }),
                    )),
            )
            .child(
                div()
                    .pt_2()
                    .w_full()
                    .flex()
                    .gap_3()
                    .child(
                        div().flex_1().min_w_0().child(
                            ZzClawButton::new("about-website", t!("about.website"))
                                .small()
                                .full_width()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_external_url_for_ui(
                                        "https://github.com/jackfahdin/ZzClawTerm",
                                        cx,
                                    );
                                })),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            ZzClawButton::new("about-issues", t!("about.issues"))
                                .small()
                                .full_width()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_external_url_for_ui(
                                        "https://github.com/jackfahdin/ZzClawTerm/issues",
                                        cx,
                                    );
                                })),
                        ),
                    ),
            )
            .into_any_element()
    }
}
