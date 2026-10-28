use gpui::{Context, IntoElement, SharedString, div, prelude::*, px};
use rust_i18n::t;
use zzclawterm_core::models::window_decoration::WindowDecoration;
use zzclawterm_ui::{NYA_FORM_CONTROL_HEIGHT_PX, ZzClawSelect, ZzClawSelectOption};

use crate::features::pages::settings::panel::SettingsPanel;
use crate::features::pages::settings::{settings_form_row, settings_form_section};

impl SettingsPanel {
    pub(super) fn window_decorations_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let requested = &self.settings.summary().window_decorations;
        let selected = match requested {
            WindowDecoration::Unknown(_) => None,
            _ => Some(requested.as_str().to_owned()),
        };
        let select = self.select_entity(
            "appearance-window-decorations".into(),
            vec![
                ZzClawSelectOption::new("client", t!("settings.windowDecorationsClient")),
                ZzClawSelectOption::new("server", t!("settings.windowDecorationsServer")),
            ],
            selected,
            false,
            cx,
        );
        select.update(cx, |state, cx| {
            state.set_placeholder(t!("settings.windowDecorationsUnknown"), cx);
        });
        let palette = self.theme_palette();
        settings_form_section(
            palette,
            Some(t!("settings.windowDecorations")),
            None,
            settings_form_row(
                palette,
                t!("settings.windowDecorations"),
                Some(SharedString::from(t!("settings.windowDecorationsDesc"))),
                div()
                    .id("appearance-window-decorations-control")
                    .w_full()
                    .h(px(NYA_FORM_CONTROL_HEIGHT_PX))
                    .child(ZzClawSelect::new(&select)),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use zzclawterm_core::models::window_decoration::WindowDecoration;
    use zzclawterm_core::test_support::TestTempDir;

    #[test]
    fn building_decoration_controls_preserves_future_preferences() {
        let root = TestTempDir::new("zzclawterm-window-decoration-controls");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = crate::features::test_support::app_with_visible_local_session(
            &mut cx,
            root.path(),
            "local",
        );
        for preference in [
            WindowDecoration::Client,
            WindowDecoration::Server,
            WindowDecoration::Unknown("future".into()),
        ] {
            let panel = app.update(&mut cx, |app, cx| {
                app.settings.set_window_decorations(preference.clone());
                app.flush_settings_panel_snapshots(cx);
                app.settings_panel.clone()
            });
            panel.update(&mut cx, |panel, cx| {
                let _ = panel.window_decorations_section(cx);
            });
            cx.run_until_parked();
            app.update(&mut cx, |app, _| {
                assert_eq!(app.settings.summary().window_decorations, preference);
            });
        }
    }
}
