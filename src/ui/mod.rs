mod account_form;
mod accounts;

use account_form::{AccountForm, SaveAccount};
use cracker_desktop::{Account, Snapshot, Storage};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, IntoElement, Render, Window, div, prelude::*, px};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Transactions,
    Accounts,
    Categories,
}

impl Section {
    const ALL: [Self; 3] = [Self::Accounts, Self::Categories, Self::Transactions];

    fn title(self) -> &'static str {
        match self {
            Self::Transactions => "Transactions",
            Self::Accounts => "Accounts",
            Self::Categories => "Categories",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Transactions => "Income, expenses, and transfers between accounts.",
            Self::Accounts => "Bank accounts, cash, and other wallets.",
            Self::Categories => {
                "Income and expense categories to organize your transaction history."
            }
        }
    }

    fn count(self, snapshot: &Snapshot) -> usize {
        match self {
            Self::Transactions => snapshot.transactions.len(),
            Self::Accounts => snapshot.accounts.len(),
            Self::Categories => snapshot.categories.len(),
        }
    }
}

pub struct TrackerApp {
    // Retain the initial connection; writes use a separate connection on the background executor.
    _storage: Storage,
    snapshot: Snapshot,
    database_path: PathBuf,
    section: Section,
}

impl TrackerApp {
    pub fn new(storage: Storage, snapshot: Snapshot, database_path: PathBuf) -> Self {
        Self {
            _storage: storage,
            snapshot,
            database_path,
            section: Section::Accounts,
        }
    }

    fn edit_account(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(account) = self
            .snapshot
            .accounts
            .iter()
            .find(|account| account.id == id)
        {
            self.open_account_form(Some(account.clone()), window, cx);
        }
    }

    fn open_account_form(
        &mut self,
        account: Option<Account>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locked = account.as_ref().is_some_and(|account| {
            self.snapshot.transactions.iter().any(|transaction| {
                transaction.account_id == account.id
                    || transaction.transfer_account_id == Some(account.id)
            })
        });
        let title = if account.is_some() {
            "Edit account"
        } else {
            "Add account"
        };
        let form = cx.new(|cx| AccountForm::new(account, locked, window, cx));
        cx.subscribe_in(
            &form,
            window,
            |this, form, event: &SaveAccount, window, cx| {
                let form = form.clone();
                let path = this.database_path.clone();
                let account = event.account.clone();
                let editing = event.editing;
                let write = cx.background_executor().spawn(async move {
                    let mut storage = Storage::open(&path)?;
                    if editing {
                        storage.update_account(&account)
                    } else {
                        storage.create_account(&account)
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
                .w(px(520.0))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel_form.read(cx).saving())
                .child(form.clone())
                .footer(div())
        });
    }
}

impl Render for TrackerApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut navigation = div()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(220.0))
            .flex_shrink_0()
            .p_5()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(div().text_xl().mb_4().child("Cracker"));

        for (index, section) in Section::ALL.into_iter().enumerate() {
            let label = format!("{} · {}", section.title(), section.count(&self.snapshot));
            let button = Button::new(("navigation", index))
                .label(label)
                .w_full()
                .when(self.section == section, |button| button.primary())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.section = section;
                    cx.notify();
                }));
            navigation = navigation.child(button);
        }

        let content = if self.section == Section::Accounts {
            accounts::accounts_panel(&self.snapshot, cx).into_any_element()
        } else if self.section == Section::Categories {
            div().child("No categories yet.").into_any_element()
        } else {
            div().child("No transactions yet.").into_any_element()
        };
        let theme = cx.theme();
        let panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .p_8()
            .gap_4()
            .child(div().text_2xl().child(self.section.title()))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(self.section.description()),
            )
            .child(content)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Local storage · SQLite · no cloud"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(self.database_path.display().to_string()),
            );

        div()
            .flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(navigation)
            .child(panel)
    }
}
