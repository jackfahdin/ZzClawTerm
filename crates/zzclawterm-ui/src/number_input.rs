use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, RenderOnce, SharedString,
    Styled as _, Subscription, Window, div, prelude::FluentBuilder as _,
};
use gpui_kit::component::Disableable;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{
    InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, NumberStep, StepAction,
};

use crate::input_focus::{preserve_nya_input_focus_on_pointer_down, register_nya_input_focus};
use crate::sizing::{form_control_height, form_control_size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzClawNumberStep {
    Decrement,
    Increment,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ZzClawNumberInputEvent {
    Changed(String),
    Submitted(String),
    Stepped(ZzClawNumberStep),
}

#[derive(Clone, Debug)]
pub struct ZzClawNumberInputOptions {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub decimal_places: Option<usize>,
    pub allow_infinity: bool,
    pub disabled: bool,
    pub prefix: Option<SharedString>,
    pub suffix: Option<SharedString>,
}

impl Default for ZzClawNumberInputOptions {
    fn default() -> Self {
        Self {
            min: f64::MIN,
            max: f64::MAX,
            step: 1.0,
            decimal_places: None,
            allow_infinity: false,
            disabled: false,
            prefix: None,
            suffix: None,
        }
    }
}

impl ZzClawNumberInputOptions {
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn decimal_places(mut self, decimal_places: usize) -> Self {
        self.decimal_places = Some(decimal_places);
        self
    }

    pub fn allow_infinity(mut self, allow_infinity: bool) -> Self {
        self.allow_infinity = allow_infinity;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }
}

pub struct ZzClawNumberInputState {
    state: Option<Entity<InputState>>,
    seed: SharedString,
    pending_value: Option<SharedString>,
    silent_value: Option<SharedString>,
    placeholder: SharedString,
    options: ZzClawNumberInputOptions,
    focus: FocusHandle,
    focused: bool,
    subscriptions: Vec<Subscription>,
}

impl ZzClawNumberInputState {
    pub fn new(
        cx: &mut Context<Self>,
        seed: impl Into<SharedString>,
        options: ZzClawNumberInputOptions,
    ) -> Self {
        Self {
            state: None,
            seed: seed.into(),
            pending_value: None,
            silent_value: None,
            placeholder: SharedString::default(),
            options,
            focus: cx.focus_handle(),
            focused: false,
            subscriptions: Vec::new(),
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn value(&self, cx: &App) -> String {
        if let Some(state) = &self.state {
            state.read(cx).value().to_string()
        } else if let Some(value) = &self.pending_value {
            value.to_string()
        } else {
            self.seed.to_string()
        }
    }

    pub fn set_content(&mut self, text: &str, cx: &mut Context<Self>) {
        self.pending_value = Some(SharedString::from(text.to_string()));
        cx.notify();
    }

    /// Replace a draft buffer without reporting the rollback as a user edit.
    pub fn set_content_silent(&mut self, text: &str, cx: &mut Context<Self>) {
        let value = SharedString::from(text.to_string());
        self.silent_value = Some(value.clone());
        self.pending_value = Some(value);
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.options.disabled != disabled {
            self.options.disabled = disabled;
            cx.notify();
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub fn component_focus_handle(&self, cx: &App) -> FocusHandle {
        self.state
            .as_ref()
            .map(|state| state.read(cx).focus_handle(cx))
            .unwrap_or_else(|| self.focus.clone())
    }

    pub fn has_focus(&self) -> bool {
        self.focused
    }

    pub fn component_state(&self) -> Option<Entity<InputState>> {
        self.state.clone()
    }

    fn ensure_component(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(state) = self.state.clone() {
            if let Some(value) = self.pending_value.take() {
                state.update(cx, |state, cx| state.set_value(value, window, cx));
            }
            return state;
        }

        let value = self
            .pending_value
            .take()
            .unwrap_or_else(|| self.seed.clone());
        let placeholder = self.placeholder.clone();
        // The component installs a numeric mask unless one is set explicitly,
        // and that mask would reject the `∞` an infinity-capable field steps to.
        let mask_pattern = if self.options.allow_infinity {
            MaskPattern::None
        } else {
            MaskPattern::Number {
                separator: None,
                fraction: self.options.decimal_places,
            }
        };
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .default_value(value)
                .placeholder(placeholder)
                .mask_pattern(mask_pattern);
            // The component steps by 1 with no bounds unless told otherwise;
            // stepping here keeps the range, decimals and the `∞` wrap.
            state.set_step(None::<NumberStep>, window, cx);
            state
        });
        register_nya_input_focus(&state.read(cx).focus_handle(cx), cx);
        self.subscriptions = vec![
            cx.subscribe_in(
                &state,
                window,
                |this, input, event: &NumberInputEvent, window, cx| {
                    let NumberInputEvent::Step(action) = event;
                    let step = match action {
                        StepAction::Decrement => ZzClawNumberStep::Decrement,
                        StepAction::Increment => ZzClawNumberStep::Increment,
                    };
                    let next = this.stepped_value(input.read(cx).value().as_ref(), step);
                    input.update(cx, |input, cx| {
                        input.set_value(SharedString::from(next.clone()), window, cx);
                    });
                    cx.emit(ZzClawNumberInputEvent::Stepped(step));
                    cx.emit(ZzClawNumberInputEvent::Changed(next));
                },
            ),
            cx.subscribe_in(
                &state,
                window,
                |this, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let value = input.read(cx).value().to_string();
                        if this
                            .silent_value
                            .take()
                            .is_some_and(|expected| expected.as_ref() == value)
                        {
                            return;
                        }
                        cx.emit(ZzClawNumberInputEvent::Changed(value));
                    }
                    InputEvent::PressEnter { .. } => {
                        let committed = this.committed_value(input.read(cx).value().as_ref());
                        input.update(cx, |input, cx| {
                            input.set_value(SharedString::from(committed.clone()), window, cx);
                        });
                        cx.emit(ZzClawNumberInputEvent::Submitted(committed));
                    }
                    InputEvent::Focus | InputEvent::Blur => {}
                },
            ),
        ];
        self.state = Some(state.clone());
        state
    }

    fn stepped_value(&self, text: &str, direction: ZzClawNumberStep) -> String {
        stepped_number_text(text, direction, &self.options)
    }

    fn committed_value(&self, text: &str) -> String {
        committed_number_text(text, &self.options)
    }
}

impl EventEmitter<ZzClawNumberInputEvent> for ZzClawNumberInputState {}

impl Focusable for ZzClawNumberInputState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.component_focus_handle(cx)
    }
}

impl Render for ZzClawNumberInputState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.ensure_component(window, cx);
        let component_focus = state.read(cx).focus_handle(cx);
        if self.focus.is_focused(window) && !component_focus.is_focused(window) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
        self.focused = self.focus.is_focused(window) || component_focus.is_focused(window);
        NumberInput::new(&state)
            .with_size(form_control_size())
            .h(form_control_height())
            .disabled(self.options.disabled)
            .when_some(self.options.prefix.clone(), |this, prefix| {
                this.prefix(text_affix(prefix).into_any_element())
            })
            .when_some(self.options.suffix.clone(), |this, suffix| {
                this.suffix(text_affix(suffix).into_any_element())
            })
    }
}

#[derive(IntoElement)]
pub struct ZzClawNumberInput {
    state: Entity<ZzClawNumberInputState>,
    appearance: bool,
}

impl ZzClawNumberInput {
    pub fn new(state: &Entity<ZzClawNumberInputState>) -> Self {
        Self {
            state: state.clone(),
            appearance: true,
        }
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for ZzClawNumberInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (state, options, focused) = self.state.update(cx, |state, cx| {
            (
                state.ensure_component(window, cx),
                state.options.clone(),
                state.focus.is_focused(window),
            )
        });
        if focused {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
        self.state.update(cx, |input_state, cx| {
            let component_focus = state.read(cx).focus_handle(cx);
            input_state.focused =
                input_state.focus.is_focused(window) || component_focus.is_focused(window);
        });
        div()
            .size_full()
            .capture_any_mouse_down(|_, _, cx| {
                preserve_nya_input_focus_on_pointer_down(cx);
            })
            .child(
                NumberInput::new(&state)
                    .w_full()
                    .with_size(form_control_size())
                    .h(form_control_height())
                    .appearance(self.appearance)
                    .disabled(options.disabled)
                    .when_some(options.prefix, |this, prefix| {
                        this.prefix(text_affix(prefix).into_any_element())
                    })
                    .when_some(options.suffix, |this, suffix| {
                        this.suffix(text_affix(suffix).into_any_element())
                    }),
            )
    }
}

fn text_affix(text: SharedString) -> impl IntoElement {
    div().child(text)
}

fn stepped_number_text(
    text: &str,
    direction: ZzClawNumberStep,
    options: &ZzClawNumberInputOptions,
) -> String {
    if options.allow_infinity && is_infinity_text(text) {
        return match direction {
            ZzClawNumberStep::Decrement => "∞".to_string(),
            ZzClawNumberStep::Increment => format_number(options.min.max(1.0), options),
        };
    }

    let delta = match direction {
        ZzClawNumberStep::Decrement => -options.step.abs(),
        ZzClawNumberStep::Increment => options.step.abs(),
    };
    let current = text.trim().parse::<f64>().unwrap_or({
        if delta > 0.0 {
            options.min - delta
        } else {
            options.max - delta
        }
    });
    let next = current + delta;
    if options.allow_infinity && next < options.min {
        return "∞".to_string();
    }
    format_number(next.clamp(options.min, options.max), options)
}

fn committed_number_text(text: &str, options: &ZzClawNumberInputOptions) -> String {
    if options.allow_infinity && is_infinity_text(text) {
        return "∞".to_string();
    }
    match text.trim().parse::<f64>() {
        Ok(value) if value.is_finite() => {
            format_number(value.clamp(options.min, options.max), options)
        }
        _ => text.to_string(),
    }
}

fn is_infinity_text(text: &str) -> bool {
    let text = text.trim();
    text == "∞" || text.eq_ignore_ascii_case("inf")
}

fn format_number(value: f64, options: &ZzClawNumberInputOptions) -> String {
    if let Some(decimal_places) = options.decimal_places {
        return format!("{value:.decimal_places$}");
    }
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, div,
        px,
    };

    use super::{
        ZzClawNumberInput, ZzClawNumberInputOptions, ZzClawNumberInputState, ZzClawNumberStep,
        committed_number_text, stepped_number_text,
    };
    use crate::sizing::{NYA_FORM_CONTROL_HEIGHT_PX, form_control_size};

    struct NumberLayoutFixture {
        count: gpui::Entity<ZzClawNumberInputState>,
        interval: gpui::Entity<ZzClawNumberInputState>,
    }

    impl Render for NumberLayoutFixture {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().flex().children([
                div()
                    .w(px(128.))
                    .h(px(32.))
                    .flex_none()
                    .child(ZzClawNumberInput::new(&self.count).appearance(false)),
                div()
                    .w(px(160.))
                    .h(px(32.))
                    .flex_none()
                    .child(ZzClawNumberInput::new(&self.interval).appearance(false)),
            ])
        }
    }

    #[gpui::test]
    fn borderless_number_inputs_leave_room_for_values_and_both_step_buttons(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (fixture, cx) = cx.add_window_view(|_, cx| NumberLayoutFixture {
            count: cx.new(|cx| {
                ZzClawNumberInputState::new(
                    cx,
                    "9999",
                    ZzClawNumberInputOptions::default()
                        .range(1., 9999.)
                        .allow_infinity(true),
                )
            }),
            interval: cx.new(|cx| {
                ZzClawNumberInputState::new(
                    cx,
                    "60.00",
                    ZzClawNumberInputOptions::default()
                        .range(0., 60.)
                        .decimal_places(2)
                        .suffix("s"),
                )
            }),
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        fixture.read_with(cx, |fixture, cx| {
            for (field, left, width, expected) in [
                (&fixture.count, 0., 128., "9999"),
                (&fixture.interval, 128., 160., "60.00"),
            ] {
                let input = field.read(cx).component_state().unwrap();
                let bounds = input.read(cx).input_bounds();
                assert!(bounds.size.width >= px(32.));
                assert!(bounds.size.height > px(0.));
                assert!(bounds.origin.x >= px(left + 32.));
                assert!(bounds.right() <= px(left + width - 32.));
                assert_eq!(field.read(cx).value(cx), expected);
            }
        });
    }

    #[test]
    fn stepped_number_increments_decrements_and_clamps() {
        let options = ZzClawNumberInputOptions::default()
            .range(1.0, 10.0)
            .step(2.0);

        assert_eq!(
            stepped_number_text("3", ZzClawNumberStep::Increment, &options),
            "5"
        );
        assert_eq!(
            stepped_number_text("10", ZzClawNumberStep::Increment, &options),
            "10"
        );
        assert_eq!(
            stepped_number_text("1", ZzClawNumberStep::Decrement, &options),
            "1"
        );
    }

    #[test]
    fn stepped_number_starts_from_bounds_for_invalid_text() {
        let options = ZzClawNumberInputOptions::default()
            .range(1.0, 10.0)
            .step(2.0);

        assert_eq!(
            stepped_number_text("", ZzClawNumberStep::Increment, &options),
            "1"
        );
        assert_eq!(
            stepped_number_text("nope", ZzClawNumberStep::Decrement, &options),
            "10"
        );
    }

    #[test]
    fn committed_number_allows_empty_and_invalid_draft_text() {
        let options = ZzClawNumberInputOptions::default()
            .range(1.0, 10.0)
            .step(2.0);

        assert_eq!(committed_number_text("", &options), "");
        assert_eq!(committed_number_text("nope", &options), "nope");
        assert_eq!(committed_number_text("99", &options), "10");
    }

    #[test]
    fn stepped_number_preserves_decimal_places() {
        let options = ZzClawNumberInputOptions::default()
            .range(0.0, 60.0)
            .step(0.25)
            .decimal_places(2);

        assert_eq!(
            stepped_number_text("1.00", ZzClawNumberStep::Increment, &options),
            "1.25"
        );
        assert_eq!(committed_number_text("999", &options), "60.00");
    }

    #[test]
    fn stepped_number_supports_infinity_cycle() {
        let options = ZzClawNumberInputOptions::default()
            .range(1.0, 9999.0)
            .allow_infinity(true);

        assert_eq!(
            stepped_number_text("1", ZzClawNumberStep::Decrement, &options),
            "∞"
        );
        assert_eq!(
            stepped_number_text("∞", ZzClawNumberStep::Increment, &options),
            "1"
        );
        assert_eq!(committed_number_text("inf", &options), "∞");
    }

    #[test]
    fn number_input_uses_standard_form_control_size() {
        assert_eq!(NYA_FORM_CONTROL_HEIGHT_PX, 32.);
        assert_eq!(form_control_size(), gpui_kit::component::Size::Medium);
    }

    #[test]
    fn number_input_state_exposes_seed_value_before_render() {
        let mut cx = TestAppContext::single();
        let input =
            cx.new(|cx| ZzClawNumberInputState::new(cx, "64", ZzClawNumberInputOptions::default()));

        assert_eq!(cx.read_entity(&input, |input, cx| input.value(cx)), "64");
    }
}
