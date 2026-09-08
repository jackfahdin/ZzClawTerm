use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use gpui::{Context, KeyDownEvent};
use nyaterm_transport::SessionKind;

use crate::features::NyaTermApp;
use crate::send_command::{
    SendCommandControlFocus, SendCommandDataType, SendCommandLineEnding, SendCommandMode,
    SendCommandTarget,
};

impl NyaTermApp {
    pub(in crate::features) fn apply_send_command_control_input(
        &mut self,
        control_id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(control) = (match control_id {
            "count" => Some(SendCommandControlFocus::Count),
            "interval" => Some(SendCommandControlFocus::Interval),
            _ => None,
        }) else {
            return;
        };
        let filtered = normalize_send_command_control_input(control, &text);
        if !self
            .send_command
            .apply_control_input(control, filtered.clone())
        {
            let value = self.send_command.synced_control_input(control);
            self.reset_text_input(&format!("send-command.{control_id}"), &value, cx);
            return;
        }
        if filtered != text {
            self.reset_text_input(&format!("send-command.{control_id}"), &filtered, cx);
        }
        cx.notify();
    }

    pub(in crate::features) fn handle_send_command_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.mark_user_activity();
        let keystroke = &event.keystroke;
        if keystroke.modifiers.alt || keystroke.modifiers.function {
            return false;
        }
        let accel = keystroke.modifiers.platform || keystroke.modifiers.control;

        // The box owns the text and takes Enter as a newline, the way Tauri's
        // textarea does; Ctrl/Cmd+Enter is what sends, and Escape clears.
        match keystroke.key.as_str() {
            "enter" if accel => self.send_bottom_command(true, cx),
            "escape" if !accel => {
                self.send_command.clear_draft();
                self.reset_text_input("send-command.draft", "", cx);
                self.shell.set_status("command send cleared".to_string());
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    /// Apply an edit from the command send box.
    ///
    /// Hex is normalised as it is typed — the digits are regrouped into pairs
    /// and anything that is not a hex digit is dropped — so the box is written
    /// back with what the draft actually holds.
    pub(in crate::features) fn apply_send_command_draft(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(formatted) = self.send_command.apply_draft(text) {
            self.reset_text_input("send-command.draft", &formatted, cx);
        }
        cx.notify();
    }

    pub(in crate::features) fn send_bottom_command(
        &mut self,
        append_enter: bool,
        cx: &mut Context<Self>,
    ) {
        if self.send_command.is_sending() {
            self.stop_send_command(cx);
            return;
        }

        let session_kind = self.active_session_kind();
        let draft = self.send_command.draft_for_send(append_enter);
        let units = match self.build_send_command_units(&draft, session_kind) {
            Ok(units) => units,
            Err(message) => {
                self.shell.set_status(message);
                cx.notify();
                return;
            }
        };
        if units.is_empty() {
            self.shell.set_status("command send is empty".to_string());
            cx.notify();
            return;
        }
        let target_session_ids = self.send_command_target_session_ids();
        if target_session_ids.is_empty() {
            if matches!(self.send_command.target(), SendCommandTarget::Current)
                && self
                    .session
                    .active_id()
                    .is_some_and(|session_id| self.session.is_disconnected(session_id))
            {
                self.shell
                    .set_status("session disconnected — reconnect before sending".to_string());
                cx.notify();
                return;
            }
            self.shell
                .set_status("start a session before sending".to_string());
            cx.notify();
            return;
        }

        let units_per_round = units.len() as u32;
        let failed_writes = Arc::new(AtomicUsize::new(0));
        let run = self.send_command.begin_send(units_per_round);
        let cancel = run.cancel;
        let infinite = run.infinite;
        let rounds = run.rounds;
        let interval = run.interval_seconds;
        let raw_units = run.raw_units;
        self.shell.set_status(if infinite {
            format!("sending {units_per_round} unit(s) × ∞")
        } else {
            format!("sending {units_per_round} unit(s) × {rounds}")
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut first = true;
            let mut aborted = false;
            let mut round = 0u32;
            let failed_writes_for_send = failed_writes.clone();
            'outer: loop {
                if !infinite && round >= rounds {
                    break;
                }
                if cancel.load(Ordering::SeqCst) {
                    aborted = true;
                    break;
                }
                round = round.saturating_add(1);
                let _ = this.update(cx, |this, cx| {
                    this.send_command.set_progress_round(round);
                    cx.notify();
                });
                for unit in &units {
                    if cancel.load(Ordering::SeqCst) {
                        aborted = true;
                        break 'outer;
                    }
                    if !first && interval > 0.0 {
                        cx.background_executor().timer(Duration::from_secs_f64(interval)).await;
                        if cancel.load(Ordering::SeqCst) {
                            aborted = true;
                            break 'outer;
                        }
                    }
                    first = false;
                    let unit = unit.clone();
                    let targets = target_session_ids.clone();
                    let failed_writes = failed_writes_for_send.clone();
                    let _ = this.update(cx, |this, cx| {
                        for session_id in &targets {
                            let sent = if raw_units {
                                this.send_terminal_raw_input_to_session(
                                    session_id.clone(),
                                    unit.clone(),
                                    cx,
                                )
                            } else {
                                this.send_terminal_input_to_session(
                                    session_id.clone(),
                                    unit.clone(),
                                    cx,
                                )
                            };
                            if !sent {
                                failed_writes.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        this.send_command.complete_progress_unit();
                        cx.notify();
                    });
                }
            }
            let _ = this.update(cx, |this, cx| {
                let progress = this.send_command.finish_send();
                let failed_writes = failed_writes.load(Ordering::SeqCst);
                if aborted {
                    this.shell.set_status(if infinite {
                        format!(
                            "command send stopped at {} unit(s) · round {}",
                            progress.completed, progress.round
                        )
                    } else {
                        format!(
                            "command send stopped at {}/{}",
                            progress.completed, progress.total
                        )
                    });
                    if failed_writes > 0 {
                        this.shell.set_status(format!(
                            "{}, {failed_writes} failed write(s)",
                            this.shell.status()
                        ));
                    }
                } else if infinite {
                    this.shell.set_status(if failed_writes == 0 {
                        format!(
                            "command send completed: {} unit(s)",
                            progress.completed
                        )
                    } else {
                        format!(
                            "command send completed: {} unit(s), {failed_writes} failed write(s)",
                            progress.completed
                        )
                    });
                } else {
                    this.shell.set_status(if failed_writes == 0 {
                        format!("command send completed: {rounds} round(s)")
                    } else {
                        format!(
                            "command send completed: {rounds} round(s), {failed_writes} failed write(s)"
                        )
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::features) fn stop_send_command(&mut self, cx: &mut Context<Self>) {
        if self.send_command.request_cancel() {
            self.shell.set_status("stopping command send…".to_string());
            cx.notify();
        }
    }

    pub(in crate::features) fn send_command_target_session_ids(&self) -> Vec<String> {
        // Live sessions only from local metadata (skip disconnected tabs).
        let sessions = self
            .session
            .ordered_sessions()
            .into_iter()
            .filter(|session| {
                !self.session.is_disconnected(&session.id)
                    && !matches!(session.kind, SessionKind::Rdp | SessionKind::Vnc)
            })
            .collect::<Vec<_>>();
        let live_session_ids = sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let active_kind = self.active_session_kind();
        let is_compatible = |kind: SessionKind| -> bool {
            match active_kind {
                Some(SessionKind::Serial) => matches!(kind, SessionKind::Serial),
                Some(SessionKind::Rdp | SessionKind::Vnc) => false,
                Some(_) => !matches!(kind, SessionKind::Serial),
                None => true,
            }
        };
        match self.send_command.target() {
            SendCommandTarget::Current => self
                .session
                .active_id()
                .filter(|session_id| live_session_ids.contains(*session_id))
                .map(ToOwned::to_owned)
                .into_iter()
                .collect(),
            SendCommandTarget::AllCompatible => {
                if active_kind.is_none() {
                    return Vec::new();
                }
                sessions
                    .into_iter()
                    .filter(|session| is_compatible(session.kind))
                    .map(|session| session.id)
                    .collect()
            }
            SendCommandTarget::Group(group_id) => {
                let Some(group) = self
                    .sync_input
                    .groups()
                    .iter()
                    .find(|group| &group.id == group_id)
                else {
                    return Vec::new();
                };
                if !group.enabled {
                    return Vec::new();
                }
                let paused: std::collections::HashSet<&str> = group
                    .paused_session_ids
                    .iter()
                    .map(String::as_str)
                    .collect();
                let session_kind_by_id: std::collections::HashMap<&str, SessionKind> = sessions
                    .iter()
                    .map(|session| (session.id.as_str(), session.kind))
                    .collect();
                group
                    .session_ids
                    .iter()
                    .filter(|session_id| !paused.contains(session_id.as_str()))
                    .filter(|session_id| {
                        session_kind_by_id
                            .get(session_id.as_str())
                            .copied()
                            .is_some_and(is_compatible)
                    })
                    .cloned()
                    .collect()
            }
        }
    }

    pub(in crate::features) fn send_command_group_target_options(
        &self,
    ) -> Vec<(String, String, usize)> {
        let sessions = self
            .session
            .ordered_sessions()
            .into_iter()
            .filter(|session| {
                !self.session.is_disconnected(&session.id)
                    && !matches!(session.kind, SessionKind::Rdp | SessionKind::Vnc)
            })
            .collect::<Vec<_>>();
        let active_kind = self.active_session_kind();
        let is_compatible = |kind: SessionKind| -> bool {
            match active_kind {
                Some(SessionKind::Serial) => matches!(kind, SessionKind::Serial),
                Some(SessionKind::Rdp | SessionKind::Vnc) => false,
                Some(_) => !matches!(kind, SessionKind::Serial),
                None => true,
            }
        };
        let session_kind_by_id: std::collections::HashMap<&str, SessionKind> = sessions
            .iter()
            .map(|session| (session.id.as_str(), session.kind))
            .collect();
        self.sync_input
            .groups()
            .iter()
            .filter(|group| group.enabled)
            .filter_map(|group| {
                let paused: std::collections::HashSet<&str> = group
                    .paused_session_ids
                    .iter()
                    .map(String::as_str)
                    .collect();
                let count = group
                    .session_ids
                    .iter()
                    .filter(|session_id| !paused.contains(session_id.as_str()))
                    .filter(|session_id| {
                        session_kind_by_id
                            .get(session_id.as_str())
                            .copied()
                            .is_some_and(is_compatible)
                    })
                    .count();
                if count == 0 {
                    None
                } else {
                    Some((group.id.clone(), group.name.clone(), count))
                }
            })
            .collect()
    }

    pub(in crate::features) fn set_send_command_target(
        &mut self,
        target: SendCommandTarget,
        cx: &mut Context<Self>,
    ) {
        if !self.send_command.set_target(target) {
            return;
        }
        let label = match self.send_command.target() {
            SendCommandTarget::Current => "Current".to_string(),
            SendCommandTarget::AllCompatible => "All compatible".to_string(),
            SendCommandTarget::Group(id) => self
                .sync_input
                .groups()
                .iter()
                .find(|group| &group.id == id)
                .map(|group| format!("Group: {}", group.name))
                .unwrap_or_else(|| "Group".to_string()),
        };
        self.shell
            .set_status(format!("command send target: {label}"));
        cx.notify();
    }

    pub(in crate::features) fn build_send_command_units(
        &self,
        draft: &str,
        session_kind: Option<SessionKind>,
    ) -> Result<Vec<Vec<u8>>, String> {
        self.send_command.build_units(draft, session_kind)
    }

    pub(in crate::features) fn active_session_kind(&self) -> Option<SessionKind> {
        let active_id = self.session.active_id()?;
        if self.session.is_disconnected(active_id) {
            return None;
        }
        self.session
            .session_info(active_id)
            .map(|session| session.kind)
    }

    pub(in crate::features) fn set_send_command_data_type(
        &mut self,
        data_type: SendCommandDataType,
        cx: &mut Context<Self>,
    ) {
        let Some(interval) = self.send_command.set_data_type(data_type) else {
            return;
        };
        self.reset_text_input("send-command.interval", &interval, cx);
        self.shell.set_status(format!(
            "command send data: {}",
            match data_type {
                SendCommandDataType::Text => "Text",
                SendCommandDataType::Hex => "Hex",
            }
        ));
        cx.notify();
    }

    pub(in crate::features) fn set_send_command_mode(
        &mut self,
        mode: SendCommandMode,
        cx: &mut Context<Self>,
    ) {
        let Some(interval) = self.send_command.set_mode(mode) else {
            return;
        };
        self.reset_text_input("send-command.interval", &interval, cx);
        cx.notify();
    }

    pub(in crate::features) fn set_send_command_line_ending(
        &mut self,
        line_ending: SendCommandLineEnding,
        cx: &mut Context<Self>,
    ) {
        if self.send_command.set_line_ending(line_ending) {
            cx.notify();
        }
    }
}

fn normalize_send_command_control_input(control: SendCommandControlFocus, text: &str) -> String {
    match control {
        SendCommandControlFocus::Count => text
            .chars()
            .filter(|ch| {
                ch.is_ascii_digit() || matches!(ch, 'i' | 'n' | 'f' | 'I' | 'N' | 'F' | '∞')
            })
            .collect(),
        SendCommandControlFocus::Interval => text
            .chars()
            .filter(|ch| ch.is_ascii_digit() || *ch == '.')
            .collect(),
    }
}

#[cfg(test)]
mod control_input_tests {
    use super::{SendCommandControlFocus, normalize_send_command_control_input};

    #[test]
    fn count_input_keeps_numbers_and_infinity_spellings() {
        assert_eq!(
            normalize_send_command_control_input(SendCommandControlFocus::Count, "12 x INF ∞"),
            "12INF∞"
        );
    }

    #[test]
    fn interval_input_keeps_decimal_characters() {
        assert_eq!(
            normalize_send_command_control_input(SendCommandControlFocus::Interval, "1s.25"),
            "1.25"
        );
    }
}
