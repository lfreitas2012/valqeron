-- ================ ISSUERS ================
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

-- ================ SECURITIES ================
CREATE TABLE IF NOT EXISTS security
(
    id                     BLOB PRIMARY KEY,
    issuer_id              BLOB    NOT NULL REFERENCES issuer (id),
    name                   TEXT,
    kind                   TEXT    NOT NULL CHECK (kind IN
                                                   ('COMMON_SHARE', 'PREFERRED_SHARE', 'UNIT', 'DEPOSITARY_RECEIPT')),
    status                 TEXT    NOT NULL CHECK (status IN ('ACTIVE', 'RETIRED')),
    created_at             TEXT    NOT NULL,
    version                INTEGER NOT NULL DEFAULT 1,
    isin                   TEXT UNIQUE,
    cfi                    TEXT CHECK (cfi IS NULL OR length(cfi) = 6),
    underlying_security_id BLOB REFERENCES security (id),
    dr_ratio_receipts      INTEGER CHECK (dr_ratio_receipts IS NULL OR dr_ratio_receipts > 0),
    dr_ratio_underlying    INTEGER CHECK (dr_ratio_underlying IS NULL OR dr_ratio_underlying > 0),
    CHECK ((dr_ratio_receipts IS NULL) = (dr_ratio_underlying IS NULL)),
    CHECK (kind = 'DEPOSITARY_RECEIPT' OR
           (underlying_security_id IS NULL AND dr_ratio_receipts IS NULL AND dr_ratio_underlying IS NULL)),
    CHECK (underlying_security_id IS NULL OR underlying_security_id <> id)
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_security_issuer_id ON security (issuer_id);

CREATE INDEX IF NOT EXISTS idx_security_underlying_security_id ON security (underlying_security_id)
    WHERE underlying_security_id IS NOT NULL;