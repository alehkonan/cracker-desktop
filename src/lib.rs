//! Minimal finance domain and SQLite persistence; no UI or fabricated seed data.

pub mod currency;
pub mod domain;
pub mod storage;

pub use domain::{
    Account, Category, CategoryColor, Currency, DomainError, Transaction, TransactionKind,
    format_money, parse_money,
};
pub use storage::{Snapshot, Storage};
