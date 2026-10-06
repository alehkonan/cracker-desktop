use cracker_desktop::Category;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, EventEmitter, IntoElement, Render, Window, div, prelude::*};
use uuid::Uuid;

pub(super) struct DeleteCategory(pub Uuid);

pub(super) struct CategoryDelete {
    category: Category,
    transaction_count: usize,
    deleting: bool,
    error: Option<String>,
}

impl EventEmitter<DeleteCategory> for CategoryDelete {}

impl CategoryDelete {
    pub fn new(category: Category, transaction_count: usize) -> Self {
        Self {
            category,
            transaction_count,
            deleting: false,
            error: None,
        }
    }

    pub fn deleting(&self) -> bool {
        self.deleting
    }

    pub fn delete_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.deleting = false;
        self.error = Some(error);
        cx.notify();
    }
}

impl Render for CategoryDelete {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        div().flex().flex_col().gap_4()
            .child(format!("Delete the category “{}”?", self.category.name))
            .child(format!("{} linked transaction(s) will remain in history without a category. Their type, amount, date, and account will not change.", self.transaction_count))
            .child("This action cannot be undone.")
            .when_some(self.error.as_ref(), |view, error| view.child(div().text_color(danger).child(error.clone())))
            .child(div().flex().justify_end().gap_2()
                .child(Button::new("cancel-delete-category").label("Cancel").disabled(self.deleting)
                    .on_click(|_, window, cx| window.close_dialog(cx)))
                .child(Button::new("confirm-delete-category").danger()
                    .label(if self.deleting { "Deleting…" } else { "Delete category" }).disabled(self.deleting)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.deleting {
                            this.deleting = true;
                            this.error = None;
                            cx.emit(DeleteCategory(this.category.id));
                            cx.notify();
                        }
                    }))))
    }
}
