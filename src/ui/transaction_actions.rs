use cracker_desktop::{Storage, Transaction};
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, Window, div, prelude::*, px};
use uuid::Uuid;

use super::{
    TrackerApp,
    transaction_delete::{DeleteTransaction, TransactionDelete},
    transaction_form::{SaveTransaction, TransactionForm},
};

impl TrackerApp {
    pub(super) fn edit_transaction(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(transaction) = self
            .snapshot
            .transactions
            .iter()
            .find(|transaction| transaction.id == id)
        {
            self.open_transaction_form(Some(transaction.clone()), window, cx);
        }
    }

    pub(super) fn open_transaction_form(
        &mut self,
        transaction: Option<Transaction>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = if transaction.is_some() {
            "Edit transaction"
        } else {
            "Add transaction"
        };
        let form = cx.new(|cx| TransactionForm::new(transaction, &self.snapshot, window, cx));
        cx.subscribe_in(
            &form,
            window,
            |this, form, event: &SaveTransaction, window, cx| {
                let form = form.clone();
                let path = this.database_path.clone();
                let transaction = event.transaction.clone();
                let editing = event.editing;
                let write = cx.background_executor().spawn(async move {
                    let mut storage = Storage::open(&path)?;
                    if editing {
                        storage.update_transaction(&transaction)
                    } else {
                        storage.create_transaction(&transaction)
                    }
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = write.await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(snapshot) => {
                            this.snapshot = snapshot;
                            window.close_dialog(cx);
                            cx.notify();
                        }
                        Err(error) => {
                            form.update(cx, |form, cx| form.save_failed(format!("{error:#}"), cx))
                        }
                    });
                })
                .detach();
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancel_form = form.clone();
            dialog
                .title(title)
                .w(px(560.0))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel_form.read(cx).saving())
                .child(form.clone())
                .footer(div())
        });
    }

    pub(super) fn confirm_delete_transaction(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(transaction) = self
            .snapshot
            .transactions
            .iter()
            .find(|transaction| transaction.id == id)
            .cloned()
        else {
            return;
        };
        let confirmation = cx.new(|_| TransactionDelete::new(transaction));
        cx.subscribe_in(
            &confirmation,
            window,
            |this, confirmation, event: &DeleteTransaction, window, cx| {
                let confirmation = confirmation.clone();
                let path = this.database_path.clone();
                let id = event.0;
                let write = cx
                    .background_executor()
                    .spawn(async move { Storage::open(&path)?.delete_transaction(id) });
                cx.spawn_in(window, async move |this, cx| {
                    let result = write.await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(snapshot) => {
                            this.snapshot = snapshot;
                            window.close_dialog(cx);
                            cx.notify();
                        }
                        Err(error) => confirmation.update(cx, |confirmation, cx| {
                            confirmation.delete_failed(format!("{error:#}"), cx)
                        }),
                    });
                })
                .detach();
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancel_confirmation = confirmation.clone();
            dialog
                .title("Delete transaction")
                .w(px(520.0))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel_confirmation.read(cx).deleting())
                .child(confirmation.clone())
                .footer(div())
        });
    }
}
