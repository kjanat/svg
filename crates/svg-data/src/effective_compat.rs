//! Effective records use owned strings so refreshed data and baked data follow the same path.
use crate::compat_model::{Baseline, BaselineDate, Discouraged};
use crate::{BaselineQualifier, BaselineTier, CompatVerdict, VerdictReason, VerdictRecommendation};

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
    /// Complete BCD browser details.
    pub browser_support: Option<BrowserSupport>,
}
/// Owned `BrowserFlag` facts from the selected source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserFlag {
    /// Upstream flag category, when available.
    pub kind: Option<String>,
    /// Required flag value, when available.
    pub value_to_set: Option<String>,
    /// Flag/preference name.
    pub name: String,
}

/// Owned `BrowserVersion` facts from the selected source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserVersion {
    /// Explicit support flag, when the data states one (`false` = unsupported).
    pub supported: Option<bool>,
    /// Whether support is partial.
    pub partial_implementation: bool,
    /// Upstream notes.
    pub notes: Vec<String>,
    /// Vendor prefix required, when any.
    pub prefix: Option<String>,
    /// Alternative name the browser ships under, when any.
    pub alternative_name: Option<String>,
    /// Runtime flags gating the feature.
    pub flags: Vec<BrowserFlag>,
    /// First version (`"15"`, `"≤37"`), when known.
    pub version_added: Option<String>,
    /// Qualifier on the added version's date inexactness.
    pub version_qualifier: Option<BaselineQualifier>,
    /// Version support was removed in, when any.
    pub version_removed: Option<String>,
    /// Qualifier on the removed version's date inexactness.
    pub version_removed_qualifier: Option<BaselineQualifier>,
}

/// Owned `BrowserSupport` facts from the selected source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserSupport {
    /// Chrome support.
    pub chrome: Option<BrowserVersion>,
    /// Edge support.
    pub edge: Option<BrowserVersion>,
    /// Firefox support.
    pub firefox: Option<BrowserVersion>,
    /// Safari support.
    pub safari: Option<BrowserVersion>,
}
impl From<crate::CompatFacts> for Facts {
    fn from(f: crate::CompatFacts) -> Self {
        Self {
            deprecated: f.deprecated,
            experimental: f.experimental,
            standard_track: f.standard_track,
            baseline: f.baseline.map(|b| Baseline {
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
            browser_support: f.browser_support.map(Into::into),
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
impl From<crate::BrowserSupport> for BrowserSupport {
    fn from(s: crate::BrowserSupport) -> Self {
        Self {
            chrome: s.chrome.map(Into::into),
            edge: s.edge.map(Into::into),
            firefox: s.firefox.map(Into::into),
            safari: s.safari.map(Into::into),
        }
    }
}
impl From<crate::BrowserVersion> for BrowserVersion {
    fn from(v: crate::BrowserVersion) -> Self {
        Self {
            supported: v.supported,
            partial_implementation: v.partial_implementation,
            notes: v.notes.iter().map(|s| (*s).to_owned()).collect(),
            prefix: v.prefix.map(str::to_owned),
            alternative_name: v.alternative_name.map(str::to_owned),
            flags: v
                .flags
                .iter()
                .map(|f| BrowserFlag {
                    name: f.name.to_owned(),
                    kind: None,
                    value_to_set: None,
                })
                .collect(),
            version_added: v.version_added.map(str::to_owned),
            version_qualifier: v.version_qualifier,
            version_removed: v.version_removed.map(str::to_owned),
            version_removed_qualifier: v.version_removed_qualifier,
        }
    }
}

/// Derive every compatibility warning from one selected record.
#[must_use]
pub fn verdict(facts: &Facts) -> Option<CompatVerdict> {
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
        collect_browser_reasons(&mut reasons, "chrome", support.chrome.as_ref());
        collect_browser_reasons(&mut reasons, "edge", support.edge.as_ref());
        collect_browser_reasons(&mut reasons, "firefox", support.firefox.as_ref());
        collect_browser_reasons(&mut reasons, "safari", support.safari.as_ref());
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
    browser: &'static str,
    version: Option<&BrowserVersion>,
) {
    let Some(version) = version else {
        return;
    };
    if version.supported == Some(false) {
        reasons.push(VerdictReason::UnsupportedIn(browser));
    }
    if version.partial_implementation {
        reasons.push(VerdictReason::PartialImplementationIn(browser));
    }
    if let Some(prefix) = &version.prefix {
        reasons.push(VerdictReason::PrefixRequiredIn {
            browser,
            prefix: prefix.clone(),
        });
    }
    if !version.flags.is_empty() {
        reasons.push(VerdictReason::BehindFlagIn(browser));
    }
    if let Some(version_removed) = &version.version_removed {
        reasons.push(VerdictReason::RemovedIn {
            browser,
            version: version_removed.clone(),
            qualifier: version.version_removed_qualifier,
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
