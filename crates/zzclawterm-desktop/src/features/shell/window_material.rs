//! Window material resolution and surface composition shared by shell views.

use gpui::{Context, Rgba, WindowBackgroundAppearance, rgb, rgba};
use zzclawterm_core::models::window_material::WindowMaterial;

use crate::features::ZzClawTermApp;
use crate::platform::{WindowMaterialCapabilities, window_material_capabilities};

pub(in crate::features) fn resolve_window_material(
    requested: &WindowMaterial,
    capabilities: WindowMaterialCapabilities,
) -> WindowBackgroundAppearance {
    match requested {
        WindowMaterial::Transparent if capabilities.transparent => {
            WindowBackgroundAppearance::Transparent
        }
        WindowMaterial::Blurred if capabilities.blurred => WindowBackgroundAppearance::Blurred,
        _ => WindowBackgroundAppearance::Opaque,
    }
}

#[derive(Clone, Copy)]
pub(in crate::features) struct ShellBackground {
    pub native_transparent: bool,
    pub wallpaper: bool,
    pub content_opacity: u8,
    pub window_opacity: u8,
}

impl ShellBackground {
    pub fn transparent_surfaces(self) -> bool {
        self.wallpaper || self.native_transparent
    }

    pub fn surface_color(self, color: u32) -> Rgba {
        if self.transparent_surfaces() {
            Self::with_opacity(color, self.content_opacity)
        } else {
            rgb(color)
        }
    }

    pub fn terminal_color(self, color: u32) -> Rgba {
        let mut background = self.surface_color(color);
        // Preserve the legacy wallpaper shading only for opaque windows. Native
        // surfaces paint once, including connecting/empty/reconnecting states.
        if !self.native_transparent {
            background.a += (1.0 - background.a) * background.a;
        }
        background
    }

    pub fn transparent_color(self, color: u32) -> Rgba {
        if self.transparent_surfaces() {
            rgba(color << 8)
        } else {
            rgb(color)
        }
    }

    pub fn root_color(self, color: u32) -> Rgba {
        if self.native_transparent {
            Self::with_opacity(color, self.window_opacity)
        } else {
            self.transparent_color(color)
        }
    }

    fn with_opacity(color: u32, opacity: u8) -> Rgba {
        let alpha = (f32::from(opacity.min(100)) * 255. / 100.).round() as u32;
        rgba((color << 8) | alpha)
    }
}

impl ZzClawTermApp {
    pub(in crate::features) fn shell_background(&self) -> ShellBackground {
        let settings = self.settings.summary();
        ShellBackground {
            native_transparent: resolve_window_material(
                &settings.window_material,
                window_material_capabilities(),
            ) != WindowBackgroundAppearance::Opaque,
            wallpaper: self.wallpaper_enabled(),
            content_opacity: settings.background_content_opacity,
            window_opacity: settings.window_background_opacity,
        }
    }

    pub(in crate::features) fn sync_native_window_material(&self, cx: &mut Context<Self>) {
        // Load/probe capability outside rendering, including on the initial load
        // before the app has acquired its window handle.
        let appearance = resolve_window_material(
            &self.settings.summary().window_material,
            window_material_capabilities(),
        );
        let Some(handle) = self.shell.main_window() else {
            return;
        };
        // The root is leased during initial window setup and some settings
        // callbacks. Apply after those leases are released, without repaint IO.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |root, window, cx| {
                window.set_background_appearance(appearance);
                zzclawterm_ui::set_nya_root_transparent(
                    root,
                    appearance != WindowBackgroundAppearance::Opaque,
                );
                cx.notify();
                window.refresh();
            });
        });
    }

    pub(in crate::features) fn set_window_material(
        &mut self,
        material: WindowMaterial,
        cx: &mut Context<Self>,
    ) {
        if self.settings.summary().window_material == material {
            return;
        }
        self.settings.set_window_material(material);
        self.sync_native_window_material(cx);
        self.refresh_background_surfaces(cx);
        self.save_appearance_settings(cx);
    }

    pub(in crate::features) fn set_window_background_opacity(
        &mut self,
        opacity: u8,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings
            .set_window_background_opacity(opacity.min(100))
        {
            return;
        }
        self.refresh_background_surfaces(cx);
        self.save_appearance_settings(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{ShellBackground, resolve_window_material};
    use crate::platform::WindowMaterialCapabilities;
    use gpui::WindowBackgroundAppearance;
    use zzclawterm_core::models::window_material::WindowMaterial;

    #[test]
    fn unsupported_and_future_materials_fall_back_without_rewriting_preferences() {
        for supported in [false, true] {
            let caps = WindowMaterialCapabilities {
                transparent: supported,
                blurred: supported,
            };
            for (material, expected) in [
                (WindowMaterial::Opaque, WindowBackgroundAppearance::Opaque),
                (
                    WindowMaterial::Transparent,
                    WindowBackgroundAppearance::Transparent,
                ),
                (WindowMaterial::Blurred, WindowBackgroundAppearance::Blurred),
                (
                    WindowMaterial::Unknown("future".into()),
                    WindowBackgroundAppearance::Opaque,
                ),
            ] {
                assert_eq!(
                    resolve_window_material(&material, caps),
                    if supported {
                        expected
                    } else {
                        WindowBackgroundAppearance::Opaque
                    }
                );
            }
        }
    }

    #[test]
    fn native_tint_is_independent_of_single_layer_terminal_and_content_opacity() {
        for wallpaper in [false, true] {
            for opacity in [0, 20, 82, 100, 255] {
                let background = ShellBackground {
                    native_transparent: true,
                    wallpaper,
                    content_opacity: opacity,
                    window_opacity: 20,
                };
                assert_eq!(
                    background.surface_color(0x123456),
                    background.terminal_color(0x123456)
                );
                assert_eq!(background.transparent_color(0x123456).a, 0.);
                assert!((background.root_color(0x123456).a - 0.2).abs() < 0.001);
                assert!(
                    (background.surface_color(0x123456).a - f32::from(opacity.min(100)) / 100.)
                        .abs()
                        < 0.002
                );
            }
        }
    }

    #[test]
    fn opaque_windows_keep_existing_wallpaper_shading_and_solid_defaults() {
        let mut background = ShellBackground {
            native_transparent: false,
            wallpaper: false,
            content_opacity: 82,
            window_opacity: 0,
        };
        assert_eq!(background.root_color(0).a, 1.);
        assert_eq!(background.surface_color(0).a, 1.);
        background.wallpaper = true;
        assert_eq!(background.root_color(0).a, 0.);
        let alpha = background.surface_color(0).a;
        assert!((background.terminal_color(0).a - (2. * alpha - alpha * alpha)).abs() < 0.00001);
    }

    #[test]
    fn native_window_material_switches_refresh_root_and_paint_one_tint_layer() {
        use crate::features::test_support::app_with_visible_local_session;
        use crate::test_support::TestConfigDir;
        use gpui::{AppContext as _, Styled as _, TestAppContext};

        let dir = TestConfigDir::new("zzclawterm-native-window-material-paint");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app_with_visible_local_session(&mut cx, dir.path(), "material-test");
        cx.update_entity(&app, |app, cx| app.sync_component_theme(cx));
        let view = app.clone();
        let (root, vcx) =
            cx.add_window_view(move |window, cx| zzclawterm_ui::zzclaw_root(view, window, cx));
        vcx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.shell
                    .remember_main_window(window.window_handle().downcast().unwrap());
                app.begin_settings_draft(cx);
            });
        });
        for material in [
            WindowMaterial::Blurred,
            WindowMaterial::Transparent,
            WindowMaterial::Opaque,
            WindowMaterial::Blurred,
            WindowMaterial::Opaque,
        ] {
            vcx.update(|_, cx| {
                app.update(cx, |app, cx| app.set_window_material(material.clone(), cx))
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let native = app.read(cx).shell_background().native_transparent;
                root.update(cx, |root, _| {
                    assert_eq!(root.style().background.is_some(), native)
                });
                app.update(cx, |_, cx| cx.notify());
                let _ = window.draw(cx);
                if native {
                    let tint = app
                        .read(cx)
                        .shell_background()
                        .root_color(app.read(cx).theme_palette().bg);
                    assert_eq!(
                        window
                            .painted_quads()
                            .iter()
                            .filter(|quad| quad.background == gpui::Background::from(tint))
                            .count(),
                        1,
                        "the theme tint must be painted once, below all content"
                    );
                }
            });
        }
        vcx.update(|_, cx| app.update(cx, |app, cx| app.cancel_settings(cx)));
        vcx.run_until_parked();
    }
}
