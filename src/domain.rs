//! Fields are public for UI access. Constructors validate; callers modifying fields
//! or using struct literals must call `validate()` before treating a model as valid.

pub use crate::currency::Currency;
use std::{fmt, str::FromStr};
use time::{Date, Time};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("name must not be empty or whitespace-only")]
    EmptyName,
    #[error("transaction amount must be positive")]
    NonPositiveAmount,
    #[error("unsupported currency: {0}")]
    InvalidCurrency(String),
    #[error("unsupported transaction kind: {0}")]
    InvalidTransactionKind(String),
    #[error("invalid money: use a signed decimal with at most the currency's exponent digits")]
    InvalidMoney,
    #[error("money is outside the signed 64-bit minor-unit range")]
    MoneyOverflow,
    #[error("category color index {0} must be between 0 and 15")]
    InvalidCategoryColor(usize),
    #[error(
        "transfers require a different destination account, a positive destination amount, and no category"
    )]
    InvalidTransfer,
    #[error("income and expense transactions must not have transfer fields")]
    UnexpectedTransferFields,
    #[error("categories must be income or expense")]
    InvalidCategoryKind,
    #[error("transaction time must have whole-second precision")]
    InvalidTransactionTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Income,
    Expense,
    Transfer,
}

impl TransactionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::Transfer => "transfer",
        }
    }
}

impl fmt::Display for TransactionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TransactionKind {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            "transfer" => Ok(Self::Transfer),
            _ => Err(DomainError::InvalidTransactionKind(value.into())),
        }
    }
}

/// Exact, locale-independent formatting, e.g. `-12.03 RUB`.
/// Uses an unsigned magnitude so even `i64::MIN` is safe.
pub fn format_money(amount_minor: i64, currency: Currency) -> String {
    let magnitude = amount_minor.unsigned_abs();
    let sign = if amount_minor < 0 { "-" } else { "" };
    let exponent = currency.exponent();
    let scale = 10_u64.pow(exponent);
    if exponent == 0 {
        format!("{sign}{magnitude} {currency}")
    } else {
        format!(
            "{sign}{}.{:0width$} {currency}",
            magnitude / scale,
            magnitude % scale,
            width = exponent as usize
        )
    }
}

/// Parse a trimmed, locale-independent decimal into exact signed minor units.
/// An optional `+`/`-` is allowed; a whole digit is required. No grouping,
/// scientific notation, currency suffix, rounding, or excess fractional digits.
/// Negative values are valid for opening balances; transactions still validate positive.
pub fn parse_money(input: &str, currency: Currency) -> Result<i64, DomainError> {
    let input = input.trim();
    let (negative, digits) = if let Some(rest) = input.strip_prefix('-') {
        (true, rest)
    } else {
        (false, input.strip_prefix('+').unwrap_or(input))
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > currency.exponent() as usize
        || (digits.contains('.') && fraction.is_empty())
    {
        return Err(DomainError::InvalidMoney);
    }
    let mut magnitude = 0_u64;
    for digit in whole.bytes().chain(fraction.bytes()) {
        magnitude = magnitude
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit - b'0')))
            .ok_or(DomainError::MoneyOverflow)?;
    }
    magnitude = magnitude
        .checked_mul(10_u64.pow(currency.exponent() - fraction.len() as u32))
        .ok_or(DomainError::MoneyOverflow)?;
    if negative && magnitude == i64::MIN.unsigned_abs() {
        Ok(i64::MIN)
    } else {
        let amount = i64::try_from(magnitude).map_err(|_| DomainError::MoneyOverflow)?;
        Ok(if negative { -amount } else { amount })
    }
}

fn validate_name(name: &str) -> Result<(), DomainError> {
    if name.trim().is_empty() {
        Err(DomainError::EmptyName)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub currency: Currency,
    pub opening_balance_minor: i64,
    pub is_active: bool,
}

impl Account {
    pub fn new(
        name: impl Into<String>,
        currency: Currency,
        opening_balance_minor: i64,
    ) -> Result<Self, DomainError> {
        let account = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            currency,
            opening_balance_minor,
            is_active: true,
        };
        account.validate()?;
        Ok(account)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_name(&self.name)
    }
}

/// A palette slot, independent of UI color names or RGB values.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CategoryColor(u8);

impl CategoryColor {
    pub const ALL: [Self; 16] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
        Self(8),
        Self(9),
        Self(10),
        Self(11),
        Self(12),
        Self(13),
        Self(14),
        Self(15),
    ];

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<usize> for CategoryColor {
    type Error = DomainError;

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        Self::ALL
            .get(index)
            .copied()
            .ok_or(DomainError::InvalidCategoryColor(index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub kind: TransactionKind,
    pub color: CategoryColor,
}

impl Category {
    pub fn new(name: impl Into<String>, kind: TransactionKind) -> Result<Self, DomainError> {
        let category = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            color: CategoryColor::default(),
        };
        category.validate()?;
        Ok(category)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        validate_name(&self.name)?;
        if self.kind == TransactionKind::Transfer {
            return Err(DomainError::InvalidCategoryKind);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: Uuid,
    pub account_id: Uuid,
    pub category_id: Option<Uuid>,
    pub amount_minor: i64,
    pub kind: TransactionKind,
    pub date: Date,
    pub time: Time,
    pub transfer_account_id: Option<Uuid>,
    pub transfer_amount_minor: Option<i64>,
    pub note: String,
}

impl Transaction {
    /// Creates an income/expense at midnight with no transfer fields. To build
    /// a transfer with this legacy signature, create an uncategorized expense,
    /// set kind and both transfer fields, then call `validate()`.
    pub fn new(
        account_id: Uuid,
        category_id: impl Into<Option<Uuid>>,
        amount_minor: i64,
        kind: TransactionKind,
        date: Date,
        note: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let transaction = Self {
            id: Uuid::new_v4(),
            account_id,
            category_id: category_id.into(),
            amount_minor,
            kind,
            date,
            time: Time::MIDNIGHT,
            transfer_account_id: None,
            transfer_amount_minor: None,
            note: note.into(),
        };
        transaction.validate()?;
        Ok(transaction)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.amount_minor <= 0 {
            return Err(DomainError::NonPositiveAmount);
        }
        if self.time.nanosecond() != 0 {
            return Err(DomainError::InvalidTransactionTime);
        }
        match self.kind {
            TransactionKind::Transfer => {
                if self.category_id.is_some()
                    || self.transfer_account_id.is_none()
                    || self.transfer_account_id == Some(self.account_id)
                    || !matches!(self.transfer_amount_minor, Some(amount) if amount > 0)
                {
                    return Err(DomainError::InvalidTransfer);
                }
            }
            TransactionKind::Income | TransactionKind::Expense => {
                if self.transfer_account_id.is_some() || self.transfer_amount_minor.is_some() {
                    return Err(DomainError::UnexpectedTransferFields);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "A failed test fixture must fail the test immediately."
)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn transfer_validation_and_datetime_precision() {
        let source = Uuid::new_v4();
        let destination = Uuid::new_v4();
        let mut transaction = Transaction::new(
            source,
            None,
            10,
            TransactionKind::Expense,
            date!(2026 - 10 - 06),
            "",
        )
        .unwrap();
        assert_eq!(transaction.time, Time::MIDNIGHT);
        assert_eq!(transaction.transfer_account_id, None);
        assert_eq!(transaction.transfer_amount_minor, None);
        transaction.transfer_account_id = Some(destination);
        assert_eq!(
            transaction.validate(),
            Err(DomainError::UnexpectedTransferFields)
        );
        transaction.kind = TransactionKind::Transfer;
        assert_eq!(transaction.validate(), Err(DomainError::InvalidTransfer));
        for amount in [i64::MIN, -1, 0] {
            transaction.transfer_amount_minor = Some(amount);
            assert_eq!(transaction.validate(), Err(DomainError::InvalidTransfer));
        }
        transaction.transfer_amount_minor = Some(i64::MAX);
        assert_eq!(transaction.validate(), Ok(()));
        transaction.transfer_account_id = Some(source);
        assert_eq!(transaction.validate(), Err(DomainError::InvalidTransfer));
        transaction.transfer_account_id = None;
        assert_eq!(transaction.validate(), Err(DomainError::InvalidTransfer));
        transaction.transfer_account_id = Some(destination);
        transaction.category_id = Some(Uuid::new_v4());
        assert_eq!(transaction.validate(), Err(DomainError::InvalidTransfer));
        transaction.category_id = None;
        transaction.time = time::macros::time!(23:59:59.001);
        assert_eq!(
            transaction.validate(),
            Err(DomainError::InvalidTransactionTime)
        );
        assert_eq!(
            Category::new("Transfer", TransactionKind::Transfer),
            Err(DomainError::InvalidCategoryKind)
        );
        assert_eq!(TransactionKind::Transfer.as_str(), "transfer");
    }

    #[test]
    fn category_colors_and_optional_category_are_typed() {
        assert_eq!(CategoryColor::default(), CategoryColor::ALL[0]);
        for (index, color) in CategoryColor::ALL.iter().enumerate() {
            assert_eq!(color.index(), index);
            assert_eq!(CategoryColor::try_from(index), Ok(*color));
        }
        for index in [16, 256, usize::MAX] {
            assert_eq!(
                CategoryColor::try_from(index),
                Err(DomainError::InvalidCategoryColor(index))
            );
        }
        let account = Uuid::new_v4();
        let category = Uuid::new_v4();
        let linked = Transaction::new(
            account,
            category,
            1,
            TransactionKind::Income,
            date!(2026 - 10 - 06),
            "",
        )
        .unwrap();
        assert_eq!(linked.category_id, Some(category));
        let unlinked = Transaction::new(
            account,
            None,
            1,
            TransactionKind::Expense,
            date!(2026 - 10 - 06),
            "",
        )
        .unwrap();
        assert_eq!(unlinked.category_id, None);
    }

    #[test]
    fn exact_signed_money() {
        for (amount, expected) in [
            (0, "0.00 USD"),
            (1, "0.01 USD"),
            (-1, "-0.01 USD"),
            (1203, "12.03 USD"),
            (i64::MAX, "92233720368547758.07 USD"),
            (i64::MIN, "-92233720368547758.08 USD"),
        ] {
            assert_eq!(format_money(amount, Currency::Usd), expected);
        }
        assert_eq!(format_money(100, Currency::Rub), "1.00 RUB");
        assert_eq!(format_money(100, Currency::Eur), "1.00 EUR");
    }

    #[test]
    fn parses_exact_currency_money() {
        for (code, input, expected, formatted) in [
            ("JPY", "-123", -123, "-123 JPY"),
            ("USD", " +12.3 ", 1230, "12.30 USD"),
            ("KWD", "-0.001", -1, "-0.001 KWD"),
            ("CLF", "1.2345", 12345, "1.2345 CLF"),
            ("UYW", "-2.0001", -20001, "-2.0001 UYW"),
        ] {
            let currency = code.parse().unwrap();
            assert_eq!(parse_money(input, currency), Ok(expected));
            assert_eq!(format_money(expected, currency), formatted);
        }
        for code in ["JPY", "USD", "KWD", "CLF"] {
            let currency = code.parse().unwrap();
            for amount in [i64::MIN, i64::MAX, -1, 0, 1] {
                let formatted = format_money(amount, currency);
                let decimal = formatted.split_once(' ').unwrap().0;
                assert_eq!(parse_money(decimal, currency), Ok(amount));
            }
        }
        assert_eq!(
            parse_money("-92233720368547758.08", Currency::Usd),
            Ok(i64::MIN)
        );
        for input in [
            "92233720368547758.08",
            "-92233720368547758.09",
            "1844674407370955161600",
        ] {
            assert_eq!(
                parse_money(input, Currency::Usd),
                Err(DomainError::MoneyOverflow)
            );
        }
        for input in [
            "", "-", "+", ".1", "1.", "1.234", "1,00", "1e2", "1 2", "--1", "NaN", "1.2.3",
            "1 USD", "١",
        ] {
            assert_eq!(
                parse_money(input, Currency::Usd),
                Err(DomainError::InvalidMoney),
                "{input}"
            );
        }
        assert_eq!(
            parse_money("1.0", "JPY".parse().unwrap()),
            Err(DomainError::InvalidMoney)
        );
        assert_eq!(parse_money("-0.00", Currency::Usd), Ok(0));
    }

    #[test]
    fn validates_names_and_amounts() {
        for name in ["", " \t\n", "\u{2003}"] {
            assert_eq!(
                Account::new(name, Currency::Rub, 0).unwrap_err(),
                DomainError::EmptyName
            );
            assert_eq!(
                Category::new(name, TransactionKind::Income).unwrap_err(),
                DomainError::EmptyName
            );
        }
        let account = Account::new("Cash", Currency::Rub, i64::MIN).unwrap();
        assert!(account.is_active);
        let category = Category::new("Food", TransactionKind::Expense).unwrap();
        for amount in [i64::MIN, -1, 0] {
            assert_eq!(
                Transaction::new(
                    account.id,
                    category.id,
                    amount,
                    category.kind,
                    date!(2026 - 10 - 06),
                    ""
                )
                .unwrap_err(),
                DomainError::NonPositiveAmount
            );
        }
        let mut transaction = Transaction::new(
            account.id,
            category.id,
            1,
            category.kind,
            date!(2026 - 10 - 06),
            "",
        )
        .unwrap();
        transaction.amount_minor = 0;
        assert_eq!(transaction.validate(), Err(DomainError::NonPositiveAmount));
        assert!("ZZZ".parse::<Currency>().is_err());
        assert_eq!(
            "transfer".parse::<TransactionKind>(),
            Ok(TransactionKind::Transfer)
        );
    }
}
