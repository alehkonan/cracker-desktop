//! Finance domain and exact currency arithmetic.

pub mod currency;
pub mod domain;

pub use domain::{
    Account, Category, CategoryColor, Currency, DomainError, Transaction, TransactionKind,
    format_money, parse_money,
};
