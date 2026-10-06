use cracker_desktop::Transaction;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, EventEmitter, IntoElement, Render, Window, div, prelude::*};
use uuid::Uuid;

pub(super) struct DeleteTransaction(pub Uuid);

pub(super) struct TransactionDelete {
    transaction: Transaction,
    deleting: bool,
    error: Option<String>,
}

impl EventEmitter<DeleteTransaction> for TransactionDelete {}

impl TransactionDelete {
    pub fn new(transaction: Transaction) -> Self {
        Self {
            transaction,
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

impl Render for TransactionDelete {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        div().flex().flex_col().gap_4()
            .child("Delete this transaction?")
            .child("Account balances will be recalculated. Deleting a transfer removes both its debit and credit.")
            .child("This action cannot be undone.")
            .when_some(self.error.as_ref(), |view, error| view.child(div().text_color(danger).child(error.clone())))
            .child(div().flex().justify_end().gap_2()
                .child(Button::new("cancel-delete-transaction").label("Cancel").disabled(self.deleting)
                    .on_click(|_, window, cx| window.close_dialog(cx)))
                .child(Button::new("confirm-delete-transaction").danger()
                    .label(if self.deleting { "Deleting…" } else { "Delete transaction" }).disabled(self.deleting)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.deleting {
                            this.deleting = true;
                            this.error = None;
                            cx.emit(DeleteTransaction(this.transaction.id));
                            cx.notify();
                        }
                    }))))
    }
}
