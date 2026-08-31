#![no_main]

use libfuzzer_sys::fuzz_target;

use arbitrary::Arbitrary;
use valqeron_core::identifiers::Cnpj;
use valqeron_core::identifiers::arbitrary;

#[derive(Arbitrary, Debug)]
enum CnpjOp {
    Parse(String),
    FromBytes([u8; 14]),
    Format,
    CheckRoot,
    CheckBranch,
    CompareWithStr(String),
}

fuzz_target!(|ops: Vec<CnpjOp>| {
    let mut current_cnpj: Option<Cnpj> = None;

    for op in ops {
        match op {
            CnpjOp::Parse(s) => {
                if let Ok(cnpj) = Cnpj::parse(&s) {
                    current_cnpj = Some(cnpj);
                }
            }
            CnpjOp::FromBytes(bytes) => {
                if let Ok(cnpj) = Cnpj::from_bytes(bytes) {
                    current_cnpj = Some(cnpj);
                }
            }
            CnpjOp::Format => {
                if let Some(cnpj) = &current_cnpj {
                    let formatted = cnpj.formatted();
                    assert_eq!(formatted.as_str().len(), 18);
                    assert_eq!(formatted.to_string(), formatted.as_str());
                }
            }
            CnpjOp::CheckRoot => {
                if let Some(cnpj) = &current_cnpj {
                    let root = cnpj.root();
                    assert_eq!(root.len(), 8);
                    assert_eq!(root, &cnpj.as_str()[..8]);
                }
            }
            CnpjOp::CheckBranch => {
                if let Some(cnpj) = &current_cnpj {
                    let branch = cnpj.branch_code();
                    assert_eq!(branch.len(), 4);
                    assert_eq!(cnpj.is_root(), branch == "0001");

                    if let Some(num) = cnpj.branch_number() {
                        // Numeric branches should re-format correctly
                        assert_eq!(format!("{num:04}"), branch);
                    }
                }
            }
            CnpjOp::CompareWithStr(s) => {
                if let Some(cnpj) = &current_cnpj {
                    let eq_str = cnpj == s.as_str();
                    let eq_compact = cnpj.as_str() == s.as_str();
                    assert_eq!(eq_str, eq_compact);
                }
            }
        }
    }
});
