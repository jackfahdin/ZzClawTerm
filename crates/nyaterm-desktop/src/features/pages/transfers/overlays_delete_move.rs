use rust_i18n::t;

use gpui::{AnyElement, Context, IntoElement, ParentElement as _, div};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::TransferMoveState;

impl NyaTermApp {
    pub(in crate::features) fn transfer_move_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self
            .transfer
            .move_dialog()
            .cloned()
            .unwrap_or(TransferMoveState {
                old_path: String::new(),
                raw_path_token: None,
                name: String::new(),
                value: String::new(),
                additional_entries: Vec::new(),
            });
        let path_input = self
            .text_input_box(
                "transfer.move.path",
                &state.value,
                TextInputSetup::placeholder(t!("fileExplorer.location")),
                cx,
            )
            .into_any_element();
        div().child(path_input).into_any_element()
    }
}
