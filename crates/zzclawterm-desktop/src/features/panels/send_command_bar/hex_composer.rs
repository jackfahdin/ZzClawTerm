use super::state::{SendCommandBarViewState, SendCommandDensity};
use crate::features::ZzClawTermApp;
use gpui::IntoElement;
use rust_i18n::t;
use zzclawterm_ui::hex_editor::ZzClawHexEditor;

impl ZzClawTermApp {
    pub(super) fn send_command_hex_composer(
        &self,
        state: &SendCommandBarViewState,
    ) -> gpui::AnyElement {
        ZzClawHexEditor::new(&state.send.hex, state.palette)
            .id("send-command.hex")
            .compact(state.density == SendCommandDensity::Compact)
            .placeholder(state.input_hint.clone())
            .menu_labels([
                t!("serialSend.copyHex").into(),
                t!("serialSend.copyCompact").into(),
                t!("serialSend.copyCArray").into(),
                t!("serialSend.copyEscaped").into(),
                t!("serialSend.copyAscii").into(),
                t!("menu.paste").into(),
                t!("menu.selectAll").into(),
            ])
            .into_any_element()
    }
}
