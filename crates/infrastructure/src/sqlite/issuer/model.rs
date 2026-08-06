use rusqlite::Row;
use valqeron_core::{Issuer, Versioned};

use crate::sqlite::issuer::mapping::{
    column_datetime, column_issuer_id, column_opt_cnpj, column_opt_country_code, column_opt_lei,
    column_opt_name, column_status,
};
use crate::sqlite::row::FromRow;

#[derive(Debug)]
pub(crate) struct IssuerRow(pub Versioned<Issuer>);

impl IssuerRow {
    pub(crate) fn into_inner(self) -> Versioned<Issuer> {
        self.0
    }
}

impl FromRow for IssuerRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let id = column_issuer_id(row, "id")?;
        let status = column_status(row, "status")?;
        let created_at = column_datetime(row, "created_at")?;
        let name = column_opt_name(row, "name")?;
        let cnpj = column_opt_cnpj(row, "cnpj")?;
        let lei = column_opt_lei(row, "lei")?;
        let country_code = column_opt_country_code(row, "country_code")?;
        let version: u32 = row.get("version")?;

        let issuer = Issuer::reconstitute(id, status, created_at, name, cnpj, lei, country_code);

        Ok(Self(Versioned {
            data: issuer,
            version,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connection::{Database, Db};
    use std::str::FromStr;
    use valqeron_core::{IssuerId, IssuerStatus};
    use valqeron_identifiers::{Cnpj, CountryCode};

    #[test]
    fn issuer_row_round_trips_all_columns() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();

        let id = IssuerId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO issuer (id, name, status, created_at, cnpj, lei, country_code, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    id.as_bytes(),
                    "Acme Corp",
                    "RETIRED",
                    "2026-01-02T03:04:05+00:00",
                    "12.345.678/0001-95",
                    Option::<String>::None,
                    "BR",
                    7u32,
                ],
            )
            .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, name, status, created_at, cnpj, lei, country_code, version
                 FROM issuer WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                IssuerRow::from_row,
            )
            .unwrap();

        let Versioned { data, version } = row.into_inner();
        assert_eq!(data.id(), &id);
        assert_eq!(data.status(), IssuerStatus::Retired);
        assert_eq!(data.created_at().to_rfc3339(), "2026-01-02T03:04:05+00:00");
        assert_eq!(data.name().unwrap().as_str(), "Acme Corp");
        assert_eq!(
            data.cnpj().unwrap(),
            &Cnpj::new("12.345.678/0001-95").unwrap()
        );
        assert!(data.lei().is_none());
        assert_eq!(
            data.country_code().unwrap(),
            &CountryCode::from_str("BR").unwrap()
        );
        assert_eq!(version, 7);
    }

    #[test]
    fn issuer_row_maps_null_optionals_to_none() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();

        let id = IssuerId::new();
        {
            let conn = handle.write();
            conn.execute(
                "INSERT INTO issuer (id, status, created_at) VALUES (?1, 'ACTIVE', ?2)",
                rusqlite::params![id.as_bytes(), "2026-01-01T00:00:00+00:00"],
            )
            .unwrap();
        }

        let conn = handle.read();
        let row = conn
            .query_row(
                "SELECT id, name, status, created_at, cnpj, lei, country_code, version
                 FROM issuer WHERE id = ?1",
                rusqlite::params![id.as_bytes()],
                IssuerRow::from_row,
            )
            .unwrap();

        let issuer = row.into_inner().data;
        assert!(issuer.name().is_none());
        assert!(issuer.cnpj().is_none());
        assert!(issuer.lei().is_none());
        assert!(issuer.country_code().is_none());
        assert_eq!(issuer.status(), IssuerStatus::Active);
    }

    #[test]
    fn issuer_row_rejects_invalid_status() {
        let db = Database::open_in_memory().unwrap();
        let handle = db.handle();

        let conn = handle.read();
        let result: rusqlite::Result<IssuerRow> = conn.query_row(
            "SELECT randomblob(16) AS id, NULL AS name, 'BOGUS' AS status,
                    '2026-01-01T00:00:00+00:00' AS created_at, NULL AS cnpj,
                    NULL AS lei, NULL AS country_code, 1 AS version",
            [],
            IssuerRow::from_row,
        );

        assert!(matches!(
            result,
            Err(rusqlite::Error::FromSqlConversionFailure(..))
        ));
    }
}
