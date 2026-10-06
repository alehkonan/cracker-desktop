-- ISO 4217 List One (SIX), published 2026-09-17.
-- Rebuild dependent tables without disabling foreign keys; preserve every row.
CREATE TEMP TABLE transactions_backup AS SELECT * FROM transactions;
DROP TABLE transactions;
CREATE TABLE accounts_v2 (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    currency TEXT NOT NULL CHECK (currency IN ('AED', 'AFN', 'ALL', 'AMD', 'AOA', 'ARS', 'AUD', 'AWG', 'AZN', 'BAM', 'BBD', 'BDT', 'BHD', 'BIF', 'BMD', 'BND', 'BOB', 'BOV', 'BRL', 'BSD', 'BTN', 'BWP', 'BYN', 'BZD', 'CAD', 'CDF', 'CHE', 'CHF', 'CHW', 'CLF', 'CLP', 'CNY', 'COP', 'COU', 'CRC', 'CUP', 'CVE', 'CZK', 'DJF', 'DKK', 'DOP', 'DZD', 'EGP', 'ERN', 'ETB', 'EUR', 'FJD', 'FKP', 'GBP', 'GEL', 'GHS', 'GIP', 'GMD', 'GNF', 'GTQ', 'GYD', 'HKD', 'HNL', 'HTG', 'HUF', 'IDR', 'ILS', 'INR', 'IQD', 'IRR', 'ISK', 'JMD', 'JOD', 'JPY', 'KES', 'KGS', 'KHR', 'KMF', 'KPW', 'KRW', 'KWD', 'KYD', 'KZT', 'LAK', 'LBP', 'LKR', 'LRD', 'LSL', 'LYD', 'MAD', 'MDL', 'MGA', 'MKD', 'MMK', 'MNT', 'MOP', 'MRU', 'MUR', 'MVR', 'MWK', 'MXN', 'MXV', 'MYR', 'MZN', 'NAD', 'NGN', 'NIO', 'NOK', 'NPR', 'NZD', 'OMR', 'PAB', 'PEN', 'PGK', 'PHP', 'PKR', 'PLN', 'PYG', 'QAR', 'RON', 'RSD', 'RUB', 'RWF', 'SAR', 'SBD', 'SCR', 'SDG', 'SEK', 'SGD', 'SHP', 'SLE', 'SOS', 'SRD', 'SSP', 'STN', 'SVC', 'SYP', 'SZL', 'THB', 'TJS', 'TMT', 'TND', 'TOP', 'TRY', 'TTD', 'TWD', 'TZS', 'UAH', 'UGX', 'USD', 'USN', 'UYI', 'UYU', 'UYW', 'UZS', 'VED', 'VES', 'VND', 'VUV', 'WST', 'XAD', 'XAF', 'XAG', 'XAU', 'XBA', 'XBB', 'XBC', 'XBD', 'XCD', 'XCG', 'XDR', 'XOF', 'XPD', 'XPF', 'XPT', 'XSU', 'XTS', 'XUA', 'XXX', 'YER', 'ZAR', 'ZMW', 'ZWG')),
    opening_balance_minor INTEGER NOT NULL CHECK (typeof(opening_balance_minor) = 'integer'),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (typeof(is_active) = 'integer' AND is_active IN (0, 1))
);
INSERT INTO accounts_v2 SELECT id, name, currency, opening_balance_minor, 1 FROM accounts;
DROP TABLE accounts;
ALTER TABLE accounts_v2 RENAME TO accounts;

CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    category_id TEXT NOT NULL,
    amount_minor INTEGER NOT NULL CHECK (typeof(amount_minor) = 'integer' AND amount_minor > 0),
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    date TEXT NOT NULL CHECK (
        length(date) = 10 AND date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
        AND date(date, '+0 days') IS NOT NULL AND date(date, '+0 days') = date
    ),
    note TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (category_id, kind) REFERENCES categories(id, kind) ON DELETE RESTRICT
);

CREATE INDEX transactions_account_id ON transactions(account_id);
CREATE INDEX transactions_category_kind ON transactions(category_id, kind);

INSERT INTO transactions SELECT * FROM transactions_backup;
DROP TABLE transactions_backup;

CREATE TRIGGER accounts_currency_immutable
BEFORE UPDATE OF currency ON accounts
WHEN NEW.currency <> OLD.currency AND EXISTS (SELECT 1 FROM transactions WHERE account_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'account currency cannot change with existing transactions');
END;
