use gpui::{
    AnyElement, App, ClickEvent, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    Styled as _, Window, prelude::FluentBuilder as _,
};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, Selectable, Sizable};

type ZzClawButtonClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZzClawButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(IntoElement)]
pub struct ZzClawButton {
    id: SharedString,
    label: SharedString,
    icon_path: Option<SharedString>,
    content: Option<AnyElement>,
    variant: ZzClawButtonVariant,
    height: Option<Pixels>,
    small: bool,
    compact: bool,
    full_width: bool,
    selected: bool,
    disabled: bool,
    loading: bool,
    autofocus: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ZzClawButtonClickHandler>,
}

impl ZzClawButton {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon_path: None,
            content: None,
            variant: ZzClawButtonVariant::Secondary,
            height: None,
            small: false,
            compact: false,
            full_width: false,
            selected: false,
            disabled: false,
            loading: false,
            autofocus: false,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    pub fn icon(mut self, icon_path: impl Into<SharedString>) -> Self {
        self.icon_path = Some(icon_path.into());
        self
    }

    pub fn variant(mut self, variant: ZzClawButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Focus this button when it first appears, without stealing focus on redraw.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.autofocus && !self.disabled && !self.loading {
            // Initialize the same keyed focus state used by gpui-kit's Button.
            // Render it in this scope below so its focus ring and keyboard clicks
            // use this handle too. Keyed state expires when the button disappears.
            window.use_keyed_state(self.id.clone(), cx, |window, cx| {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                focus
            });
        }
        let mut button = Button::new(self.id).label(self.label).loading(self.loading);
        if let Some(icon_path) = self.icon_path {
            button = button.icon(Icon::default().path(icon_path));
        }
        if let Some(content) = self.content {
            button = button.child(content);
        }
        if self.small {
            button = button.small();
        }
        if self.compact {
            button = button.compact();
        }
        if self.full_width {
            button = button.w_full();
        }
        if self.selected {
            button = button.selected(true);
        }
        button = match self.variant {
            ZzClawButtonVariant::Primary => button.primary(),
            ZzClawButtonVariant::Secondary => button,
            ZzClawButtonVariant::Ghost => button.ghost(),
            ZzClawButtonVariant::Danger => button.danger(),
        };
        if let Some(tooltip) = self.tooltip {
            button = button.tooltip(tooltip);
        }
        if let Some(on_click) = self.on_click {
            button = button.on_click(on_click);
        }
        if let Some(height) = self.height {
            button = button.h(height);
        }
        button.disabled(self.disabled).render(window, cx)
    }
}

#[derive(IntoElement)]
pub struct ZzClawIconButton {
    id: SharedString,
    icon_path: SharedString,
    icon_size: Option<Pixels>,
    size: Option<Pixels>,
    disabled: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ZzClawButtonClickHandler>,
}

impl ZzClawIconButton {
    pub fn new(id: impl Into<SharedString>, icon_path: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon_path: icon_path.into(),
            icon_size: None,
            size: None,
            disabled: false,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn icon_size(mut self, size: Pixels) -> Self {
        self.icon_size = Some(size);
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = Some(size);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawIconButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon = Icon::default()
            .path(self.icon_path)
            .when_some(self.icon_size, |icon, size| icon.with_size(size));
        let mut button = Button::new(self.id).icon(icon).ghost().small();
        if let Some(size) = self.size {
            button = button.size(size);
        }
        if let Some(tooltip) = self.tooltip {
            button = button.tooltip(tooltip);
        }
        if let Some(on_click) = self.on_click {
            button = button.on_click(on_click);
        }
        button.disabled(self.disabled)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent,
        Keystroke, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
        Window, div,
    };

    use super::ZzClawButton;

    struct AutofocusHost {
        show_button: bool,
        other_focus: FocusHandle,
        clicks: usize,
    }

    fn press_enter(cx: &mut VisualTestContext) {
        let keystroke = Keystroke::parse("enter").unwrap();
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
    }

    impl Render for AutofocusHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut root = div()
                .size_full()
                .child(div().track_focus(&self.other_focus));
            if self.show_button {
                root = root.child(ZzClawButton::new("paste", "Paste").autofocus().on_click(
                    cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    }),
                ));
            }
            root
        }
    }

    #[gpui::test]
    fn autofocus_supports_enter_without_stealing_focus_on_redraw(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (host, cx) = cx.add_window_view(|_, cx| AutofocusHost {
            show_button: true,
            other_focus: cx.focus_handle(),
            clicks: 0,
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
            assert!(window.focused(cx).is_some());
        });
        press_enter(cx);
        assert_eq!(host.read_with(cx, |host, _| host.clicks), 1);

        cx.update(|window, cx| {
            let other_focus = host.read(cx).other_focus.clone();
            window.focus(&other_focus, cx);
            host.update(cx, |_, cx| cx.notify());
            let _ = window.draw(cx);
            assert!(host.read(cx).other_focus.is_focused(window));
        });
        press_enter(cx);
        assert_eq!(host.read_with(cx, |host, _| host.clicks), 1);

        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.show_button = false;
                cx.notify();
            });
            let _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.show_button = true;
                cx.notify();
            });
            let _ = window.draw(cx);
        });
        press_enter(cx);
        assert_eq!(host.read_with(cx, |host, _| host.clicks), 2);
    }

    #[test]
    fn ordinary_buttons_keep_content_width_and_no_icon_by_default() {
        let button = ZzClawButton::new("action", "Action");

        assert!(!button.full_width);
        assert!(button.icon_path.is_none());
    }

    #[test]
    fn full_width_is_opt_in_for_equal_width_dialog_actions() {
        let button = ZzClawButton::new("website", "Website").small().full_width();

        assert!(button.full_width);
        assert!(button.small);
        assert_eq!(button.label.as_ref(), "Website");
    }

    #[test]
    fn adding_an_icon_preserves_the_accessible_button_label() {
        let button = ZzClawButton::new("copy", "Copy all").icon("icons/copy.svg");

        assert_eq!(button.icon_path.as_deref(), Some("icons/copy.svg"));
        assert_eq!(button.label.as_ref(), "Copy all");
    }
}
