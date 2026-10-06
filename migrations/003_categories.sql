-- Rust backfills name_key using trim().to_lowercase() and then creates unique
-- indexes, all within the migration transaction. Raw SQL writers must provide
-- these same normalized keys; SQLite lower() does not handle Unicode casing.
ALTER TABLE accounts ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
ALTER TABLE categories ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
ALTER TABLE categories ADD COLUMN color INTEGER NOT NULL DEFAULT 0
    CHECK (typeof(color) = 'integer' AND color BETWEEN 0 AND 15);

CREATE TEMP TABLE transactions_backup AS SELECT * FROM transactions;
DROP TABLE transactions;

CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    amount_minor INTEGER NOT NULL CHECK (typeof(amount_minor) = 'integer' AND amount_minor > 0),
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    date TEXT NOT NULL CHECK (
        length(date) = 10 AND date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
        AND date(date, '+0 days') IS NOT NULL AND date(date, '+0 days') = date
    ),
    note TEXT NOT NULL DEFAULT '',
    -- NO ACTION is checked after the single-column SET NULL has taken effect.
    -- A composite SET NULL would incorrectly clear kind too.
    FOREIGN KEY (category_id, kind) REFERENCES categories(id, kind)
);
CREATE INDEX transactions_account_id ON transactions(account_id);
CREATE INDEX transactions_category_kind ON transactions(category_id, kind);
INSERT INTO transactions SELECT * FROM transactions_backup;
DROP TABLE transactions_backup;

CREATE TRIGGER categories_kind_immutable
BEFORE UPDATE OF kind ON categories
WHEN NEW.kind <> OLD.kind AND EXISTS (SELECT 1 FROM transactions WHERE category_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'category kind cannot change with existing transactions');
END;

CREATE TRIGGER accounts_name_key_insert BEFORE INSERT ON accounts
WHEN NEW.name_key = ''
BEGIN
    SELECT RAISE(ABORT, 'account normalized name must not be empty');
END;
CREATE TRIGGER accounts_name_key_update BEFORE UPDATE OF name_key ON accounts
WHEN NEW.name_key = ''
BEGIN
    SELECT RAISE(ABORT, 'account normalized name must not be empty');
END;
CREATE TRIGGER categories_name_key_insert BEFORE INSERT ON categories
WHEN NEW.name_key = ''
BEGIN
    SELECT RAISE(ABORT, 'category normalized name must not be empty');
END;
CREATE TRIGGER categories_name_key_update BEFORE UPDATE OF name_key ON categories
WHEN NEW.name_key = ''
BEGIN
    SELECT RAISE(ABORT, 'category normalized name must not be empty');
END;
