//! Verifies the baked W3C edition index, status mapping, freshness primitives,
//! and the `SpecSnapshotId` -> index entry mapping.

use svg_data::edition::{
    self, CapturedEditionIdentity, Freshness, Series, Status, VersionsEnvelope,
};
use svg_data::types::SpecSnapshotId;

#[test]
fn svg2_has_the_three_candidate_recommendations() {
    let svg2 = edition::published_versions(Series::Svg2);

    let crs: Vec<(&str, &str)> = svg2
        .iter()
        .filter(|v| v.status == Status::CandidateRecommendation)
        .map(|v| (v.date.as_ref(), v.uri.as_ref()))
        .collect();

    assert!(
        crs.contains(&("2018-10-04", "https://www.w3.org/TR/2018/CR-SVG2-20181004/")),
        "missing 2018-10-04 CR, got {crs:?}",
    );
    assert!(
        crs.contains(&("2018-08-07", "https://www.w3.org/TR/2018/CR-SVG2-20180807/")),
        "missing 2018-08-07 CR, got {crs:?}",
    );
    assert!(
        crs.contains(&("2016-09-15", "https://www.w3.org/TR/2016/CR-SVG2-20160915/")),
        "missing 2016-09-15 CR, got {crs:?}",
    );
    assert_eq!(crs.len(), 3, "expected exactly 3 SVG2 CRs, got {crs:?}");

    // SVG2 has a rolling editor's draft on every version record.
    for version in &svg2 {
        assert_eq!(
            version.editor_draft.as_deref(),
            Some("https://w3c.github.io/svgwg/svg2-draft/"),
            "SVG2 {} should carry the editor-draft URL",
            version.uri,
        );
    }
}

#[test]
fn svg11_has_both_recommendations() {
    let svg11 = edition::published_versions(Series::Svg11);
    let recs: Vec<(&str, &str)> = svg11
        .iter()
        .filter(|v| v.status == Status::Recommendation)
        .map(|v| (v.date.as_ref(), v.uri.as_ref()))
        .collect();

    assert!(
        recs.contains(&(
            "2003-01-14",
            "https://www.w3.org/TR/2003/REC-SVG11-20030114/"
        )),
        "missing SVG 1.1 first-edition REC, got {recs:?}",
    );
    assert!(
        recs.contains(&(
            "2011-08-16",
            "https://www.w3.org/TR/2011/REC-SVG11-20110816/"
        )),
        "missing SVG 1.1 second-edition REC, got {recs:?}",
    );
    assert_eq!(recs.len(), 2, "expected exactly 2 SVG11 RECs, got {recs:?}");

    // SVG 1.1 history is the only one carrying Last Call Working Drafts.
    assert!(
        svg11
            .iter()
            .any(|v| v.status == Status::LastCallWorkingDraft),
        "SVG11 history should include a Last Call Working Draft",
    );
}

#[test]
fn svg10_has_the_2001_recommendation() {
    let svg10 = edition::published_versions(Series::Svg10);
    let rec = svg10
        .iter()
        .find(|v| v.status == Status::Recommendation)
        .unwrap_or_else(|| panic!("SVG 1.0 must have a Recommendation"));
    assert_eq!(rec.date, "2001-09-04");
    assert_eq!(rec.uri, "https://www.w3.org/TR/2001/REC-SVG-20010904/");
    assert!(rec.rec_track);
}

#[test]
fn status_strings_map_to_the_enum() {
    // Round-trip every distinct API status string through serde to prove the
    // rename mapping is exact (no stringly-typed status survives parsing).
    let cases = [
        ("Working Draft", Status::WorkingDraft),
        ("Last Call Working Draft", Status::LastCallWorkingDraft),
        (
            "Candidate Recommendation Snapshot",
            Status::CandidateRecommendation,
        ),
        ("Proposed Recommendation", Status::ProposedRecommendation),
        ("Recommendation", Status::Recommendation),
    ];
    for (raw, expected) in cases {
        let parsed: Status = serde_json::from_str(&format!("\"{raw}\""))
            .unwrap_or_else(|e| panic!("status {raw:?} should deserialize: {e}"));
        assert_eq!(parsed, expected, "status {raw:?}");
    }
}

#[test]
fn envelope_parse_stamps_series_and_extracts_all_versions() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/data/sources/w3c-api/svg2.versions.json"
    ))
    .unwrap_or_else(|e| panic!("vendored svg2 versions file must be readable: {e}"));
    let parsed = VersionsEnvelope::parse(Series::Svg2, &json)
        .unwrap_or_else(|e| panic!("svg2 versions JSON must parse: {e}"));

    assert_eq!(parsed.len(), 10, "SVG2 history has 10 versions");
    assert!(
        parsed.iter().all(|v| v.series == Series::Svg2),
        "parse must stamp every record with its series",
    );
    // Newest-first ordering preserved from the API file.
    assert_eq!(parsed[0].date, "2018-10-04");
}

#[test]
fn latest_published_returns_newest_per_series() {
    let svg2 =
        edition::latest_published(Series::Svg2).unwrap_or_else(|| panic!("SVG2 has a latest"));
    assert_eq!(svg2.date, "2018-10-04");
    assert_eq!(svg2.status, Status::CandidateRecommendation);
    assert_eq!(svg2.uri, "https://www.w3.org/TR/2018/CR-SVG2-20181004/");

    let svg11 =
        edition::latest_published(Series::Svg11).unwrap_or_else(|| panic!("SVG11 has a latest"));
    assert_eq!(svg11.date, "2011-08-16");
    assert_eq!(svg11.status, Status::Recommendation);

    let svg10 =
        edition::latest_published(Series::Svg10).unwrap_or_else(|| panic!("SVG10 has a latest"));
    assert_eq!(svg10.date, "2001-09-04");
    assert_eq!(svg10.status, Status::Recommendation);
}

#[test]
fn snapshot_ids_map_to_their_index_entries() {
    let cases = [
        (
            SpecSnapshotId::Svg11Rec20030114,
            "https://www.w3.org/TR/2003/REC-SVG11-20030114/",
            Status::Recommendation,
        ),
        (
            SpecSnapshotId::Svg11Rec20110816,
            "https://www.w3.org/TR/2011/REC-SVG11-20110816/",
            Status::Recommendation,
        ),
        (
            SpecSnapshotId::Svg2Cr20181004,
            "https://www.w3.org/TR/2018/CR-SVG2-20181004/",
            Status::CandidateRecommendation,
        ),
    ];
    for (snapshot, uri, status) in cases {
        let entry = edition::index_entry_for_snapshot(snapshot)
            .unwrap_or_else(|| panic!("{snapshot:?} should map to an index entry"));
        assert_eq!(entry.uri, uri, "{snapshot:?}");
        assert_eq!(entry.status, status, "{snapshot:?}");
    }

    // The rolling editor's draft has no /TR/ index entry.
    assert!(
        edition::index_entry_for_snapshot(SpecSnapshotId::Svg2EditorsDraft).is_none(),
        "editor's draft must not resolve to a /TR/ index entry",
    );
}

#[test]
fn freshness_classifies_dated_and_rolling_editions() {
    // A dated /TR/ capture in the index => Final.
    let dated = CapturedEditionIdentity::Dated {
        uri: "https://www.w3.org/TR/2018/CR-SVG2-20181004/",
    };
    match edition::classify_freshness(&dated, None) {
        Freshness::Final { uri } => {
            assert_eq!(uri, "https://www.w3.org/TR/2018/CR-SVG2-20181004/");
        }
        other => panic!("dated capture should be Final, got {other:?}"),
    }

    // Rolling, no reference known => RollingCurrent.
    let rolling = CapturedEditionIdentity::Rolling {
        commit: "19482daf4094e72becde92b38c6a1c0d384b56a9",
    };
    assert_eq!(
        edition::classify_freshness(&rolling, None),
        Freshness::RollingCurrent,
    );

    // Rolling, reference matches capture => RollingCurrent.
    assert_eq!(
        edition::classify_freshness(&rolling, Some("19482daf4094e72becde92b38c6a1c0d384b56a9"),),
        Freshness::RollingCurrent,
    );

    // Rolling, newer reference upstream => RollingStale.
    match edition::classify_freshness(&rolling, Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")) {
        Freshness::RollingStale { latest } => {
            assert_eq!(latest, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
        }
        other => panic!("stale rolling capture expected, got {other:?}"),
    }
}
