use gpui::{
    AnyElement, App, AppContext as _, BoxShadow, ClickEvent, Context, Entity, FontWeight,
    IntoElement, ParentElement as _, Rgba, SharedString, Styled as _, Subscription, WeakEntity,
    Window, div, prelude::*, px, rgb, rgba,
};
use rust_i18n::t;
use zzclawterm_core::themes::management::ThemeApplyTarget;
use zzclawterm_core::themes::{COLOR_FIELDS, ThemeColorField, ThemeConfig, builtins, parse_color};
use zzclawterm_ui::{
    ThemePalette, ZzClawButton, ZzClawButtonVariant, ZzClawColorPicker, ZzClawColorPickerEvent,
    ZzClawColorPickerState, ZzClawDropdownMenu, ZzClawMenuAnchor, ZzClawMenuItem,
    ZzClawScrollable as _, ZzClawTabItem, ZzClawTabs, ZzClawTabsVariant,
};

use crate::features::ZzClawTermApp;
use crate::features::pages::settings::panel::SettingsPanel;
use crate::features::settings::theme_designer::{
    MIN_TEXT_CONTRAST, ThemeDesignerState, ThemeIntent,
};

use ThemeColorField as F;

/// Every entry of `COLOR_FIELDS` appears in exactly one UI group, one terminal
/// group, or one ANSI pair.
const UI_GROUPS: &[(&str, &[ThemeColorField])] = &[
    (
        "themeDesigner.groups.surfaces",
        &[
            F::Bg,
            F::BgPanel,
            F::BgSectionHeader,
            F::BgTerminal,
            F::BgHover,
            F::BgInput,
            F::Border,
        ],
    ),
    (
        "themeDesigner.groups.text",
        &[F::Text, F::TextMuted, F::TextDimmed, F::Link],
    ),
    (
        "themeDesigner.groups.accent",
        &[
            F::Primary,
            F::PrimaryHover,
            F::OnPrimary,
            F::Accent,
            F::FocusRing,
        ],
    ),
    (
        "themeDesigner.groups.status",
        &[F::Success, F::Warning, F::Danger, F::DangerHover],
    ),
    ("themeDesigner.groups.effects", &[F::Shadow, F::ScrollThumb]),
];

const TERMINAL_GROUPS: &[(&str, &[ThemeColorField])] = &[
    (
        "themeDesigner.groups.terminalBase",
        &[
            F::TerminalBackground,
            F::TerminalForeground,
            F::TerminalForegroundIntense,
            F::TerminalCursor,
        ],
    ),
    (
        "themeDesigner.groups.terminalHighlight",
        &[
            F::TerminalSelectionBackground,
            F::TerminalLineHighlight,
            F::TerminalFindMatchBackground,
            F::TerminalFindMatchBorder,
        ],
    ),
];

/// Normal and bright variants share a row so each hue reads as one pair.
const ANSI_PAIRS: &[(ThemeColorField, ThemeColorField)] = &[
    (F::TerminalBlack, F::TerminalBrightBlack),
    (F::TerminalRed, F::TerminalBrightRed),
    (F::TerminalGreen, F::TerminalBrightGreen),
    (F::TerminalYellow, F::TerminalBrightYellow),
    (F::TerminalBlue, F::TerminalBrightBlue),
    (F::TerminalMagenta, F::TerminalBrightMagenta),
    (F::TerminalCyan, F::TerminalBrightCyan),
    (F::TerminalWhite, F::TerminalBrightWhite),
];

const NARROW_WIDTH: f32 = 850.;
const LIST_WIDTH: f32 = 248.;
const FIELD_INPUT_WIDTH: f32 = 148.;

#[derive(Default)]
pub(in crate::features::pages::settings) struct ThemePickerControls {
    fields:
        std::collections::HashMap<String, (Entity<ZzClawColorPickerState>, Subscription, String)>,
}

struct ThemeUsage {
    ui: bool,
    terminal: bool,
    terminal_follows_ui: bool,
}

impl SettingsPanel {
    /// Called from a deferred snapshot update, never from rendering.
    pub(in crate::features::pages::settings) fn sync_theme_picker_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &self.settings.theme_designer;
        let Some(theme) = state
            .draft
            .as_ref()
            .filter(|_| state.open && state.editable())
        else {
            self.theme_picker_controls.fields.clear();
            return;
        };
        let mut values = vec![("swatch".to_string(), theme.swatch.clone(), false)];
        values.extend(
            COLOR_FIELDS
                .iter()
                .map(|f| (f.path().to_string(), theme.color(*f).to_string(), f.alpha())),
        );
        let mut changed = false;
        for (path, value, alpha) in values {
            let Some(bytes) = parse_color(&value) else {
                continue;
            };
            let color = rgba(u32::from_be_bytes(bytes));
            if let Some((picker, _, previous)) = self.theme_picker_controls.fields.get_mut(&path) {
                if *previous != value {
                    picker.update(cx, |picker, cx| picker.set_value(color, window, cx));
                    *previous = value;
                    changed = true;
                }
                continue;
            }
            let picker = cx.new(|cx| ZzClawColorPickerState::new(window, cx).default_value(color));
            let field = path.clone();
            let subscription = cx.subscribe(&picker, move |panel, _, event, cx| {
                let ZzClawColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                let c = Rgba::from(*color);
                let channel = |v: f32| (v * 255.).round().clamp(0., 255.) as u8;
                let value = if alpha {
                    format!(
                        "rgba({}, {}, {}, {:.4})",
                        channel(c.r),
                        channel(c.g),
                        channel(c.b),
                        c.a
                    )
                } else {
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        channel(c.r),
                        channel(c.g),
                        channel(c.b)
                    )
                };
                panel.with_app(cx, |app, cx| {
                    app.edit_theme_designer(&field, value, cx);
                    app.ensure_theme_designer_inputs(cx);
                });
            });
            self.theme_picker_controls
                .fields
                .insert(path, (picker, subscription, value));
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// The Appearance page's entry point into the theme manager.
    pub(in crate::features::pages::settings) fn theme_designer_entry_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ZzClawButton::new("theme-designer-toggle", t!("themeDesigner.manage"))
            .icon("icons/menu/palette.svg")
            .form_control()
            .when(self.settings.theme_designer.restore_focus, |button| {
                button.autofocus()
            })
            .on_click(cx.listener(|panel, _, _, cx| {
                panel.with_app(cx, |app, cx| {
                    app.theme_designer_action(ThemeIntent::Toggle, cx)
                });
            }))
            .into_any_element()
    }

    pub(in crate::features::pages::settings) fn theme_designer_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.settings.theme_designer.clone();
        let palette = self.theme_palette();
        let narrow = self.viewport_width < NARROW_WIDTH;

        let list = self.theme_list(&state, palette, cx);
        let editor = self.theme_editor(&state, palette, narrow, cx);
        let body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(narrow, |body| body.flex_col())
            .child(
                div()
                    .flex_none()
                    .min_h_0()
                    .bg(rgb(palette.surface))
                    .border_color(rgb(palette.border))
                    .when(narrow, |column| column.h(px(168.)).w_full().border_b_1())
                    .when(!narrow, |column| {
                        column.w(px(LIST_WIDTH)).h_full().border_r_1()
                    })
                    .child(list.size_full().overflow_y_scrollbar()),
            )
            .child(editor);

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .child(self.theme_designer_header(&state, palette, cx))
            .child(body);
        if let Some(overlay) = self.theme_designer_confirmation(&state, palette, cx) {
            root = root.child(overlay);
        }
        root.into_any_element()
    }

    fn theme_designer_header(
        &self,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blocked = state.blocked();
        let source = state
            .draft
            .as_ref()
            .map(|theme| theme.id.clone())
            .unwrap_or_default();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.section_header))
            .child(
                ZzClawButton::new("theme-manager-back", t!("themeDesigner.returnAppearance"))
                    .icon("icons/menu/chevron-left.svg")
                    .variant(ZzClawButtonVariant::Ghost)
                    .autofocus()
                    .disabled(blocked)
                    .on_click(cx.listener(|panel, _, _, cx| {
                        panel.with_app(cx, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Toggle, cx)
                        });
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(14.))
                    .font_weight(FontWeight(600.))
                    .child(t!("themeDesigner.manage")),
            )
            .child(
                ZzClawButton::new("theme-manager-import", t!("themeDesigner.import"))
                    .variant(ZzClawButtonVariant::Ghost)
                    .disabled(blocked)
                    .on_click(cx.listener(|panel, _, _, cx| {
                        panel.with_app(cx, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Import, cx)
                        });
                    })),
            )
            .child(
                ZzClawButton::new("theme-manager-create", t!("themeDesigner.create"))
                    .icon("icons/conn/add.svg")
                    .variant(ZzClawButtonVariant::Primary)
                    .tooltip(t!("themeDesigner.createTooltip"))
                    .disabled(blocked || source.is_empty() || state.unsaved_new())
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        let source = source.clone();
                        panel.with_app(cx, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Copy(source), cx);
                            app.focus_text_input_if_present("theme.designer.name", window, cx);
                        });
                    })),
            )
    }

    fn theme_list(
        &self,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let blocked = state.blocked();
        let summary = self.settings.summary();
        let current = state
            .draft
            .as_ref()
            .map(|theme| theme.id.clone())
            .unwrap_or_default();
        let mine = summary.custom_themes.available();
        let mut list = div().flex().flex_col().gap_0p5().p_2().w_full().min_w_0();

        list = list.child(list_heading(palette, t!("themeDesigner.mine")));
        if let Some(draft) = state.draft.as_ref().filter(|_| state.unsaved_new()) {
            list = list.child(self.theme_list_row(
                draft,
                palette,
                ThemeRowState {
                    selected: true,
                    dirty: true,
                    disabled: blocked,
                },
                cx,
            ));
        }
        if mine.is_empty() && !state.unsaved_new() {
            list = list.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(t!("themeDesigner.mineEmpty")),
            );
        }
        for theme in &mine {
            let selected = current == theme.id;
            list = list.child(self.theme_list_row(
                theme,
                palette,
                ThemeRowState {
                    selected,
                    dirty: selected && state.dirty(),
                    disabled: blocked,
                },
                cx,
            ));
        }

        list = list
            .child(div().h(px(8.)))
            .child(list_heading(palette, t!("themeDesigner.builtins")));
        for theme in builtins() {
            list = list.child(self.theme_list_row(
                theme,
                palette,
                ThemeRowState {
                    selected: current == theme.id,
                    dirty: false,
                    disabled: blocked,
                },
                cx,
            ));
        }
        for diagnostic in self.settings.theme_diagnostics.iter() {
            list = list.child(
                div()
                    .px_2()
                    .pt_2()
                    .text_size(px(11.))
                    .text_color(rgb(palette.warning))
                    .child(diagnostic.clone()),
            );
        }
        list
    }

    fn theme_list_row(
        &self,
        theme: &ThemeConfig,
        palette: ThemePalette,
        row: ThemeRowState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let usage = self.theme_usage(&theme.id);
        let id = theme.id.clone();
        let mut content = div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(swatch(palette, &theme.swatch, 14.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_left()
                    .child(theme.name.clone()),
            );
        if row.dirty {
            content = content.child(tag(
                palette,
                t!("themeDesigner.unsavedTag"),
                palette.warning,
            ));
        }
        if usage.ui {
            content = content.child(tag(
                palette,
                t!("themeDesigner.currentUi"),
                palette.text_muted,
            ));
        }
        if usage.terminal {
            content = content.child(tag(
                palette,
                t!("themeDesigner.currentTerminal"),
                palette.text_muted,
            ));
        }
        ZzClawButton::new(format!("theme-browse-{id}"), "")
            .content(content)
            .variant(ZzClawButtonVariant::Ghost)
            .full_width()
            .selected(row.selected)
            .disabled(row.disabled)
            .on_click(cx.listener(move |panel, _, _, cx| {
                let id = id.clone();
                panel.with_app(cx, |app, cx| {
                    app.theme_designer_action(ThemeIntent::Select(id), cx)
                });
            }))
            .into_any_element()
    }

    fn theme_editor(
        &self,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut column = div().flex().flex_col().flex_1().min_h_0().min_w_0();
        let Some(theme) = state.draft.as_ref() else {
            return column.into_any_element();
        };
        column = column.child(self.theme_toolbar(theme, state, palette, cx));

        let preview = self.theme_preview_block(theme, state, palette);
        let mut scroll = div().flex().flex_col().gap_5().p_4().w_full().min_w_0();
        if narrow {
            scroll = scroll.child(preview);
        } else {
            column = column.child(
                div()
                    .flex_none()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .child(preview),
            );
        }
        if state.editable() {
            scroll = scroll.child(self.theme_fields(state, palette, narrow, cx));
        } else {
            scroll = scroll.child(self.builtin_notice(theme, state, palette, cx));
        }
        column = column.child(
            div()
                .flex_1()
                .min_h_0()
                .child(scroll.size_full().overflow_y_scrollbar()),
        );
        if let Some(footer) = self.theme_footer(theme, state, palette, cx) {
            column = column.child(footer);
        }
        column.into_any_element()
    }

    fn theme_toolbar(
        &self,
        theme: &ThemeConfig,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blocked = state.blocked();
        let editable = state.editable();
        let usage = self.theme_usage(&theme.id);
        let panel = cx.entity().downgrade();

        let mut title = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_1()
            .min_w_0()
            .child(swatch(palette, &theme.swatch, 18.))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.))
                    .child(theme.name.clone()),
            )
            .child(tag(
                palette,
                if editable {
                    t!("themeDesigner.customTag")
                } else {
                    t!("themeDesigner.builtinTag")
                },
                palette.text_muted,
            ));
        if usage.ui || usage.terminal {
            title = title.child(tag(palette, t!("themeDesigner.inUse"), palette.success));
        }

        let dirty = state.dirty();
        let can_apply = !blocked && state.errors.is_empty();
        let apply_panel = panel.clone();
        let apply = ZzClawDropdownMenu::new("theme-apply")
            .label(if dirty {
                t!("themeDesigner.saveAndApply")
            } else {
                t!("themeDesigner.applyTo")
            })
            .icon("icons/menu/palette.svg")
            .anchor(ZzClawMenuAnchor::TopRight)
            .min_width(px(240.))
            .disabled(!can_apply)
            .items_dynamic(move |_, _| {
                [
                    (
                        ThemeApplyTarget::BothFollowUi,
                        "themeDesigner.applyBoth",
                        usage.ui && usage.terminal_follows_ui,
                    ),
                    (ThemeApplyTarget::Ui, "themeDesigner.applyUi", usage.ui),
                    (
                        ThemeApplyTarget::Terminal,
                        "themeDesigner.applyTerminal",
                        usage.terminal && !usage.terminal_follows_ui,
                    ),
                ]
                .into_iter()
                .map(|(target, label, checked)| {
                    ZzClawMenuItem::action(t!(label))
                        .checked(checked)
                        .on_click(menu_action(&apply_panel, move |app, cx| {
                            app.save_theme_designer(Some(target), cx);
                        }))
                })
                .collect()
            });

        let builtin = !editable;
        let deletable = editable && self.settings.summary().custom_themes.contains_id(&theme.id);
        let exportable = state.errors.is_empty();
        let copyable = !state.unsaved_new();
        let copy_id = theme.id.clone();
        let more = ZzClawDropdownMenu::new("theme-operations")
            .icon("icons/conn/more.svg")
            .tooltip(t!("themeDesigner.operations"))
            .anchor(ZzClawMenuAnchor::TopRight)
            .disabled(blocked)
            .items_dynamic(move |_, _| {
                let copy_id = copy_id.clone();
                vec![
                    ZzClawMenuItem::action(t!("themeDesigner.createCopy"))
                        .disabled(!copyable)
                        .on_click(menu_action(&panel, move |app, cx| {
                            app.theme_designer_action(ThemeIntent::Copy(copy_id.clone()), cx)
                        })),
                    ZzClawMenuItem::action(t!("themeDesigner.export"))
                        .icon("icons/menu/export.svg")
                        .disabled(!exportable)
                        .on_click(menu_action(&panel, |app, cx| app.export_theme_designer(cx))),
                    ZzClawMenuItem::separator(),
                    ZzClawMenuItem::action(t!("themeDesigner.reload"))
                        .icon("icons/fe/refresh.svg")
                        .on_click(menu_action(&panel, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Reload, cx)
                        })),
                    ZzClawMenuItem::separator(),
                    ZzClawMenuItem::action(t!("themeDesigner.delete"))
                        .icon("icons/fe/delete.svg")
                        .danger()
                        .disabled(builtin || !deletable)
                        .on_click(menu_action(&panel, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Delete, cx)
                        })),
                ]
            });

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .min_h(px(48.))
            .border_b_1()
            .border_color(rgb(palette.border))
            .child(title)
            .child(apply)
            .child(more)
    }

    fn theme_preview_block(
        &self,
        theme: &ThemeConfig,
        state: &ThemeDesignerState,
        palette: ThemePalette,
    ) -> AnyElement {
        let mut block = div().flex().flex_col().gap_2().w_full().min_w_0();
        if let Some(preview) = state.preview {
            block = block.child(theme_preview(preview, &theme.name));
        }
        let mut warnings: Vec<String> = Vec::new();
        if !state.errors.is_empty() {
            warnings.push(t!("themeDesigner.previewUnchanged").to_string());
        }
        if let Some(ratio) = state.ui_contrast {
            warnings.push(contrast_warning("themeDesigner.contrastUi", ratio));
        }
        if let Some(ratio) = state.terminal_contrast {
            warnings.push(contrast_warning("themeDesigner.contrastTerminal", ratio));
        }
        for warning in warnings {
            block = block.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.warning))
                    .child(warning),
            );
        }
        block.into_any_element()
    }

    fn builtin_notice(
        &self,
        theme: &ThemeConfig,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = theme.id.clone();
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_3()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight(600.))
                    .child(t!("themeDesigner.readOnlyTitle")),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("themeDesigner.readOnly")),
            )
            .child(
                ZzClawButton::new("theme-edit", t!("themeDesigner.createCopy"))
                    .icon("icons/conn/add.svg")
                    .variant(ZzClawButtonVariant::Primary)
                    .disabled(state.blocked())
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        let id = id.clone();
                        panel.with_app(cx, |app, cx| {
                            app.theme_designer_action(ThemeIntent::Copy(id), cx);
                            app.focus_text_input_if_present("theme.designer.name", window, cx);
                        });
                    })),
            )
            .into_any_element()
    }

    fn theme_fields(
        &self,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let editor_width = if narrow {
            self.viewport_width - 32.
        } else {
            self.viewport_width - LIST_WIDTH - 32.
        };
        let columns = if editor_width >= 600. { 2 } else { 1 };

        let basics = div()
            .flex()
            .flex_wrap()
            .gap_3()
            .w_full()
            .child(div().flex_1().min_w(px(200.)).child(self.theme_text_field(
                "name",
                t!("themeDesigner.name"),
                state,
                palette,
            )))
            .child(div().w(px(180.)).flex_none().child(self.theme_text_field(
                "label",
                t!("themeDesigner.label"),
                state,
                palette,
            )))
            .child(div().w(px(200.)).flex_none().child(self.theme_text_field(
                "swatch",
                t!("themeDesigner.swatch"),
                state,
                palette,
            )));

        let terminal = state.terminal_tab;
        let tabs = div().w(px(320.)).max_w_full().child(
            ZzClawTabs::new("theme-color-tabs")
                .variant(ZzClawTabsVariant::Segmented)
                .items([
                    ZzClawTabItem::new(t!("themeDesigner.uiColors")),
                    ZzClawTabItem::new(t!("themeDesigner.terminalColors")),
                ])
                .selected_index(usize::from(terminal))
                .on_select(cx.listener(|panel, index: &usize, _, cx| {
                    let terminal = *index == 1;
                    panel.with_app(cx, |app, cx| {
                        app.settings.theme_designer_mut().terminal_tab = terminal;
                        app.request_settings_panel_refresh(cx);
                    });
                })),
        );

        let mut colors = div().flex().flex_col().gap_5().w_full();
        let groups = if terminal { TERMINAL_GROUPS } else { UI_GROUPS };
        for (title, fields) in groups {
            let cells: Vec<AnyElement> = fields
                .iter()
                .map(|field| self.theme_color_row(*field, state, palette))
                .collect();
            colors = colors.child(field_group(palette, t!(*title), cells, columns));
        }
        if terminal {
            let cells: Vec<AnyElement> = ANSI_PAIRS
                .iter()
                .flat_map(|(normal, bright)| [*normal, *bright])
                .map(|field| self.theme_color_row(field, state, palette))
                .collect();
            colors = colors.child(field_group(
                palette,
                t!("themeDesigner.groups.ansi"),
                cells,
                columns,
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap_5()
            .w_full()
            .min_w_0()
            .child(field_group(
                palette,
                t!("themeDesigner.groups.basics"),
                vec![basics.into_any_element()],
                1,
            ))
            .child(tabs)
            .child(colors)
    }

    fn theme_text_field(
        &self,
        field: &str,
        label: impl Into<SharedString>,
        state: &ThemeDesignerState,
        palette: ThemePalette,
    ) -> AnyElement {
        let invalid = state.errors.iter().any(|error| error == field);
        let mut row = div().flex().items_center().gap_2().w_full();
        if let Some((picker, _, _)) = self.theme_picker_controls.fields.get(field) {
            row = row.child(ZzClawColorPicker::new(picker));
        }
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .h(px(32.))
                .child(self.theme_input_box(field)),
        );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(if invalid {
                        palette.danger
                    } else {
                        palette.text_muted
                    }))
                    .child(label.into()),
            )
            .child(row)
            .when(invalid, |this| this.child(invalid_hint(palette)))
            .into_any_element()
    }

    fn theme_color_row(
        &self,
        field: ThemeColorField,
        state: &ThemeDesignerState,
        palette: ThemePalette,
    ) -> AnyElement {
        let path = field.path();
        let label = t!(format!("themeDesigner.colors.{path}")).to_string();
        let invalid = state.errors.iter().any(|error| error == path);
        let mut row = div().flex().items_center().gap_2().w_full().min_w_0();
        if let Some((picker, _, _)) = self.theme_picker_controls.fields.get(path) {
            row = row.child(ZzClawColorPicker::new(picker).accessibility_label(label.clone()));
        }
        row = row
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(rgb(if invalid {
                        palette.danger
                    } else {
                        palette.text
                    }))
                    .child(label),
            )
            .child(
                div()
                    .w(px(FIELD_INPUT_WIDTH))
                    .flex_none()
                    .h(px(32.))
                    .child(self.theme_input_box(path)),
            );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .min_w_0()
            .child(row)
            .when(invalid, |this| this.child(invalid_hint(palette)))
            .into_any_element()
    }

    /// Inputs are built by `ensure_theme_designer_inputs`; a frame drawn before that
    /// lands shows an empty slot instead of asserting.
    fn theme_input_box(&self, field: &str) -> AnyElement {
        let id = format!("theme.designer.{field}");
        if self.existing_text_input(&id).is_some() {
            self.existing_text_input_box(id, false)
        } else {
            div().into_any_element()
        }
    }

    fn theme_footer(
        &self,
        theme: &ThemeConfig,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editable = state.editable();
        if !editable && state.message.is_empty() {
            return None;
        }
        let dirty = state.dirty();
        let usage = self.theme_usage(&theme.id);
        let (status, status_color) = if !state.message.is_empty() {
            (state.message.clone(), palette.text_muted)
        } else if !state.errors.is_empty() {
            (t!("themeDesigner.fixInvalid").to_string(), palette.danger)
        } else if dirty {
            (t!("themeDesigner.dirty").to_string(), palette.warning)
        } else if usage.ui || usage.terminal {
            (t!("themeDesigner.liveHint").to_string(), palette.text_muted)
        } else {
            (String::new(), palette.text_muted)
        };
        let mut footer = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .min_h(px(48.))
            .border_t_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.section_header))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.))
                    .text_color(rgb(status_color))
                    .child(status),
            );
        if editable {
            let blocked = state.blocked();
            footer = footer
                .child(
                    ZzClawButton::new("theme-discard", t!("themeDesigner.discardChanges"))
                        .variant(ZzClawButtonVariant::Ghost)
                        .disabled(blocked || !dirty)
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| app.discard_theme_designer(cx));
                        })),
                )
                .child(
                    ZzClawButton::new("theme-save", t!("themeDesigner.save"))
                        .variant(ZzClawButtonVariant::Primary)
                        .loading(state.submitting)
                        .disabled(blocked || !dirty || !state.errors.is_empty())
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.save_theme_designer(None, cx);
                            })
                        })),
                );
        }
        Some(footer.into_any_element())
    }

    fn theme_designer_confirmation(
        &self,
        state: &ThemeDesignerState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let name = state
            .draft
            .as_ref()
            .map(|theme| theme.name.clone())
            .unwrap_or_default();
        let (title, body, buttons): (SharedString, SharedString, Vec<AnyElement>) =
            if state.pending.is_some() {
                let mut body = t!("themeDesigner.unsaved", name = name).to_string();
                if !state.errors.is_empty() {
                    body = format!("{body}\n{}", t!("themeDesigner.fixInvalid"));
                }
                let button = |id: &'static str, label: SharedString, choice: Option<bool>| {
                    ZzClawButton::new(id, label)
                        .disabled(
                            state.submitting || (choice == Some(true) && !state.errors.is_empty()),
                        )
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.resolve_theme_designer_pending(choice, cx)
                            });
                        }))
                };
                (
                    t!("themeDesigner.unsavedTitle").into(),
                    body.into(),
                    vec![
                        button("theme-pending-back", t!("themeDesigner.back").into(), None)
                            .variant(ZzClawButtonVariant::Ghost)
                            .autofocus()
                            .into_any_element(),
                        button(
                            "theme-pending-discard",
                            t!("themeDesigner.discard").into(),
                            Some(false),
                        )
                        .variant(ZzClawButtonVariant::Danger)
                        .into_any_element(),
                        button(
                            "theme-pending-save",
                            t!("themeDesigner.save").into(),
                            Some(true),
                        )
                        .variant(ZzClawButtonVariant::Primary)
                        .into_any_element(),
                    ],
                )
            } else if state.delete_confirmation {
                (
                    t!("themeDesigner.delete").into(),
                    t!("themeDesigner.deleteConfirmation", name = name).into(),
                    vec![
                        ZzClawButton::new("theme-delete-cancel", t!("common.cancel"))
                            .variant(ZzClawButtonVariant::Ghost)
                            .autofocus()
                            .disabled(state.submitting)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |app, cx| {
                                    app.settings.theme_designer_mut().delete_confirmation = false;
                                    app.request_settings_panel_refresh(cx);
                                })
                            }))
                            .into_any_element(),
                        ZzClawButton::new("theme-delete-confirm", t!("themeDesigner.delete"))
                            .variant(ZzClawButtonVariant::Danger)
                            .loading(state.submitting)
                            .disabled(state.submitting)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |app, cx| app.confirm_theme_delete(cx))
                            }))
                            .into_any_element(),
                    ],
                )
            } else if state.close_confirmation {
                (
                    t!("settings.discardChanges").into(),
                    t!("settings.discardChangesDesc").into(),
                    vec![
                        ZzClawButton::new("theme-close-keep-settings", t!("themeDesigner.back"))
                            .variant(ZzClawButtonVariant::Ghost)
                            .autofocus()
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |app, cx| {
                                    app.settings.theme_designer_mut().close_confirmation = false;
                                    app.request_settings_panel_refresh(cx);
                                });
                            }))
                            .into_any_element(),
                        ZzClawButton::new(
                            "theme-close-discard-settings",
                            t!("settings.discardChanges"),
                        )
                        .variant(ZzClawButtonVariant::Danger)
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| app.cancel_settings(cx));
                        }))
                        .into_any_element(),
                    ],
                )
            } else {
                return None;
            };
        Some(
            div()
                .id("theme-designer-confirmation")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .bg(rgba(0x0000_0066))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .w(px(420.))
                        .max_w_full()
                        .p_5()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.surface_elevated))
                        .shadow_lg()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight(600.))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(palette.text_muted))
                                .child(body),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .justify_end()
                                .gap_2()
                                .pt_2()
                                .children(buttons),
                        ),
                )
                .into_any_element(),
        )
    }

    fn theme_usage(&self, id: &str) -> ThemeUsage {
        let summary = self.settings.summary();
        let ui = summary.theme == id;
        let terminal_follows_ui = summary
            .terminal_theme
            .as_deref()
            .is_none_or(|theme| theme.trim().is_empty());
        ThemeUsage {
            ui,
            terminal: if terminal_follows_ui {
                ui
            } else {
                summary.terminal_theme.as_deref() == Some(id)
            },
            terminal_follows_ui,
        }
    }
}

#[derive(Clone, Copy)]
struct ThemeRowState {
    selected: bool,
    dirty: bool,
    disabled: bool,
}

fn menu_action(
    panel: &WeakEntity<SettingsPanel>,
    action: impl Fn(&mut ZzClawTermApp, &mut Context<ZzClawTermApp>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let panel = panel.clone();
    move |_, _, cx| {
        let _ = panel.update(cx, |panel, cx| {
            panel.with_app(cx, |app, cx| action(app, cx))
        });
    }
}

fn contrast_warning(key: &str, ratio: f32) -> String {
    t!(
        key,
        ratio = format!("{ratio:.1}"),
        minimum = format!("{MIN_TEXT_CONTRAST:.1}")
    )
    .to_string()
}

fn list_heading(palette: ThemePalette, label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .px_2()
        .pt_1()
        .pb_1()
        .text_size(px(11.))
        .font_weight(FontWeight(600.))
        .text_color(rgb(palette.text_dimmed))
        .child(label.into())
}

fn swatch(palette: ThemePalette, value: &str, size: f32) -> impl IntoElement {
    let color = parse_color(value)
        .map(|color| rgba(u32::from_be_bytes(color)))
        .unwrap_or(rgb(palette.primary));
    div()
        .size(px(size))
        .flex_none()
        .rounded_sm()
        .border_1()
        .border_color(rgba((palette.border << 8) | 0xcc))
        .bg(color)
}

fn tag(palette: ThemePalette, label: impl Into<SharedString>, color: u32) -> impl IntoElement {
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .border_1()
        .border_color(rgba((palette.border << 8) | 0xcc))
        .text_size(px(10.))
        .text_color(rgb(color))
        .child(label.into())
}

fn invalid_hint(palette: ThemePalette) -> impl IntoElement {
    div()
        .text_size(px(11.))
        .text_color(rgb(palette.danger))
        .child(t!("themeDesigner.invalidColor"))
}

fn field_group(
    palette: ThemePalette,
    title: impl Into<SharedString>,
    cells: Vec<AnyElement>,
    columns: usize,
) -> impl IntoElement {
    let mut rows = div().flex().flex_col().gap_2().w_full();
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut row = div().flex().gap_4().w_full();
        for _ in 0..columns {
            row = row.child(div().flex_1().min_w_0().children(cells.next()));
        }
        rows = rows.child(row);
    }
    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .text_color(rgb(palette.text_muted))
                .child(title.into()),
        )
        .child(rows)
}

fn theme_preview(p: ThemePalette, name: &str) -> AnyElement {
    let label = |key: &str| t!(format!("themeDesigner.colors.{key}")).to_string();
    let ui = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(220.))
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(p.border))
        .bg(rgb(p.bg))
        .text_color(rgb(p.text))
        .text_size(px(12.))
        .shadow(vec![
            BoxShadow::new(px(0.), px(2.), rgba(p.shadow).into()).blur_radius(px(8.)),
        ])
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .py_1()
                .bg(rgb(p.section_header))
                .border_b_1()
                .border_color(rgb(p.border))
                .child(div().min_w_0().truncate().child(name.to_string()))
                .child(
                    div()
                        .flex_none()
                        .px_2()
                        .rounded_sm()
                        .bg(rgb(p.primary))
                        .text_color(rgb(p.on_primary))
                        .child(t!("themeDesigner.previewButton")),
                ),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .w(px(84.))
                        .flex_none()
                        .p_1()
                        .bg(rgb(p.surface))
                        .border_r_1()
                        .border_color(rgb(p.border))
                        .child(div().px_1().rounded_sm().truncate().child(label("text")))
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .truncate()
                                .bg(rgb(p.hover))
                                .child(label("bgHover")),
                        )
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .truncate()
                                .text_color(rgb(p.accent))
                                .child(label("accent")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .p_2()
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(div().truncate().child(label("text")))
                                .child(
                                    div()
                                        .truncate()
                                        .text_color(rgb(p.text_muted))
                                        .child(label("textMuted")),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_color(rgb(p.text_dimmed))
                                        .child(label("textDimmed")),
                                ),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_color(rgb(p.link))
                                .child(label("link")),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(p.input))
                                .border_1()
                                .border_color(rgb(p.focus_ring))
                                .text_color(rgb(p.text_muted))
                                .truncate()
                                .child(label("bgInput")),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .text_size(px(11.))
                                .child(div().text_color(rgb(p.success)).child(label("success")))
                                .child(div().text_color(rgb(p.warning)).child(label("warning")))
                                .child(div().text_color(rgb(p.danger)).child(label("danger"))),
                        ),
                ),
        );

    let ansi = |index: usize| rgb(p.terminal_ansi[index]);
    let terminal = div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_0p5()
        .p_2()
        .rounded_sm()
        .bg(rgb(p.terminal_bg))
        .text_color(rgb(p.terminal_fg))
        .font_family(crate::features::shell::gpui_code_font_family())
        .text_size(px(11.))
        .child(
            div()
                .flex()
                .child(div().text_color(ansi(2)).child("nya@term"))
                .child(":")
                .child(div().text_color(ansi(4)).child("~/project"))
                .child("$ ls"),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(div().text_color(ansi(12)).child("src/"))
                .child(div().text_color(ansi(10)).child("build.sh"))
                .child(div().text_color(ansi(6)).child("link"))
                .child("README.md"),
        )
        .child(
            div()
                .text_color(rgb(p.terminal_fg_intense.unwrap_or(p.terminal_fg)))
                .child("ZzClawTerm — UTF-8 你好"),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(div().text_color(ansi(1)).child("error"))
                .child(div().text_color(ansi(3)).child("warning"))
                .child(div().text_color(ansi(5)).child("note")),
        )
        .child(
            div()
                .bg(rgb(p.terminal_line_highlight))
                .child(t!("themeDesigner.previewLine")),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .bg(rgb(p.terminal_selection))
                        .child(t!("themeDesigner.previewSelection")),
                )
                .child(
                    div()
                        .bg(rgb(p.terminal_find_bg))
                        .border_1()
                        .border_color(rgb(p.terminal_find_border))
                        .child(t!("themeDesigner.previewSearch")),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .child(div().text_color(ansi(2)).child("$ "))
                .child(div().w(px(7.)).h(px(14.)).bg(rgb(p.terminal_cursor))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .pt_1()
                .children([0usize, 8].map(|start| {
                    div().flex().gap_0p5().children(
                        (start..start + 8)
                            .map(|index| div().h(px(8.)).flex_1().rounded_sm().bg(ansi(index))),
                    )
                })),
        );

    div()
        .flex()
        .flex_wrap()
        .gap_3()
        .w_full()
        .min_w_0()
        .child(ui)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w(px(220.))
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(p.border))
                .bg(rgb(p.ui_terminal_bg))
                .child(terminal),
        )
        .into_any_element()
}
