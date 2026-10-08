use rust_i18n::t;

use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
    rgb, svg,
};
use zzclawterm_core::ai::AiCommandCard;
use zzclawterm_ui::{ZzClawButton, ZzClawButtonVariant};

use crate::features::formatting::compact_id;
use crate::theme::ThemePalette;

use super::AiPanel;

pub(super) fn ai_command_target_label(card: &AiCommandCard) -> String {
    card.target
        .as_ref()
        .map(|target| target.label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .or_else(|| card.target_terminal_session_id.as_deref().map(compact_id))
        .unwrap_or_default()
}

pub(super) fn ai_send_button(
    _palette: ThemePalette,
    running: bool,
    disabled: bool,
    cx: &mut Context<AiPanel>,
) -> impl IntoElement {
    div()
        .debug_selector(|| "ai-send-control".to_string())
        .size(px(28.))
        .flex_none()
        .child(
            ZzClawButton::new("ai-ask-run", "")
                .small()
                .full_width()
                .height(px(28.))
                .icon(if running {
                    "icons/ai/stop.svg"
                } else {
                    "icons/ai/send.svg"
                })
                .variant(if running {
                    ZzClawButtonVariant::Secondary
                } else {
                    ZzClawButtonVariant::Primary
                })
                .tooltip(if running {
                    t!("ai.cancelTask")
                } else {
                    t!("ai.sendMessage")
                })
                .disabled(disabled)
                .on_click(cx.listener(move |panel, _, _, cx| {
                    panel.with_app(cx, move |app, cx| {
                        if app.ai.chat_or_agent_is_running() {
                            app.cancel_ai_chat(cx);
                        } else if !disabled {
                            app.start_ai_ask(cx);
                        }
                    });
                })),
        )
}

pub(super) fn ai_user_pre_wrap_text(palette: ThemePalette, text: &str) -> gpui::AnyElement {
    let mut block = div()
        .min_w_0()
        .w_full()
        .flex()
        .flex_col()
        .text_size(px(12.))
        .text_color(rgb(palette.text))
        .line_height(px(18.));
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line_text = if line.is_empty() { " " } else { line }.to_string();
        block = block.child(div().min_w_0().line_height(px(18.)).child(line_text));
    }
    block.into_any_element()
}

pub(super) fn ai_message_menu_button(
    palette: ThemePalette,
    id: &'static str,
    icon: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .id(SharedString::from(id))
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .rounded_sm()
        .text_size(px(12.))
        .text_color(rgb(palette.text))
        .cursor_pointer()
        .hover(move |this| this.bg(rgb(palette.hover)))
        .on_click(on_click)
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(icon)
                .text_color(rgb(palette.text_muted)),
        )
        .child(label)
}

pub(super) fn ai_setup_step(
    palette: ThemePalette,
    index: &'static str,
    label: impl Into<SharedString>,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .w_full()
        .max_w(px(280.))
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.bg))
        .px_3()
        .py_2()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .size(px(18.))
                .rounded_full()
                .bg(rgb(palette.hover))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .font_weight(FontWeight(800.))
                .text_color(rgb(palette.link))
                .child(index),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(palette.text))
                .child(label),
        )
}

pub(super) fn ai_message_menu_position(
    x: f32,
    y: f32,
    menu_width: f32,
    menu_height: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32, f32) {
    let margin = 8.;
    let max_height = (viewport_height - margin * 2.).max(64.);
    let height = menu_height.min(max_height);
    let max_x = (viewport_width - menu_width - margin).max(margin);
    let max_y = (viewport_height - height - margin).max(margin);
    (x.clamp(margin, max_x), y.clamp(margin, max_y), max_height)
}

#[cfg(test)]
mod tests {
    use zzclawterm_core::ai::{AiCommandCard, AiTerminalTarget};

    use super::ai_command_target_label;

    #[test]
    fn legacy_command_target_is_compact_and_keeps_its_execution_id() {
        let mut card: AiCommandCard = serde_json::from_value(serde_json::json!({
            "id": "legacy-command",
            "title": "Inspect resources",
            "command": "pwd",
            "explanation": "",
            "expectedEffect": "",
            "targetTerminalSessionId": "af0280b6-e62b-4d17-8f3c-000000000001"
        }))
        .unwrap();
        assert_eq!(ai_command_target_label(&card), "af0280b6..0001");
        assert_eq!(
            card.target_terminal_session_id.as_deref(),
            Some("af0280b6-e62b-4d17-8f3c-000000000001")
        );

        card.target = Some(AiTerminalTarget {
            terminal_session_id: card.target_terminal_session_id.clone().unwrap(),
            connection_id: None,
            label: "Production SSH".into(),
            host: Some("example.test".into()),
            username: None,
            session_type: "SSH".into(),
        });
        assert_eq!(ai_command_target_label(&card), "Production SSH");
        card.target.as_mut().unwrap().label = " ".into();
        assert_eq!(ai_command_target_label(&card), "af0280b6..0001");
    }
}
