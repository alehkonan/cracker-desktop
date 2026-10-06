use cracker_desktop::{Snapshot, format_money};
use gpui_kit::component::{
    ActiveTheme,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, IntoElement, div, prelude::*};

use super::TrackerApp;

pub(super) fn accounts_panel(
    snapshot: &Snapshot,
    cx: &mut Context<TrackerApp>,
) -> impl IntoElement {
    let theme = cx.theme();
    let mut list = div()
        .id("accounts-list")
        .flex()
        .flex_col()
        .gap_3()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll();
    if snapshot.accounts.is_empty() {
        list = list.child(
            div()
                .p_6()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted)
                .child("No accounts yet. Add your first account to get started."),
        );
    }
    for (index, account) in snapshot.accounts.iter().enumerate() {
        let id = account.id;
        list =
            list.child(
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
                            .child(div().text_lg().child(account.name.clone()))
                            .child(div().text_sm().text_color(theme.muted_foreground).child(
                                format!(
                                    "Balance: {} · Opening balance: {} · {}",
                                    snapshot.account_balance(account.id).map_or_else(
                                        |error| format!("Error: {error}"),
                                        |balance| format_money(balance, account.currency)
                                    ),
                                    format_money(account.opening_balance_minor, account.currency),
                                    if account.is_active {
                                        "Active"
                                    } else {
                                        "Inactive"
                                    },
                                ),
                            )),
                    )
                    .child(Button::new(("edit-account", index)).label("Edit").on_click(
                        cx.listener(move |this, _, window, cx| this.edit_account(id, window, cx)),
                    )),
            );
    }
    div()
        .flex()
        .flex_col()
        .gap_4()
        .flex_1()
        .min_h_0()
        .child(
            div().flex().justify_end().child(
                Button::new("add-account")
                    .primary()
                    .label("Add account")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_account_form(None, window, cx)),
                    ),
            ),
        )
        .child(list)
}
