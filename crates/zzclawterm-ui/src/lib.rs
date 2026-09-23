//! Shared GPUI theme tokens and reusable presentation widgets for ZzClawTerm.

mod app_menu_bar;
mod button;
mod child_window;
mod command;
mod dialog;
mod document_editor;
pub mod document_syntax;
mod hover_card;
mod input;
mod input_focus;
mod markdown;
mod menu;
pub mod notification;
mod number_input;
mod popover;
mod root;
mod selectable_text;
mod selection;
mod settings;
mod sizing;
mod tabs;
mod theme;
mod theme_bridge;
mod tooltip;
mod widgets;

pub use app_menu_bar::{ZzClawAppMenu, ZzClawAppMenuBar};
pub use button::{ZzClawButton, ZzClawButtonVariant, ZzClawIconButton};
pub use child_window::{ChildWindowSlot, activate_child_window};
pub use command::{ZzClawCommand, ZzClawCommandIndex, ZzClawCommandItem, ZzClawCommandState};
pub use dialog::{ZzClawConfirmDialog, ZzClawDialog, ZzClawDialogFooter, ZzClawDialogWindowExt};
pub use document_editor::{
    ZzClawDocumentEditor, ZzClawDocumentEditorEvent, ZzClawDocumentEditorState,
};
pub use gpui_kit::component::input::{
    Copy as ZzClawCopy, Cut as ZzClawCut, Paste as ZzClawPaste, Redo as ZzClawRedo,
    SelectAll as ZzClawSelectAll, Undo as ZzClawUndo,
};
pub use gpui_kit::component::kbd::Kbd as ZzClawKbd;
pub use gpui_kit::component::scroll::ScrollableElement as ZzClawScrollable;
pub use gpui_kit::component::scroll::ScrollbarAxis as ZzClawScrollbarAxis;
pub use hover_card::ZzClawHoverCard;
pub use input::{
    ZzClawInput, ZzClawInputEvent, ZzClawInputShell, ZzClawInputState, ZzClawSearchInput,
    ZzClawTextArea,
};
pub use markdown::ZzClawMarkdown;
pub use menu::{ZzClawContextMenu, ZzClawDropdownMenu, ZzClawMenuAnchor, ZzClawMenuItem};
pub use number_input::{
    ZzClawNumberInput, ZzClawNumberInputEvent, ZzClawNumberInputOptions, ZzClawNumberInputState,
    ZzClawNumberStep,
};
pub use popover::{ZzClawPopover, ZzClawPopoverAlign, ZzClawPopoverPlacement};
pub use root::{ZzClawRoot, ZzClawWindowHandle, zzclaw_root};
pub use selectable_text::ZzClawSelectableText;
pub use selection::{
    ZzClawCheckbox, ZzClawRadioGroup, ZzClawSelect, ZzClawSelectEvent, ZzClawSelectOption,
    ZzClawSelectState, ZzClawSwitch,
};
pub use settings::{ZzClawSettingsLayout, ZzClawSettingsNavGroup, ZzClawSettingsNavItem};
pub use sizing::NYA_FORM_CONTROL_HEIGHT_PX;
pub use tabs::{ZzClawTabItem, ZzClawTabs, ZzClawTabsVariant};
pub use theme::{APPEARANCE_THEME_IDS, ThemePalette, appearance_theme_label, theme_palette};
pub use theme_bridge::apply_component_theme;
pub use tooltip::ZzClawTooltip;
pub use widgets::{
    ZzClawHorizontalScrollbar, ZzClawScrollArea, ZzClawUniformListScrollbar, capability_line,
    empty_panel, empty_panel_with_icon, mode_button, section_header, session_info_row,
    small_button, status_pill, svg_icon_button,
};

#[cfg(test)]
mod tests {
    /// ZzClawTerm localises `gpui-kit`'s own widget strings by setting one
    /// process-wide locale, which only works because both crates read the same
    /// `rust_i18n` global and `gpui-kit` ships the locales ZzClawTerm offers.
    /// This pins the whole chain without mutating the global, which parallel tests
    /// would race.
    #[test]
    fn gpui_kit_shares_the_rust_i18n_locale_and_ships_simplified_chinese() {
        assert_eq!(&*gpui_kit::component::locale(), &*rust_i18n::locale());

        let english = gpui_kit::component::_rust_i18n_try_translate("en", "Calendar.month.January");
        let chinese =
            gpui_kit::component::_rust_i18n_try_translate("zh-CN", "Calendar.month.January");
        assert_eq!(english.as_deref(), Some("January"));
        assert_eq!(chinese.as_deref(), Some("一月"));
    }
}
