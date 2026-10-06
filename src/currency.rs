//! Current ISO 4217 List One from SIX (2026-09-17), including fund and special codes.
//! No cryptocurrencies or withdrawn List Three codes. Source: checked-in `assets/iso4217.xml`.

use std::{fmt, str::FromStr};

use crate::domain::DomainError;

/// A validated ISO 4217 code with its English name and minor-unit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Currency {
    code: &'static str,
    name: &'static str,
    minor_unit_exponent: Option<u32>,
}

include!("currency_data.rs");

impl Currency {
    #[allow(non_upper_case_globals, reason = "Preserve the original Currency API.")]
    pub const Rub: Self = Self {
        code: "RUB",
        name: "Russian Ruble",
        minor_unit_exponent: Some(2),
    };
    #[allow(non_upper_case_globals, reason = "Preserve the original Currency API.")]
    pub const Usd: Self = Self {
        code: "USD",
        name: "US Dollar",
        minor_unit_exponent: Some(2),
    };
    #[allow(non_upper_case_globals, reason = "Preserve the original Currency API.")]
    pub const Eur: Self = Self {
        code: "EUR",
        name: "Euro",
        minor_unit_exponent: Some(2),
    };

    /// All 178 current codes, sorted by ASCII code.
    pub const fn all() -> &'static [Self] {
        CURRENCIES
    }
    pub const fn code(self) -> &'static str {
        self.code
    }
    pub const fn name(self) -> &'static str {
        self.name
    }
    /// Accounting exponent; ISO `N.A.` entries use zero (whole units).
    pub const fn exponent(self) -> u32 {
        match self.minor_unit_exponent {
            Some(value) => value,
            None => 0,
        }
    }
    /// Original ISO minor-unit value; `None` means `N.A.`, not ISO exponent zero.
    pub const fn minor_unit_exponent(self) -> Option<u32> {
        self.minor_unit_exponent
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for Currency {
    type Err = DomainError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CURRENCIES
            .binary_search_by_key(&value, |currency| currency.code)
            .map(|index| CURRENCIES[index])
            .map_err(|_| DomainError::InvalidCurrency(value.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_round_trips_and_matches_source() {
        let xml = include_str!("../assets/iso4217.xml");
        let mut codes: Vec<_> = xml
            .split("<Ccy>")
            .skip(1)
            .filter_map(|entry| entry.split_once("</Ccy>").map(|(code, _)| code))
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 178);
        assert_eq!(
            codes,
            Currency::all().iter().map(|c| c.code()).collect::<Vec<_>>()
        );
        for currency in Currency::all() {
            assert_eq!(currency.code().parse::<Currency>(), Ok(*currency));
            assert!(!currency.name().is_empty());
            assert!(currency.exponent() <= 4);
        }
        assert_eq!("USD".parse::<Currency>(), Ok(Currency::Usd));
        assert_eq!("RUB".parse::<Currency>(), Ok(Currency::Rub));
        assert_eq!("EUR".parse::<Currency>(), Ok(Currency::Eur));
        for code in ["ZZZ", "BTC", "ETH", "usd", " USD", "HRK"] {
            assert!(code.parse::<Currency>().is_err());
        }
    }
}
