use rust_i18n::t;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb, rgba, svg};

use crate::features::ZzClawTermApp;

impl ZzClawTermApp {
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
    ) -> gpui::AnyElement {
        let Some(field) = self.existing_text_input("security.unlock.password") else {
            return div().into_any_element();
        };
        let busy = self.security.unlock_busy();
        field.update(cx, |field, cx| field.set_disabled(busy, cx));
        let palette = self.theme_palette();
        div()
            .debug_selector(|| "security-unlock-dialog-content".to_string())
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("secretUnlock.unlockDescription")),
            )
            .child(zzclawterm_ui::ZzClawInputShell::new(
                "security-unlock-input",
                &field,
            ))
            .when_some(
                self.security.unlock_error().map(str::to_owned),
                |this, error| {
                    this.child(div().text_xs().text_color(rgb(palette.danger)).child(error))
                },
            )
            .into_any_element()
    }
}
