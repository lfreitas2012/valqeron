CREATE TABLE IF NOT EXISTS issuer
(
    id           BLOB PRIMARY KEY,
    name         TEXT,
    status       TEXT    NOT NULL CHECK (status IN ('ACTIVE', 'RETIRED')),
    created_at   TEXT    NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1,
    cnpj         TEXT UNIQUE,
    lei          TEXT UNIQUE,
    country_code TEXT CHECK (country_code IS NULL OR length(country_code) = 2),
    CHECK (cnpj IS NULL OR country_code = 'BR')
) STRICT, WITHOUT ROWID;