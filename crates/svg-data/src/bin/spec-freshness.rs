//! Live spec-freshness sentinel.
//!
//! Compares the crate's **baked** spec catalog against what W3C and `svgwg`
//! publish *right now*, and reports any drift:
//!
//! * **Published editions** — for each SVG series, fetch the W3C specification
//!   API `version-history` and flag any dated `/TR/` version the baked
//!   [`EDITION_INDEX`](svg_data::edition::EDITION_INDEX) has not vendored yet
//!   (via the pure [`unseen_versions`](svg_data::edition::unseen_versions)).
//! * **Rolling editor's draft** — fetch `svgwg` git `master` HEAD and compare it
//!   against the baked [`ROLLING_PIN`](svg_data::edition::ROLLING_PIN) commit
//!   (via the pure [`classify_freshness`](svg_data::edition::classify_freshness)).
//!
//! The decision logic lives in `svg_data::edition` and is unit-tested offline;
//! this binary is only the network shell + reporting around it.
//!
//! Exit codes: `0` = up to date, `1` = drift detected (a refresh is due), `2` =
//! an operational error (network/parse) prevented the check. CI keys an issue
//! off exit `1`; exit `2` should fail the job loudly instead.
//!
//! Usage: `spec-freshness [--json]`. With `--json` the report is emitted as a
//! single JSON object on stdout (for use as an issue body); otherwise a
//! human-readable summary is printed.

use std::process::ExitCode;

use serde::Serialize;
use svg_data::edition::{
    CapturedEditionIdentity, Freshness, PublishedVersion, ROLLING_PIN, Series, VersionsEnvelope,
    classify_freshness, unseen_versions,
};

/// A single newly-published edition the baked catalog has not caught up to.
#[derive(Debug, Serialize)]
struct PublishedDrift {
    series: Series,
    date: String,
    status: String,
    uri: String,
}

/// Rolling editor's-draft comparison result.
#[derive(Debug, Serialize)]
struct RollingReport {
    repository: String,
    pinned_commit: String,
    head_commit: String,
    /// `"current"` when HEAD matches the pin, `"stale"` when it has advanced.
    state: &'static str,
}

/// The full freshness verdict, serialised to stdout under `--json`.
#[derive(Debug, Serialize)]
struct FreshnessReport {
    fresh: bool,
    published_drift: Vec<PublishedDrift>,
    rolling: RollingReport,
}

/// W3C specification API endpoint for a series' full version history.
fn w3c_versions_url(series: Series) -> String {
    format!(
        "https://api.w3.org/specifications/{}/versions?embed=1&items=100",
        series.shortname()
    )
}

/// GET `url`, returning the response body as a string.
///
/// Sends a `User-Agent` (GitHub rejects requests without one) and an optional
/// bearer token from `GITHUB_TOKEN` so CI runs use the authenticated rate limit.
fn fetch(url: &str) -> Result<String, String> {
    let mut request = ureq::get(url).header("User-Agent", "svg-language-server-spec-freshness");
    if url.contains("api.github.com")
        && let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = request.call().map_err(|e| format!("fetch {url}: {e}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body {url}: {e}"))
}

/// Collect every published edition newer than the baked index, across all series.
fn check_published() -> Result<Vec<PublishedDrift>, String> {
    let mut drift = Vec::new();
    for series in Series::ALL {
        let json = fetch(&w3c_versions_url(series))?;
        let live = VersionsEnvelope::parse(series, &json)
            .map_err(|e| format!("parse {} versions: {e}", series.shortname()))?;
        for version in unseen_versions(series, &live) {
            drift.push(published_drift(series, &version));
        }
    }
    Ok(drift)
}

fn published_drift(series: Series, version: &PublishedVersion) -> PublishedDrift {
    PublishedDrift {
        series,
        date: version.date.to_string(),
        status: format!("{:?}", version.status),
        uri: version.uri.to_string(),
    }
}

/// Compare the baked rolling pin against live `svgwg` `master` HEAD.
fn check_rolling() -> Result<RollingReport, String> {
    let json = fetch("https://api.github.com/repos/w3c/svgwg/commits/master")?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("parse svgwg HEAD: {e}"))?;
    let head = value
        .get("sha")
        .and_then(serde_json::Value::as_str)
        .ok_or("svgwg HEAD response had no `sha` field")?;

    let captured = CapturedEditionIdentity::Rolling {
        commit: ROLLING_PIN.commit,
    };
    let state = match classify_freshness(&captured, Some(head)) {
        Freshness::RollingCurrent | Freshness::Final { .. } => "current",
        Freshness::RollingStale { .. } => "stale",
    };
    Ok(RollingReport {
        repository: ROLLING_PIN.repository.to_string(),
        pinned_commit: ROLLING_PIN.commit.to_string(),
        head_commit: head.to_string(),
        state,
    })
}

/// Run both checks and assemble the verdict.
fn run() -> Result<FreshnessReport, String> {
    let published_drift = check_published()?;
    let rolling = check_rolling()?;
    let fresh = published_drift.is_empty() && rolling.state == "current";
    Ok(FreshnessReport {
        fresh,
        published_drift,
        rolling,
    })
}

/// Render a human-readable summary to stdout.
fn print_human(report: &FreshnessReport) {
    if report.fresh {
        println!("✅ spec data is up to date");
    } else {
        println!("⚠️  spec data is STALE — a refresh is due");
    }

    println!("\nPublished editions (W3C API vs baked EDITION_INDEX):");
    if report.published_drift.is_empty() {
        println!("  · no new /TR/ publications");
    } else {
        for drift in &report.published_drift {
            println!(
                "  · NEW {:?} {} [{}] {}",
                drift.series, drift.date, drift.status, drift.uri
            );
        }
    }

    println!("\nRolling editor's draft (svgwg master HEAD vs baked pin):");
    println!("  · pinned: {}", report.rolling.pinned_commit);
    println!("  · head:   {}", report.rolling.head_commit);
    println!(
        "  · {}",
        if report.rolling.state == "stale" {
            "STALE — svgwg master has advanced past the pin"
        } else {
            "current"
        }
    );
}

fn main() -> ExitCode {
    let json_mode = std::env::args().any(|arg| arg == "--json");

    let report = match run() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("spec-freshness: {error}");
            return ExitCode::from(2);
        }
    };

    if json_mode {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("spec-freshness: serialise report: {error}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(&report);
    }

    if report.fresh {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
