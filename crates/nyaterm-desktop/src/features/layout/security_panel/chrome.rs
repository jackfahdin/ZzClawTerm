use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb, rgba,
    svg,
};
use nyaterm_ui::NyaInput;

use crate::features::view_widgets::full_window_input_layer;
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::widgets::small_button;

impl NyaTermApp {
    pub(in crate::features) fn security_secret_footer(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let unlocked =
            self.settings.summary().has_master_password && self.security.secrets_unlocked();
        let palette = self.theme_palette();
        div()
            .id(SharedString::from("security-secrets-toggle"))
            .h(px(40.))
            .flex_none()
            .border_t_1()
            .border_color(rgb(palette.border))
            .bg(rgba((palette.primary << 8) | 0x1a))
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .cursor_pointer()
            .hover(move |this| this.bg(rgba((palette.primary << 8) | 0x26)))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(24.))
                            .flex_none()
                            .rounded_md()
                            .border_1()
                            .border_color(rgba((palette.primary << 8) | 0x40))
                            .bg(self.shell_surface_color(palette.bg))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .size(px(14.))
                                    .path("icons/lock.svg")
                                    .text_color(rgb(palette.primary)),
                            ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_xs()
                            .text_color(rgb(palette.text))
                            .child(t!(if unlocked {
                                "secretUnlock.unlockedTitle"
                            } else {
                                "secretUnlock.lockedTitle"
                            })),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.primary))
                    .child(t!(if unlocked {
                        "secretUnlock.lockAction"
                    } else {
                        "secretUnlock.unlockAction"
                    })),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.security_secrets_locked() {
                    this.open_security_unlock_prompt(window, cx);
                } else if this.settings.summary().has_master_password {
                    this.lock_security_secrets(window, cx);
                } else {
                    this.open_security_unlock_prompt(window, cx);
                }
            }))
    }

    pub(in crate::features) fn security_unlock_prompt(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let unlock_draft = self.security.unlock_draft().to_string();
        let password_input = self.text_input(
            "security.unlock.password",
            &unlock_draft,
            TextInputSetup::masked(),
            cx,
        );
        let password_focus = password_input.read(cx).focus_handle();
        full_window_input_layer("security-unlock-input-layer")
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x0d1117cc))
            .child(
                div()
                    .w(px(280.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface))
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .track_focus(self.security.unlock_focus())
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_security_unlock_key_down(event, window, cx);
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight(800.))
                            .text_color(rgb(palette.text))
                            .child(t!("secretUnlock.unlockTitle")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(t!("secretUnlock.unlockDescription")),
                    )
                    .child(
                        div()
                            .id(SharedString::from("security-unlock-input"))
                            .relative()
                            .h(px(32.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(palette.link))
                            .bg(rgb(palette.input))
                            .font_family(crate::features::shell::gpui_code_font_family())
                            .text_xs()
                            .text_color(rgb(palette.text))
                            .cursor_text()
                            .on_click(move |_, window, cx| {
                                window.focus(&password_focus, cx);
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(NyaInput::new(&password_input)),
                            ),
                    )
                    .when_some(
                        self.security.unlock_error().map(str::to_string),
                        |this, error| {
                            this.child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.danger))
                                    .child(error),
                            )
                        },
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(small_button(
                                palette,
                                "security-unlock-cancel",
                                t!("common.cancel"),
                                cx.listener(|this, _, _, cx| {
                                    this.cancel_security_unlock_prompt(cx);
                                }),
                            ))
                            .child(small_button(
                                palette,
                                "security-unlock-submit",
                                t!("secretUnlock.unlock"),
                                cx.listener(|this, _, window, cx| {
                                    this.submit_security_unlock(window, cx);
                                }),
                            )),
                    ),
            )
    }

    pub(in crate::features) fn security_master_required_prompt(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        full_window_input_layer("security-master-required-input-layer")
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x0d1117cc))
            .child(
                div()
                    .w(px(320.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface))
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight(800.))
                            .text_color(rgb(palette.text))
                            .child(t!("settings.masterPasswordRequired")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(t!("settings.masterPasswordRequiredDesc")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(small_button(
                                palette,
                                "security-master-required-cancel",
                                t!("common.cancel"),
                                cx.listener(|this, _, _, cx| {
                                    this.close_security_master_required_prompt(cx);
                                }),
                            ))
                            .child(small_button(
                                palette,
                                "security-master-required-settings",
                                t!("settings.security"),
                                cx.listener(|this, _, _, cx| {
                                    this.open_security_settings_from_prompt(cx);
                                }),
                            )),
                    ),
            )
    }
}
