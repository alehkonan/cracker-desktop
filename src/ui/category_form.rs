use cracker_desktop::{Category, CategoryColor, TransactionKind};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use gpui_kit::{Context, Entity, EventEmitter, IntoElement, Render, Window, div, prelude::*};

use super::category_palette;

pub(super) struct SaveCategory {
    pub category: Category,
    pub editing: bool,
}

pub(super) struct CategoryForm {
    original: Option<Category>,
    name: Entity<InputState>,
    kind: TransactionKind,
    color: CategoryColor,
    kind_locked: bool,
    saving: bool,
    error: Option<String>,
}

impl EventEmitter<SaveCategory> for CategoryForm {}

impl CategoryForm {
    pub fn new(
        category: Option<Category>,
        kind_locked: bool,
        default_color: CategoryColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Category name")
                .default_value(
                    category
                        .as_ref()
                        .map_or("", |category| category.name.as_str()),
                )
        });
        let kind = category
            .as_ref()
            .map_or(TransactionKind::Expense, |category| category.kind);
        let color = category
            .as_ref()
            .map_or(default_color, |category| category.color);
        Self {
            original: category,
            name,
            kind,
            color,
            kind_locked,
            saving: false,
            error: None,
        }
    }

    pub fn saving(&self) -> bool {
        self.saving
    }

    pub fn save_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.saving = false;
        self.error = Some(error);
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        match category_from_fields(
            self.original.as_ref(),
            self.name.read(cx).value().as_str(),
            self.kind,
            self.color,
        ) {
            Ok(category) => {
                self.error = None;
                self.saving = true;
                cx.emit(SaveCategory {
                    category,
                    editing: self.original.is_some(),
                });
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

fn category_from_fields(
    original: Option<&Category>,
    name: &str,
    kind: TransactionKind,
    color: CategoryColor,
) -> Result<Category, String> {
    let mut category = Category::new(name.trim(), kind).map_err(|error| error.to_string())?;
    if let Some(original) = original {
        category.id = original.id;
    }
    category.color = color;
    Ok(category)
}

impl Render for CategoryForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let dark = theme.is_dark();
        let background = theme.background;
        let border = theme.foreground;
        let mut types = div().flex().gap_2();
        for (index, kind) in [TransactionKind::Income, TransactionKind::Expense]
            .into_iter()
            .enumerate()
        {
            types = types.child(
                Button::new(("category-kind", index))
                    .label(if kind == TransactionKind::Income {
                        "Income"
                    } else {
                        "Expense"
                    })
                    .when(self.kind == kind, |button| button.primary())
                    .disabled(self.kind_locked || self.saving)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = kind;
                        cx.notify();
                    })),
            );
        }
        let mut palette = div().flex().flex_col().gap_2();
        for row in CategoryColor::ALL.chunks(4) {
            let mut colors = div().flex().gap_2();
            for &color in row {
                let selected = self.color == color;
                let name = category_palette::name(color);
                colors = colors.child(
                    Button::new(("category-color", color.index()))
                        .flex_1()
                        .min_w_0()
                        .accessibility_label(format!(
                            "{name}{}",
                            if selected { ", selected" } else { "" }
                        ))
                        .when(selected, |button| button.border_2().border_color(border))
                        .disabled(self.saving)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(category_palette::swatch(color, dark, background, border))
                                .child(div().text_sm().child(name)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.color = color;
                            cx.notify();
                        })),
                );
            }
            palette = palette.child(colors);
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Name")
                    .child(Input::new(&self.name).disabled(self.saving)),
            )
            .child(div().flex().flex_col().gap_2().child("Type").child(types))
            .when(self.kind_locked, |view| {
                view.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Type cannot be changed because this category has transactions."),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(format!(
                        "Color · {} selected",
                        category_palette::name(self.color)
                    ))
                    .child(palette),
            )
            .when_some(self.error.as_ref(), |view, error| {
                view.child(div().text_color(danger).child(error.clone()))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-category")
                            .label("Cancel")
                            .disabled(self.saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-category")
                            .primary()
                            .label(if self.saving {
                                "Saving…"
                            } else if self.original.is_some() {
                                "Save changes"
                            } else {
                                "Add category"
                            })
                            .disabled(self.saving)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Invalid test fixtures must fail immediately."
)]
mod tests {
    use super::*;

    #[test]
    fn validates_category_fields_and_preserves_id_and_color_on_edit() {
        let color = CategoryColor::ALL[8];
        let original =
            category_from_fields(None, "  Food  ", TransactionKind::Expense, color).unwrap();
        assert_eq!(original.name, "Food");
        assert_eq!(original.color, color);
        let updated = category_from_fields(
            Some(&original),
            "Groceries",
            TransactionKind::Expense,
            CategoryColor::ALL[4],
        )
        .unwrap();
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.color, CategoryColor::ALL[4]);
        assert!(category_from_fields(None, " \t ", TransactionKind::Expense, color).is_err());
    }
}
