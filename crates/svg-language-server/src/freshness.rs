//! Best-effort runtime spec-freshness check.
//!
//! Mirrors the [`compat`](crate::compat) runtime fetch: a synchronous,
//! `spawn_blocking`-friendly network probe that compares the crate's **baked**
//! spec catalog against what W3C and `svgwg` publish right now. The decision
//! logic is the pure, offline-tested half in [`svg_data::edition`]; this module
//! only adds the network shell and a user-facing message.
//!
//! It is opt-in (the `svg.spec_freshness_check` initialization option) because it
//! contacts `api.w3.org` and `api.github.com`, and degrades silently when
//! offline — a failed probe is never reported as "fresh" *or* surfaced to the
//! user; only a confirmed staleness shows a message.

use std::time::Duration;

use svg_data::edition::{
    CapturedEditionIdentity, Freshness, ROLLING_PIN, Series, VersionsEnvelope, classify_freshness,
    unseen_versions,
};

/// Outcome of a runtime freshness probe.
pub struct SpecFreshness {
    /// `svgwg` `master` HEAD has advanced past the baked rolling pin.
    rolling_stale: bool,
    /// Dated `/TR/` URIs W3C has published that the baked index does not carry.
    unseen_uris: Vec<String>,
}

impl SpecFreshness {
    /// Whether anything upstream has moved past the baked catalog.
    pub const fn is_stale(&self) -> bool {
        self.rolling_stale || !self.unseen_uris.is_empty()
    }

    /// A single-line, user-facing staleness summary with the remediation path.
    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        if self.rolling_stale {
            parts.push(format!(
                "the svgwg editor's draft advanced past the baked pin ({})",
                short_commit(ROLLING_PIN.commit)
            ));
        }
        if !self.unseen_uris.is_empty() {
            parts.push(format!(
                "{} new W3C publication(s): {}",
                self.unseen_uris.len(),
                self.unseen_uris.join(", ")
            ));
        }
        format!(
            "SVG spec data may be stale — {}. Refresh with `just refresh-editions` / `just refresh-svgwg <commit>`.",
            parts.join("; ")
        )
    }
}

fn short_commit(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

const GITHUB_HEAD_URL: &str = "https://api.github.com/repos/w3c/svgwg/commits/master";

fn w3c_versions_url(series: Series) -> String {
    format!(
        "https://api.w3.org/specifications/{}/versions?embed=1&items=100",
        series.shortname()
    )
}

/// Probe W3C + `svgwg` and classify the baked catalog's freshness.
///
/// Synchronous (intended for `spawn_blocking`). Returns `None` only when *every*
/// probe failed (offline); a reachable-but-up-to-date catalog returns
/// `Some(report)` whose [`SpecFreshness::is_stale`] is `false`.
pub fn fetch_spec_freshness() -> Option<SpecFreshness> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build(),
    );

    let head = fetch_text(&agent, GITHUB_HEAD_URL).and_then(|json| parse_head_sha(&json));
    let rolling_stale = head.as_deref().is_some_and(|head| {
        let captured = CapturedEditionIdentity::Rolling {
            commit: ROLLING_PIN.commit,
        };
        matches!(
            classify_freshness(&captured, Some(head)),
            Freshness::RollingStale { .. }
        )
    });

    let mut any_published_probe = false;
    let mut unseen_uris = Vec::new();
    for series in Series::ALL {
        let Some(json) = fetch_text(&agent, &w3c_versions_url(series)) else {
            continue;
        };
        any_published_probe = true;
        match VersionsEnvelope::parse(series, &json) {
            Ok(live) => {
                for version in unseen_versions(series, &live) {
                    unseen_uris.push(version.uri.to_string());
                }
            }
            Err(error) => {
                tracing::warn!(series = series.shortname(), %error, "parse W3C versions failed");
            }
        }
    }

    if head.is_none() && !any_published_probe {
        // Every probe failed — treat as offline, not as a freshness verdict.
        return None;
    }
    Some(SpecFreshness {
        rolling_stale,
        unseen_uris,
    })
}

/// GET `url` as text, sending the `User-Agent` GitHub requires and an optional
/// `GITHUB_TOKEN` bearer for the authenticated rate limit. Errors are logged and
/// folded to `None` so a single failed endpoint cannot abort the whole probe.
fn fetch_text(agent: &ureq::Agent, url: &str) -> Option<String> {
    let mut request = agent
        .get(url)
        .header("User-Agent", "svg-language-server-spec-freshness");
    if url.starts_with("https://api.github.com/")
        && let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    request
        .call()
        .map_err(|err| tracing::warn!(url, error = %err, "freshness HTTP request failed"))
        .ok()?
        .body_mut()
        .read_to_string()
        .map_err(|err| tracing::warn!(url, error = %err, "freshness response body read failed"))
        .ok()
}

/// Extract the `sha` field from a GitHub commit-object JSON response.
fn parse_head_sha(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .map_err(|err| tracing::warn!(error = %err, "parse svgwg HEAD failed"))
        .ok()?
        .get("sha")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_report_is_not_stale_and_has_no_message_trigger() {
        let report = SpecFreshness {
            rolling_stale: false,
            unseen_uris: Vec::new(),
        };
        assert!(!report.is_stale());
    }

    #[test]
    fn rolling_drift_alone_is_stale() {
        let report = SpecFreshness {
            rolling_stale: true,
            unseen_uris: Vec::new(),
        };
        assert!(report.is_stale());
        let message = report.message();
        assert!(message.contains("editor's draft"));
        assert!(message.contains(short_commit(ROLLING_PIN.commit)));
        assert!(message.contains("just refresh"));
    }

    #[test]
    fn published_drift_alone_is_stale_and_lists_uris() {
        let report = SpecFreshness {
            rolling_stale: false,
            unseen_uris: vec!["https://www.w3.org/TR/2099/CR-SVG2-20990101/".to_string()],
        };
        assert!(report.is_stale());
        let message = report.message();
        assert!(message.contains("1 new W3C publication"));
        assert!(message.contains("CR-SVG2-20990101"));
    }

    #[test]
    fn parse_head_sha_reads_github_commit_object() {
        let json = r#"{"sha":"899b4bbcbd43925800a915aad6a90b643c7e9bad","commit":{}}"#;
        assert_eq!(
            parse_head_sha(json).as_deref(),
            Some("899b4bbcbd43925800a915aad6a90b643c7e9bad")
        );
        assert_eq!(parse_head_sha("{}"), None);
        assert_eq!(parse_head_sha("not json"), None);
    }

    #[test]
    fn short_commit_truncates_to_eight_hex() {
        assert_eq!(
            short_commit("19482daf4094e72becde92b38c6a1c0d384b56a9"),
            "19482daf"
        );
        assert_eq!(short_commit("abc"), "abc");
    }
}
