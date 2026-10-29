//! The dataset and the engine have to agree.
//!
//! The dataset says which values each family of the specification accepts
//! and refuses.  The engine is what actually matches.  If a family does not
//! accept the values the dataset attributes to it, the intent model is
//! pointing at a specification that does not work - and the only place that
//! shows up is here.
//!
//! The dataset is written in the serde form of the specification, so this
//! test reads it back with the types themselves: there is no second, hand
//! written parser that could drift away from the real one.

use regie_core::spec::CandidateSpec;
use regie_core::Regex;

/// One record of the dataset.
struct Record {
    request: String,
    spec: CandidateSpec,
    positives: Vec<String>,
    negatives: Vec<String>,
}

/// Every record of the dataset agrees with the engine.
#[test]
fn every_record_of_the_dataset_agrees_with_the_engine() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../datasets/intent_train.jsonl"
    );
    let raw = std::fs::read_to_string(path).expect("the dataset is readable");

    let mut records = Vec::new();
    for (number, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        records.push(parse_record(number + 1, line));
    }
    assert_eq!(records.len(), 744, "the dataset holds 744 records");
    assert!(!records.is_empty(), "the dataset is not empty");

    let mut failures: Vec<String> = Vec::new();
    for record in &records {
        let regex = match Regex::from_spec(&record.spec) {
            Ok(regex) => regex,
            Err(error) => {
                failures.push(format!(
                    "{}: the specification of {:?} does not compile: {error}",
                    record.request,
                    record.spec.kind_name()
                ));
                continue;
            }
        };

        for positive in &record.positives {
            if !regex.is_match(positive.as_bytes()) {
                failures.push(format!(
                    "{}: {:?} should accept {positive:?}",
                    record.request,
                    record.spec.kind_name()
                ));
            }
        }
        for negative in &record.negatives {
            if regex.is_match(negative.as_bytes()) {
                failures.push(format!(
                    "{}: {:?} should refuse {negative:?}",
                    record.request,
                    record.spec.kind_name()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} records disagree with the engine:\n{}",
        failures.len(),
        records.len(),
        failures.join("\n")
    );
}

/// Reads one record of the dataset.
fn parse_record(number: usize, line: &str) -> Record {
    let value: serde_json::Value =
        serde_json::from_str(line).unwrap_or_else(|error| panic!("line {number}: {error}"));

    let request = value["request"]
        .as_str()
        .unwrap_or_else(|| panic!("line {number}: no request"))
        .to_owned();

    let spec: CandidateSpec = serde_json::from_value(value["spec"].clone())
        .unwrap_or_else(|error| panic!("line {number}: the specification is malformed: {error}"));

    let read_list = |key: &str| -> Vec<String> {
        value[key]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };

    Record {
        request,
        spec,
        positives: read_list("positives"),
        negatives: read_list("negatives"),
    }
}
