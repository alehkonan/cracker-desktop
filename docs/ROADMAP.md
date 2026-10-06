# Roadmap

## 0. Basic scaffold — completed

- [x] Cargo project, pinned GPUI Kit, and separation of UI and core code.
- [x] Window, navigation, clear empty states, and database path display.
- [x] Models, exact money formatting, SQLite, and the initial migration.
- [x] Tests for validation, schema constraints, reads, and reopening a database file.

This initial stage provided navigation without CRUD, forms, or record lists.
Account and category functionality has since been implemented; demo records are
never inserted automatically.

## 1. Application services and accounts — implemented, with future refinements

- [x] Account creation, reading, and editing, including name/currency validation,
      exact opening-balance parsing, and UUID generation on creation.
- [x] Account list and shared modal form using GPUI Kit Input/Select/Button controls.
- [x] Searchable ISO 4217 currencies, currency-specific precision, and active status.
- [x] Writes on the GPUI background executor, saving/error states, and Entity
      updates when tasks complete.
- [x] Currency changes blocked for accounts with transactions; inactive accounts
      retain their history and are excluded by `Snapshot::active_accounts()`.
- [x] Unique account names, atomic snapshots, and rollback/currency-lock tests.
- [ ] Consider `service.rs` with commands and typed operation errors when additional
      application workflows justify it.
- [ ] Consider a single background worker owning a SQLite connection for sequential
      commands and more complex workflows; current writes use separate connections.

**Completion criteria:** accounts can be created and renamed, data survives
restart, invalid input remains visible in the form and is not saved, and storage
tests verify rollback and currency-change restrictions. Manual runtime checks
are still required.

## 2. Categories — implemented

- [x] Creation/editing: name, income/expense kind, and one of 16 colors.
- [x] Income/Expense groups, one shared add button, and a modal form.
- [x] Kind changes blocked for categories with transactions.
- [x] Unique category names across both kinds and unique account names among
      accounts, ignoring case and leading/trailing whitespace.
- [x] Deletion with confirmation and linked transaction count; transactions remain
      without a category and retain their kind, amount, date, note, and account.
- [x] v3 migration, atomic background writes, and constraint/rollback/palette tests.
- [ ] Category selection filtered by kind in the future transaction form.

Manual testing of dialogs, focus, VoiceOver, and both themes is still required.

## 3. Transactions — first useful MVP

1. Create and edit transactions: account, optional category, income/expense kind,
   amount, date, and optional note; use one editor for create/edit.
2. Strict amount parsing without `f64`, overflow checks, and relationship validation.
3. Atomic SQL writes; show success only after commit.
4. Table/virtualized list: date, account, category, amount, and note;
   newest-first ordering and an edit button/action.
5. Transaction deletion with confirmation. Recalculate the current account balance
   from its opening balance and transactions, checking overflow.
6. Filters by period/account/category and stable pagination rather than loading
   the entire history on every render. Add appropriate indexes through migrations.

**Completion criteria:** users can track real income and expenses; editing an
amount/account updates balances, and data survives restart. Do not combine
currencies without conversion; show totals separately. Tests cover create/edit/delete,
account changes, category-kind mismatches, nonexistent IDs, overflow, rollback,
ordering, and filters.

## 4. Usability and data safety

- Keyboard shortcuts: new transaction, save, cancel; predictable Tab/focus handling.
- Light/dark themes and system theme changes; localized amount/date display.
- CSV export, followed by import with preview, validation, and atomic writes.
  Add `serde`/`csv` at that stage, not in advance.
- Backup/restore through the SQLite Backup API; test restoration and migration
  of older databases.
- Form/navigation UI tests through `gpui-kit/test-support`, plus manual IME and
  VoiceOver testing.

**Completion criteria:** backups and verified restoration are available; the
interface is keyboard-accessible and remains responsive with a large history.

## 5. Release for macOS 27

- Reproducible release builds; choose a bundling tool after verifying its current
  GPUI/Metal support, for example by evaluating `cargo-bundle`.
- `.app`, Info.plist, bundle identifier, icon, assets, and Metal resources.
- Developer ID signing, notarization, and Gatekeeper checks on a clean machine.
- Verify required architectures: Apple Silicon; Intel only if needed and a
  compatible target environment is available.
- Local checks before release: fmt → check → clippy → tests;
  manual runtime smoke test on macOS 27. No GitHub workflows.

**Completion criteria:** the application opens from Finder on the target machine
without a development environment, preserves data across versions, and passes
backup → update → restore testing.

## After the MVP, only when needed

Transfers between accounts as atomic linked operations, multicurrency analytics,
budgets, recurring transactions, encryption, and synchronization. Keep these out
of the initial MVP: they significantly complicate the money model, data conflicts,
and UX.
