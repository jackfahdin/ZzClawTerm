use gpui::{
    Action as _, AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _,
    Render, RenderOnce, SharedString, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_base::input::InputContextMenuCapabilities;
use gpui_kit::component::input::{
    Copy, Cut, Editor, EditorState, Enter, Input, InputEvent, InputState, Paste, SelectAll,
    Textarea, TextareaState,
};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable, Size};

use crate::input_focus::{preserve_nya_input_focus_on_pointer_down, register_nya_input_focus};
use crate::menu::{ZzClawContextMenu, ZzClawMenuItem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZzClawInputEvent {
    Changed(String),
    Submitted(String),
    Blurred(String),
}

#[derive(Clone)]
enum ComponentState {
    Input(Entity<InputState>),
    Textarea(Entity<TextareaState>),
    /// A code surface: the same editing engine as `Textarea`, plus the line-number
    /// gutter and soft-wrap control that a script box needs.
    Editor(Entity<EditorState>),
}

impl ComponentState {
    fn set_context_menu_enabled(&self, enabled: bool, cx: &mut App) {
        match self {
            Self::Input(state) => {
                state.update(cx, |state, _| state.set_context_menu_enabled(enabled))
            }
            Self::Textarea(state) => {
                state.update(cx, |state, _| state.set_context_menu_enabled(enabled))
            }
            Self::Editor(state) => {
                state.update(cx, |state, _| state.set_context_menu_enabled(enabled))
            }
        }
    }

    fn context_menu_capabilities(&self, cx: &App) -> InputContextMenuCapabilities {
        match self {
            Self::Input(state) => state.read(cx).context_menu_capabilities(),
            Self::Textarea(state) => state.read(cx).context_menu_capabilities(),
            Self::Editor(state) => state.read(cx).context_menu_capabilities(),
        }
    }

    fn value(&self, cx: &App) -> String {
        match self {
            Self::Input(state) => state.read(cx).value().to_string(),
            Self::Textarea(state) => state.read(cx).value().to_string(),
            Self::Editor(state) => state.read(cx).value().to_string(),
        }
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Input(state) => state.read(cx).focus_handle(cx),
            Self::Textarea(state) => state.read(cx).focus_handle(cx),
            Self::Editor(state) => state.read(cx).focus_handle(cx),
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Input(state) => state.update(cx, |state, cx| state.focus(window, cx)),
            Self::Textarea(state) => state.update(cx, |state, cx| state.focus(window, cx)),
            Self::Editor(state) => state.update(cx, |state, cx| state.focus(window, cx)),
        }
    }

    fn set_value(&self, value: SharedString, window: &mut Window, cx: &mut App) {
        match self {
            Self::Input(state) => {
                state.update(cx, |state, cx| state.set_value(value.clone(), window, cx))
            }
            Self::Textarea(state) => {
                state.update(cx, |state, cx| state.set_value(value.clone(), window, cx))
            }
            Self::Editor(state) => {
                state.update(cx, |state, cx| state.set_value(value.clone(), window, cx))
            }
        }
    }

    fn select_all_with_cursor_at_end(&self, cx: &mut App) {
        let end = self.value(cx).len();
        match self {
            Self::Input(state) => {
                state.update(cx, |state, cx| state.set_selected_range(0..end, cx))
            }
            Self::Textarea(state) => {
                state.update(cx, |state, cx| state.set_selected_range(0..end, cx))
            }
            Self::Editor(state) => {
                state.update(cx, |state, cx| state.set_selected_range(0..end, cx))
            }
        }
    }
}

pub struct ZzClawInputState {
    state: Option<ComponentState>,
    seed: SharedString,
    pending_value: Option<SharedString>,
    silent_value: Option<SharedString>,
    placeholder: SharedString,
    placeholder_pending: bool,
    masked: bool,
    applied_masked: bool,
    multi_line: bool,
    submit_on_enter: bool,
    code: bool,
    language: Option<SharedString>,
    rows: Option<usize>,
    disabled: bool,
    readonly: bool,
    error: bool,
    max_chars: Option<usize>,
    focus: FocusHandle,
    focused: bool,
    subscription: Option<Subscription>,
}

impl ZzClawInputState {
    pub fn new(cx: &mut Context<Self>, seed: impl Into<SharedString>) -> Self {
        Self {
            state: None,
            seed: seed.into(),
            pending_value: None,
            silent_value: None,
            placeholder: SharedString::default(),
            placeholder_pending: false,
            masked: false,
            applied_masked: false,
            multi_line: false,
            submit_on_enter: false,
            code: false,
            language: None,
            rows: None,
            disabled: false,
            readonly: false,
            error: false,
            max_chars: None,
            focus: cx.focus_handle(),
            focused: false,
            subscription: None,
        }
    }

    pub fn single_line(cx: &mut Context<Self>, seed: impl Into<SharedString>) -> Self {
        Self::new(cx, seed)
    }

    pub fn multi_line(mut self, rows: Option<usize>) -> Self {
        self.multi_line = true;
        self.rows = rows;
        self
    }

    /// Send plain Enter through the multi-line input; Shift+Enter inserts a newline.
    pub fn submit_on_enter(mut self, submit: bool) -> Self {
        self.submit_on_enter = submit;
        self
    }

    /// A multi-line box that is source code: line-number gutter, no soft wrap.
    pub fn code(mut self, rows: Option<usize>) -> Self {
        self.multi_line = true;
        self.code = true;
        self.rows = rows;
        self
    }

    /// Sets the syntax-highlighting language for a source-code editor.
    pub fn language(mut self, language: impl Into<SharedString>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        if self.placeholder != placeholder {
            self.placeholder = placeholder;
            self.placeholder_pending = true;
            cx.notify();
        }
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.masked != masked {
            self.masked = masked;
            cx.notify();
        }
    }

    pub fn is_masked(&self) -> bool {
        self.masked
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    pub fn error(mut self, error: bool) -> Self {
        self.error = error;
        self
    }

    pub fn max_chars(mut self, max_chars: Option<usize>) -> Self {
        self.max_chars = max_chars;
        self
    }

    pub fn value(&self, cx: &App) -> String {
        if let Some(state) = &self.state {
            state.value(cx)
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

    /// Queue a programmatic replacement without reporting it as a user edit.
    pub fn set_content_silent(&mut self, text: &str, cx: &mut Context<Self>) {
        let value = SharedString::from(text.to_string());
        self.silent_value = Some(value.clone());
        self.pending_value = Some(value);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.ensure_component(window, cx);
        state.focus(window, cx);
        window.dispatch_action(SelectAll.boxed_clone(), cx);
    }

    pub fn select_all_with_cursor_at_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.ensure_component(window, cx);
        state.focus(window, cx);
        state.select_all_with_cursor_at_end(cx);
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub fn component_focus_handle(&self, cx: &App) -> FocusHandle {
        self.state
            .as_ref()
            .map(|state| state.focus_handle(cx))
            .unwrap_or_else(|| self.focus.clone())
    }

    pub fn has_focus(&self) -> bool {
        self.focused
    }

    pub fn component_state(&self) -> Option<Entity<InputState>> {
        match self.state.as_ref() {
            Some(ComponentState::Input(state)) => Some(state.clone()),
            _ => None,
        }
    }

    fn ensure_component(&mut self, window: &mut Window, cx: &mut Context<Self>) -> ComponentState {
        if let Some(state) = self.state.clone() {
            if std::mem::take(&mut self.placeholder_pending) {
                let placeholder = component_placeholder(self.placeholder.clone(), self.multi_line);
                match &state {
                    ComponentState::Input(input) => {
                        input.update(cx, |input, cx| {
                            input.set_placeholder(placeholder, window, cx)
                        });
                    }
                    ComponentState::Textarea(input) => {
                        input.update(cx, |input, cx| {
                            input.set_placeholder(placeholder, window, cx)
                        });
                    }
                    ComponentState::Editor(input) => {
                        input.update(cx, |input, cx| {
                            input.set_placeholder(placeholder, window, cx)
                        });
                    }
                }
            }
            if let Some(value) = self.pending_value.take() {
                state.set_value(value, window, cx);
            }
            return state;
        }

        let value = self
            .pending_value
            .take()
            .unwrap_or_else(|| self.seed.clone());
        let masked = self.masked;
        let multi_line = self.multi_line;
        self.placeholder_pending = false;
        let placeholder = component_placeholder(self.placeholder.clone(), multi_line);
        let language = self.language.clone();
        let rows = self.rows;
        let submit_on_enter = self.submit_on_enter;
        let (state, subscription) = if multi_line && self.code {
            let state = cx.new(|cx| {
                let mut editor = EditorState::new(window, cx)
                    .default_value(value)
                    .placeholder(placeholder);
                if let Some(language) = language {
                    editor = editor.language(language);
                }
                editor
                    // Source newlines define the script structure; wrapping would
                    // make gutter numbers stop matching the visible lines.
                    .soft_wrap(false)
            });
            let subscription = cx.subscribe(&state, |this, input, event: &InputEvent, cx| {
                forward_input_event(this, input.read(cx).value().to_string(), event, cx)
            });
            (ComponentState::Editor(state), subscription)
        } else if multi_line {
            let state = cx.new(|cx| {
                let mut input = TextareaState::new(window, cx)
                    .default_value(value)
                    .placeholder(placeholder)
                    .submit_on_enter(submit_on_enter);
                if let Some(rows) = rows {
                    input = input.rows(rows);
                }
                input
            });
            let subscription = cx.subscribe(&state, |this, input, event: &InputEvent, cx| {
                forward_input_event(this, input.read(cx).value().to_string(), event, cx)
            });
            (ComponentState::Textarea(state), subscription)
        } else {
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
                    .placeholder(placeholder)
                    .masked(masked)
            });
            let subscription = cx.subscribe(&state, |this, input, event: &InputEvent, cx| {
                forward_input_event(this, input.read(cx).value().to_string(), event, cx)
            });
            (ComponentState::Input(state), subscription)
        };
        register_nya_input_focus(&state.focus_handle(cx), cx);
        self.subscription = Some(subscription);
        self.state = Some(state.clone());
        self.applied_masked = masked;
        state
    }
}

fn forward_input_event(
    state: &mut ZzClawInputState,
    value: String,
    event: &InputEvent,
    cx: &mut Context<ZzClawInputState>,
) {
    match event {
        InputEvent::Change => {
            if state
                .silent_value
                .take()
                .is_some_and(|expected| expected.as_ref() == value)
            {
                return;
            }
            cx.emit(ZzClawInputEvent::Changed(value));
        }
        InputEvent::PressEnter { .. } => cx.emit(ZzClawInputEvent::Submitted(value)),
        InputEvent::Focus => {
            state.focused = true;
            cx.notify();
        }
        InputEvent::Blur => {
            state.focused = false;
            cx.emit(ZzClawInputEvent::Blurred(value));
            cx.notify();
        }
    }
}

fn component_placeholder(placeholder: SharedString, multi_line: bool) -> SharedString {
    if !multi_line || (!placeholder.contains('\r') && !placeholder.contains('\n')) {
        return placeholder;
    }

    let mut normalized = String::with_capacity(placeholder.len());
    let mut in_line_break = false;
    for ch in placeholder.chars() {
        if matches!(ch, '\r' | '\n') {
            if !in_line_break {
                normalized.push(' ');
                in_line_break = true;
            }
        } else {
            normalized.push(ch);
            in_line_break = false;
        }
    }
    SharedString::from(normalized)
}

impl EventEmitter<ZzClawInputEvent> for ZzClawInputState {}

impl Focusable for ZzClawInputState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.component_focus_handle(cx)
    }
}

impl Render for ZzClawInputState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.ensure_component(window, cx);
        if let ComponentState::Input(input) = &state
            && self.applied_masked != self.masked
        {
            let masked = self.masked;
            input.update(cx, |state, cx| state.set_masked(masked, window, cx));
            self.applied_masked = masked;
        }
        let component_focus = state.focus_handle(cx);
        if self.focus.is_focused(window) && !component_focus.is_focused(window) {
            state.focus(window, cx);
        }
        self.focused = self.focus.is_focused(window) || component_focus.is_focused(window);
        match state {
            ComponentState::Input(state) => Input::new(&state)
                .disabled(self.disabled)
                .readonly(self.readonly)
                .into_any_element(),
            ComponentState::Textarea(state) => Textarea::new(&state)
                .disabled(self.disabled)
                .readonly(self.readonly)
                .h_full()
                .into_any_element(),
            ComponentState::Editor(state) => Editor::new(&state)
                .disabled(self.disabled)
                .readonly(self.readonly)
                .h_full()
                .into_any_element(),
        }
    }
}

#[derive(IntoElement)]
pub struct ZzClawInput {
    state: Entity<ZzClawInputState>,
    font_size: Option<gpui::Pixels>,
}

impl ZzClawInput {
    pub fn new(state: &Entity<ZzClawInputState>) -> Self {
        Self {
            state: state.clone(),
            font_size: None,
        }
    }

    /// Override the compact component size for fields that follow the UI font.
    pub fn text_size(mut self, font_size: gpui::Pixels) -> Self {
        self.font_size = Some(font_size);
        self
    }
}

impl RenderOnce for ZzClawInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (state, disabled, readonly, multi_line) =
            prepare_input_component(&self.state, window, cx);
        let focus_state = state.clone();
        let input = match state {
            ComponentState::Input(state) => Input::new(&state)
                .xsmall()
                .when_some(self.font_size, |this, font_size| {
                    this.text_size(font_size)
                        .line_height(font_size * 1.5)
                        .h_full()
                })
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .disabled(disabled)
                .readonly(readonly)
                .into_any_element(),
            ComponentState::Textarea(state) => Textarea::new(&state)
                .appearance(false)
                .bordered(false)
                .disabled(disabled)
                .readonly(readonly)
                .h_full()
                .text_xs()
                .when_some(self.font_size, |this, font_size| this.text_size(font_size))
                .into_any_element(),
            ComponentState::Editor(state) => Editor::new(&state)
                .appearance(false)
                .bordered(false)
                .disabled(disabled)
                .readonly(readonly)
                .h_full()
                .text_xs()
                .when_some(self.font_size, |this, font_size| this.text_size(font_size))
                .into_any_element(),
        };
        div()
            .size_full()
            .when(!multi_line, |this| this.flex().items_center())
            .capture_any_mouse_down(|_, _, cx| {
                preserve_nya_input_focus_on_pointer_down(cx);
            })
            .on_any_mouse_down(move |_, window, cx| {
                if !disabled {
                    focus_state.focus(window, cx);
                }
            })
            .child(input)
    }
}

type KeyDownHandler = Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App) + 'static>;
type SecondaryEnterHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

fn prepare_input_component(
    input_state: &Entity<ZzClawInputState>,
    window: &mut Window,
    cx: &mut App,
) -> (ComponentState, bool, bool, bool) {
    let (state, disabled, readonly, multi_line, focused) = input_state.update(cx, |input, cx| {
        let state = input.ensure_component(window, cx);
        if let ComponentState::Input(component) = &state
            && input.applied_masked != input.masked
        {
            let masked = input.masked;
            component.update(cx, |component, cx| component.set_masked(masked, window, cx));
            input.applied_masked = masked;
        }
        (
            state,
            input.disabled,
            input.readonly,
            input.multi_line,
            input.focus.is_focused(window),
        )
    });
    if focused {
        state.focus(window, cx);
    }
    input_state.update(cx, |input_state, cx| {
        let component_focus = state.focus_handle(cx);
        input_state.focused =
            input_state.focus.is_focused(window) || component_focus.is_focused(window);
    });
    (state, disabled, readonly, multi_line)
}

#[derive(IntoElement)]
pub struct ZzClawInputShell {
    id: SharedString,
    state: Entity<ZzClawInputState>,
    size: Size,
    multi_line: bool,
    search: bool,
    framed: bool,
    height: Option<gpui::Pixels>,
    fill_height: bool,
    trailing: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_key_down: Option<KeyDownHandler>,
    on_secondary_enter: Option<SecondaryEnterHandler>,
    context_menu_labels: Option<[SharedString; 4]>,
}

impl ZzClawInputShell {
    pub fn new(id: impl Into<SharedString>, state: &Entity<ZzClawInputState>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            size: Size::Medium,
            multi_line: false,
            search: false,
            framed: true,
            height: None,
            fill_height: false,
            trailing: Vec::new(),
            footer: None,
            on_key_down: None,
            on_secondary_enter: None,
            context_menu_labels: None,
        }
    }

    /// Use the 24px input size for controls embedded in compact rows.
    pub fn compact(mut self) -> Self {
        self.size = Size::Small;
        self
    }

    pub fn multi_line(mut self) -> Self {
        self.multi_line = true;
        self
    }

    /// Opt into the ZzClawTerm popup menu. Labels are Cut, Copy, Paste, Select All,
    /// localized by the caller. Other inputs keep the platform's default menu.
    pub fn gpui_context_menu(mut self, labels: [SharedString; 4]) -> Self {
        self.context_menu_labels = Some(labels);
        self
    }

    pub fn height(mut self, height: gpui::Pixels) -> Self {
        self.height = Some(height);
        self.fill_height = false;
        self
    }

    /// Fill a definite-height parent instead of using the default textarea height.
    pub fn fill_height(mut self) -> Self {
        self.height = None;
        self.fill_height = true;
        self
    }

    fn search(mut self) -> Self {
        self.search = true;
        self
    }

    /// Remove the component chrome when a parent surface owns the frame.
    pub fn bare(mut self) -> Self {
        self.framed = false;
        self
    }

    pub fn trailing(mut self, child: impl IntoElement) -> Self {
        self.trailing.push(child.into_any_element());
        self
    }

    /// Add a fixed action row inside a textarea's frame, outside its scroll viewport.
    pub fn footer(mut self, child: impl IntoElement) -> Self {
        self.footer = Some(child.into_any_element());
        self
    }

    pub fn on_key_down(
        mut self,
        handler: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Box::new(handler));
        self
    }

    /// Take Ctrl/Cmd+Enter before the field sees it. A multi-line field binds
    /// that keystroke to a newline and stops propagation, so an `on_key_down`
    /// on an ancestor never receives it.
    pub fn on_secondary_enter(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_secondary_enter = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawInputShell {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ZzClawInputShell {
            id,
            state,
            size,
            multi_line,
            search,
            framed,
            height,
            fill_height,
            trailing,
            footer,
            on_key_down,
            on_secondary_enter,
            context_menu_labels,
        } = self;
        let (state, disabled, readonly, state_multi_line) =
            prepare_input_component(&state, window, cx);
        state.set_context_menu_enabled(context_menu_labels.is_none(), cx);
        let menu_state = state.clone();
        let capture_focus_state = state.clone();
        let gpui_context_menu = context_menu_labels.is_some();
        let focus_state = state.clone();
        let debug_selector = id.to_string();
        let prefix_debug_selector = format!("{}-prefix", id);
        let has_footer = footer.is_some();
        let input = match state {
            ComponentState::Input(state) => {
                let mut input = Input::new(&state)
                    .with_size(size)
                    .appearance(framed)
                    .bordered(framed)
                    .focus_bordered(framed)
                    .disabled(disabled)
                    .readonly(readonly);
                if multi_line || state_multi_line {
                    input = input.h(px(88.));
                }
                if search {
                    input = input.prefix(
                        div()
                            .debug_selector(move || prefix_debug_selector.clone())
                            .flex()
                            .items_center()
                            .child(Icon::new(IconName::Search).small()),
                    );
                }
                if !trailing.is_empty() {
                    input = input.suffix(div().flex().items_center().gap_1().children(trailing));
                }
                input.into_any_element()
            }
            ComponentState::Textarea(state) if has_footer => {
                let theme = cx.theme();
                let focused = state.read(cx).focus_handle(cx).is_focused(window);
                div()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .h(if fill_height {
                        gpui::relative(1.)
                    } else {
                        height.unwrap_or(px(88.)).into()
                    })
                    .when(framed, |this| {
                        this.rounded(theme.radius)
                            .border_1()
                            .border_color(if focused { theme.ring } else { theme.input })
                            .bg(theme.input.opacity(if theme.is_dark() { 0.3 } else { 0.0 }))
                    })
                    .child(
                        div().flex_1().min_h_0().min_w_0().overflow_hidden().child(
                            Textarea::new(&state)
                                .appearance(false)
                                .bordered(false)
                                .disabled(disabled)
                                .readonly(readonly)
                                .h_full(),
                        ),
                    )
                    .child(footer.unwrap())
                    .into_any_element()
            }
            ComponentState::Textarea(state) => Textarea::new(&state)
                .appearance(framed)
                .bordered(framed)
                .disabled(disabled)
                .readonly(readonly)
                .h(if fill_height {
                    gpui::relative(1.)
                } else {
                    height.unwrap_or(px(88.)).into()
                })
                .into_any_element(),
            ComponentState::Editor(state) => Editor::new(&state)
                .disabled(disabled)
                .readonly(readonly)
                // A script box needs more than the two-line note height a textarea
                // gets; the gutter makes short boxes read as cramped.
                .h(if fill_height {
                    gpui::relative(1.)
                } else {
                    height.unwrap_or(px(168.)).into()
                })
                .text_size(px(14.))
                .into_any_element(),
        };

        let mut container = div()
            .id(id)
            .debug_selector(move || debug_selector.clone())
            .w_full()
            .min_w_0()
            .when(fill_height, |this| this.h_full().min_h_0())
            .capture_any_mouse_down(move |event, window, cx| {
                preserve_nya_input_focus_on_pointer_down(cx);
                // ContextMenu captures the focus to restore before bubble handlers.
                // Right-clicking an unfocused field must restore this input on Esc.
                if gpui_context_menu && !disabled && event.button == MouseButton::Right {
                    capture_focus_state.focus(window, cx);
                }
            })
            .when(!has_footer, |this| {
                this.on_any_mouse_down(move |_, window, cx| {
                    if !disabled {
                        focus_state.focus(window, cx);
                    }
                })
            });
        if let Some(handler) = on_key_down {
            container = container.on_key_down(handler);
        }
        if let Some(handler) = on_secondary_enter {
            container = container.capture_action(move |action: &Enter, window, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    handler(window, cx);
                }
            });
        }
        let container = container.child(input);
        if let Some(labels) = context_menu_labels {
            ZzClawContextMenu::new_dynamic(container, move |_, cx| {
                let capabilities = menu_state.context_menu_capabilities(cx);
                let focus = menu_state.focus_handle(cx);
                let editable = capabilities.is_editable();
                let copyable = capabilities.is_copyable();
                let actions: [Box<dyn gpui::Action>; 4] = [
                    Box::new(Cut),
                    Box::new(Copy),
                    Box::new(Paste),
                    Box::new(SelectAll),
                ];
                let enabled = [
                    editable && copyable,
                    copyable,
                    editable && cx.read_from_clipboard().is_some(),
                    true,
                ];
                let mut items = Vec::with_capacity(5);
                for (index, ((label, action), enabled)) in
                    labels.iter().zip(actions).zip(enabled).enumerate()
                {
                    if index == 3 {
                        items.push(ZzClawMenuItem::separator());
                    }
                    let focus = focus.clone();
                    items.push(
                        ZzClawMenuItem::action(label.clone())
                            .disabled(!enabled)
                            .on_click(move |_, window, cx| {
                                // PopupMenu can initially select a disabled first row.
                                // Keep keyboard confirmation under the same policy as clicks.
                                if enabled {
                                    focus.dispatch_action(action.as_ref(), window, cx);
                                }
                            }),
                    );
                }
                items
            })
            .min_width(px(144.))
            .enabled(!disabled)
            .into_any_element()
        } else {
            container.into_any_element()
        }
    }
}

#[derive(IntoElement)]
pub struct ZzClawSearchInput {
    shell: ZzClawInputShell,
}

impl ZzClawSearchInput {
    pub fn new(id: impl Into<SharedString>, state: &Entity<ZzClawInputState>) -> Self {
        Self {
            shell: ZzClawInputShell::new(id, state).search(),
        }
    }

    pub fn trailing(mut self, child: impl IntoElement) -> Self {
        self.shell = self.shell.trailing(child);
        self
    }

    /// Use the 24px input size inside dense toolbars and overlays.
    pub fn compact(mut self) -> Self {
        self.shell = self.shell.compact();
        self
    }

    /// Remove the input frame when the containing overlay draws it.
    pub fn bare(mut self) -> Self {
        self.shell = self.shell.bare();
        self
    }

    pub fn on_key_down(
        mut self,
        handler: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.shell = self.shell.on_key_down(handler);
        self
    }
}

impl RenderOnce for ZzClawSearchInput {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.shell
    }
}

pub type ZzClawTextArea = ZzClawInput;

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        AppContext as _, ClipboardItem, InteractiveElement as _, IntoElement, MouseButton,
        ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, div, point,
        prelude::FluentBuilder as _, px,
    };

    use gpui_kit::component::highlighter::LanguageRegistry;

    use super::{ComponentState, ZzClawInputShell, ZzClawInputState, component_placeholder};

    struct InputHeightFixture {
        height: f32,
        footer: bool,
        flexible: gpui::Entity<ZzClawInputState>,
        fixed: gpui::Entity<ZzClawInputState>,
        ordinary: gpui::Entity<ZzClawInputState>,
    }

    impl Render for InputHeightFixture {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div()
                .w(px(320.))
                .flex()
                .flex_col()
                .child(
                    div().h(px(self.height)).flex_none().child(
                        ZzClawInputShell::new("flexible-textarea", &self.flexible)
                            .multi_line()
                            .fill_height()
                            .when(self.footer, |this| {
                                this.footer(
                                    div()
                                        .id("textarea-footer")
                                        .debug_selector(|| "textarea-footer".into())
                                        .h(px(34.))
                                        .flex_none()
                                        .child("Send"),
                                )
                            }),
                    ),
                )
                .child(ZzClawInputShell::new("fixed-textarea", &self.fixed).multi_line())
                .child(ZzClawInputShell::new("ordinary-input", &self.ordinary))
        }
    }

    #[gpui::test]
    fn textarea_fills_resized_parent_without_changing_default_input_heights(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (fixture, cx) = cx.add_window_view(|_, cx| InputHeightFixture {
            height: 68.,
            footer: false,
            flexible: cx
                .new(|cx| ZzClawInputState::new(cx, "draft\nsecond line").multi_line(Some(4))),
            fixed: cx.new(|cx| ZzClawInputState::new(cx, "fixed").multi_line(Some(4))),
            ordinary: cx.new(|cx| ZzClawInputState::new(cx, "ordinary")),
        });
        let field = fixture.read_with(cx, |fixture, _| fixture.flexible.clone());
        for (height, footer) in [
            (68., false),
            (128., false),
            (468., false),
            (68., false),
            (68., true),
            (128., true),
            (468., true),
            (68., true),
        ] {
            fixture.update(cx, |fixture, cx| {
                fixture.height = height;
                fixture.footer = footer;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            assert_eq!(
                cx.debug_bounds("flexible-textarea").unwrap().size.height,
                px(height),
            );
            assert_eq!(
                cx.debug_bounds("fixed-textarea").unwrap().size.height,
                px(88.)
            );
            assert_eq!(
                cx.debug_bounds("ordinary-input").unwrap().size.height,
                px(32.)
            );
            let footer_bounds = footer.then(|| cx.debug_bounds("textarea-footer").unwrap());
            field.read_with(cx, |field, cx| {
                let Some(ComponentState::Textarea(component)) = field.state.as_ref() else {
                    panic!("expected a rendered textarea");
                };
                // Check the editing engine, not just the shell hit target.
                let bounds = component.read(cx).input_bounds();
                let editing_height = height - if footer { 34. } else { 0. };
                assert!(bounds.size.height > px((editing_height - 24.).max(0.)));
                assert!(bounds.size.height <= px(height));
                if let Some(footer_bounds) = footer_bounds {
                    assert_eq!(footer_bounds.size.height, px(34.));
                    assert!(
                        bounds.bottom() <= footer_bounds.origin.y,
                        "height {height}: editing bounds {bounds:?}, footer {footer_bounds:?}"
                    );
                    assert!(footer_bounds.bottom() <= px(height));
                }
                assert_eq!(field.value(cx), "draft\nsecond line");
            });
        }
    }

    struct InputContextMenuFixture {
        field: gpui::Entity<ZzClawInputState>,
    }

    impl Render for InputContextMenuFixture {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().w(px(320.)).child(
                ZzClawInputShell::new("popup-input", &self.field).gpui_context_menu([
                    "Cut".into(),
                    "Copy".into(),
                    "Paste".into(),
                    "Select All".into(),
                ]),
            )
        }
    }

    fn open_input_context_menu(cx: &mut VisualTestContext) {
        cx.simulate_mouse_down(
            point(px(12.), px(12.)),
            MouseButton::Right,
            Default::default(),
        );
        cx.simulate_mouse_up(
            point(px(12.), px(12.)),
            MouseButton::Right,
            Default::default(),
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn popup_input_menu_reuses_edit_actions_and_restores_input_focus(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (fixture, cx) = cx.add_window_view(|_, cx| InputContextMenuFixture {
            field: cx.new(|cx| ZzClawInputState::new(cx, "hello").multi_line(Some(3))),
        });
        let field = fixture.read_with(cx, |fixture, _| fixture.field.clone());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        // A right click from outside the field must restore focus to the field.
        open_input_context_menu(cx);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(field.read(cx).component_focus_handle(cx).is_focused(window));
            field.update(cx, |field, cx| field.select_all(window, cx));
        });
        cx.run_until_parked();

        open_input_context_menu(cx);
        cx.simulate_keystrokes("down down enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("hello".into())
            );
            assert!(field.read(cx).component_focus_handle(cx).is_focused(window));
        });
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "hello");

        open_input_context_menu(cx);
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "");
        // Undo verifies Cut went through the editor's history.
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        });
        cx.run_until_parked();
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "hello");

        cx.update(|window, cx| {
            field.update(cx, |field, cx| field.select_all(window, cx));
            cx.write_to_clipboard(ClipboardItem::new_string("world".into()));
        });
        cx.run_until_parked();
        open_input_context_menu(cx);
        cx.simulate_keystrokes("down down down enter");
        cx.run_until_parked();
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "world");

        open_input_context_menu(cx);
        cx.simulate_keystrokes("up enter");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let state = field.read(cx).state.as_ref().expect("rendered input");
            assert!(state.context_menu_capabilities(cx).has_selection());
        });
    }

    #[gpui::test]
    fn readonly_popup_input_menu_allows_copy_without_editing(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (fixture, cx) = cx.add_window_view(|_, cx| InputContextMenuFixture {
            field: cx.new(|cx| ZzClawInputState::new(cx, "read only").readonly(true)),
        });
        let field = fixture.read_with(cx, |fixture, _| fixture.field.clone());
        cx.update(|window, cx| {
            _ = window.draw(cx);
            field.update(cx, |field, cx| field.select_all(window, cx));
            cx.write_to_clipboard(ClipboardItem::new_string("replacement".into()));
        });
        cx.run_until_parked();
        open_input_context_menu(cx);
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("replacement".into())
            );
        });
        open_input_context_menu(cx);
        cx.simulate_keystrokes("down down enter");
        cx.run_until_parked();
        assert_eq!(
            field.read_with(cx, |field, cx| field.value(cx)),
            "read only"
        );
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("read only".into())
            );
        });
        open_input_context_menu(cx);
        cx.simulate_keystrokes("up enter");
        cx.run_until_parked();
        assert_eq!(
            field.read_with(cx, |field, cx| field.value(cx)),
            "read only"
        );
    }

    struct AncestorKeyListenerFixture {
        plain: gpui::Entity<ZzClawInputState>,
        masked: gpui::Entity<ZzClawInputState>,
        handled: Rc<Cell<usize>>,
    }

    struct SubmitOnEnterFixture {
        field: gpui::Entity<ZzClawInputState>,
        submitted: Rc<Cell<usize>>,
    }

    impl Render for SubmitOnEnterFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let submitted = Rc::clone(&self.submitted);
            div()
                .on_key_down(move |event, _, cx| {
                    if event.keystroke.key == "enter" && !event.keystroke.modifiers.shift {
                        submitted.set(submitted.get() + 1);
                        cx.stop_propagation();
                    }
                })
                .child(self.field.clone())
        }
    }

    impl Render for AncestorKeyListenerFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let handled = Rc::clone(&self.handled);
            div()
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                        handled.set(handled.get() + 1);
                        cx.stop_propagation();
                    }
                })
                .child(self.plain.clone())
                .child(self.masked.clone())
        }
    }

    #[test]
    fn code_input_keeps_registered_bash_language() {
        assert!(LanguageRegistry::singleton().language("bash").is_some());

        let mut cx = TestAppContext::single();
        let field = cx.new(|cx| {
            ZzClawInputState::new(cx, "echo hello")
                .code(Some(4))
                .language("bash")
        });

        assert_eq!(
            cx.read_entity(&field, |field, _| field.language.clone()),
            Some("bash".into())
        );
    }

    #[test]
    fn value_tracks_seed_reset_and_clear_before_component_renders() {
        let mut cx = TestAppContext::single();
        let field = cx.new(|cx| ZzClawInputState::new(cx, "seed").placeholder("Name"));

        assert_eq!(cx.read_entity(&field, |field, cx| field.value(cx)), "seed");

        field.update(&mut cx, |field, cx| field.set_content("reset", cx));
        assert_eq!(cx.read_entity(&field, |field, cx| field.value(cx)), "reset");

        field.update(&mut cx, |field, cx| field.clear(cx));
        assert_eq!(cx.read_entity(&field, |field, cx| field.value(cx)), "");
    }

    #[gpui::test]
    fn ancestor_key_listener_does_not_block_plain_or_masked_ascii_input(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let handled = Rc::new(Cell::new(0));
        let (fixture, cx) = cx.add_window_view({
            let handled = Rc::clone(&handled);
            move |_, cx| AncestorKeyListenerFixture {
                plain: cx.new(|cx| ZzClawInputState::new(cx, "")),
                masked: cx.new(|cx| ZzClawInputState::new(cx, "").masked(true)),
                handled,
            }
        });
        let (plain, masked) = fixture.read_with(cx, |fixture, _| {
            (fixture.plain.clone(), fixture.masked.clone())
        });

        cx.update(|window, cx| {
            _ = window.draw(cx);
            window.focus(&plain.read(cx).focus_handle(), cx);
        });
        cx.simulate_keystrokes("a");
        cx.run_until_parked();
        assert_eq!(plain.read_with(cx, |state, cx| state.value(cx)), "a");

        cx.update(|window, cx| window.focus(&masked.read(cx).focus_handle(), cx));
        cx.simulate_keystrokes("b enter escape");
        cx.run_until_parked();
        assert_eq!(masked.read_with(cx, |state, cx| state.value(cx)), "b");
        assert_eq!(handled.get(), 2);
    }

    #[gpui::test]
    fn textarea_submit_on_enter_reaches_ancestor_and_shift_enter_inserts_newline(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let submitted = Rc::new(Cell::new(0));
        let (fixture, cx) = cx.add_window_view({
            let submitted = Rc::clone(&submitted);
            move |_, cx| SubmitOnEnterFixture {
                field: cx.new(|cx| {
                    ZzClawInputState::new(cx, "hello")
                        .multi_line(Some(4))
                        .submit_on_enter(true)
                }),
                submitted,
            }
        });
        let field = fixture.read_with(cx, |fixture, _| fixture.field.clone());
        cx.update(|window, cx| {
            _ = window.draw(cx);
            window.focus(&field.read(cx).focus_handle(), cx);
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(submitted.get(), 1);
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "hello");

        cx.simulate_keystrokes("shift-enter");
        cx.run_until_parked();
        assert_eq!(submitted.get(), 1);
        assert_eq!(field.read_with(cx, |field, cx| field.value(cx)), "\nhello");
    }

    #[test]
    fn multiline_placeholder_collapses_line_breaks() {
        let placeholder = component_placeholder("first\nsecond".into(), true);

        assert_eq!(placeholder.as_ref(), "first second");
    }

    #[test]
    fn multiline_placeholder_collapses_crlf_and_blank_lines() {
        let placeholder = component_placeholder("first\r\n\r\nsecond\n\nthird".into(), true);

        assert_eq!(placeholder.as_ref(), "first second third");
        assert!(!placeholder.contains('\r'));
        assert!(!placeholder.contains('\n'));
    }

    #[test]
    fn single_line_placeholder_is_preserved() {
        let placeholder = component_placeholder("first\nsecond".into(), false);

        assert_eq!(placeholder.as_ref(), "first\nsecond");
    }

    #[test]
    fn multiline_placeholder_preserves_multibyte_text() {
        let placeholder =
            component_placeholder("例如：ls -la\n使用 {{变量名}} 注入动态参数。".into(), true);

        assert_eq!(
            placeholder.as_ref(),
            "例如：ls -la 使用 {{变量名}} 注入动态参数。"
        );
    }
}
