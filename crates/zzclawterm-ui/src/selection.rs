use std::sync::Arc;

use crate::sizing::{form_control_height, form_control_size};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement as _, Render, RenderOnce, SharedString, Styled as _, Subscription,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable, IndexPath, Sizable,
    checkbox::Checkbox,
    radio::RadioGroup,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    switch::Switch,
};

type ZzClawToggleHandler = Box<dyn Fn(&bool, &mut Window, &mut App)>;
type ZzClawIndexSelectHandler = Box<dyn Fn(&usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ZzClawSwitch {
    id: SharedString,
    checked: bool,
    disabled: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ZzClawToggleHandler>,
}

impl ZzClawSwitch {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            checked: false,
            disabled: false,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
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

    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawSwitch {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut switch = Switch::new(self.id).checked(self.checked).small();
        if let Some(tooltip) = self.tooltip {
            switch = switch.tooltip(tooltip);
        }
        if let Some(on_click) = self.on_click {
            switch = switch.on_click(on_click);
        }
        switch.disabled(self.disabled)
    }
}

#[derive(IntoElement)]
pub struct ZzClawCheckbox {
    id: SharedString,
    label: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_click: Option<ZzClawToggleHandler>,
}

impl ZzClawCheckbox {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: None,
            checked: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawCheckbox {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut checkbox = Checkbox::new(self.id).checked(self.checked).small();
        if let Some(label) = self.label {
            checkbox = checkbox.label(label);
        }
        if let Some(on_click) = self.on_click {
            checkbox = checkbox.on_click(on_click);
        }
        checkbox.disabled(self.disabled)
    }
}

#[derive(IntoElement)]
pub struct ZzClawRadioGroup {
    id: SharedString,
    items: Vec<SharedString>,
    selected_index: Option<usize>,
    horizontal: bool,
    disabled: bool,
    on_select: Option<ZzClawIndexSelectHandler>,
}

impl ZzClawRadioGroup {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            selected_index: None,
            horizontal: false,
            disabled: false,
            on_select: None,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.items = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected_index(mut self, selected_index: Option<usize>) -> Self {
        self.selected_index = selected_index;
        self
    }

    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawRadioGroup {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut group = if self.horizontal {
            RadioGroup::horizontal(self.id)
        } else {
            RadioGroup::vertical(self.id)
        }
        .selected_index(self.selected_index)
        .disabled(self.disabled)
        .children(self.items);
        if let Some(on_select) = self.on_select {
            group = group.on_click(on_select);
        }
        group
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZzClawSelectOption {
    value: String,
    label: SharedString,
    search_text: Option<SharedString>,
    subtitle: Option<SharedString>,
    font_family: Option<SharedString>,
}

impl ZzClawSelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            search_text: None,
            subtitle: None,
            font_family: None,
        }
    }

    pub fn search_text(mut self, search_text: impl Into<SharedString>) -> Self {
        self.search_text = Some(search_text.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn font_family(mut self, font_family: impl Into<SharedString>) -> Self {
        self.font_family = Some(font_family.into());
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZzClawSelectItem {
    value: String,
    label: SharedString,
    search_text: Option<SharedString>,
    subtitle: Option<SharedString>,
    font_family: Option<SharedString>,
}

impl gpui_component::select::SelectItem for ZzClawSelectItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        self.font_family.as_ref().map(|font_family| {
            div()
                .font_family(font_family.clone())
                .child(self.label.clone())
                .into_any_element()
        })
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .when_some(self.font_family.clone(), |this, font_family| {
                this.font_family(font_family)
            })
            .child(self.label.clone())
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.label.as_ref().to_lowercase().contains(&query)
            || self
                .search_text
                .as_ref()
                .is_some_and(|search_text| search_text.as_ref().to_lowercase().contains(&query))
            || self
                .subtitle
                .as_ref()
                .is_some_and(|subtitle| subtitle.as_ref().to_lowercase().contains(&query))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZzClawSelectEvent {
    Changed(Option<String>),
}

pub struct ZzClawSelectState {
    state: Option<Entity<SelectState<SearchableVec<ZzClawSelectItem>>>>,
    options: Arc<[ZzClawSelectOption]>,
    selected_value: Option<String>,
    placeholder: SharedString,
    search_placeholder: Option<SharedString>,
    disabled: bool,
    searchable: bool,
    options_dirty: bool,
    selected_dirty: bool,
    focus: FocusHandle,
    trigger_focus: Option<FocusHandle>,
    subscription: Option<Subscription>,
}

impl ZzClawSelectState {
    pub fn new(
        cx: &mut Context<Self>,
        options: impl Into<Vec<ZzClawSelectOption>>,
        selected_value: Option<String>,
    ) -> Self {
        Self::new_shared(cx, options.into().into(), selected_value)
    }

    /// Creates a select state from an immutable catalog shared by sibling controls.
    pub fn new_shared(
        cx: &mut Context<Self>,
        options: Arc<[ZzClawSelectOption]>,
        selected_value: Option<String>,
    ) -> Self {
        Self {
            state: None,
            options,
            selected_value,
            placeholder: SharedString::default(),
            search_placeholder: None,
            disabled: false,
            searchable: false,
            options_dirty: false,
            selected_dirty: false,
            focus: cx.focus_handle(),
            trigger_focus: None,
            subscription: None,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    pub fn search_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.search_placeholder = Some(placeholder.into());
        self
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.selected_value.as_deref()
    }

    pub fn set_options(
        &mut self,
        options: impl Into<Vec<ZzClawSelectOption>>,
        cx: &mut Context<Self>,
    ) {
        let options = options.into();
        if self.options.as_ref() != options.as_slice() {
            self.options = options.into();
            self.options_dirty = self.state.is_some();
            self.selected_dirty = self.state.is_some();
            cx.notify();
        }
    }

    /// Reuses an immutable option catalog when several controls share the same choices.
    ///
    /// Pointer equality makes the steady-state render path constant-time. A content comparison
    /// remains for callers that rebuild an equivalent `Arc` instead of retaining the shared one.
    pub fn set_options_shared(
        &mut self,
        options: Arc<[ZzClawSelectOption]>,
        cx: &mut Context<Self>,
    ) {
        if !Arc::ptr_eq(&self.options, &options) && self.options.as_ref() != options.as_ref() {
            self.options = options;
            self.options_dirty = self.state.is_some();
            self.selected_dirty = self.state.is_some();
            cx.notify();
        }
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        if self.placeholder != placeholder {
            self.placeholder = placeholder;
            cx.notify();
        }
    }

    pub fn set_selected_value(&mut self, selected_value: Option<String>, cx: &mut Context<Self>) {
        if self.selected_value != selected_value {
            self.selected_value = selected_value;
            self.selected_dirty = self.state.is_some();
            cx.notify();
        }
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    pub fn set_searchable(&mut self, searchable: bool, cx: &mut Context<Self>) {
        if self.searchable != searchable {
            self.searchable = searchable;
            self.state = None;
            self.trigger_focus = None;
            self.subscription = None;
            self.options_dirty = false;
            self.selected_dirty = false;
            cx.notify();
        }
    }

    pub fn set_search_placeholder(
        &mut self,
        placeholder: Option<impl Into<SharedString>>,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.map(Into::into);
        if self.search_placeholder != placeholder {
            self.search_placeholder = placeholder;
            cx.notify();
        }
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state
            .as_ref()
            .map(|state| state.read(cx).focus_handle(cx))
            .unwrap_or_else(|| self.focus.clone())
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }

    pub fn is_menu_focused(&self, window: &Window, cx: &App) -> bool {
        let Some(trigger_focus) = self.trigger_focus.as_ref() else {
            return false;
        };
        let focus = self.focus_handle(cx);
        focus != *trigger_focus && focus.is_focused(window)
    }

    fn items(&self) -> Vec<ZzClawSelectItem> {
        self.options
            .iter()
            .map(|option| ZzClawSelectItem {
                value: option.value.clone(),
                label: option.label.clone(),
                search_text: option.search_text.clone(),
                subtitle: option.subtitle.clone(),
                font_family: option.font_family.clone(),
            })
            .collect()
    }

    fn selected_index(&self) -> Option<IndexPath> {
        let selected = self.selected_value.as_ref()?;
        self.options
            .iter()
            .position(|option| &option.value == selected)
            .map(|row| IndexPath::default().row(row))
    }

    fn sync_selected_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_dirty {
            return;
        }
        if let Some(state) = self.state.clone() {
            if let Some(value) = self.selected_value.clone() {
                state.update(cx, |state, cx| state.set_selected_value(&value, window, cx));
            } else {
                state.update(cx, |state, cx| state.set_selected_index(None, window, cx));
            }
        }
        self.selected_dirty = false;
    }

    fn sync_component(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options_dirty {
            if let Some(state) = self.state.clone() {
                let items = SearchableVec::new(self.items());
                state.update(cx, |state, cx| state.set_items(items, window, cx));
            }
            self.options_dirty = false;
        }
        self.sync_selected_value(window, cx);
    }

    fn ensure_component(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<SearchableVec<ZzClawSelectItem>>> {
        if let Some(state) = self.state.clone() {
            return state;
        }

        let items = SearchableVec::new(self.items());
        let selected_index = self.selected_index();
        let searchable = self.searchable;
        let state =
            cx.new(|cx| SelectState::new(items, selected_index, window, cx).searchable(searchable));
        self.trigger_focus = Some(state.read(cx).focus_handle(cx));
        let subscription = cx.subscribe(
            &state,
            |this: &mut Self, _, event: &SelectEvent<SearchableVec<ZzClawSelectItem>>, cx| {
                match event {
                    SelectEvent::Confirm(value) => {
                        this.selected_value = value.clone();
                        this.selected_dirty = false;
                        cx.emit(ZzClawSelectEvent::Changed(value.clone()));
                    }
                }
            },
        );
        self.subscription = Some(subscription);
        self.state = Some(state.clone());
        self.options_dirty = false;
        state
    }
}

impl EventEmitter<ZzClawSelectEvent> for ZzClawSelectState {}

impl Focusable for ZzClawSelectState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        ZzClawSelectState::focus_handle(self, cx)
    }
}

impl Render for ZzClawSelectState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.ensure_component(window, cx);
        self.sync_component(window, cx);
        Select::new(&state)
            .with_size(form_control_size())
            .h(form_control_height())
            .placeholder(self.placeholder.clone())
            .when_some(self.search_placeholder.clone(), |this, placeholder| {
                this.search_placeholder(placeholder)
            })
            .disabled(self.disabled)
    }
}

#[derive(IntoElement)]
pub struct ZzClawSelect {
    state: Entity<ZzClawSelectState>,
    appearance: bool,
    placeholder_content: Option<AnyElement>,
}

impl ZzClawSelect {
    pub fn new(state: &Entity<ZzClawSelectState>) -> Self {
        Self {
            state: state.clone(),
            appearance: true,
            placeholder_content: None,
        }
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn placeholder_content(mut self, content: impl IntoElement) -> Self {
        self.placeholder_content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for ZzClawSelect {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            state,
            appearance,
            placeholder_content,
        } = self;
        let (state, placeholder, search_placeholder, disabled) = state.update(cx, |state, cx| {
            let component = state.ensure_component(window, cx);
            state.sync_component(window, cx);
            (
                component,
                state.placeholder.clone(),
                state.search_placeholder.clone(),
                state.disabled,
            )
        });
        let select = Select::new(&state)
            .with_size(form_control_size())
            .h(form_control_height())
            .appearance(appearance)
            .placeholder(if placeholder_content.is_some() {
                SharedString::default()
            } else {
                placeholder.clone()
            })
            .when_some(search_placeholder, |this, placeholder| {
                this.search_placeholder(placeholder)
            })
            .disabled(disabled);
        // GPUI's select fixes placeholder text to the muted theme color. Overlay only the
        // opt-in placeholder content so saved values can use distinct status styling without
        // changing menu data.
        if let Some(content) = placeholder_content {
            div()
                .relative()
                .size_full()
                .child(select)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(12.))
                        .right(px(32.))
                        .flex()
                        .items_center()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(content),
                )
                .into_any_element()
        } else {
            select.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext};

    use super::{ZzClawSelectOption, ZzClawSelectState};
    use crate::sizing::{NYA_FORM_CONTROL_HEIGHT_PX, form_control_size};

    #[test]
    fn selected_value_tracks_pre_render_updates() {
        let mut cx = TestAppContext::single();
        let select = cx.new(|cx| {
            ZzClawSelectState::new(
                cx,
                vec![
                    ZzClawSelectOption::new("light", "Light"),
                    ZzClawSelectOption::new("dark", "Dark"),
                ],
                Some("light".to_string()),
            )
        });

        assert_eq!(
            cx.read_entity(&select, |select, _| select
                .selected_value()
                .map(str::to_string)),
            Some("light".to_string())
        );

        select.update(&mut cx, |select, cx| {
            select.set_selected_value(Some("dark".to_string()), cx);
        });
        assert_eq!(
            cx.read_entity(&select, |select, _| select
                .selected_value()
                .map(str::to_string)),
            Some("dark".to_string())
        );
    }

    #[test]
    fn shared_options_keep_the_same_catalog_arc() {
        let mut cx = TestAppContext::single();
        let options: Arc<[ZzClawSelectOption]> = vec![
            ZzClawSelectOption::new("light", "Light"),
            ZzClawSelectOption::new("dark", "Dark"),
        ]
        .into();
        let select = cx.new(|cx| {
            ZzClawSelectState::new_shared(cx, Arc::clone(&options), Some("light".to_string()))
        });

        select.update(&mut cx, |select, cx| {
            select.set_options_shared(Arc::clone(&options), cx);
            assert!(Arc::ptr_eq(&select.options, &options));
            assert!(!select.options_dirty);
        });
    }

    #[gpui::test]
    fn selected_value_sync_only_runs_when_dirty(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let select = cx.new(|cx| {
                ZzClawSelectState::new(
                    cx,
                    vec![
                        ZzClawSelectOption::new("light", "Light"),
                        ZzClawSelectOption::new("dark", "Dark"),
                    ],
                    Some("light".to_string()),
                )
            });

            select.update(cx, |select, cx| {
                select.ensure_component(window, cx);
                assert!(!select.selected_dirty);

                select.set_selected_value(Some("light".to_string()), cx);
                assert!(!select.selected_dirty);

                select.set_selected_value(Some("dark".to_string()), cx);
                assert!(select.selected_dirty);

                select.sync_selected_value(window, cx);
                assert!(!select.selected_dirty);
            });
        });
    }

    #[test]
    fn select_uses_standard_form_control_size() {
        assert_eq!(NYA_FORM_CONTROL_HEIGHT_PX, 32.);
        assert_eq!(form_control_size(), gpui_component::Size::Medium);
    }
}
