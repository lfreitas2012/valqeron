use rusqlite::Row;
use valqeron_core::{Security, SecuritySnapshot, Versioned};

use crate::sqlite::row::{FromRow, column_datetime};
use crate::sqlite::security::mapping::{
    column_issuer_id, column_kind, column_opt_cfi, column_opt_dr_ratio, column_opt_isin,
    column_opt_name, column_opt_security_id, column_security_id, column_status,
};

#[derive(Debug)]
pub(crate) struct SecurityRow(pub Versioned<Security>);

impl SecurityRow {
    pub(crate) fn into_inner(self) -> Versioned<Security> {
        self.0
    }
}

impl FromRow for SecurityRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let snapshot = SecuritySnapshot {
            id: column_security_id(row, "id")?,
            issuer_id: column_issuer_id(row, "issuer_id")?,
            kind: column_kind(row, "kind")?,
            status: column_status(row, "status")?,
            created_at: column_datetime(row, "created_at")?,
            name: column_opt_name(row, "name")?,
            isin: column_opt_isin(row, "isin")?,
            cfi: column_opt_cfi(row, "cfi")?,
            underlying_security_id: column_opt_security_id(row, "underlying_security_id")?,
            dr_ratio: column_opt_dr_ratio(row, "dr_ratio_receipts", "dr_ratio_underlying")?,
        };
        let version: u32 = row.get("version")?;

        Ok(Self(Versioned {
            data: Security::reconstitute(snapshot),
            version,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::database::{Database, Db, DbHandle};
    use valqeron_core::{
        DepositaryReceiptRatio, IssuerId, SecurityId, SecurityKind, SecurityStatus,
    };
    use valqeron_identifiers::{Cfi, Isin};

    fn insert_issuer(handle: &DbHandle, id: &IssuerId) {
        let conn = handle.write();
        conn.execute(
            "INSERT INTO issuer (id, status, created_at) VALUES (?1, 'ACTIVE', ?2)",
            rusqlite::params![id.as_bytes(), "2026-01-01T00:00:00.000Z"],
        )
        .unwrap();
    }

    #[test]
    fn security_row_round_trips_all_columns() {
        let db = Database::open_temp();
        let handle = db.handle();

        let issuer_id = IssuerId::new();
        insert_issuer(&handle, &issuer_id);

        let underlying_id = SecurityId::new();
        let id = SecurityId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO security (id, issuer_id, name, kind, status, created_at)
                 VALUES (?1, ?2, 'Vale ON', 'COMMON_SHARE', 'ACTIVE', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![underlying_id.as_bytes(), issuer_id.as_bytes()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO security (id, issuer_id, name, kind, status, created_at, isin, cfi,
                                       underlying_security_id, dr_ratio_receipts,
                                       dr_ratio_underlying, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    id.as_bytes(),
                    issuer_id.as_bytes(),
                    "Vale ADR",
                    "DEPOSITARY_RECEIPT",
                    "RETIRED",
                    "2026-01-02T03:04:05.000Z",
                    "US91912E1055",
                    "ESVUFR",
                    underlying_id.as_bytes(),
                    1u32,
                    2u32,
                    7u32,
                ],
            )
            .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, issuer_id, name, kind, status, created_at, isin, cfi,
                        underlying_security_id, dr_ratio_receipts, dr_ratio_underlying, version
                 FROM security WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                SecurityRow::from_row,
            )
            .unwrap();

        let Versioned { data, version } = row.into_inner();
        assert_eq!(data.id(), &id);
        assert_eq!(data.issuer_id(), &issuer_id);
        assert_eq!(data.kind(), SecurityKind::DepositaryReceipt);
        assert_eq!(data.status(), SecurityStatus::Retired);
        assert_eq!(data.name().unwrap().as_str(), "Vale ADR");
        assert_eq!(data.isin().unwrap(), &Isin::new("US91912E1055").unwrap());
        assert_eq!(data.cfi().unwrap(), &Cfi::new("ESVUFR").unwrap());
        assert_eq!(data.underlying_security_id().unwrap(), &underlying_id);
        assert_eq!(
            data.dr_ratio().unwrap(),
            &DepositaryReceiptRatio::new(1, 2).unwrap()
        );
        assert_eq!(version, 7);
    }

    #[test]
    fn security_row_maps_null_optionals_to_none() {
        let db = Database::open_temp();
        let handle = db.handle();

        let issuer_id = IssuerId::new();
        insert_issuer(&handle, &issuer_id);

        let id = SecurityId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO security (id, issuer_id, kind, status, created_at)
                 VALUES (?1, ?2, 'UNIT', 'ACTIVE', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![id.as_bytes(), issuer_id.as_bytes()],
            )
            .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, issuer_id, name, kind, status, created_at, isin, cfi,
                        underlying_security_id, dr_ratio_receipts, dr_ratio_underlying, version
                 FROM security WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                SecurityRow::from_row,
            )
            .unwrap();

        let security = row.into_inner().data;
        assert!(security.name().is_none());
        assert!(security.isin().is_none());
        assert!(security.cfi().is_none());
        assert!(security.underlying_security_id().is_none());
        assert!(security.dr_ratio().is_none());
        assert_eq!(security.kind(), SecurityKind::Unit);
    }

    #[test]
    fn security_row_rejects_one_sided_dr_ratio() {
        let db = Database::open_temp();
        let handle = db.handle();

        let conn = handle.read();
        let result: rusqlite::Result<SecurityRow> = conn.query_row(
            "SELECT randomblob(16) AS id, randomblob(16) AS issuer_id, NULL AS name,
                    'DEPOSITARY_RECEIPT' AS kind, 'ACTIVE' AS status,
                    '2026-01-01T00:00:00.000Z' AS created_at, NULL AS isin, NULL AS cfi,
                    NULL AS underlying_security_id, 2 AS dr_ratio_receipts,
                    NULL AS dr_ratio_underlying, 1 AS version",
            [],
            SecurityRow::from_row,
        );

        assert!(matches!(
            result,
            Err(rusqlite::Error::FromSqlConversionFailure(..))
        ));
    }

    #[test]
    fn security_row_rejects_invalid_kind() {
        let db = Database::open_temp();
        let handle = db.handle();

        let conn = handle.read();
        let result: rusqlite::Result<SecurityRow> = conn.query_row(
            "SELECT randomblob(16) AS id, randomblob(16) AS issuer_id, NULL AS name,
                    'BOGUS' AS kind, 'ACTIVE' AS status,
                    '2026-01-01T00:00:00.000Z' AS created_at, NULL AS isin, NULL AS cfi,
                    NULL AS underlying_security_id, NULL AS dr_ratio_receipts,
                    NULL AS dr_ratio_underlying, 1 AS version",
            [],
            SecurityRow::from_row,
        );

        assert!(matches!(
            result,
            Err(rusqlite::Error::FromSqlConversionFailure(..))
        ));
    }
}
