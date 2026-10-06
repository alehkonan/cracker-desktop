use super::TrackerApp;
use cracker_desktop::{Snapshot, TransactionKind, format_money};
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, IntoElement, div, prelude::*};

pub(super) fn transactions_panel(
    snapshot: &Snapshot,
    cx: &mut Context<TrackerApp>,
) -> impl IntoElement {
    let theme = cx.theme();
    let mut list = div()
        .id("transactions-list")
        .flex()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll();
    if snapshot.transactions.is_empty() {
        list = list.child(
            div()
                .p_6()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted)
                .child("No transactions yet. Add an income, expense, or transfer."),
        );
    }
    for (index, transaction) in snapshot.transactions.iter().rev().enumerate() {
        let id = transaction.id;
        let account = snapshot
            .accounts
            .iter()
            .find(|account| account.id == transaction.account_id);
        let category = transaction.category_id.and_then(|id| {
            snapshot
                .categories
                .iter()
                .find(|category| category.id == id)
        });
        let kind = match transaction.kind {
            TransactionKind::Income => "Income",
            TransactionKind::Expense => "Expense",
            TransactionKind::Transfer => "Transfer",
        };
        let mut amount = account.map_or_else(
            || "Unknown account".to_owned(),
            |account| format_money(transaction.amount_minor, account.currency),
        );
        let mut account_name = account.map_or_else(
            || "Unknown account".to_owned(),
            |account| account.name.clone(),
        );
        if let Some(destination) = transaction
            .transfer_account_id
            .and_then(|id| snapshot.accounts.iter().find(|account| account.id == id))
        {
            account_name.push_str(&format!(" → {}", destination.name));
            if let Some(received) = transaction.transfer_amount_minor {
                amount.push_str(&format!(
                    " → {}",
                    format_money(received, destination.currency)
                ));
            }
        }
        let details = format!(
            "{} {:02}:{:02}:{:02} · {} · {}",
            transaction.date,
            transaction.time.hour(),
            transaction.time.minute(),
            transaction.time.second(),
            account_name,
            category.map_or("No category", |category| category.name.as_str())
        );
        list = list.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_lg().child(format!("{kind} · {amount}")))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(details),
                        )
                        .when(!transaction.note.is_empty(), |view| {
                            view.child(transaction.note.clone())
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new(("edit-transaction", index))
                                .label("Edit")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.edit_transaction(id, window, cx)
                                })),
                        )
                        .child(
                            Button::new(("delete-transaction", index))
                                .danger()
                                .label("Delete")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_delete_transaction(id, window, cx)
                                })),
                        ),
                ),
        );
    }
    let can_add = snapshot.active_accounts().next().is_some();
    div()
        .flex()
        .flex_col()
        .gap_4()
        .flex_1()
        .min_h_0()
        .child(
            div().flex().justify_end().child(
                Button::new("add-transaction")
                    .primary()
                    .label("Add transaction")
                    .disabled(!can_add)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_transaction_form(None, window, cx)
                    })),
            ),
        )
        .when(!can_add, |view| {
            view.child("Add or activate an account before creating a transaction.")
        })
        .child(list)
}
