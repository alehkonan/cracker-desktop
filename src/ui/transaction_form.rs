use cracker_desktop::{
    Account, Category, Snapshot, Transaction, TransactionKind, format_money, parse_money,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
};
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, Render, SharedString, Window, div, prelude::*, px,
};
use time::{Date, OffsetDateTime, Time, macros::format_description};
use uuid::Uuid;

#[derive(Clone)]
struct Choice<T> {
    title: SharedString,
    value: T,
}

impl<T: Clone + PartialEq> SearchableListItem for Choice<T> {
    type Value = T;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &T {
        &self.value
    }
}

type AccountSelect = SelectState<SearchableVec<Choice<Uuid>>>;
type CategorySelect = SelectState<SearchableVec<Choice<Option<Uuid>>>>;

pub(super) struct SaveTransaction {
    pub transaction: Transaction,
    pub editing: bool,
}

pub(super) struct TransactionForm {
    original: Option<Transaction>,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    source: Entity<AccountSelect>,
    destination: Entity<AccountSelect>,
    category: Entity<CategorySelect>,
    kind: TransactionKind,
    amount: Entity<InputState>,
    destination_amount: Entity<InputState>,
    date: Entity<InputState>,
    time: Entity<InputState>,
    note: Entity<InputState>,
    saving: bool,
    error: Option<String>,
}

impl EventEmitter<SaveTransaction> for TransactionForm {}

impl TransactionForm {
    pub(super) fn new(
        original: Option<Transaction>,
        snapshot: &Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = cx.new(|cx| {
            let mut state = SelectState::new(
                SearchableVec::new(account_choices(
                    &snapshot.accounts,
                    original.as_ref().map(|t| t.account_id),
                )),
                None,
                window,
                cx,
            )
            .searchable(true);
            if let Some(transaction) = &original {
                state.set_selected_value(&transaction.account_id, window, cx);
            }
            state
        });
        let destination = cx.new(|cx| {
            let mut state = SelectState::new(
                SearchableVec::new(account_choices(
                    &snapshot.accounts,
                    original.as_ref().and_then(|t| t.transfer_account_id),
                )),
                None,
                window,
                cx,
            )
            .searchable(true);
            if let Some(id) = original.as_ref().and_then(|t| t.transfer_account_id) {
                state.set_selected_value(&id, window, cx);
            }
            state
        });
        let kind = original
            .as_ref()
            .map_or(TransactionKind::Expense, |t| t.kind);
        let category = cx.new(|cx| {
            let mut state = SelectState::new(
                SearchableVec::new(category_choices(&snapshot.categories, kind)),
                None,
                window,
                cx,
            )
            .searchable(true);
            state.set_selected_value(&original.as_ref().and_then(|t| t.category_id), window, cx);
            state
        });
        // Select state changes must redraw the currency labels, not just the selects.
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        cx.observe(&destination, |_, _, cx| cx.notify()).detach();
        let amount_value = original
            .as_ref()
            .and_then(|t| {
                snapshot
                    .accounts
                    .iter()
                    .find(|a| a.id == t.account_id)
                    .map(|a| amount_text(t.amount_minor, a))
            })
            .unwrap_or_default();
        let destination_value = original
            .as_ref()
            .and_then(|t| {
                let id = t.transfer_account_id?;
                let amount = t.transfer_amount_minor?;
                snapshot
                    .accounts
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| amount_text(amount, a))
            })
            .unwrap_or_default();
        let amount = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Amount")
                .default_value(amount_value)
        });
        let destination_amount = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Amount received")
                .default_value(destination_value)
        });
        let date = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("yyyy-mm-dd (blank = today)")
                .default_value(
                    original
                        .as_ref()
                        .map_or_else(String::new, |t| t.date.to_string()),
                )
        });
        let time = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("hh:mm:ss (blank = now)")
                .default_value(original.as_ref().map_or_else(String::new, |t| {
                    format!(
                        "{:02}:{:02}:{:02}",
                        t.time.hour(),
                        t.time.minute(),
                        t.time.second()
                    )
                }))
        });
        let note = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Optional note")
                .default_value(original.as_ref().map_or("", |t| t.note.as_str()))
        });
        Self {
            original,
            accounts: snapshot.accounts.clone(),
            categories: snapshot.categories.clone(),
            source,
            destination,
            category,
            kind,
            amount,
            destination_amount,
            date,
            time,
            note,
            saving: false,
            error: None,
        }
    }

    pub(super) fn saving(&self) -> bool {
        self.saving
    }

    pub(super) fn save_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.saving = false;
        self.error = Some(error);
        cx.notify();
    }

    fn change_kind(&mut self, kind: TransactionKind, window: &mut Window, cx: &mut Context<Self>) {
        self.kind = kind;
        let items = category_choices(&self.categories, kind);
        self.category.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(items), window, cx);
            state.set_selected_value(&None, window, cx);
        });
        self.error = None;
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let result = transaction_from_fields(
            self.original.as_ref(),
            &self.accounts,
            &self.categories,
            Fields {
                source: self.source.read(cx).selected_value().copied(),
                destination: self.destination.read(cx).selected_value().copied(),
                category: self.category.read(cx).selected_value().copied().flatten(),
                kind: self.kind,
                amount: self.amount.read(cx).value().as_str(),
                destination_amount: self.destination_amount.read(cx).value().as_str(),
                date: self.date.read(cx).value().as_str(),
                time: self.time.read(cx).value().as_str(),
                note: self.note.read(cx).value().as_str(),
            },
            local_now,
        );
        match result {
            Ok(transaction) => {
                self.error = None;
                self.saving = true;
                cx.emit(SaveTransaction {
                    transaction,
                    editing: self.original.is_some(),
                });
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

fn eligible_account(account: &Account, original_id: Option<Uuid>) -> bool {
    account.is_active || Some(account.id) == original_id
}

fn account_choices(accounts: &[Account], original_id: Option<Uuid>) -> Vec<Choice<Uuid>> {
    accounts
        .iter()
        .filter(|a| eligible_account(a, original_id))
        .map(|a| Choice {
            title: format!(
                "{} — {}{}",
                a.name,
                a.currency.code(),
                if a.is_active { "" } else { " (inactive)" }
            )
            .into(),
            value: a.id,
        })
        .collect()
}

fn category_choices(categories: &[Category], kind: TransactionKind) -> Vec<Choice<Option<Uuid>>> {
    let mut items = vec![Choice {
        title: "No category".into(),
        value: None,
    }];
    items.extend(
        categories
            .iter()
            .filter(|c| kind != TransactionKind::Transfer && c.kind == kind)
            .map(|c| Choice {
                title: c.name.clone().into(),
                value: Some(c.id),
            }),
    );
    items
}

fn amount_text(amount: i64, account: &Account) -> String {
    let formatted = format_money(amount, account.currency);
    formatted
        .rsplit_once(' ')
        .map_or_else(|| formatted.clone(), |(amount, _)| amount.to_owned())
}

struct Fields<'a> {
    source: Option<Uuid>,
    destination: Option<Uuid>,
    category: Option<Uuid>,
    kind: TransactionKind,
    amount: &'a str,
    destination_amount: &'a str,
    date: &'a str,
    time: &'a str,
    note: &'a str,
}

fn transaction_from_fields(
    original: Option<&Transaction>,
    accounts: &[Account],
    categories: &[Category],
    fields: Fields<'_>,
    now: impl FnOnce() -> Result<(Date, Time), String>,
) -> Result<Transaction, String> {
    let source = accounts
        .iter()
        .find(|a| {
            Some(a.id) == fields.source && eligible_account(a, original.map(|t| t.account_id))
        })
        .ok_or_else(|| "Select an eligible source account.".to_owned())?;
    let amount = parse_money(fields.amount.trim(), source.currency)
        .map_err(|e| format!("Source amount: {e}"))?;
    let (destination_id, destination_amount, category_id) =
        if fields.kind == TransactionKind::Transfer {
            let destination = accounts
                .iter()
                .find(|a| {
                    Some(a.id) == fields.destination
                        && eligible_account(a, original.and_then(|t| t.transfer_account_id))
                })
                .ok_or_else(|| "Select an eligible destination account.".to_owned())?;
            if destination.id == source.id {
                return Err("Source and destination accounts must be different.".to_owned());
            }
            let received = if source.currency == destination.currency {
                amount
            } else {
                if fields.destination_amount.trim().is_empty() {
                    return Err(
                        "Enter the destination amount for this cross-currency transfer.".to_owned(),
                    );
                }
                parse_money(fields.destination_amount.trim(), destination.currency)
                    .map_err(|e| format!("Destination amount: {e}"))?
            };
            (Some(destination.id), Some(received), None)
        } else {
            if let Some(id) = fields.category
                && !categories
                    .iter()
                    .any(|c| c.id == id && c.kind == fields.kind)
            {
                return Err(
                    "Select a category matching the transaction type, or No category.".to_owned(),
                );
            }
            (None, None, fields.category)
        };
    let (date, time) = parse_date_time(fields.date, fields.time, now)?;
    let transaction = Transaction {
        id: original.map_or_else(Uuid::new_v4, |t| t.id),
        account_id: source.id,
        category_id,
        amount_minor: amount,
        kind: fields.kind,
        date,
        time,
        transfer_account_id: destination_id,
        transfer_amount_minor: destination_amount,
        note: fields.note.trim().to_owned(),
    };
    transaction.validate().map_err(|e| e.to_string())?;
    Ok(transaction)
}

fn parse_date(value: &str) -> Result<Date, String> {
    if !valid_shape(value, b'-', 4, 7) {
        return Err("Enter a valid date as yyyy-mm-dd.".to_owned());
    }
    Date::parse(value, format_description!("[year]-[month]-[day]"))
        .map_err(|_| "Enter a valid date as yyyy-mm-dd.".to_owned())
}

fn parse_time(value: &str) -> Result<Time, String> {
    if !valid_shape(value, b':', 2, 5) {
        return Err("Enter a valid time as hh:mm:ss (00:00:00–23:59:59).".to_owned());
    }
    Time::parse(value, format_description!("[hour]:[minute]:[second]"))
        .map_err(|_| "Enter a valid time as hh:mm:ss (00:00:00–23:59:59).".to_owned())
}

fn valid_shape(value: &str, separator: u8, first: usize, second: usize) -> bool {
    value.len() == second + 3
        && value.bytes().enumerate().all(|(i, b)| {
            if i == first || i == second {
                b == separator
            } else {
                b.is_ascii_digit()
            }
        })
}

fn parse_date_time(
    date: &str,
    time: &str,
    now: impl FnOnce() -> Result<(Date, Time), String>,
) -> Result<(Date, Time), String> {
    let date = date.trim();
    let time = time.trim();
    let parsed_date = if date.is_empty() {
        None
    } else {
        Some(parse_date(date)?)
    };
    let parsed_time = if time.is_empty() {
        None
    } else {
        Some(parse_time(time)?)
    };
    match (parsed_date, parsed_time) {
        (Some(date), Some(time)) => Ok((date, time)),
        (date, time) => {
            let (today, current_time) = now()?;
            Ok((date.unwrap_or(today), time.unwrap_or(current_time)))
        }
    }
}

fn local_now() -> Result<(Date, Time), String> {
    if let Ok(now) = OffsetDateTime::now_local() {
        let time =
            Time::from_hms(now.hour(), now.minute(), now.second()).map_err(|e| e.to_string())?;
        return Ok((now.date(), time));
    }
    #[cfg(target_os = "macos")]
    {
        // time's local-offset lookup can refuse multithreaded processes. A child
        // process queries the system timezone without unsafe process-global state.
        if let Ok(output) = std::process::Command::new("/bin/date")
            .arg("+%Y-%m-%d %H:%M:%S")
            .output()
            && output.status.success()
            && let Ok(text) = std::str::from_utf8(&output.stdout)
            && let Some((date, time)) = text.trim().split_once(' ')
            && let (Ok(date), Ok(time)) = (parse_date(date), parse_time(time))
        {
            return Ok((date, time));
        }
    }
    Err("Could not determine the computer's local date/time. Enter both date (yyyy-mm-dd) and time (hh:mm:ss), or check the system timezone settings.".to_owned())
}

impl Render for TransactionForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let source = self
            .source
            .read(cx)
            .selected_value()
            .and_then(|id| self.accounts.iter().find(|a| a.id == *id));
        let destination = self
            .destination
            .read(cx)
            .selected_value()
            .and_then(|id| self.accounts.iter().find(|a| a.id == *id));
        let amount_label = source.map_or_else(
            || "Amount".to_owned(),
            |a| format!("Amount · {}", a.currency.code()),
        );
        let destination_label = destination.map_or_else(
            || "Amount received".to_owned(),
            |a| format!("Amount received · {}", a.currency.code()),
        );
        let same_currency = source
            .zip(destination)
            .is_some_and(|(a, b)| a.currency == b.currency);
        let mut types = div().flex().gap_2();
        for (index, kind, label) in [
            (0_usize, TransactionKind::Income, "Income"),
            (1, TransactionKind::Expense, "Expense"),
            (2, TransactionKind::Transfer, "Transfer"),
        ] {
            types = types.child(
                Button::new(("transaction-kind", index))
                    .label(label)
                    .when(self.kind == kind, |button| button.primary())
                    .disabled(self.saving)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.change_kind(kind, window, cx)),
                    ),
            );
        }
        div()
            .id("transaction-form")
            .flex()
            .flex_col()
            .max_h(px(560.0))
            .overflow_y_scroll()
            .gap_4()
            .child(div().flex().flex_col().gap_2().child("Type").child(types))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(if self.kind == TransactionKind::Transfer {
                        "Source account"
                    } else {
                        "Account"
                    })
                    .child(
                        Select::new(&self.source)
                            .placeholder("Select an account")
                            .search_placeholder("Search accounts…")
                            .disabled(self.saving)
                            .w_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(amount_label)
                    .child(Input::new(&self.amount).disabled(self.saving)),
            )
            .when(self.kind == TransactionKind::Transfer, |view| {
                view.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Destination account")
                        .child(
                            Select::new(&self.destination)
                                .placeholder("Select a destination account")
                                .search_placeholder("Search accounts…")
                                .disabled(self.saving)
                                .w_full(),
                        ),
                )
                .when(same_currency, |view| {
                    view.child(div().text_sm().text_color(muted).child(
                        "Amount received automatically equals the source amount (same currency).",
                    ))
                })
                .when(!same_currency, |view| {
                    view.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(destination_label)
                            .child(Input::new(&self.destination_amount).disabled(self.saving)),
                    )
                })
            })
            .when(self.kind != TransactionKind::Transfer, |view| {
                view.child(
                    div().flex().flex_col().gap_2().child("Category").child(
                        Select::new(&self.category)
                            .placeholder("No category")
                            .search_placeholder("Search categories…")
                            .disabled(self.saving)
                            .w_full(),
                    ),
                )
            })
            .child(
                div().text_sm().text_color(muted).child(
                    "Use positive amounts with a decimal point, without grouping separators.",
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Date (optional)")
                    .child(Input::new(&self.date).disabled(self.saving)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Time (optional)")
                    .child(Input::new(&self.time).disabled(self.saving)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Blank date/time fields use the computer's local date/time when saved."),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Note")
                    .child(Input::new(&self.note).disabled(self.saving)),
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
                        Button::new("cancel-transaction")
                            .label("Cancel")
                            .disabled(self.saving)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-transaction")
                            .primary()
                            .label(if self.saving {
                                "Saving…"
                            } else if self.original.is_some() {
                                "Save changes"
                            } else {
                                "Add transaction"
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

    fn account(name: &str, currency: &str) -> Account {
        Account::new(name, currency.parse().unwrap(), 0).unwrap()
    }

    fn fields(source: &Account) -> Fields<'static> {
        Fields {
            source: Some(source.id),
            destination: None,
            category: None,
            kind: TransactionKind::Expense,
            amount: "12.34",
            destination_amount: "",
            date: "2024-02-29",
            time: "13:14:15",
            note: " note ",
        }
    }

    fn now() -> Result<(Date, Time), String> {
        Ok((parse_date("2026-10-07")?, parse_time("09:10:11")?))
    }

    #[test]
    fn parses_strict_dates_times_and_optional_local_defaults() {
        for date in ["2023-02-29", "2024-2-29", "2024-13-01", "not a date"] {
            assert!(parse_date(date).is_err());
        }
        for time in ["24:00:00", "12:60:00", "12:00:60", "1:02:03", "12:00:00.1"] {
            assert!(parse_time(time).is_err());
        }
        assert_eq!(parse_date_time(" ", "", now).unwrap(), now().unwrap());
        assert_eq!(
            parse_date_time("2024-02-29", "", now).unwrap().1,
            now().unwrap().1
        );
        assert_eq!(
            parse_date_time("", "00:00:00", now).unwrap().1,
            Time::MIDNIGHT
        );
        assert!(parse_date_time("", "", || Err("local clock unavailable".into())).is_err());
        assert!(
            parse_date_time("2024-02-29", "00:00:00", || Err(
                "must not query clock".into()
            ))
            .is_ok()
        );
    }

    #[test]
    fn eligible_accounts_include_only_the_original_inactive_account_per_role() {
        let active = account("Active", "USD");
        let mut source = account("Old source", "USD");
        source.is_active = false;
        let mut destination = account("Old destination", "EUR");
        destination.is_active = false;
        let accounts = [active.clone(), source.clone(), destination.clone()];
        assert_eq!(account_choices(&accounts, None).len(), 1);
        let sources = account_choices(&accounts, Some(source.id));
        assert_eq!(sources.len(), 2);
        assert!(!sources.iter().any(|a| a.value == destination.id));
        let destinations = account_choices(&accounts, Some(destination.id));
        assert_eq!(destinations.len(), 2);
        assert!(!destinations.iter().any(|a| a.value == source.id));
        assert!(sources[1].matches("old source"));
        assert!(sources[1].matches("usd"));
    }

    #[test]
    fn validates_amounts_categories_and_preserves_edits() {
        let accounts = [account("Cash", "USD")];
        let categories = [
            Category::new("Food", TransactionKind::Expense).unwrap(),
            Category::new("Pay", TransactionKind::Income).unwrap(),
        ];
        let choices = category_choices(&categories, TransactionKind::Expense);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].value, None);
        assert_eq!(
            category_choices(&categories, TransactionKind::Transfer).len(),
            1
        );
        let original =
            transaction_from_fields(None, &accounts, &categories, fields(&accounts[0]), now)
                .unwrap();
        assert_eq!(original.amount_minor, 1234);
        assert_eq!(original.note, "note");
        let mut edit = fields(&accounts[0]);
        edit.category = Some(categories[0].id);
        let updated =
            transaction_from_fields(Some(&original), &accounts, &categories, edit, now).unwrap();
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.date, original.date);
        assert_eq!(updated.time, original.time);
        for amount in ["0", "-1", "1.001", "bad", "9223372036854775808"] {
            let mut input = fields(&accounts[0]);
            input.amount = amount;
            assert!(transaction_from_fields(None, &accounts, &categories, input, now).is_err());
        }
        let mut input = fields(&accounts[0]);
        input.category = Some(categories[1].id);
        assert!(transaction_from_fields(None, &accounts, &categories, input, now).is_err());
        let mut input = fields(&accounts[0]);
        input.source = None;
        assert!(transaction_from_fields(None, &accounts, &categories, input, now).is_err());
    }

    #[test]
    fn transfers_validate_destination_and_use_its_currency_precision() {
        let accounts = [
            account("Source", "USD"),
            account("Same currency", "USD"),
            account("Destination", "KWD"),
        ];
        let mut input = fields(&accounts[0]);
        input.kind = TransactionKind::Transfer;
        input.destination = Some(accounts[1].id);
        input.destination_amount = "ignored";
        input.category = Some(Uuid::new_v4());
        let transfer = transaction_from_fields(None, &accounts, &[], input, now).unwrap();
        assert_eq!(transfer.transfer_amount_minor, Some(1234));
        assert_eq!(transfer.category_id, None);
        for (destination, amount) in [
            (None, ""),
            (Some(accounts[0].id), "1"),
            (Some(accounts[2].id), ""),
            (Some(accounts[2].id), "0"),
            (Some(accounts[2].id), "1.0001"),
        ] {
            let mut input = fields(&accounts[0]);
            input.kind = TransactionKind::Transfer;
            input.destination = destination;
            input.destination_amount = amount;
            assert!(transaction_from_fields(None, &accounts, &[], input, now).is_err());
        }
        let mut input = fields(&accounts[0]);
        input.kind = TransactionKind::Transfer;
        input.destination = Some(accounts[2].id);
        input.destination_amount = "4.567";
        let transfer = transaction_from_fields(None, &accounts, &[], input, now).unwrap();
        assert_eq!(transfer.transfer_amount_minor, Some(4567));
        let mut inactive_accounts = accounts;
        inactive_accounts[0].is_active = false;
        inactive_accounts[2].is_active = false;
        let mut input = fields(&inactive_accounts[0]);
        input.kind = TransactionKind::Transfer;
        input.destination = Some(inactive_accounts[2].id);
        input.destination_amount = "4.567";
        assert!(transaction_from_fields(None, &inactive_accounts, &[], input, now).is_err());
        let mut input = fields(&inactive_accounts[0]);
        input.kind = TransactionKind::Transfer;
        input.destination = Some(inactive_accounts[2].id);
        input.destination_amount = "4.567";
        assert!(
            transaction_from_fields(Some(&transfer), &inactive_accounts, &[], input, now).is_ok()
        );
        let mut input = fields(&inactive_accounts[0]);
        input.kind = TransactionKind::Expense;
        input.destination = Some(inactive_accounts[2].id);
        let expense =
            transaction_from_fields(Some(&transfer), &inactive_accounts, &[], input, now).unwrap();
        assert_eq!(expense.transfer_account_id, None);
        assert_eq!(expense.transfer_amount_minor, None);
    }
}
