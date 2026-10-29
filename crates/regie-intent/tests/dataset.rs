//! Checks the model against the training dataset.
//!
//! The dataset is the contract between the offline trainer and the embedded
//! weights: every request in it has to come back as the specification it was
//! labelled with.

use std::collections::BTreeMap;
use std::path::PathBuf;

use regie_core::spec::{
    CandidateSpec, CharsetPolicy, ExtractionKind, IntentSpec, PatternTemplate, ValidationKind,
    EXTRACTION_KINDS, VALIDATION_KINDS,
};
use regie_intent::IntentModel;
use serde_json::Value;

/// The dataset that ships with the repository.
fn dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("datasets")
        .join("intent_train.jsonl")
}

/// One line of the dataset.
struct Record {
    request: String,
    spec: Value,
}

/// Reads the dataset.
fn dataset() -> Vec<Record> {
    let path = dataset_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {}", path.display(), error));
    let mut records = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("line {} is not JSON: {}", number + 1, error));
        records.push(Record {
            request: value["request"]
                .as_str()
                .expect("request is a string")
                .to_string(),
            spec: value["spec"].clone(),
        });
    }
    assert!(
        records.len() >= 150,
        "the dataset must hold at least 150 lines"
    );
    records
}

/// The family of a labelled specification.
fn family(spec: &Value) -> &str {
    let object = spec.as_object().expect("the spec is an object");
    assert_eq!(object.len(), 1, "the spec names exactly one family");
    object.keys().next().expect("the spec names a family")
}

/// The validation kind of a labelled specification.
fn validation_kind(spec: &Value) -> ValidationKind {
    let name = spec["Validate"]["kind"]
        .as_str()
        .expect("a validation kind");
    VALIDATION_KINDS
        .iter()
        .copied()
        .find(|kind| format!("{:?}", kind) == name)
        .unwrap_or_else(|| panic!("unknown validation kind {}", name))
}

/// The extraction kind of a labelled specification.
fn extraction_kind(spec: &Value) -> ExtractionKind {
    let name = spec["Extract"]["kind"]
        .as_str()
        .expect("an extraction kind");
    EXTRACTION_KINDS
        .iter()
        .copied()
        .find(|kind| format!("{:?}", kind) == name)
        .unwrap_or_else(|| panic!("unknown extraction kind {}", name))
}

/// Whether the model produced the labelled family and kind.
fn matches_family(spec: &IntentSpec, labelled: &Value) -> bool {
    match (spec, family(labelled)) {
        (IntentSpec::Confident(CandidateSpec::Validate { kind, .. }), "Validate") => {
            *kind == validation_kind(labelled)
        }
        (IntentSpec::Confident(CandidateSpec::Extract { kind, .. }), "Extract") => {
            *kind == extraction_kind(labelled)
        }
        (IntentSpec::Confident(CandidateSpec::Filter { .. }), "Filter") => true,
        (IntentSpec::Confident(CandidateSpec::Custom { .. }), "Custom") => true,
        _ => false,
    }
}

/// Every validation kind is reachable from the dataset.
#[test]
fn every_validation_kind_is_reachable() {
    let model = IntentModel::embedded();
    let records = dataset();
    let mut reached: BTreeMap<String, usize> = BTreeMap::new();

    for record in &records {
        if family(&record.spec) != "Validate" {
            continue;
        }
        let kind = validation_kind(&record.spec);
        let name = format!("{:?}", kind);
        let spec = model.interpret(&record.request);
        if spec.is_confident() && matches_family(&spec, &record.spec) {
            *reached.entry(name.clone()).or_insert(0) += 1;
        }
    }

    for kind in VALIDATION_KINDS {
        let name = format!("{:?}", kind);
        assert!(
            reached.get(name.as_str()).copied().unwrap_or(0) > 0,
            "the validation kind {} is not reachable",
            name
        );
    }
    assert!(
        reached.len() >= 20,
        "the dataset must cover at least 20 validation kinds, it covers {}",
        reached.len()
    );
}

/// Every extraction kind is reachable from the dataset.
#[test]
fn every_extraction_kind_is_reachable() {
    let model = IntentModel::embedded();
    let records = dataset();
    let mut reached: BTreeMap<String, usize> = BTreeMap::new();

    for record in &records {
        if family(&record.spec) != "Extract" {
            continue;
        }
        let kind = extraction_kind(&record.spec);
        let name = format!("{:?}", kind);
        let spec = model.interpret(&record.request);
        if spec.is_confident() && matches_family(&spec, &record.spec) {
            *reached.entry(name.clone()).or_insert(0) += 1;
        }
    }

    for kind in EXTRACTION_KINDS {
        let name = format!("{:?}", kind);
        assert!(
            reached.get(name.as_str()).copied().unwrap_or(0) > 0,
            "the extraction kind {} is not reachable",
            name
        );
    }
}

/// The filter and the custom intents are reachable from the dataset.
#[test]
fn filter_and_custom_are_reachable() {
    let model = IntentModel::embedded();
    let records = dataset();
    let mut filter = 0;
    let mut custom = 0;

    for record in &records {
        match family(&record.spec) {
            "Filter" => {
                if matches_family(&model.interpret(&record.request), &record.spec) {
                    filter += 1;
                }
            }
            "Custom" if matches_family(&model.interpret(&record.request), &record.spec) => {
                custom += 1;
            }
            _ => {}
        }
    }

    assert!(filter > 0, "the filter intent is not reachable");
    assert!(custom > 0, "the custom intent is not reachable");
}

/// The model reproduces the labelled specification of the dataset.
///
/// The classifier only decides the family and the kind; the other fields
/// come from the rule engine, so the whole specification has to match.
#[test]
fn the_model_reproduces_the_dataset() {
    let model = IntentModel::embedded();
    let records = dataset();
    let mut checked = 0;
    let mut wrong_family = 0;

    for record in &records {
        let spec = model.interpret(&record.request);
        if !spec.is_confident() {
            continue;
        }
        let candidate = match spec {
            IntentSpec::Confident(candidate) => candidate,
            IntentSpec::Ambiguous { .. } => unreachable!(),
        };
        if !matches_family(&spec, &record.spec) {
            wrong_family += 1;
            continue;
        }
        match (&candidate, family(&record.spec)) {
            (
                CandidateSpec::Validate {
                    kind,
                    strictness,
                    charset,
                    max_len,
                },
                "Validate",
            ) => {
                assert_eq!(
                    format!("{:?}", kind),
                    format!("{:?}", validation_kind(&record.spec))
                );
                assert_eq!(
                    format!("{:?}", strictness),
                    record.spec["Validate"]["strictness"].as_str().unwrap()
                );
                assert_eq!(
                    format!("{:?}", charset),
                    record.spec["Validate"]["charset"].as_str().unwrap()
                );
                assert_eq!(
                    max_len.map(|value| value as u64),
                    record.spec["Validate"]["max_len"].as_u64()
                );
            }
            (
                CandidateSpec::Extract {
                    kind,
                    boundary,
                    capture,
                },
                "Extract",
            ) => {
                assert_eq!(
                    format!("{:?}", kind),
                    format!("{:?}", extraction_kind(&record.spec))
                );
                assert_eq!(
                    format!("{:?}", boundary),
                    record.spec["Extract"]["boundary"].as_str().unwrap()
                );
                assert_eq!(
                    format!("{:?}", capture),
                    record.spec["Extract"]["capture"].as_str().unwrap()
                );
            }
            (CandidateSpec::Filter { action, terms }, "Filter") => {
                assert_eq!(
                    format!("{:?}", action),
                    record.spec["Filter"]["action"].as_str().unwrap()
                );
                let labelled = record.spec["Filter"]["terms"]
                    .as_array()
                    .expect("the terms are a list");
                assert_eq!(terms.len(), labelled.len(), "the filter terms differ");
                for (term, label) in terms.iter().zip(labelled.iter()) {
                    // The dataset writes the bytes as the list of their
                    // values, the way `serde` writes a `SmallVec<u8>`.
                    let expected: Vec<u8> = label["bytes"]
                        .as_array()
                        .expect("the bytes are a list")
                        .iter()
                        .filter_map(|value| value.as_u64().map(|byte| byte as u8))
                        .collect();
                    assert_eq!(term.bytes.as_slice(), expected.as_slice());
                }
            }
            (CandidateSpec::Custom { template, params }, "Custom") => {
                assert_eq!(
                    format!("{:?}", template),
                    format!("{:?}", template_of(&record.spec))
                );
                assert_eq!(
                    params.min_len.map(|value| value as u64),
                    record.spec["Custom"]["params"]["min_len"].as_u64()
                );
                assert_eq!(
                    params.max_len.map(|value| value as u64),
                    record.spec["Custom"]["params"]["max_len"].as_u64()
                );
            }
            _ => panic!("the family of the model and of the dataset differ"),
        }
        checked += 1;
    }

    let total = records.len();
    assert!(
        checked * 100 / total >= 90,
        "the model only reproduces {} of {} requests",
        checked,
        total
    );
    assert_eq!(
        wrong_family, 0,
        "the model picked the wrong family for {} requests",
        wrong_family
    );
}

/// Rebuilds the labelled template of a custom intent.
fn template_of(spec: &Value) -> PatternTemplate {
    let template = &spec["Custom"]["template"];
    if let Some(repeat) = template.get("Repeat") {
        return PatternTemplate::Repeat {
            class: charset_of(repeat["class"].as_str().unwrap()),
            count: repeat["count"].as_u64().unwrap() as u16,
        };
    }
    if let Some(literal) = template.get("LiteralThenClass") {
        let mut bytes: regie_core::SmallVec<u8, 12> = regie_core::SmallVec::new();
        // The literal is written as the list of its bytes, the way `serde`
        // writes a `SmallVec<u8>`.
        for value in literal["literal"]
            .as_array()
            .expect("the literal is a list")
        {
            let byte = value.as_u64().expect("a byte is a number") as u8;
            if !bytes.push(byte) {
                break;
            }
        }
        return PatternTemplate::LiteralThenClass {
            literal: bytes,
            class: charset_of(literal["class"].as_str().unwrap()),
            count: literal["count"].as_u64().unwrap() as u16,
        };
    }
    panic!("unknown template shape");
}

/// Rebuilds a labelled charset policy.
fn charset_of(name: &str) -> CharsetPolicy {
    match name {
        "Digits" => CharsetPolicy::Digits,
        "Letters" => CharsetPolicy::Letters,
        "Alphanumeric" => CharsetPolicy::Alphanumeric,
        "Word" => CharsetPolicy::Word,
        "PrintableAscii" => CharsetPolicy::PrintableAscii,
        "Any" => CharsetPolicy::Any,
        other => panic!("unknown charset policy {}", other),
    }
}
