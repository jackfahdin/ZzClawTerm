use rust_i18n::t;

use gpui::{
    AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px, rgb,
};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};

impl NyaTermApp {
    pub(in crate::features) fn quick_command_category_rename_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rename = self.commands.quick_category_rename();
        let draft = rename
            .map(|rename| rename.draft.clone())
            .unwrap_or_default();
        let error = rename.and_then(|rename| rename.error.clone());
        // Renaming never moves the row, so there is nothing to say about placement.
        self.quick_command_category_name_dialog_content(
            "quick-command.category-rename",
            &draft,
            error,
            None,
            cx,
        )
    }

    pub(in crate::features) fn quick_command_category_create_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let create = self.commands.quick_category_create();
        let draft = create
            .map(|create| create.draft.clone())
            .unwrap_or_default();
        let error = create.and_then(|create| create.error.clone());
        // The dialog is reached from a group row as well as from a pseudo-row, so say
        // which one the new category lands under.
        let parent_name =
            create
                .and_then(|create| create.parent_id.clone())
                .and_then(|parent_id| {
                    self.commands
                        .quick_command_categories()
                        .iter()
                        .find(|category| category.id == parent_id)
                        .map(|category| category.name.clone())
                });
        let hint = match parent_name {
            Some(parent_name) => t!(
                "quickCommands.newCategoryParentHint",
                category = parent_name
            )
            .to_string(),
            None => t!("quickCommands.newCategoryRootHint").to_string(),
        };
        self.quick_command_category_name_dialog_content(
            "quick-command.category-create",
            &draft,
            error,
            Some(hint),
            cx,
        )
    }

    /// Shared body for both category-name dialogs. They differ only in which draft
    /// they read and which text-input id owns the box, so the field id is a
    /// parameter rather than the two growing separate layouts.
    fn quick_command_category_name_dialog_content(
        &mut self,
        input_id: &'static str,
        draft: &str,
        error: Option<String>,
        hint: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme_palette();
        let input = self
            .text_input_box(
                input_id,
                draft,
                TextInputSetup::placeholder(t!("quickCommands.categoryName")),
                cx,
            )
            .into_any_element();

        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(input)
            .when_some(hint, |this, hint| {
                this.child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(hint),
                )
            })
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.danger))
                        .child(error),
                )
            })
            .into_any_element()
    }
}
