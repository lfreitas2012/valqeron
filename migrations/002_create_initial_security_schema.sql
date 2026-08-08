CREATE TABLE IF NOT EXISTS security
(
    isin                     TEXT PRIMARY KEY,
    kind                     TEXT    NOT NULL CHECK (kind IN
                                                     ('COMMON_SHARE', 'PREFERRED_SHARE', 'UNIT', 'DEPOSITARY_RECEIPT')),
    status                   TEXT    NOT NULL CHECK (status IN ('ACTIVE', 'RETIRED')),
    created_at               TEXT    NOT NULL,
    version                  INTEGER NOT NULL DEFAULT 1,
    name                     TEXT,
    cfi                      TEXT,
    underlying_security      TEXT REFERENCES security (isin),
    depositary_receipt_ratio TEXT,

    -- Condition: DEPOSITARY_RECEIPT requires these fields to be NOT NULL
    CHECK (
        CASE
            WHEN kind = 'DEPOSITARY_RECEIPT'
                THEN underlying_security IS NOT NULL AND depositary_receipt_ratio IS NOT NULL
            ELSE
                underlying_security IS NULL AND depositary_receipt_ratio IS NULL
            END
        )
) STRICT, WITHOUT ROWID;
