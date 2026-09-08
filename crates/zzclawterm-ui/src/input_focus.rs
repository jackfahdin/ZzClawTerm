use gpui::{App, Global, MouseButton, WeakFocusHandle, Window};

#[derive(Default)]
struct ZzClawInputFocusRegistry {
    handles: Vec<WeakFocusHandle>,
    next_pointer_down: u64,
    pending_outside_pointer_down: Option<u64>,
}

impl Global for ZzClawInputFocusRegistry {}

pub(crate) fn register_nya_input_focus(handle: &gpui::FocusHandle, cx: &mut App) {
    let registry = cx.default_global::<ZzClawInputFocusRegistry>();
    registry
        .handles
        .retain(|registered| registered.upgrade().is_some());
    if !registry
        .handles
        .iter()
        .any(|registered| registered == handle)
    {
        registry.handles.push(handle.downgrade());
    }
}

fn zzclaw_input_is_focused(window: &Window, cx: &mut App) -> bool {
    let Some(focused) = window.focused(cx) else {
        return false;
    };
    let registry = cx.default_global::<ZzClawInputFocusRegistry>();
    registry
        .handles
        .retain(|registered| registered.upgrade().is_some());
    registry
        .handles
        .iter()
        .any(|registered| registered == &focused)
}

pub(crate) fn schedule_nya_input_blur_on_outside_pointer_down(
    button: MouseButton,
    window: &mut Window,
    cx: &mut App,
) {
    if button != MouseButton::Left || !zzclaw_input_is_focused(window, cx) {
        return;
    }

    let token = {
        let registry = cx.default_global::<ZzClawInputFocusRegistry>();
        registry.next_pointer_down = registry.next_pointer_down.wrapping_add(1);
        registry.pending_outside_pointer_down = Some(registry.next_pointer_down);
        registry.next_pointer_down
    };
    window.defer(cx, move |window, cx| {
        let pending = cx
            .default_global::<ZzClawInputFocusRegistry>()
            .pending_outside_pointer_down
            == Some(token);
        if pending && zzclaw_input_is_focused(window, cx) {
            window.blur(cx);
        }
        let registry = cx.default_global::<ZzClawInputFocusRegistry>();
        if registry.pending_outside_pointer_down == Some(token) {
            registry.pending_outside_pointer_down = None;
        }
    });
}

pub(crate) fn preserve_nya_input_focus_on_pointer_down(cx: &mut App) {
    cx.default_global::<ZzClawInputFocusRegistry>()
        .pending_outside_pointer_down = None;
}
