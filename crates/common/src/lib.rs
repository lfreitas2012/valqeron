use serde_json::{Map, Value};

pub fn collect_problem_detail_cause(err: &dyn std::error::Error) -> Vec<String> {
    let mut causes = Vec::new();
    let mut current = err.source();
    while let Some(source) = current {
        causes.push(source.to_string());
        current = source.source();
    }
    causes
}

pub fn extensions_json(ext: &Map<String, Value>) -> String {
    serde_json::to_string(ext).unwrap_or_else(|_| String::from("{}"))
}
