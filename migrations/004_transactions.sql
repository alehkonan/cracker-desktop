-- Preserve legacy IDs and dates; date-only history starts at midnight.
-- Drop referencing triggers during the rebuild so ALTER TABLE can resolve them.
DROP TRIGGER accounts_currency_immutable;
DROP TRIGGER categories_kind_immutable;
CREATE TEMP TABLE transactions_backup AS SELECT * FROM transactions;
DROP TABLE transactions;

CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    amount_minor INTEGER NOT NULL CHECK (typeof(amount_minor) = 'integer' AND amount_minor > 0),
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense', 'transfer')),
    date TEXT NOT NULL CHECK (
        length(date) = 10 AND date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
        AND date(date, '+0 days') IS NOT NULL AND date(date, '+0 days') = date
    ),
    note TEXT NOT NULL DEFAULT '',
    time TEXT NOT NULL DEFAULT '00:00:00' CHECK (
        length(time) = 8 AND time GLOB '[0-2][0-9]:[0-5][0-9]:[0-5][0-9]'
        AND substr(time, 1, 2) <= '23'
    ),
    transfer_account_id TEXT REFERENCES accounts(id) ON DELETE RESTRICT,
    transfer_amount_minor INTEGER,
    CHECK (
        (kind = 'transfer' AND category_id IS NULL
            AND transfer_account_id IS NOT NULL AND transfer_account_id <> account_id
            AND transfer_amount_minor IS NOT NULL
            AND typeof(transfer_amount_minor) = 'integer' AND transfer_amount_minor > 0)
        OR (kind IN ('income', 'expense') AND transfer_account_id IS NULL AND transfer_amount_minor IS NULL)
    ),
    FOREIGN KEY (category_id, kind) REFERENCES categories(id, kind)
);
CREATE INDEX transactions_account_id ON transactions(account_id);
CREATE INDEX transactions_category_kind ON transactions(category_id, kind);
CREATE INDEX transactions_transfer_account_id ON transactions(transfer_account_id);
CREATE INDEX transactions_datetime ON transactions(date, time, id);
INSERT INTO transactions (id, account_id, category_id, amount_minor, kind, date, note)
    SELECT id, account_id, category_id, amount_minor, kind, date, note FROM transactions_backup;
DROP TABLE transactions_backup;

CREATE TRIGGER accounts_currency_immutable
BEFORE UPDATE OF currency ON accounts
WHEN NEW.currency <> OLD.currency AND EXISTS (
    SELECT 1 FROM transactions WHERE account_id = OLD.id OR transfer_account_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'account currency cannot change with existing transactions');
END;
CREATE TRIGGER categories_kind_immutable
BEFORE UPDATE OF kind ON categories
WHEN NEW.kind <> OLD.kind AND EXISTS (SELECT 1 FROM transactions WHERE category_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'category kind cannot change with existing transactions');
END;

-- Inactive historical accounts remain editable, but cannot be newly selected.
CREATE TRIGGER transactions_active_accounts_insert BEFORE INSERT ON transactions
WHEN EXISTS (SELECT 1 FROM accounts WHERE is_active = 0
    AND (id = NEW.account_id OR id = NEW.transfer_account_id))
BEGIN
    SELECT RAISE(ABORT, 'newly selected transaction accounts must be active');
END;
CREATE TRIGGER transactions_active_accounts_update BEFORE UPDATE OF account_id, transfer_account_id ON transactions
WHEN EXISTS (SELECT 1 FROM accounts WHERE is_active = 0
    AND (id = NEW.account_id OR id = NEW.transfer_account_id)
    AND id <> OLD.account_id AND (OLD.transfer_account_id IS NULL OR id <> OLD.transfer_account_id))
BEGIN
    SELECT RAISE(ABORT, 'newly selected transaction accounts must be active');
END;
