//! Coverage gate: makes unparsed attribute value spaces loud instead of silent.
//!
//! The regen splits the free-text sink into two kinds: `free_text` (genuinely
//! unconstrained — event handlers, `data-*`, ARIA string values) and
//! `unresolved` (a value space we expect to model but have not derived yet).
//! Every `unresolved` attribute is a *coverage gap*, and the live gap set must
//! exactly equal the checked-in baseline (`tests/coverage_baseline.txt`).
//!
//! * A **new** gap (an attribute that regresses to `unresolved`, e.g. an
//!   upstream layout we stopped parsing) is not in the baseline → fails loudly.
//! * **Resolving** a gap removes it from the live set → the test fails until its
//!   baseline line is deleted, so progress is recorded, not silently absorbed.
//!
//! The goal is an empty baseline. Until then it is the authoritative to-do list.

use std::{collections::BTreeSet, fs, path::PathBuf};

use serde_json::Value;

fn catalog_core_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../svg-data/data/catalog.core.json")
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/coverage_baseline.txt")
}

fn live_gaps() -> BTreeSet<String> {
    let catalog: Value = serde_json::from_str(
        &fs::read_to_string(catalog_core_path()).expect("read catalog.core.json"),
    )
    .expect("parse catalog.core.json");
    catalog["attributes"]
        .as_array()
        .expect("attributes array")
        .iter()
        .filter(|attribute| attribute["values"]["kind"] == "unresolved")
        .filter_map(|attribute| attribute["name"].as_str())
        .map(str::to_owned)
        .collect()
}

fn baseline_gaps() -> BTreeSet<String> {
    fs::read_to_string(baseline_path())
        .expect("read coverage_baseline.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn coverage_gaps_match_baseline() {
    let live = live_gaps();
    let baseline = baseline_gaps();

    let regressed: Vec<_> = live.difference(&baseline).cloned().collect();
    let resolved: Vec<_> = baseline.difference(&live).cloned().collect();

    assert!(
        regressed.is_empty(),
        "new coverage gaps not in the baseline (an attribute regressed to free_text). Investigate \
         the extractor before adding these to tests/coverage_baseline.txt:\n  {}",
        regressed.join("\n  "),
    );
    assert!(
        resolved.is_empty(),
        "coverage gaps resolved — delete these lines from tests/coverage_baseline.txt to record \
         the progress:\n  {}",
        resolved.join("\n  "),
    );
}
