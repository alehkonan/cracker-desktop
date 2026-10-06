use std::{collections::HashMap, error::Error, path::Path, str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Row, TransactionBehavior, types::Type};
use time::{Date, Time, macros::format_description};

use crate::domain::{Account, Category, CategoryColor, Transaction, TransactionKind};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 4;
const TRANSACTIONS_SCHEMA: &str = include_str!("../migrations/004_transactions.sql");
const CATEGORIES_SCHEMA: &str = include_str!("../migrations/003_categories.sql");
const ACCOUNTS_SCHEMA: &str = include_str!("../migrations/002_accounts.sql");
const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub transactions: Vec<Transaction>,
}

impl Snapshot {
    /// Opening balance plus income, minus expenses and outgoing transfers,
    /// plus incoming transfers in this account's own currency minor units.
    /// Missing accounts and a final balance outside i64 are errors. Accumulation
    /// uses i128 so intermediate i64 overflow does not reject a valid final sum.
    pub fn account_balance(&self, account_id: Uuid) -> Result<i64> {
        let account = self
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .with_context(|| format!("account {account_id} does not exist"))?;
        let mut balance = i128::from(account.opening_balance_minor);
        for transaction in &self.transactions {
            transaction.validate()?;
            if transaction.account_id == account_id {
                let amount = i128::from(transaction.amount_minor);
                balance = balance
                    .checked_add(match transaction.kind {
                        TransactionKind::Income => amount,
                        TransactionKind::Expense | TransactionKind::Transfer => -amount,
                    })
                    .context("account balance overflow")?;
            }
            if transaction.transfer_account_id == Some(account_id) {
                let amount = transaction
                    .transfer_amount_minor
                    .context("missing transfer amount")?;
                balance = balance
                    .checked_add(i128::from(amount))
                    .context("account balance overflow")?;
            }
        }
        i64::try_from(balance)
            .context("account balance is outside the signed 64-bit minor-unit range")
    }

    /// Active accounts in the same stable order as `accounts`.
    pub fn active_accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(|account| account.is_active)
    }
}

pub struct Storage {
    connection: Connection,
}

impl Storage {
    /// Opens or creates a database file. Its parent directory must already exist.
    pub fn open(path: &Path) -> Result<Self> {
        Self::initialize(
            Connection::open(path).with_context(|| format!("opening {}", path.display()))?,
        )
    }

    /// SQLite uses its memory journal here rather than WAL.
    pub fn open_in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut connection).context("migrating finance database")?;
        Ok(Self { connection })
    }

    /// Reads a consistent snapshot. Names sort by binary text then ID;
    /// transactions sort by date and time ascending then ID. Malformed rows are errors.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let transaction = self.connection.unchecked_transaction()?;
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Creates an account and returns the resulting snapshot. On any error the
    /// insert is rolled back, including failures while decoding the snapshot.
    pub fn create_account(&mut self, account: &Account) -> Result<Snapshot> {
        account.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_unique_name(&transaction, "accounts", "account", &account.name, None)?;
        transaction.execute(
            "INSERT INTO accounts (id, name, currency, opening_balance_minor, is_active, name_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![account.id.to_string(), account.name, account.currency.code(), account.opening_balance_minor, account.is_active, normalized_name(&account.name)],
        ).context("creating account")?;
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Updates an existing account. Currency may change only without existing
    /// transactions (enforced by a database trigger). Deactivation retains history.
    /// A missing ID or any snapshot error rolls back the entire operation.
    pub fn update_account(&mut self, account: &Account) -> Result<Snapshot> {
        account.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_unique_name(
            &transaction,
            "accounts",
            "account",
            &account.name,
            Some(account.id),
        )?;
        let count = transaction.execute(
            "UPDATE accounts SET name = ?2, currency = ?3, opening_balance_minor = ?4, is_active = ?5, name_key = ?6 WHERE id = ?1",
            rusqlite::params![account.id.to_string(), account.name, account.currency.code(), account.opening_balance_minor, account.is_active, normalized_name(&account.name)],
        ).context("updating account (currency cannot change with existing transactions)")?;
        if count != 1 {
            bail!("account {} does not exist", account.id);
        }
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Creates a category with a globally unique normalized name, across both kinds.
    /// The write and returned snapshot are atomic; any failure rolls back.
    pub fn create_category(&mut self, category: &Category) -> Result<Snapshot> {
        category.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_unique_name(&transaction, "categories", "category", &category.name, None)?;
        transaction.execute(
            "INSERT INTO categories (id, name, kind, color, name_key) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![category.id.to_string(), category.name, category.kind.as_str(), category.color.index() as i64, normalized_name(&category.name)],
        ).context("creating category")?;
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Updates an existing category. Kind changes are forbidden while linked
    /// transactions exist, including when changed through raw SQL.
    pub fn update_category(&mut self, category: &Category) -> Result<Snapshot> {
        category.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_unique_name(
            &transaction,
            "categories",
            "category",
            &category.name,
            Some(category.id),
        )?;
        let count = transaction.execute(
            "UPDATE categories SET name = ?2, kind = ?3, color = ?4, name_key = ?5 WHERE id = ?1",
            rusqlite::params![category.id.to_string(), category.name, category.kind.as_str(), category.color.index() as i64, normalized_name(&category.name)],
        ).context("updating category (kind cannot change with existing transactions)")?;
        if count != 1 {
            bail!("category {} does not exist", category.id);
        }
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Creates a validated transaction. Selected accounts must exist and be
    /// active; categories must match kind. Write and returned snapshot are atomic.
    pub fn create_transaction(&mut self, entry: &Transaction) -> Result<Snapshot> {
        entry.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note, time, transfer_account_id, transfer_amount_minor) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![entry.id.to_string(), entry.account_id.to_string(), entry.category_id.map(|id| id.to_string()), entry.amount_minor, entry.kind.as_str(), entry.date.format(format_description!("[year]-[month]-[day]"))?, entry.note, entry.time.format(format_description!("[hour]:[minute]:[second]"))?, entry.transfer_account_id.map(|id| id.to_string()), entry.transfer_amount_minor],
        ).context("creating transaction (accounts must be active and category must match kind)")?;
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Updates an existing transaction atomically. Either original source or
    /// destination may remain selected while inactive; newly selected accounts
    /// must be active. Missing IDs and snapshot decode failures roll back.
    pub fn update_transaction(&mut self, entry: &Transaction) -> Result<Snapshot> {
        entry.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count = transaction.execute(
            "UPDATE transactions SET account_id = ?2, category_id = ?3, amount_minor = ?4, kind = ?5, date = ?6, note = ?7, time = ?8, transfer_account_id = ?9, transfer_amount_minor = ?10 WHERE id = ?1",
            rusqlite::params![entry.id.to_string(), entry.account_id.to_string(), entry.category_id.map(|id| id.to_string()), entry.amount_minor, entry.kind.as_str(), entry.date.format(format_description!("[year]-[month]-[day]"))?, entry.note, entry.time.format(format_description!("[hour]:[minute]:[second]"))?, entry.transfer_account_id.map(|id| id.to_string()), entry.transfer_amount_minor],
        ).context("updating transaction (new accounts must be active and category must match kind)")?;
        if count != 1 {
            bail!("transaction {} does not exist", entry.id);
        }
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Deletes one transaction and returns the resulting snapshot atomically.
    /// Missing IDs are errors, including when deleting twice.
    pub fn delete_transaction(&mut self, id: Uuid) -> Result<Snapshot> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count = transaction
            .execute("DELETE FROM transactions WHERE id = ?1", [id.to_string()])
            .context("deleting transaction")?;
        if count != 1 {
            bail!("transaction {id} does not exist");
        }
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    /// Deletes a category, clearing only category_id in linked transactions.
    /// Transaction kinds and all other fields are preserved. Missing IDs are errors.
    pub fn delete_category(&mut self, id: Uuid) -> Result<Snapshot> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count = transaction
            .execute("DELETE FROM categories WHERE id = ?1", [id.to_string()])
            .context("deleting category")?;
        if count != 1 {
            bail!("category {id} does not exist");
        }
        let snapshot = load_snapshot(&transaction)?;
        transaction.commit()?;
        Ok(snapshot)
    }
}

/// Deliberately Rust Unicode lowercase, not SQLite's ASCII-only lower().
fn normalized_name(name: &str) -> String {
    name.trim().to_lowercase()
}

// Table names are internal constants, never caller input. IMMEDIATE transactions
// serialize validation and writes; unique indexes additionally guard stored keys.
fn ensure_unique_name(
    connection: &Connection,
    table: &str,
    label: &str,
    name: &str,
    exclude: Option<Uuid>,
) -> Result<()> {
    let key = normalized_name(name);
    let exclude = exclude.map(|id| id.to_string());
    let mut statement = connection.prepare(&format!("SELECT id, name FROM {table}"))?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, existing) = row?;
        if exclude.as_ref() != Some(&id) && normalized_name(&existing) == key {
            bail!("{label} name '{name}' is already in use");
        }
    }
    Ok(())
}

fn backfill_name_keys(connection: &Connection, table: &str, label: &str) -> Result<()> {
    let mut statement = connection.prepare(&format!("SELECT id, name FROM {table} ORDER BY id"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut names = HashMap::new();
    for (id, name) in &rows {
        let key = normalized_name(name);
        if key.is_empty() {
            bail!("cannot migrate: {label} {id} has an empty normalized name");
        }
        if let Some(previous) = names.insert(key, name) {
            bail!(
                "cannot migrate: duplicate {label} names '{previous}' and '{name}' after trimming and Unicode lowercase"
            );
        }
    }
    for (id, name) in rows {
        connection.execute(
            &format!("UPDATE {table} SET name_key = ?1 WHERE id = ?2"),
            rusqlite::params![normalized_name(&name), id],
        )?;
    }
    connection.execute_batch(&format!(
        "CREATE UNIQUE INDEX {table}_name_key_unique ON {table}(name_key);"
    ))?;
    Ok(())
}

fn load_snapshot(connection: &Connection) -> Result<Snapshot> {
    let accounts = load(
        connection,
        "SELECT id, name, currency, opening_balance_minor, is_active FROM accounts ORDER BY name COLLATE BINARY, id",
        |row| {
            let is_active = match row.get::<_, i64>(4)? {
                0 => false,
                1 => true,
                _ => return Err(rusqlite::Error::FromSqlConversionFailure(
                    4,
                    Type::Integer,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "is_active must be 0 or 1",
                    )),
                )),
            };
            let account = Account {
                id: parse_cell(row, 0)?,
                name: row.get(1)?,
                currency: parse_cell(row, 2)?,
                opening_balance_minor: row.get(3)?,
                is_active,
            };
            account.validate().map_err(|error| decode_error(1, error))?;
            Ok(account)
        },
    ).context("loading accounts")?;
    let categories = load(
        connection,
        "SELECT id, name, kind, color FROM categories ORDER BY name COLLATE BINARY, id",
        |row| {
            let category = Category {
                id: parse_cell(row, 0)?,
                name: row.get(1)?,
                kind: parse_cell(row, 2)?,
                color: {
                    let index: i64 = row.get(3).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(3, Type::Integer, Box::new(error))
                    })?;
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| CategoryColor::try_from(index).ok())
                        .ok_or_else(|| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                Type::Integer,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "category color must be an integer between 0 and 15",
                                )),
                            )
                        })?
                },
            };
            category
                .validate()
                .map_err(|error| decode_error(1, error))?;
            Ok(category)
        },
    )
    .context("loading categories")?;
    let transactions = load(connection,
            "SELECT id, account_id, category_id, amount_minor, kind, date, note, time, transfer_account_id, transfer_amount_minor FROM transactions ORDER BY date, time, id",
            |row| {
                let date: String = row.get(5)?;
                let transaction = Transaction {
                    id: parse_cell(row, 0)?, account_id: parse_cell(row, 1)?, category_id: row.get::<_, Option<String>>(2)?.map(|value| value.parse().map_err(|error| decode_error(2, error))).transpose()?,
                    amount_minor: row.get(3)?, kind: parse_cell(row, 4)?,
                    date: Date::parse(&date, format_description!("[year]-[month]-[day]"))
                        .map_err(|error| decode_error(5, error))?,
                    note: row.get(6)?,
                    time: Time::parse(&row.get::<_, String>(7)?, format_description!("[hour]:[minute]:[second]"))
                        .map_err(|error| decode_error(7, error))?,
                    transfer_account_id: row.get::<_, Option<String>>(8)?.map(|value| value.parse().map_err(|error| decode_error(8, error))).transpose()?,
                    transfer_amount_minor: row.get(9)?,
                };
                transaction.validate().map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(3, Type::Integer, Box::new(error))
                })?;
                Ok(transaction)
            },
        ).context("loading transactions")?;
    Ok(Snapshot {
        accounts,
        categories,
        transactions,
    })
}

fn migrate(connection: &mut Connection) -> Result<()> {
    // Acquire the write lock before checking the version to serialize first opens.
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        bail!("database schema version {version} is newer than supported version {SCHEMA_VERSION}");
    }
    if version < 0 {
        bail!("invalid database schema version {version}");
    }
    if version == 0 {
        transaction.execute_batch(INITIAL_SCHEMA)?;
    }
    if version < 2 {
        transaction.execute_batch(ACCOUNTS_SCHEMA)?;
    }
    if version < 3 {
        transaction.execute_batch(CATEGORIES_SCHEMA)?;
        backfill_name_keys(&transaction, "accounts", "account")?;
        backfill_name_keys(&transaction, "categories", "category")?;
    }
    if version < 4 {
        transaction.execute_batch(TRANSACTIONS_SCHEMA)?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    transaction.commit()?;
    Ok(())
}

fn load<T>(
    connection: &Connection,
    sql: &str,
    map: fn(&Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    connection.prepare(sql)?.query_map([], map)?.collect()
}

fn parse_cell<T: FromStr>(row: &Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T::Err: Error + Send + Sync + 'static,
{
    let value: String = row.get(index)?;
    value.parse().map_err(|error| decode_error(index, error))
}

fn decode_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "A failed test fixture must fail the test immediately."
)]
mod tests {
    use super::*;
    use crate::domain::{Currency, TransactionKind};
    use time::macros::date;
    use uuid::Uuid;

    fn entry(account: Uuid, kind: TransactionKind, amount: i64) -> Transaction {
        Transaction::new(account, None, amount, kind, date!(2026 - 10 - 06), "").unwrap()
    }

    #[test]
    fn transaction_crud_balances_and_datetime_order() {
        let mut storage = Storage::open_in_memory().unwrap();
        let source = Account::new("Source", Currency::Usd, 100).unwrap();
        let destination = Account::new("Destination", Currency::Eur, -20).unwrap();
        storage.create_account(&source).unwrap();
        storage.create_account(&destination).unwrap();
        let mut income = entry(source.id, TransactionKind::Income, 200);
        income.id = Uuid::from_u128(3);
        income.time = time::macros::time!(12:00:00);
        storage.create_transaction(&income).unwrap();
        let mut expense = entry(source.id, TransactionKind::Expense, 50);
        expense.id = Uuid::from_u128(2);
        expense.time = income.time;
        storage.create_transaction(&expense).unwrap();
        let mut transfer = entry(source.id, TransactionKind::Expense, 80);
        transfer.id = Uuid::from_u128(1);
        transfer.kind = TransactionKind::Transfer;
        transfer.transfer_account_id = Some(destination.id);
        transfer.transfer_amount_minor = Some(70);
        transfer.time = time::macros::time!(13:00:00);
        let snapshot = storage.create_transaction(&transfer).unwrap();
        assert_eq!(
            snapshot.transactions,
            vec![expense.clone(), income.clone(), transfer.clone()]
        );
        assert_eq!(snapshot.account_balance(source.id).unwrap(), 170);
        assert_eq!(snapshot.account_balance(destination.id).unwrap(), 50);
        assert!(snapshot.account_balance(Uuid::new_v4()).is_err());
        transfer.time = Time::MIDNIGHT;
        transfer.transfer_amount_minor = Some(90);
        transfer.note = "Updated".into();
        let snapshot = storage.update_transaction(&transfer).unwrap();
        assert_eq!(snapshot.transactions[0], transfer);
        assert_eq!(snapshot.account_balance(destination.id).unwrap(), 70);
        let snapshot = storage.delete_transaction(transfer.id).unwrap();
        assert_eq!(snapshot.account_balance(source.id).unwrap(), 250);
        assert_eq!(snapshot.account_balance(destination.id).unwrap(), -20);
        assert!(storage.delete_transaction(transfer.id).is_err());
        assert!(storage.update_transaction(&transfer).is_err());
        assert_eq!(storage.snapshot().unwrap(), snapshot);
    }

    #[test]
    fn transaction_accounts_categories_and_inactive_history() {
        let mut storage = Storage::open_in_memory().unwrap();
        let mut source = Account::new("Source", Currency::Usd, 0).unwrap();
        let mut destination = Account::new("Destination", Currency::Eur, 0).unwrap();
        let mut other = Account::new("Other", Currency::Rub, 0).unwrap();
        storage.create_account(&source).unwrap();
        storage.create_account(&destination).unwrap();
        storage.create_account(&other).unwrap();
        let category = Category::new("Income", TransactionKind::Income).unwrap();
        storage.create_category(&category).unwrap();
        let mut transfer = entry(source.id, TransactionKind::Expense, 5);
        transfer.kind = TransactionKind::Transfer;
        transfer.transfer_account_id = Some(destination.id);
        transfer.transfer_amount_minor = Some(7);
        storage.create_transaction(&transfer).unwrap();
        source.is_active = false;
        destination.is_active = false;
        other.is_active = false;
        storage.update_account(&source).unwrap();
        storage.update_account(&destination).unwrap();
        storage.update_account(&other).unwrap();
        transfer.note = "Historical edit".into();
        storage.update_transaction(&transfer).unwrap();
        // Both original accounts remain selectable even when their roles swap.
        transfer.account_id = destination.id;
        transfer.transfer_account_id = Some(source.id);
        storage.update_transaction(&transfer).unwrap();
        let before = storage.snapshot().unwrap();
        let mut invalid = transfer.clone();
        invalid.transfer_account_id = Some(other.id);
        assert!(storage.update_transaction(&invalid).is_err());
        invalid = transfer.clone();
        invalid.account_id = Uuid::new_v4();
        assert!(storage.update_transaction(&invalid).is_err());
        invalid = transfer.clone();
        invalid.transfer_account_id = Some(Uuid::new_v4());
        assert!(storage.update_transaction(&invalid).is_err());
        invalid = transfer.clone();
        invalid.id = Uuid::new_v4();
        assert!(storage.create_transaction(&invalid).is_err());
        let mut changed_currency = source.clone();
        changed_currency.currency = Currency::Rub;
        assert!(storage.update_account(&changed_currency).is_err());
        changed_currency = destination.clone();
        changed_currency.currency = Currency::Rub;
        assert!(storage.update_account(&changed_currency).is_err());
        assert_eq!(storage.snapshot().unwrap(), before);
        other.is_active = true;
        storage.update_account(&other).unwrap();
        let mut expense = entry(other.id, TransactionKind::Expense, 1);
        expense.category_id = Some(category.id);
        assert!(storage.create_transaction(&expense).is_err());
        expense.category_id = Some(Uuid::new_v4());
        assert!(storage.create_transaction(&expense).is_err());
        expense.category_id = None;
        storage.create_transaction(&expense).unwrap();
        expense.category_id = Some(category.id);
        assert!(storage.update_transaction(&expense).is_err());
        assert_eq!(storage.snapshot().unwrap().transactions.len(), 2);
    }

    #[test]
    fn transaction_snapshot_decode_failure_rolls_back_all_crud() {
        let mut storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        let original = storage.snapshot().unwrap().transactions.remove(0);
        storage
            .connection
            .execute("UPDATE accounts SET name = char(9)", [])
            .unwrap();
        let created = entry(original.account_id, TransactionKind::Income, 1);
        assert!(storage.create_transaction(&created).is_err());
        let mut updated = original.clone();
        updated.note = "Must roll back".into();
        assert!(storage.update_transaction(&updated).is_err());
        assert!(storage.delete_transaction(original.id).is_err());
        storage
            .connection
            .execute("UPDATE accounts SET name = 'Cash'", [])
            .unwrap();
        assert_eq!(storage.snapshot().unwrap().transactions, vec![original]);
    }

    #[test]
    fn checked_balance_handles_extremes_without_intermediate_i64_overflow() {
        let account = Account::new("Cash", Currency::Usd, i64::MAX).unwrap();
        let mut snapshot = Snapshot {
            accounts: vec![account.clone()],
            ..Snapshot::default()
        };
        snapshot
            .transactions
            .push(entry(account.id, TransactionKind::Income, 1));
        assert!(snapshot.account_balance(account.id).is_err());
        snapshot
            .transactions
            .push(entry(account.id, TransactionKind::Expense, 1));
        assert_eq!(snapshot.account_balance(account.id).unwrap(), i64::MAX);
        snapshot.accounts[0].opening_balance_minor = i64::MIN;
        snapshot.transactions.remove(0);
        assert!(snapshot.account_balance(account.id).is_err());
    }

    #[test]
    fn v3_migration_preserves_rows_and_midnight_and_rolls_back_invalid_history() {
        for invalid in [false, true] {
            let connection = Connection::open_in_memory().unwrap();
            connection
                .pragma_update(None, "foreign_keys", true)
                .unwrap();
            connection.execute_batch(INITIAL_SCHEMA).unwrap();
            connection.execute_batch(ACCOUNTS_SCHEMA).unwrap();
            connection.execute_batch(CATEGORIES_SCHEMA).unwrap();
            backfill_name_keys(&connection, "accounts", "account").unwrap();
            backfill_name_keys(&connection, "categories", "category").unwrap();
            connection.pragma_update(None, "user_version", 3).unwrap();
            let mut storage = Storage { connection };
            seed(&storage);
            // History on inactive accounts must migrate without selection checks.
            storage
                .connection
                .execute("UPDATE accounts SET is_active = 0", [])
                .unwrap();
            if invalid {
                storage.connection.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE transactions SET date = 'invalid'; PRAGMA ignore_check_constraints = OFF;").unwrap();
                assert!(migrate(&mut storage.connection).is_err());
                let version: i64 = storage
                    .connection
                    .pragma_query_value(None, "user_version", |row| row.get(0))
                    .unwrap();
                assert_eq!(version, 3);
                let note: String = storage
                    .connection
                    .query_row("SELECT note FROM transactions", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(note, "Lunch");
            } else {
                migrate(&mut storage.connection).unwrap();
                let snapshot = storage.snapshot().unwrap();
                let entry = &snapshot.transactions[0];
                assert_eq!(entry.id, Uuid::from_u128(3));
                assert_eq!(entry.time, Time::MIDNIGHT);
                assert_eq!(entry.transfer_account_id, None);
                assert_eq!(entry.transfer_amount_minor, None);
                assert_eq!(entry.note, "Lunch");
                migrate(&mut storage.connection).unwrap();
                assert_eq!(storage.snapshot().unwrap(), snapshot);
            }
        }
    }

    #[test]
    fn raw_sql_rejects_invalid_transfer_shapes_and_times() {
        let mut storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        let destination = Account::new("Destination", Currency::Usd, 0).unwrap();
        storage.create_account(&destination).unwrap();
        for sql in [
            "UPDATE transactions SET time = '24:00:00'",
            "UPDATE transactions SET time = '12:60:00'",
            "UPDATE transactions SET time = '12:00:60'",
            "UPDATE transactions SET time = '1:00:00'",
            "UPDATE transactions SET time = '00:00:00.1'",
            "UPDATE transactions SET kind = 'transfer', category_id = NULL",
            "UPDATE transactions SET transfer_amount_minor = 1",
            "UPDATE transactions SET kind = 'transfer', category_id = NULL, transfer_account_id = account_id, transfer_amount_minor = 1",
        ] {
            assert!(
                storage.connection.execute(sql, []).is_err(),
                "accepted {sql}"
            );
        }
        storage.connection.execute("UPDATE transactions SET kind = 'transfer', category_id = NULL, transfer_account_id = ?1, transfer_amount_minor = 10", [destination.id.to_string()]).unwrap();
        for sql in [
            "UPDATE transactions SET transfer_amount_minor = NULL",
            "UPDATE transactions SET transfer_amount_minor = 0",
            "UPDATE transactions SET transfer_amount_minor = -1",
            "UPDATE transactions SET transfer_amount_minor = 1.5",
            "UPDATE transactions SET transfer_account_id = NULL",
            "UPDATE transactions SET transfer_account_id = 'missing'",
            "UPDATE transactions SET category_id = '00000000-0000-0000-0000-000000000002'",
            "UPDATE transactions SET kind = 'expense'",
        ] {
            assert!(
                storage.connection.execute(sql, []).is_err(),
                "accepted {sql}"
            );
        }
        assert!(
            storage
                .connection
                .execute(
                    "DELETE FROM accounts WHERE id = ?1",
                    [destination.id.to_string()]
                )
                .is_err()
        );
    }

    fn seed(storage: &Storage) {
        storage.connection.execute_batch("
            INSERT INTO accounts VALUES ('00000000-0000-0000-0000-000000000001', 'Cash', 'RUB', -123, 1, 'cash');
            INSERT INTO categories VALUES ('00000000-0000-0000-0000-000000000002', 'Food', 'expense', 'food', 0);
            INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) VALUES (
                '00000000-0000-0000-0000-000000000003',
                '00000000-0000-0000-0000-000000000001',
                '00000000-0000-0000-0000-000000000002', 456, 'expense', '2026-10-06', 'Lunch'
            );
        ").unwrap();
    }

    #[test]
    fn migration_is_idempotent_and_empty() {
        let mut storage = Storage::open_in_memory().unwrap();
        assert_eq!(storage.snapshot().unwrap(), Snapshot::default());
        seed(&storage);
        let before = storage.snapshot().unwrap();
        migrate(&mut storage.connection).unwrap();
        assert_eq!(storage.snapshot().unwrap(), before);
        let version: i64 = storage
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let foreign_keys: bool = storage
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert!(foreign_keys);
    }

    #[test]
    fn rejects_newer_schema_and_rolls_back_failed_migration() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(format!("{:#}", Storage::initialize(connection).err().unwrap()).contains("newer"));
        connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE categories (id TEXT);")
            .unwrap();
        assert!(migrate(&mut connection).is_err());
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'accounts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn schema_constraints() {
        let storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        for sql in [
            "UPDATE accounts SET name = ' '",
            "UPDATE categories SET name = ''",
            "UPDATE accounts SET currency = 'ZZZ'",
            "UPDATE accounts SET is_active = 2",
            "UPDATE accounts SET is_active = 1.5",
            "UPDATE accounts SET opening_balance_minor = 1.5",
            "UPDATE transactions SET amount_minor = 0",
            "UPDATE transactions SET amount_minor = -1",
            "UPDATE transactions SET amount_minor = 1.5",
            "UPDATE transactions SET kind = 'income'",
            "UPDATE categories SET kind = 'income'",
            "UPDATE transactions SET account_id = 'missing'",
            "UPDATE transactions SET category_id = 'missing'",
            "UPDATE transactions SET date = '2026-02-30'",
            "UPDATE transactions SET date = 'not-a-date'",
            "DELETE FROM accounts",
            "UPDATE categories SET color = -1",
            "UPDATE categories SET color = 16",
            "UPDATE categories SET color = 1.5",
        ] {
            assert!(
                storage.connection.execute(sql, []).is_err(),
                "accepted {sql}"
            );
        }
    }

    #[test]
    fn snapshot_decodes_and_orders_rows() {
        let storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        storage.connection.execute_batch("
            INSERT INTO accounts VALUES ('00000000-0000-0000-0000-000000000004', 'A', 'USD', 0, 1, 'a');
            INSERT INTO categories VALUES ('00000000-0000-0000-0000-000000000005', 'A', 'income', 'a', 0);
            INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) SELECT '00000000-0000-0000-0000-000000000006', account_id, category_id, amount_minor, kind, '2025-01-01', '' FROM transactions;
        ").unwrap();
        let snapshot = storage.snapshot().unwrap();
        assert_eq!(snapshot.accounts[0].name, "A");
        assert_eq!(snapshot.accounts[1].currency, Currency::Rub);
        assert_eq!(snapshot.accounts[1].opening_balance_minor, -123);
        assert_eq!(snapshot.categories[0].name, "A");
        assert_eq!(snapshot.transactions[0].date, date!(2025 - 01 - 01));
        let transaction = &snapshot.transactions[1];
        assert_eq!(transaction.id, Uuid::from_u128(3));
        assert_eq!(transaction.account_id, Uuid::from_u128(1));
        assert_eq!(transaction.category_id, Some(Uuid::from_u128(2)));
        assert_eq!(transaction.amount_minor, 456);
        assert_eq!(transaction.kind, TransactionKind::Expense);
        assert_eq!(transaction.date, date!(2026 - 10 - 06));
        assert_eq!(transaction.note, "Lunch");
    }

    #[test]
    fn malformed_rows_return_mapping_errors() {
        for sql in [
            "UPDATE accounts SET id = 'invalid'",
            "UPDATE categories SET id = 'invalid'",
            "UPDATE transactions SET id = 'invalid'",
            "UPDATE transactions SET account_id = 'invalid'",
            "UPDATE transactions SET category_id = 'invalid'",
            "UPDATE accounts SET currency = 'ZZZ'",
            "UPDATE accounts SET is_active = 2",
            "UPDATE categories SET kind = 'other'",
            "UPDATE categories SET color = -1",
            "UPDATE categories SET color = 16",
            "UPDATE categories SET color = 1.5",
            "UPDATE categories SET color = 'invalid'",
            "UPDATE transactions SET kind = 'other'",
            "UPDATE transactions SET date = '2026-02-30'",
            "UPDATE accounts SET name = ''",
            "UPDATE transactions SET amount_minor = 0",
        ] {
            let storage = Storage::open_in_memory().unwrap();
            seed(&storage);
            storage
                .connection
                .execute_batch("PRAGMA foreign_keys = OFF; PRAGMA ignore_check_constraints = ON; DROP TRIGGER accounts_currency_immutable; DROP TRIGGER categories_kind_immutable;")
                .unwrap();
            storage.connection.execute(sql, []).unwrap();
            let error = storage.snapshot().unwrap_err();
            assert!(
                matches!(
                    error.downcast_ref::<rusqlite::Error>(),
                    Some(rusqlite::Error::FromSqlConversionFailure(..))
                ),
                "{sql}: {error:#}"
            );
        }
    }

    #[test]
    fn migrates_populated_v1_preserving_history() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection.execute_batch(INITIAL_SCHEMA).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection.execute_batch("
            INSERT INTO accounts VALUES ('00000000-0000-0000-0000-000000000001', 'Cash', 'RUB', -123);
            INSERT INTO categories VALUES ('00000000-0000-0000-0000-000000000002', 'Food', 'expense');
            INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) VALUES ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 456, 'expense', '2026-10-06', 'Lunch');
        ").unwrap();
        migrate(&mut connection).unwrap();
        let mut storage = Storage { connection };
        let fresh = Storage::open_in_memory().unwrap();
        seed(&fresh);
        assert_eq!(storage.snapshot().unwrap(), fresh.snapshot().unwrap());
        assert!(storage.snapshot().unwrap().accounts[0].is_active);
        let violations: i64 = storage
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
        for currency in Currency::all() {
            let account = Account::new(currency.code(), *currency, 0).unwrap();
            storage.create_account(&account).unwrap();
        }
        assert_eq!(
            storage.snapshot().unwrap().accounts.len(),
            Currency::all().len() + 1
        );
    }

    #[test]
    fn account_writes_and_currency_lock_are_atomic() {
        let mut storage = Storage::open_in_memory().unwrap();
        let mut account = Account::new("Savings", "GBP".parse().unwrap(), -123).unwrap();
        let created = storage.create_account(&account).unwrap();
        assert_eq!(created.accounts, vec![account.clone()]);
        assert_eq!(created.active_accounts().count(), 1);
        assert!(storage.create_account(&account).is_err());
        account.name = "Archived".into();
        account.currency = "JPY".parse().unwrap();
        account.opening_balance_minor = i64::MIN;
        account.is_active = false;
        let updated = storage.update_account(&account).unwrap();
        assert_eq!(updated.accounts, vec![account.clone()]);
        assert_eq!(updated.active_accounts().count(), 0);
        assert_eq!(storage.snapshot().unwrap(), updated);
        let missing = Account::new("Missing", Currency::Usd, 0).unwrap();
        assert!(storage.update_account(&missing).is_err());
        account.name = " ".into();
        assert!(storage.update_account(&account).is_err());
        assert!(storage.create_account(&account).is_err());
        assert_eq!(storage.snapshot().unwrap(), updated);

        seed(&storage);
        let before = storage.snapshot().unwrap();
        let mut cash = before
            .accounts
            .iter()
            .find(|account| account.currency == Currency::Rub)
            .unwrap()
            .clone();
        cash.currency = Currency::Usd;
        cash.name = "Should roll back".into();
        assert!(storage.update_account(&cash).is_err());
        assert_eq!(storage.snapshot().unwrap(), before);
        assert!(
            storage
                .connection
                .execute(
                    "UPDATE accounts SET currency = 'EUR' WHERE currency = 'RUB'",
                    []
                )
                .is_err()
        );
        cash.currency = Currency::Rub;
        cash.is_active = false;
        let after = storage.update_account(&cash).unwrap();
        assert_eq!(after.transactions, before.transactions);
        assert_eq!(after.active_accounts().count(), 0);
    }

    #[test]
    fn snapshot_decode_failure_rolls_back_account_writes() {
        let mut storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        storage
            .connection
            .execute("UPDATE categories SET id = 'invalid'", [])
            .unwrap_err();
        storage
            .connection
            .execute_batch("PRAGMA foreign_keys = OFF; UPDATE categories SET id = 'invalid';")
            .unwrap();
        let account = Account::new("New", Currency::Usd, 0).unwrap();
        assert!(storage.create_account(&account).is_err());
        let count: i64 = storage
            .connection
            .query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let mut cash = account;
        cash.id = Uuid::from_u128(1);
        cash.currency = Currency::Rub;
        assert!(storage.update_account(&cash).is_err());
        let name: String = storage
            .connection
            .query_row("SELECT name FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Cash");
    }

    #[test]
    fn failed_v1_migration_rolls_back_rebuild() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(INITIAL_SCHEMA).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection.execute_batch("PRAGMA ignore_check_constraints = ON; INSERT INTO accounts VALUES ('id', 'Cash', 'ZZZ', 0); PRAGMA ignore_check_constraints = OFF;").unwrap();
        assert!(migrate(&mut connection).is_err());
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let currency: String = connection
            .query_row("SELECT currency FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(currency, "ZZZ");
        connection
            .execute("INSERT INTO accounts VALUES ('id2', 'USD', 'USD', 0)", [])
            .unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn names_are_unique_with_trim_case_and_unicode_and_self_is_exempt() {
        for (original, duplicate) in [
            ("Cash", " cAsH "),
            ("ÉPARGNE", "\u{2003}épargne\t"),
            ("СЧЁТ", " счёт "),
            ("İ", "i\u{307}"),
        ] {
            let mut storage = Storage::open_in_memory().unwrap();
            let mut account = Account::new(original, Currency::Usd, 0).unwrap();
            storage.create_account(&account).unwrap();
            let second = Account::new(duplicate, Currency::Eur, 0).unwrap();
            assert!(
                storage
                    .create_account(&second)
                    .unwrap_err()
                    .to_string()
                    .contains("already in use")
            );
            account.name = duplicate.into();
            account.is_active = false;
            storage.update_account(&account).unwrap();
            let mut other = Account::new("Other", Currency::Usd, 0).unwrap();
            storage.create_account(&other).unwrap();
            other.name = original.into();
            let before = storage.snapshot().unwrap();
            assert!(storage.update_account(&other).is_err());
            assert_eq!(storage.snapshot().unwrap(), before);

            // Uniqueness is per table, but categories share names across kinds.
            let mut category = Category::new(original, TransactionKind::Income).unwrap();
            storage.create_category(&category).unwrap();
            let second = Category::new(duplicate, TransactionKind::Expense).unwrap();
            assert!(
                storage
                    .create_category(&second)
                    .unwrap_err()
                    .to_string()
                    .contains("already in use")
            );
            category.name = duplicate.into();
            storage.update_category(&category).unwrap();
            let mut other = Category::new("Other", TransactionKind::Expense).unwrap();
            storage.create_category(&other).unwrap();
            other.name = original.into();
            let before = storage.snapshot().unwrap();
            assert!(storage.update_category(&other).is_err());
            assert_eq!(storage.snapshot().unwrap(), before);

            // DB indexes also reject duplicated Rust-normalized keys via raw SQL.
            let key = normalized_name(original);
            assert!(storage.connection.execute(
                "INSERT INTO accounts (id, name, currency, opening_balance_minor, name_key) VALUES ('raw', 'raw', 'USD', 0, ?1)", [&key]).is_err());
            assert!(storage.connection.execute(
                "INSERT INTO categories (id, name, kind, name_key) VALUES ('raw', 'raw', 'income', ?1)", [&key]).is_err());
        }
    }

    #[test]
    fn category_crud_kind_lock_and_deletion_preserve_transactions() {
        let mut storage = Storage::open_in_memory().unwrap();
        let mut category = Category::new("Salary", TransactionKind::Income).unwrap();
        assert_eq!(category.color, CategoryColor::default());
        assert_eq!(
            storage.create_category(&category).unwrap().categories,
            vec![category.clone()]
        );
        assert!(storage.create_category(&category).is_err());
        category.kind = TransactionKind::Expense;
        category.color = CategoryColor::ALL[15];
        category.name = "Spending".into();
        assert_eq!(
            storage.update_category(&category).unwrap().categories,
            vec![category.clone()]
        );
        seed(&storage);
        let before = storage.snapshot().unwrap();
        let mut food = before
            .categories
            .iter()
            .find(|category| category.id == Uuid::from_u128(2))
            .unwrap()
            .clone();
        food.kind = TransactionKind::Income;
        food.name = "Must roll back".into();
        food.color = CategoryColor::ALL[8];
        assert!(
            storage
                .update_category(&food)
                .unwrap_err()
                .to_string()
                .contains("kind cannot change")
        );
        assert_eq!(storage.snapshot().unwrap(), before);
        assert!(
            storage
                .connection
                .execute(
                    "UPDATE categories SET kind = 'income' WHERE name = 'Food'",
                    []
                )
                .is_err()
        );
        food.kind = TransactionKind::Expense;
        food.name = "Dining".into();
        let updated = storage.update_category(&food).unwrap();
        assert_eq!(updated.transactions, before.transactions);
        assert!(updated.categories.contains(&food));

        // More than one link, plus a transaction unrelated to the deletion.
        storage.connection.execute_batch("
            INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) SELECT '00000000-0000-0000-0000-000000000004', account_id, category_id, 789, kind, '2025-01-01', 'Dinner' FROM transactions;
        ").unwrap();
        storage.connection.execute(
            "INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) VALUES ('00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000001', ?1, 123, 'expense', '2026-01-01', 'Other')",
            [category.id.to_string()],
        ).unwrap();
        let mut expected = storage.snapshot().unwrap();
        expected
            .categories
            .retain(|category| category.id != food.id);
        for transaction in &mut expected.transactions {
            if transaction.category_id == Some(food.id) {
                transaction.category_id = None;
            }
        }
        assert_eq!(storage.delete_category(food.id).unwrap(), expected);
        assert_eq!(storage.snapshot().unwrap(), expected);
        assert_eq!(
            expected
                .transactions
                .iter()
                .filter(|transaction| transaction.category_id.is_none())
                .count(),
            2
        );
        // The now-uncategorized transaction is free to keep either kind.
        storage
            .connection
            .execute(
                "UPDATE transactions SET kind = 'income' WHERE category_id IS NULL",
                [],
            )
            .unwrap();
        assert!(
            storage
                .connection
                .execute(
                    "UPDATE transactions SET kind = 'income' WHERE category_id IS NOT NULL",
                    []
                )
                .is_err()
        );
        assert!(storage.connection.execute("INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) VALUES ('bad', '00000000-0000-0000-0000-000000000001', 'missing', 1, 'income', '2026-01-01', '')", []).is_err());
    }

    #[test]
    fn category_missing_ids_empty_names_and_snapshot_failures_roll_back() {
        let mut storage = Storage::open_in_memory().unwrap();
        seed(&storage);
        let missing = Category::new("Missing", TransactionKind::Expense).unwrap();
        let before = storage.snapshot().unwrap();
        assert!(
            storage
                .update_category(&missing)
                .unwrap_err()
                .to_string()
                .contains("does not exist")
        );
        assert!(
            storage
                .delete_category(missing.id)
                .unwrap_err()
                .to_string()
                .contains("does not exist")
        );
        let mut invalid = missing.clone();
        invalid.name = "\u{2003}".into();
        assert!(storage.create_category(&invalid).is_err());
        assert!(storage.update_category(&invalid).is_err());
        assert_eq!(storage.snapshot().unwrap(), before);

        // A malformed unrelated account forces snapshot decoding to fail after
        // each write, including after deletion has nullified the category link.
        storage
            .connection
            .execute_batch(
                "PRAGMA ignore_check_constraints = ON; UPDATE accounts SET is_active = 2;",
            )
            .unwrap();
        assert!(storage.create_category(&missing).is_err());
        let mut food = before.categories[0].clone();
        food.name = "Changed".into();
        assert!(storage.update_category(&food).is_err());
        assert!(storage.delete_category(food.id).is_err());
        storage
            .connection
            .execute("UPDATE accounts SET is_active = 1", [])
            .unwrap();
        assert_eq!(storage.snapshot().unwrap(), before);
    }

    fn legacy_database(version: i64) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection.execute_batch(INITIAL_SCHEMA).unwrap();
        connection.execute_batch("
            INSERT INTO accounts VALUES ('00000000-0000-0000-0000-000000000001', ' Cash ', 'RUB', -123);
            INSERT INTO categories VALUES ('00000000-0000-0000-0000-000000000002', 'ÉPICERIE', 'expense');
            INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note) VALUES ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 456, 'expense', '2026-10-06', 'Lunch');
        ").unwrap();
        if version == 2 {
            connection.execute_batch(ACCOUNTS_SCHEMA).unwrap();
            connection
                .execute("UPDATE accounts SET is_active = 0", [])
                .unwrap();
        }
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        connection
    }

    #[test]
    fn v1_and_v2_migrations_preserve_rows_and_backfill_keys() {
        for version in [1, 2] {
            let mut connection = legacy_database(version);
            migrate(&mut connection).unwrap();
            let mut storage = Storage { connection };
            let snapshot = storage.snapshot().unwrap();
            assert_eq!(snapshot.accounts[0].name, " Cash ");
            assert_eq!(snapshot.accounts[0].opening_balance_minor, -123);
            assert_eq!(snapshot.accounts[0].currency, Currency::Rub);
            assert_eq!(snapshot.accounts[0].is_active, version == 1);
            assert_eq!(snapshot.categories[0].name, "ÉPICERIE");
            assert_eq!(snapshot.categories[0].kind, TransactionKind::Expense);
            assert_eq!(snapshot.categories[0].color, CategoryColor::default());
            assert_eq!(
                snapshot.transactions,
                vec![Transaction {
                    id: Uuid::from_u128(3),
                    account_id: Uuid::from_u128(1),
                    category_id: Some(Uuid::from_u128(2)),
                    amount_minor: 456,
                    kind: TransactionKind::Expense,
                    date: date!(2026 - 10 - 06),
                    time: Time::MIDNIGHT,
                    transfer_account_id: None,
                    transfer_amount_minor: None,
                    note: "Lunch".into(),
                }]
            );
            let account_key: String = storage
                .connection
                .query_row("SELECT name_key FROM accounts", [], |row| row.get(0))
                .unwrap();
            let category_key: String = storage
                .connection
                .query_row("SELECT name_key FROM categories", [], |row| row.get(0))
                .unwrap();
            assert_eq!(account_key, "cash");
            assert_eq!(category_key, "épicerie");
            let violations: i64 = storage
                .connection
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(violations, 0);
            assert!(
                storage
                    .connection
                    .execute("UPDATE accounts SET currency = 'USD'", [])
                    .is_err()
            );
            assert!(
                storage
                    .connection
                    .execute("UPDATE categories SET kind = 'income'", [])
                    .is_err()
            );
            migrate(&mut storage.connection).unwrap();
            assert_eq!(storage.snapshot().unwrap(), snapshot);
        }
    }

    #[test]
    fn duplicate_legacy_names_abort_migration_without_data_loss() {
        for version in [1, 2] {
            for table in ["accounts", "categories"] {
                let mut connection = legacy_database(version);
                if table == "accounts" {
                    let sql = if version == 1 {
                        "INSERT INTO accounts VALUES ('duplicate', '\tCASH\t', 'USD', 0)"
                    } else {
                        "INSERT INTO accounts VALUES ('duplicate', '\tCASH\t', 'USD', 0, 1)"
                    };
                    connection.execute(sql, []).unwrap();
                } else {
                    connection
                        .execute(
                            "INSERT INTO categories VALUES ('duplicate', ' épicerie ', 'income')",
                            [],
                        )
                        .unwrap();
                }
                let error = migrate(&mut connection).unwrap_err().to_string();
                assert!(error.contains("duplicate"), "{error}");
                let actual_version: i64 = connection
                    .pragma_query_value(None, "user_version", |row| row.get(0))
                    .unwrap();
                assert_eq!(actual_version, version);
                let count: i64 = connection
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(count, 2);
                let fields: (String, i64, String, String) = connection
                    .query_row(
                        "SELECT category_id, amount_minor, kind, note FROM transactions",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .unwrap();
                assert_eq!(
                    fields,
                    (
                        Uuid::from_u128(2).to_string(),
                        456,
                        "expense".into(),
                        "Lunch".into()
                    )
                );
                assert!(connection.prepare("SELECT color FROM categories").is_err());
                assert!(connection.prepare("SELECT name_key FROM accounts").is_err());
            }
        }
    }

    #[test]
    fn file_persists_and_reopens() {
        let path = std::env::temp_dir().join(format!("cracker-desktop-{}.sqlite", Uuid::new_v4()));
        let expected = {
            let storage = Storage::open(&path).unwrap();
            let journal: String = storage
                .connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            assert_eq!(journal, "wal");
            let timeout: i64 = storage
                .connection
                .pragma_query_value(None, "busy_timeout", |row| row.get(0))
                .unwrap();
            assert_eq!(timeout, 5000);
            seed(&storage);
            storage.snapshot().unwrap()
        };
        {
            let storage = Storage::open(&path).unwrap();
            assert_eq!(storage.snapshot().unwrap(), expected);
        }
        std::fs::remove_file(path).unwrap();
    }
}
