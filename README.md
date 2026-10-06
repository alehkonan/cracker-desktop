# Cracker Desktop

A local-first personal finance desktop application built with Rust and GPUI for
macOS 27. It manages accounts, income/expense categories, and transactions, with
data stored locally in SQLite.

## Transactions

- Add, edit, and delete income, expenses, and transfers between accounts.
- Amounts use the selected account's currency. Cross-currency transfers require
  both the amount sent and the amount received; same-currency transfers use one amount.
- Income and expense categories are optional and must match the transaction type.
  Transfers have no category. Notes are optional.
- Optional dates and times default to the computer's local date/time at save.
  Explicit values use `yyyy-mm-dd` and `hh:mm:ss`. Times are stored as local wall
  time, without timezone conversion.
- New transactions offer active accounts only. Editing also offers the original
  inactive account in its corresponding source/destination selector.
- Transactions are listed newest first; account balances include transaction history.
- Database migration preserves existing transactions and assigns midnight to
  legacy records that had only a date.

## Libraries

| Library                         | Purpose                                      |
| ------------------------------- | -------------------------------------------- |
| `gpui-kit = 0.7.1`              | GPUI, UI components, and icons               |
| `rusqlite` with `bundled`       | Local SQLite storage                         |
| `uuid`                          | Unique record identifiers                    |
| `time`                          | Transaction dates                            |
| `directories`                   | Platform-specific application data directory |
| `thiserror`                     | Typed domain errors                          |
| `anyhow`                        | Application and database error handling      |
| `tracing`, `tracing-subscriber` | Logging and diagnostics                      |

## Running the application

Requirements:

- macOS 27.
- Rust 1.99 or newer.
- Full Xcode installation with macOS and Metal components. Command Line Tools
  alone may not be sufficient to compile Metal shaders.

Check the selected Xcode installation:

```sh
xcode-select -p
```

If necessary, select the full Xcode installation:

```sh
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
```

Run from the project directory:

```sh
cargo run --locked
```

For development, use a separate data directory and enable debug logging:

```sh
RUST_LOG=debug CRACKER_DATA_DIR=.data cargo run --locked
```

The first build may take some time due to GPUI dependencies and Metal compilation.
