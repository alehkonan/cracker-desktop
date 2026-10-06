# Architecture

## Initial MVP scope

One user, one local SQLite file, accounts, categories, and income/expense
transactions. No registration, server, synchronization, transfers between accounts,
or automatic exchange rates. A transfer must not be modeled as unrelated income
and expense transactions: it requires a separate, future atomic operation.

## Layers

```text
GPUI views / Entity / forms
          ↓ commands and results
GPUI background tasks
          ↓ validation of relationships + SQL transactions
Storage / rusqlite
          ↓
SQLite in Application Support
```

`domain` has no knowledge of the UI or SQLite. `storage` loads a typed `Snapshot`;
record decoding errors are not hidden. The UI displays account and category lists
and snapshot counts; the Transactions section remains a scaffold. Model fields
are public for reading and straightforward editing. Constructors and `validate()`
check local invariants; storage write methods also validate uniqueness and
relationships, backed by database constraints.

The database is opened and read **before the event loop starts**. The initial
connection remains in the root view. Account and category writes run on the GPUI
background executor, each using a separate connection. Their results return to a
GPUI task, which updates the snapshot and calls `cx.notify()`. Rendering does not
query SQLite. Do not share a connection between threads or block the event loop
with slow SQL or a busy timeout. A separate Tokio runtime is not needed.

A single background worker owning a `Connection`, with sequential commands over
a channel, remains a possible future refinement for more complex operations and
export/backup. A second read-only connection may be appropriate when needed.

A generic Repository trait, a DI container, or a workspace with multiple crates
is unnecessary for three entities. UI components are split into separate files;
add an application service only when commands justify it. The `gui` feature
allows core validation without compiling GPUI.

## Financial invariants

- Account: UUID, nonempty unique name, currency, opening balance as `i64`, and
  active status. New accounts are active by default.
- Supported currencies come from the current ISO 4217 List One, bundled from
  SIX: 178 codes, including fund, precious-metal, and special-purpose codes, but
  no cryptocurrencies. Decimal precision follows each currency's metadata;
  entries with `N.A.` minor units use whole units for accounting.
- Category: UUID, nonempty unique name, `income`/`expense` kind, and one of 16
  fixed colors. Its kind cannot change while linked transactions exist.
- Account names are unique among accounts; category names are unique among all
  categories, regardless of kind. Comparison uses Rust's `trim().to_lowercase()`;
  normalized keys are stored in SQLite and protected by unique indexes.
- Transaction: UUID, an existing account, an optional category, a positive `i64`
  amount in the currency's minor units, `income`/`expense` kind, a `time::Date`,
  and a note.
- A transaction's currency is determined by its account. Changing the currency
  of an account with transactions is blocked in the UI and by a database trigger;
  supporting conversion would require an explicit migration/conversion workflow.
- When a category is present, the transaction's kind must match its kind,
  protected by a composite foreign key. Deleting a category clears only
  `category_id`; kind, amount, date, note, and account remain unchanged.
- Opening balances may be negative. Current balances are not calculated yet.
  Future formula: opening balance + income − expenses. Check overflow; do not
  use unchecked arithmetic or add different currencies together.
- Parse amounts from decimal strings, for example `12.34` → `1234` for a currency
  with two decimal places, without `f64`. `parse_money` accepts a decimal point,
  an optional sign, and no grouping separators. Check precision and overflow
  before writing to the database.
- A transaction date is a calendar date, not a UTC timestamp. SQLite stores it
  as ISO `YYYY-MM-DD`; creation/update timestamps can be added in a later migration.

`format_money` uses no floating point and correctly handles `i64::MIN`.
It produces technical formatting with a decimal point, not localized display.

## Database and migrations

The schema is updated through `001_initial.sql`, `002_accounts.sql`, and
`003_categories.sql`; its version is stored in `PRAGMA user_version`. Migrations
and the version update are written in a single SQL transaction. A newer,
unrecognized version is rejected: do not treat it as an empty database or delete
its data. Duplicate legacy names stop the v3 migration without renaming records
or losing data.

Foreign keys, WAL, and a busy timeout of up to five seconds are enabled. Foreign
keys prevent deletion of accounts referenced by transactions. Accounts have an
active status; `Snapshot::active_accounts()` supplies choices for future new
transactions. Categories are not archived: deletion requires confirmation, and
`ON DELETE SET NULL` leaves linked transactions without a category. CRUD methods
use IMMEDIATE transactions and return a snapshot atomically with the write; a
snapshot decoding error also rolls back the change.

SQL CHECK constraints enforce integer amounts and validate currency, kind, date,
and category color. UUIDs are parsed on read, and names are validated again in
Rust. Direct SQL writers must supply `name_key` values normalized with the same
Rust trimming/lowercase rules; SQLite's built-in `lower()` is ASCII-only.

Snapshot loading reads all three tables in one read transaction. This is a small
bootstrap, **not a strategy for a large transaction history**. Larger lists will
need filtered queries, stable `date DESC, id DESC` ordering, pagination, and
virtualization. Current snapshot transaction ordering is `date ASC, id ASC` and
is covered by a test.

## macOS

The target system is macOS 27. Before release, separately verify Retina rendering,
keyboard/IME handling, focus, VoiceOver, light/dark themes, and system theme
changes. Using GPUI Kit alone does not establish full application accessibility.

Category colors use named palette slots with light/dark variants. Markers are
adjusted to at least 3:1 contrast against the theme background. Names, selection
outlines, and selection text remain independent of hue; distinct colors do not
guarantee distinction for every color-vision condition. Manual UI testing is
still required.

The application currently runs as a binary via `cargo run`, not an `.app` bundle.
Distribution requires an Info.plist, a stable bundle identifier, an icon, a
deployment target, Metal/assets, then Developer ID signing and notarization.
Do not claim compatibility with older macOS versions without building and testing
on them. Developer ID requires membership in the Apple Developer Program;
local execution does not.

Storage is unencrypted. Encryption/Keychain support is a separate task with its
own threat model; a form password alone does not protect the SQLite file. Do not
write notes, amounts, or complete transactions to technical logs.
