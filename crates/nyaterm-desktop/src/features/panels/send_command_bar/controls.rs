use rust_i18n::t;

use super::state::SendCommandBarViewState;
use gpui::{Context, IntoElement, div, prelude::*, px};
use nyaterm_transport::SessionKind;
use nyaterm_ui::{NyaNumberInputOptions, NyaSelectOption};

use super::super::send_command_control_group;
use crate::features::NyaTermApp;
use crate::send_command::{
    SendCommandDataType, SendCommandLineEnding, SendCommandMode, SendCommandTarget,
};

impl NyaTermApp {
    pub(super) fn send_command_bar_controls(
        &mut self,
        state: &SendCommandBarViewState,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = state.palette;
        let is_sending = state.is_sending;
        let is_serial = matches!(self.active_session_kind(), Some(SessionKind::Serial));
        let data_options = vec![
            NyaSelectOption::new("text", t!("serialSend.text")),
            NyaSelectOption::new("hex", t!("serialSend.hex")),
        ];
        let selected_data = match state.send.data_type {
            SendCommandDataType::Text => "text",
            SendCommandDataType::Hex => "hex",
        }
        .to_string();
        let (mode_options, selected_mode) = if state.send.data_type == SendCommandDataType::Hex {
            (
                vec![
                    NyaSelectOption::new("byte", t!("serialSend.byteByByte")),
                    NyaSelectOption::new("packet", t!("serialSend.packet")),
                ],
                match state.send.mode {
                    SendCommandMode::Packet => "packet",
                    _ => "byte",
                },
            )
        } else {
            (
                vec![
                    NyaSelectOption::new("line", t!("serialSend.lineByLine")),
                    NyaSelectOption::new("character", t!("serialSend.characterByCharacter")),
                ],
                match state.send.mode {
                    SendCommandMode::Character => "character",
                    _ => "line",
                },
            )
        };
        let mut target_options = vec![NyaSelectOption::new(
            "current",
            t!("serialSend.currentSession"),
        )];
        if !is_serial {
            target_options.push(NyaSelectOption::new("all", t!("serialSend.allSessions")));
        }
        target_options.extend(state.group_targets.iter().map(|(group_id, name, count)| {
            NyaSelectOption::new(
                format!("group:{group_id}"),
                t!("serialSend.groupSession", name = name, count = count),
            )
        }));
        let selected_target = match &state.send.target {
            SendCommandTarget::Current => "current".to_string(),
            SendCommandTarget::AllCompatible if !is_serial => "all".to_string(),
            SendCommandTarget::AllCompatible => "current".to_string(),
            SendCommandTarget::Group(group_id) => format!("group:{group_id}"),
        };
        if !target_options
            .iter()
            .any(|option| option.value() == selected_target)
        {
            target_options.push(NyaSelectOption::new(
                selected_target.clone(),
                t!("network.group"),
            ));
        }
        let line_ending_options = vec![
            NyaSelectOption::new("none", t!("serialSend.noLineEnding")),
            NyaSelectOption::new("cr", "CR"),
            NyaSelectOption::new("lf", "LF"),
            NyaSelectOption::new("crlf", "CR+LF"),
        ];
        let selected_line_ending = match state.send.line_ending {
            SendCommandLineEnding::None => "none",
            SendCommandLineEnding::Cr => "cr",
            SendCommandLineEnding::Lf => "lf",
            SendCommandLineEnding::Crlf => "crlf",
        }
        .to_string();

        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(send_command_control_group(
                palette,
                t!("serialSend.dataType"),
                self.bare_select_control(
                    "bottom-command-data-select",
                    data_options,
                    Some(selected_data),
                    is_sending,
                    cx,
                ),
            ))
            .child(send_command_control_group(
                palette,
                t!("serialSend.sendMode"),
                self.bare_select_control(
                    "bottom-command-mode-select",
                    mode_options,
                    Some(selected_mode.to_string()),
                    is_sending,
                    cx,
                ),
            ))
            .child(send_command_control_group(
                palette,
                t!("serialSend.target"),
                self.bare_select_control(
                    "bottom-command-target-select",
                    target_options,
                    Some(selected_target),
                    is_sending,
                    cx,
                ),
            ))
            .child(send_command_control_group(
                palette,
                t!("serialSend.count"),
                div().w(px(112.)).child(
                    self.number_input_box(
                        "send-command.count",
                        &state.send.count_input,
                        NyaNumberInputOptions::default()
                            .range(1.0, 9_999.0)
                            .step(1.0)
                            .allow_infinity(true)
                            .disabled(is_sending),
                        cx,
                    ),
                ),
            ))
            .child(send_command_control_group(
                palette,
                t!("serialSend.interval"),
                div().w(px(136.)).child(
                    self.number_input_box(
                        "send-command.interval",
                        &state.send.interval_input,
                        NyaNumberInputOptions::default()
                            .range(0.0, 60.0)
                            .step(0.01)
                            .decimal_places(2)
                            .suffix(t!("serialSend.seconds"))
                            .disabled(is_sending),
                        cx,
                    ),
                ),
            ))
            .when(state.is_serial_text_line, |this| {
                this.child(send_command_control_group(
                    palette,
                    t!("serialSend.lineEnding"),
                    self.bare_select_control(
                        "bottom-command-eol-select",
                        line_ending_options,
                        Some(selected_line_ending),
                        is_sending,
                        cx,
                    ),
                ))
            })
            .into_any_element()
    }
}
