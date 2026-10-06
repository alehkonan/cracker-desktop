use cracker_desktop::{Account, Currency, format_money, parse_money};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
    switch::Switch,
};
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, Render, SharedString, Window, div, prelude::*,
};

#[derive(Clone)]
struct CurrencyItem(Currency);

impl SearchableListItem for CurrencyItem {
    type Value = Currency;

    fn title(&self) -> SharedString {
        format!("{} — {}", self.0.code(), self.0.name()).into()
    }

    fn value(&self) -> &Currency {
        &self.0
    }
}

pub struct SaveAccount {
    pub account: Account,
    pub editing: bool,
}

pub struct AccountForm {
    original: Option<Account>,
    name: Entity<InputState>,
    currency: Entity<SelectState<SearchableVec<CurrencyItem>>>,
    balance: Entity<InputState>,
    is_active: bool,
    currency_locked: bool,
    saving: bool,
    error: Option<String>,
}

impl EventEmitter<SaveAccount> for AccountForm {}

impl AccountForm {
    pub fn new(
        account: Option<Account>,
        currency_locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Account name")
                .default_value(account.as_ref().map_or("", |account| account.name.as_str()))
        });
        let currency = cx.new(|cx| {
            let items = Currency::all()
                .iter()
                .copied()
                .map(CurrencyItem)
                .collect::<Vec<_>>();
            let mut state =
                SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true);
            if let Some(account) = &account {
                state.set_selected_value(&account.currency, window, cx);
            }
            state
        });
        let balance_value = account.as_ref().map_or_else(
            || "0".to_owned(),
            |account| {
                let mut formatted = format_money(account.opening_balance_minor, account.currency);
                if let Some(separator) = formatted.rfind(' ') {
                    formatted.truncate(separator);
                }
                formatted
            },
        );
        let balance = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Opening balance")
                .default_value(balance_value)
        });
        let is_active = account.as_ref().is_none_or(|account| account.is_active);
        Self {
            original: account,
            name,
            currency,
            balance,
            is_active,
            currency_locked,
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
        let result = account_from_fields(
            self.original.as_ref(),
            self.name.read(cx).value().as_str(),
            self.currency.read(cx).selected_value().copied(),
            self.balance.read(cx).value().as_str(),
            self.is_active,
        );
        match result {
            Ok(account) => {
                self.error = None;
                self.saving = true;
                cx.emit(SaveAccount {
                    account,
                    editing: self.original.is_some(),
                });
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

fn account_from_fields(
    original: Option<&Account>,
    name: &str,
    currency: Option<Currency>,
    balance: &str,
    is_active: bool,
) -> Result<Account, String> {
    let currency = currency.ok_or_else(|| "Select a currency.".to_owned())?;
    let amount = parse_money(balance, currency).map_err(|error| error.to_string())?;
    let mut account =
        Account::new(name.trim(), currency, amount).map_err(|error| error.to_string())?;
    if let Some(original) = original {
        account.id = original.id;
    }
    account.is_active = is_active;
    Ok(account)
}

impl Render for AccountForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
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
            .child(
                div().flex().flex_col().gap_2().child("Currency").child(
                    Select::new(&self.currency)
                        .placeholder("Select a currency")
                        .search_placeholder("Search by code or name…")
                        .disabled(self.currency_locked || self.saving)
                        .w_full(),
                ),
            )
            .when(self.currency_locked, |view| {
                view.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Currency cannot be changed because this account has transactions."),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Opening balance")
                    .child(Input::new(&self.balance).disabled(self.saving)),
            )
            .child(div().text_sm().text_color(muted).child(
                "Use a decimal point, without grouping separators. Negative balances are allowed.",
            ))
            .child(
                Switch::new("account-active")
                    .label("Active account")
                    .checked(self.is_active)
                    .disabled(self.saving)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.is_active = *checked;
                        cx.notify();
                    })),
            )
            .child(div().text_sm().text_color(muted).child(
                "Inactive accounts remain in history but are not offered for new transactions.",
            ))
            .when_some(self.error.as_ref(), |view, error| {
                view.child(div().text_color(danger).child(error.clone()))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-account")
                            .label("Cancel")
                            .disabled(self.saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-account")
                            .primary()
                            .label(if self.saving {
                                "Saving…"
                            } else if self.original.is_some() {
                                "Save changes"
                            } else {
                                "Add account"
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
    fn currency_search_matches_code_and_name() {
        let item = CurrencyItem("CHF".parse().unwrap());
        assert!(item.matches("chf"));
        assert!(item.matches("SWISS"));
        assert!(!item.matches("dollar"));
        assert!(
            Currency::all()
                .iter()
                .copied()
                .map(CurrencyItem)
                .all(|item| item.matches(item.0.code()))
        );
    }

    #[test]
    fn form_validates_fields_and_preserves_identity_on_edit() {
        let kwd = "KWD".parse().unwrap();
        let original = account_from_fields(None, "  Cash  ", Some(kwd), "-12.345", true).unwrap();
        assert_eq!(original.name, "Cash");
        assert_eq!(original.opening_balance_minor, -12345);
        assert!(original.is_active);
        let updated =
            account_from_fields(Some(&original), "Wallet", Some(kwd), "0", false).unwrap();
        assert_eq!(updated.id, original.id);
        assert!(!updated.is_active);
        assert!(account_from_fields(None, " ", Some(kwd), "0", true).is_err());
        assert!(account_from_fields(None, "Cash", None, "0", true).is_err());
        assert!(account_from_fields(None, "Cash", Some(kwd), "0.0001", true).is_err());
        assert!(account_from_fields(None, "Cash", Some(kwd), "not money", true).is_err());
    }
}
