use rust_i18n::t;

use gpui::{Context, IntoElement, Window};
use zzclawterm_core::updater::UpdateSource;
use zzclawterm_ui::ZzClawDialogWindowExt as _;

use super::UpdateCheckKind;
use crate::features::ZzClawTermApp;

impl ZzClawTermApp {
    pub(in crate::features) fn open_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_nya_dialog(cx) {
            cx.notify();
            return;
        }
        self.open_content_dialog(
            t!("updater.checking").to_string(),
            560.,
            |app, _, cx| app.update_dialog_content(cx).into_any_element(),
            |_, _| {},
            window,
            cx,
        );
        let should_check = !matches!(
            self.update.read(cx).phase(),
            super::UpdatePhase::Available
                | super::UpdatePhase::Downloading { .. }
                | super::UpdatePhase::Ready
                | super::UpdatePhase::Checking
        );
        if should_check {
            self.start_update_check(cx);
        }
    }

    pub(in crate::features) fn close_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_nya_dialog(cx);
        cx.notify();
    }

    pub(in crate::features) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.desktop_controller.clone() {
            let _ = controller.update(cx, |controller, cx| {
                controller.start_update_check(UpdateCheckKind::Manual, cx)
            });
        }
        cx.notify();
    }

    pub(in crate::features) fn set_update_source(
        &mut self,
        source: UpdateSource,
        cx: &mut Context<Self>,
    ) {
        let changed = self.update.update(cx, |update, cx| {
            let changed = update.set_source(source);
            if changed {
                cx.notify();
            }
            changed
        });
        if changed {
            self.start_update_check(cx);
        }
    }
}
