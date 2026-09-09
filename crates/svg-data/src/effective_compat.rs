//! Effective records use owned strings so refreshed data and baked data follow the same path.
use crate::compat_model::{Baseline, BaselineDate, Discouraged};
use crate::{BaselineTier, CompatVerdict, VerdictReason, VerdictRecommendation};

/// BCD flags used to annotate supported features.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleFlags {
    /// BCD deprecation flag.
    pub deprecated: bool,
    /// BCD experimental flag.
    pub experimental: bool,
}

/// Combine specification lifecycle with the selected compatibility record.
///
/// Explicit spec status survives refreshes. A runtime record replaces both
/// bundled flags, including when both are false. Bundled current-web advice
/// does not promote diagnostics or completions for historical profiles.
#[must_use]
pub fn lifecycle(
    profile: crate::SpecSnapshotId,
    specification: crate::SpecLifecycle,
    bundled: LifecycleFlags,
    runtime: Option<LifecycleFlags>,
) -> crate::SpecLifecycle {
    use crate::SpecLifecycle;
    let flags = runtime.unwrap_or_else(|| {
        if profile == crate::SpecSnapshotId::LATEST {
            bundled
        } else {
            LifecycleFlags::default()
        }
    });
    match specification {
        SpecLifecycle::Deprecated | SpecLifecycle::Obsolete => specification,
        _ if flags.deprecated => SpecLifecycle::Deprecated,
        SpecLifecycle::Experimental => specification,
        _ if flags.experimental => SpecLifecycle::Experimental,
        SpecLifecycle::Stable => SpecLifecycle::Stable,
    }
}

/// All compatibility dimensions for one element or attribute context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// BCD deprecation flag.
    pub deprecated: bool,
    /// BCD experimental flag.
    pub experimental: bool,
    /// BCD standards status.
    pub standard_track: Option<bool>,
    /// Web Features Baseline facts.
    pub baseline: Option<Baseline>,
    /// Feature-scoped `WebDX` advice.
    pub discouraged: Vec<Discouraged>,
    /// All BCD browser products and support statements.
    pub browser_support: Option<BrowserSupport>,
}
use crate::browser_compat::{BrowserFlag, BrowserSupport, BrowserVersion};

impl From<crate::CompatFacts> for Facts {
    fn from(f: crate::CompatFacts) -> Self {
        Self {
            deprecated: f.deprecated,
            experimental: f.experimental,
            standard_track: f.standard_track,
            baseline: f.baseline.map(|b| Baseline {
                support: b.support.map(|entries| {
                    entries
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                        .collect()
                }),
                status: b.status,
                raw_status: b.raw_status.map(str::to_owned),
                status_diagnostic: b.status_diagnostic,
                low_date: b.low_date.map(owned_date),
                high_date: b.high_date.map(owned_date),
            }),
            discouraged: f
                .discouraged
                .iter()
                .map(|d| Discouraged {
                    feature_id: d.feature_id.to_owned(),
                    compat_key: d.compat_key.to_owned(),
                    scope: d.scope,
                    feature_name: d.feature_name.map(str::to_owned),
                    reason: d.reason.to_owned(),
                    reason_html: d.reason_html.map(str::to_owned),
                    according_to: d.according_to.iter().map(|s| (*s).to_owned()).collect(),
                    alternatives: d.alternatives.iter().map(|s| (*s).to_owned()).collect(),
                    removal_date: d.removal_date.map(str::to_owned),
                })
                .collect(),
            browser_support: f.browser_support.map(|support| {
                support
                    .iter()
                    .map(|(id, versions)| {
                        (
                            (*id).to_owned(),
                            versions.iter().copied().map(Into::into).collect(),
                        )
                    })
                    .collect()
            }),
        }
    }
}
fn owned_date(d: crate::BaselineDate) -> BaselineDate {
    BaselineDate {
        raw: d.raw.to_owned(),
        date: d.date.map(str::to_owned),
        qualifier: d.qualifier,
    }
}
impl From<crate::BrowserVersion> for BrowserVersion {
    fn from(v: crate::BrowserVersion) -> Self {
        Self {
            version_added: v.version_added.map(|v| match v {
                crate::browser_compat::VersionAdded::Version(s) => {
                    crate::browser_compat::VersionAdded::Version(s.to_owned())
                }
                crate::browser_compat::VersionAdded::Unsupported(v) => {
                    crate::browser_compat::VersionAdded::Unsupported(v)
                }
            }),
            partial_implementation: v.partial_implementation,
            notes: v.notes.iter().map(|s| (*s).to_owned()).collect(),
            impl_url: v.impl_url.iter().map(|s| (*s).to_owned()).collect(),
            prefix: v.prefix.map(str::to_owned),
            alternative_name: v.alternative_name.map(str::to_owned),
            flags: v
                .flags
                .iter()
                .map(|f| BrowserFlag {
                    name: f.name.to_owned(),
                    kind: f.kind,
                    value_to_set: f.value_to_set.map(str::to_owned),
                })
                .collect(),
            version_removed: v.version_removed.map(str::to_owned),
            version_last: v.version_last.map(str::to_owned),
        }
    }
}

/// Derive every compatibility warning from one selected record.
#[must_use]
pub fn verdict(facts: &Facts) -> Option<CompatVerdict> {
    verdict_for_browsers(facts, ["chrome", "edge", "firefox", "safari"])
}

/// Derive a presentation verdict for explicitly selected browser products.
#[must_use]
pub fn verdict_for_browsers<'a>(
    facts: &Facts,
    browsers: impl IntoIterator<Item = &'a str>,
) -> Option<CompatVerdict> {
    let mut reasons = Vec::new();
    if facts.deprecated {
        reasons.push(VerdictReason::BcdDeprecated);
    }
    if facts.experimental {
        reasons.push(VerdictReason::BcdExperimental);
    }
    if facts.standard_track == Some(false) {
        reasons.push(VerdictReason::BcdNonStandard);
    }
    match facts.baseline.as_ref().and_then(|baseline| baseline.status) {
        Some(BaselineTier::Limited) => reasons.push(VerdictReason::BaselineLimited),
        Some(BaselineTier::Newly) => {
            let date = facts
                .baseline
                .as_ref()
                .and_then(|baseline| baseline.low_date.as_ref());
            reasons.push(VerdictReason::BaselineNewly {
                since: date.and_then(BaselineDate::year),
                qualifier: date.and_then(|date| date.qualifier),
            });
        }
        Some(BaselineTier::Widely) | None => {}
    }
    if let Some(support) = facts.browser_support.as_ref() {
        for browser in browsers {
            collect_browser_reasons(
                &mut reasons,
                browser,
                support
                    .get(browser)
                    .and_then(|v| crate::browser_compat::select_statement(v)),
            );
        }
    }
    if reasons.is_empty() {
        return None;
    }
    let recommendation = recommendation_for_reasons(&reasons);
    Some(CompatVerdict {
        recommendation,
        headline_template: headline_for_recommendation(recommendation),
        reasons,
    })
}

fn collect_browser_reasons(
    reasons: &mut Vec<VerdictReason>,
    browser: &str,
    version: Option<&BrowserVersion>,
) {
    let Some(version) = version else {
        return;
    };
    if version.supported() == Some(false) {
        reasons.push(VerdictReason::UnsupportedIn(browser.to_owned()));
    }
    if version.partial_implementation {
        reasons.push(VerdictReason::PartialImplementationIn(browser.to_owned()));
    }
    if let Some(prefix) = &version.prefix {
        reasons.push(VerdictReason::PrefixRequiredIn {
            browser: browser.to_owned(),
            prefix: prefix.clone(),
        });
    }
    if !version.flags.is_empty() {
        reasons.push(VerdictReason::BehindFlagIn(browser.to_owned()));
    }
    if let Some(version_removed) = &version.version_removed {
        reasons.push(VerdictReason::RemovedIn {
            browser: browser.to_owned(),
            version: version_removed.clone(),
        });
    }
}

fn recommendation_for_reasons(reasons: &[VerdictReason]) -> VerdictRecommendation {
    if reasons
        .iter()
        .any(|reason| matches!(reason, VerdictReason::ProfileObsolete { .. }))
    {
        return VerdictRecommendation::Forbid;
    }
    if reasons.iter().any(|reason| {
        matches!(
            reason,
            VerdictReason::BcdDeprecated
                | VerdictReason::BcdNonStandard
                | VerdictReason::RemovedIn { .. }
        )
    }) {
        return VerdictRecommendation::Avoid;
    }
    VerdictRecommendation::Caution
}

const fn headline_for_recommendation(recommendation: VerdictRecommendation) -> &'static str {
    match recommendation {
        VerdictRecommendation::Safe => "safe to use",
        VerdictRecommendation::Caution => "use with care",
        VerdictRecommendation::Avoid => "avoid in new work",
        VerdictRecommendation::Forbid => "do not use",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_precedence_preserves_spec_and_replaces_browser_flags() {
        use crate::SpecLifecycle::{Deprecated, Experimental, Obsolete, Stable};
        let clear = LifecycleFlags::default();
        let deprecated = LifecycleFlags {
            deprecated: true,
            experimental: false,
        };
        let experimental = LifecycleFlags {
            deprecated: false,
            experimental: true,
        };
        for (spec, bundled, runtime, latest, historical) in [
            (Obsolete, deprecated, Some(clear), Obsolete, Obsolete),
            (
                Deprecated,
                clear,
                Some(experimental),
                Deprecated,
                Deprecated,
            ),
            (Experimental, deprecated, None, Deprecated, Experimental),
            (
                Experimental,
                clear,
                Some(deprecated),
                Deprecated,
                Deprecated,
            ),
            (Stable, deprecated, None, Deprecated, Stable),
            (Stable, experimental, None, Experimental, Stable),
            (Stable, deprecated, Some(clear), Stable, Stable),
            (Stable, experimental, Some(clear), Stable, Stable),
            (
                Stable,
                deprecated,
                Some(experimental),
                Experimental,
                Experimental,
            ),
            (
                Experimental,
                deprecated,
                Some(clear),
                Experimental,
                Experimental,
            ),
        ] {
            assert_eq!(
                lifecycle(crate::SpecSnapshotId::LATEST, spec, bundled, runtime),
                latest
            );
            assert_eq!(
                lifecycle(
                    crate::SpecSnapshotId::Svg11Rec20110816,
                    spec,
                    bundled,
                    runtime
                ),
                historical
            );
        }
    }
    #[test]
    fn embedded_and_refreshed_statements_have_the_same_information()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::browser_compat::{FlagKind, VersionAdded};
        let baked = crate::BrowserVersion {
            version_added: Some(VersionAdded::Version("≤20")),
            version_removed: Some("70"),
            version_last: Some("69"),
            prefix: Some("-webkit-"),
            alternative_name: Some("oldFeature"),
            partial_implementation: true,
            notes: &["First caveat", "A <code>second</code> caveat"],
            impl_url: &[
                "https://example.com/implementation/1",
                "https://example.com/implementation/2",
            ],
            flags: &[
                crate::BrowserFlag {
                    kind: FlagKind::Preference,
                    name: "feature.enabled",
                    value_to_set: Some("true"),
                },
                crate::BrowserFlag {
                    kind: FlagKind::RuntimeFlag,
                    name: "enable-feature",
                    value_to_set: None,
                },
            ],
        };
        let raw = serde_json::from_str(include_str!("fixtures/browser-support.json"))?;
        let support = crate::browser_compat::extract_browser_support(&raw).ok_or("support")?;
        assert_eq!(BrowserVersion::from(baked), support["chrome"][0]);
        Ok(())
    }
}
