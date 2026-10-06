CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    currency TEXT NOT NULL CHECK (currency IN ('RUB', 'USD', 'EUR')),
    opening_balance_minor INTEGER NOT NULL CHECK (typeof(opening_balance_minor) = 'integer')
);

CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    UNIQUE (id, kind)
);

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
