use crate::v1;
use crate::v1::write_outcome_proto::{Applied, Missing, Outcome, VersionMismatch};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use uuid::Uuid;
use valqeron_core::{
    Cnpj, CnpjError, CountryCode, CountryCodeError, Issuer, IssuerBuilderError, IssuerId,
    IssuerName, IssuerNameError, IssuerPatch, IssuerPatchBuilder, IssuerStatus, IssuerStatusError,
    Lei, LeiError, NonEmpty, Versioned, WriteOutcome,
};

#[derive(Debug)]
pub struct PatchCommand {
    pub id: IssuerId,
    pub expected_version: u32,
    pub patch: IssuerPatch,
    pub dry_run: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum IssuerMappingError {
    #[error("invalid issuer id: {0}")]
    InvalidId(String),

    #[error("invalid created_at timestamp: {0}")]
    InvalidTimestamp(String),

    #[error(transparent)]
    Name(#[from] IssuerNameError),

    #[error(transparent)]
    Status(#[from] IssuerStatusError),

    #[error(transparent)]
    Cnpj(#[from] CnpjError),

    #[error(transparent)]
    Lei(#[from] LeiError),

    #[error(transparent)]
    CountryCode(#[from] CountryCodeError),

    #[error(transparent)]
    Builder(#[from] IssuerBuilderError),

    #[error("a patch must set at least one field")]
    EmptyPatch,

    #[error("missing required field `{0}`")]
    MissingField(&'static str),
}

pub fn parse_issuer_id(raw: &str) -> Result<IssuerId, IssuerMappingError> {
    Uuid::from_str(raw)
        .map(IssuerId::from_uuid)
        .map_err(|e| IssuerMappingError::InvalidId(e.to_string()))
}

fn parse_timestamp(raw: &str) -> Result<DateTime<Utc>, IssuerMappingError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| IssuerMappingError::InvalidTimestamp(e.to_string()))
}

pub fn issuer_to_proto(versioned: &Versioned<Issuer>) -> v1::IssuerProto {
    let issuer = &versioned.data;
    v1::IssuerProto {
        id: issuer.id().value(),
        status: String::from(issuer.status()),
        created_at: issuer.created_at().to_rfc3339(),
        version: versioned.version,
        name: issuer.name().map(|n| n.as_str().to_string()),
        cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
        lei: issuer.lei().map(|l| l.as_str().to_string()),
        country_code: issuer.country_code().map(|c| c.as_str().to_string()),
    }
}

pub fn issuer_from_proto(msg: &v1::IssuerProto) -> Result<Versioned<Issuer>, IssuerMappingError> {
    let mut builder = Issuer::builder()
        .id(parse_issuer_id(&msg.id)?)
        .status(IssuerStatus::from_str(&msg.status)?)
        .created_at(parse_timestamp(&msg.created_at)?);

    if let Some(name) = msg.name.as_deref() {
        builder = builder.name(IssuerName::new(name)?);
    }
    if let Some(cnpj) = msg.cnpj.as_deref() {
        builder = builder.cnpj(Cnpj::parse(cnpj)?);
    }
    if let Some(lei) = msg.lei.as_deref() {
        builder = builder.lei(Lei::parse(lei)?);
    }
    if let Some(cc) = msg.country_code.as_deref() {
        builder = builder.country_code(CountryCode::parse(cc)?);
    }

    Ok(Versioned {
        data: builder.build()?,
        version: msg.version,
    })
}

pub fn register_request_to_issuer(
    req: &v1::RegisterIssuerRequestProto,
) -> Result<Issuer, IssuerMappingError> {
    let now = Utc::now();
    let created_at = DateTime::<Utc>::from_timestamp_millis(now.timestamp_millis()).unwrap_or(now);
    let mut builder = Issuer::builder().created_at(created_at);
    if let Some(name) = req.name.as_deref() {
        builder = builder.name(IssuerName::new(name)?);
    }
    if let Some(status) = req.status.as_deref() {
        builder = builder.status(IssuerStatus::from_str(status)?);
    }
    if let Some(cnpj) = req.cnpj.as_deref() {
        builder = builder.cnpj(Cnpj::parse(cnpj)?);
    }
    if let Some(lei) = req.lei.as_deref() {
        builder = builder.lei(Lei::parse(lei)?);
    }
    if let Some(cc) = req.country_code.as_deref() {
        builder = builder.country_code(CountryCode::parse(cc)?);
    }
    Ok(builder.build()?)
}

pub fn issuer_to_register_request(
    issuer: &Issuer,
    dry_run: bool,
) -> v1::RegisterIssuerRequestProto {
    v1::RegisterIssuerRequestProto {
        name: issuer.name().map(|n| n.as_str().to_string()),
        status: Some(String::from(issuer.status())),
        cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
        lei: issuer.lei().map(|l| l.as_str().to_string()),
        country_code: issuer.country_code().map(|c| c.as_str().to_string()),
        dry_run,
    }
}

pub fn patch_request_to_domain(
    req: &v1::PatchIssuerRequestProto,
) -> Result<PatchCommand, IssuerMappingError> {
    let id = parse_issuer_id(&req.id)?;
    let mut builder: Option<IssuerPatchBuilder<NonEmpty>> = None;

    if let Some(name) = req.name.as_deref() {
        let name = IssuerName::new(name)?;
        builder = Some(match builder {
            Some(b) => b.name(name),
            None => IssuerPatch::builder().name(name),
        });
    }
    if let Some(status) = req.status.as_deref() {
        let status = IssuerStatus::from_str(status)?;
        builder = Some(match builder {
            Some(b) => b.status(status),
            None => IssuerPatch::builder().status(status),
        });
    }
    if let Some(cnpj) = req.cnpj.as_deref() {
        let cnpj = Cnpj::parse(cnpj)?;
        builder = Some(match builder {
            Some(b) => b.cnpj(cnpj),
            None => IssuerPatch::builder().cnpj(cnpj),
        });
    }
    if let Some(lei) = req.lei.as_deref() {
        let lei = Lei::parse(lei)?;
        builder = Some(match builder {
            Some(b) => b.lei(lei),
            None => IssuerPatch::builder().lei(lei),
        });
    }
    if let Some(cc) = req.country_code.as_deref() {
        let cc = CountryCode::parse(cc)?;
        builder = Some(match builder {
            Some(b) => b.country_code(cc),
            None => IssuerPatch::builder().country_code(cc),
        });
    }

    let patch = builder.ok_or(IssuerMappingError::EmptyPatch)?.build();
    Ok(PatchCommand {
        id,
        expected_version: req.expected_version,
        patch,
        dry_run: req.dry_run,
    })
}

pub fn write_outcome_from_proto(
    msg: &v1::WriteOutcomeProto,
) -> Result<WriteOutcome, IssuerMappingError> {
    match msg.outcome.as_ref() {
        Some(Outcome::Applied(_)) => Ok(WriteOutcome::Applied),
        Some(Outcome::VersionMismatch(vm)) => Ok(WriteOutcome::VersionMismatch {
            expected: vm.expected,
            actual: vm.actual,
        }),
        Some(Outcome::Missing(_)) => Ok(WriteOutcome::Missing),
        None => Err(IssuerMappingError::MissingField("outcome")),
    }
}

pub fn write_outcome_to_proto(outcome: WriteOutcome) -> v1::WriteOutcomeProto {
    let outcome = match outcome {
        WriteOutcome::Applied => Outcome::Applied(Applied {}),
        WriteOutcome::VersionMismatch { expected, actual } => {
            Outcome::VersionMismatch(VersionMismatch { expected, actual })
        }
        WriteOutcome::Missing => Outcome::Missing(Missing {}),
    };
    v1::WriteOutcomeProto {
        outcome: Some(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_issuer(
        name: Option<&str>,
        status: IssuerStatus,
        cnpj: Option<&str>,
        lei: Option<&str>,
        country: Option<&str>,
    ) -> Versioned<Issuer> {
        let mut builder = Issuer::builder().status(status);
        if let Some(name) = name {
            builder = builder.name(IssuerName::new(name).unwrap());
        }
        if let Some(cnpj) = cnpj {
            builder = builder.cnpj(Cnpj::parse(cnpj).unwrap());
        }
        if let Some(lei) = lei {
            builder = builder.lei(Lei::parse(lei).unwrap());
        }
        if let Some(country) = country {
            builder = builder.country_code(CountryCode::parse(country).unwrap());
        }
        Versioned {
            data: builder.build().unwrap(),
            version: 3,
        }
    }

    fn assert_issuers_match(a: &Versioned<Issuer>, b: &Versioned<Issuer>) {
        assert_eq!(a.version, b.version);
        assert_eq!(a.data.id(), b.data.id());
        assert_eq!(a.data.status(), b.data.status());
        assert_eq!(a.data.created_at(), b.data.created_at());
        assert_eq!(
            a.data.name().map(|n| n.as_str()),
            b.data.name().map(|n| n.as_str())
        );
        assert_eq!(
            a.data.cnpj().map(|c| c.as_str().to_string()),
            b.data.cnpj().map(|c| c.as_str().to_string())
        );
        assert_eq!(
            a.data.lei().map(|l| l.as_str().to_string()),
            b.data.lei().map(|l| l.as_str().to_string())
        );
        assert_eq!(
            a.data.country_code().map(|c| c.as_str()),
            b.data.country_code().map(|c| c.as_str())
        );
    }

    #[test]
    fn round_trip_is_lossless_for_representative_issuers() {
        let cases = vec![
            build_issuer(
                Some("Vale S.A."),
                IssuerStatus::Active,
                Some("33.592.510/0001-54"),
                Some("549300BQO2QG6F9A2A21"),
                Some("BR"),
            ),
            build_issuer(
                Some("Plain Name Corp"),
                IssuerStatus::Retired,
                None,
                None,
                None,
            ),
            build_issuer(
                None,
                IssuerStatus::Active,
                None,
                Some("5493000IBP32UQZ0KL24"),
                Some("US"),
            ),
            build_issuer(
                None,
                IssuerStatus::Active,
                Some("12.345.678/0001-95"),
                None,
                None,
            ),
        ];

        for original in cases {
            let wire = issuer_to_proto(&original);
            let back = issuer_from_proto(&wire).expect("round trip must parse");
            assert_issuers_match(&original, &back);
        }
    }

    #[test]
    fn timestamps_survive_the_round_trip_including_legacy_offsets() {
        let versioned = build_issuer(Some("Timely"), IssuerStatus::Active, None, None, None);
        let wire = issuer_to_proto(&versioned);
        let back = issuer_from_proto(&wire).unwrap();
        assert_eq!(versioned.data.created_at(), back.data.created_at());

        // The storage layer historically parsed offset formats too; the wire
        // must accept them.
        let mut offset_wire = wire.clone();
        offset_wire.created_at = "2026-07-30T09:00:00.123-03:00".to_string();
        let parsed = issuer_from_proto(&offset_wire).unwrap();
        assert_eq!(
            parsed.data.created_at().to_rfc3339(),
            "2026-07-30T12:00:00.123+00:00"
        );
    }

    #[test]
    fn overlong_name_is_a_typed_error() {
        let wire = v1::IssuerProto {
            id: Uuid::now_v7().to_string(),
            status: "ACTIVE".to_string(),
            created_at: Utc::now().to_rfc3339(),
            version: 1,
            name: Some("A".repeat(201)),
            cnpj: None,
            lei: None,
            country_code: None,
        };
        assert!(matches!(
            issuer_from_proto(&wire),
            Err(IssuerMappingError::Name(IssuerNameError::TooLong { .. }))
        ));
    }

    #[test]
    fn malformed_cnpj_is_a_typed_error() {
        let req = v1::RegisterIssuerRequestProto {
            cnpj: Some("00.000.000/0000-00".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            register_request_to_issuer(&req),
            Err(IssuerMappingError::Cnpj(_))
        ));
    }

    #[test]
    fn cnpj_with_non_br_country_is_rejected_by_the_builder() {
        let req = v1::RegisterIssuerRequestProto {
            cnpj: Some("12.345.678/0001-95".to_string()),
            country_code: Some("US".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            register_request_to_issuer(&req),
            Err(IssuerMappingError::Builder(
                IssuerBuilderError::InvalidCountryForCnpj(_)
            ))
        ));
    }

    #[test]
    fn invalid_status_is_a_typed_error() {
        let req = v1::RegisterIssuerRequestProto {
            status: Some("DORMANT".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            register_request_to_issuer(&req),
            Err(IssuerMappingError::Status(_))
        ));
    }

    #[test]
    fn empty_patch_is_rejected() {
        let req = v1::PatchIssuerRequestProto {
            id: Uuid::now_v7().to_string(),
            expected_version: 1,
            ..Default::default()
        };
        assert!(matches!(
            patch_request_to_domain(&req),
            Err(IssuerMappingError::EmptyPatch)
        ));
    }

    #[test]
    fn single_field_patch_builds() {
        let req = v1::PatchIssuerRequestProto {
            id: Uuid::now_v7().to_string(),
            expected_version: 4,
            name: Some("Renamed Corp".to_string()),
            dry_run: true,
            ..Default::default()
        };
        let cmd = patch_request_to_domain(&req).unwrap();
        assert_eq!(cmd.expected_version, 4);
        assert!(cmd.dry_run);
        assert_eq!(cmd.patch.name().map(|n| n.as_str()), Some("Renamed Corp"));
        assert!(cmd.patch.status().is_none());
    }

    #[test]
    fn write_outcome_round_trips_every_variant() {
        for outcome in [
            WriteOutcome::Applied,
            WriteOutcome::VersionMismatch {
                expected: 2,
                actual: 5,
            },
            WriteOutcome::Missing,
        ] {
            let wire = write_outcome_to_proto(outcome);
            let back = write_outcome_from_proto(&wire).unwrap();
            assert_eq!(outcome, back);
        }
    }

    #[test]
    fn missing_outcome_is_a_typed_error() {
        let wire = v1::WriteOutcomeProto { outcome: None };
        assert!(matches!(
            write_outcome_from_proto(&wire),
            Err(IssuerMappingError::MissingField("outcome"))
        ));
    }

    #[test]
    fn invalid_id_is_a_typed_error() {
        assert!(matches!(
            parse_issuer_id("not-a-uuid"),
            Err(IssuerMappingError::InvalidId(_))
        ));
    }
}
