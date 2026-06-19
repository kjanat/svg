//! SVG specification series, the rolling editor's-draft pin, and the pure
//! freshness-classification logic the LSP's network probe builds on.

/// An SVG specification series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Series {
    /// SVG 1.0.
    Svg10,
    /// SVG 1.1.
    Svg11,
    /// SVG 2.
    Svg2,
}

impl Series {
    /// Every tracked series.
    pub const ALL: [Self; 3] = [Self::Svg10, Self::Svg11, Self::Svg2];

    /// The W3C specification shortname (the `/specifications/<shortname>` path
    /// segment in the W3C API).
    #[must_use]
    pub const fn shortname(self) -> &'static str {
        match self {
            Self::Svg10 => "SVG10",
            Self::Svg11 => "SVG11",
            Self::Svg2 => "SVG2",
        }
    }
}

/// The rolling editor's-draft pin: the svgwg commit the baked catalog was
/// derived from. The fields are populated by the extraction pipeline from the
/// fetched canonical commit; empty until it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingPin {
    /// Upstream repository URL.
    pub repository: &'static str,
    /// The fetched commit the catalog was derived from.
    pub commit: &'static str,
    /// The date the catalog captured that commit.
    pub captured_date: &'static str,
}

/// The baked rolling pin.
pub static ROLLING_PIN: RollingPin = RollingPin {
    repository: "https://github.com/w3c/svgwg",
    commit: "",
    captured_date: "",
};

/// Identity of a captured (baked) edition, for freshness classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedEditionIdentity {
    /// A rolling editor's draft pinned at a commit.
    Rolling {
        /// The pinned commit.
        commit: &'static str,
    },
    /// A dated `/TR/` edition.
    Dated {
        /// The series.
        series: Series,
        /// The dated `/TR/` URI.
        uri: &'static str,
    },
}

/// Freshness verdict for a captured edition versus what is published upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// The captured edition is up to date.
    Fresh,
    /// The rolling pin's upstream HEAD advanced past the captured commit.
    RollingStale {
        /// The upstream HEAD commit.
        head: String,
    },
}

/// Classify a captured edition against an upstream head commit (rolling case).
///
/// Pure and offline: a rolling pin is stale iff the upstream HEAD differs from
/// the captured commit. Dated editions are classified via [`unseen_versions`].
#[must_use]
pub fn classify_freshness(captured: &CapturedEditionIdentity, head: Option<&str>) -> Freshness {
    match captured {
        CapturedEditionIdentity::Rolling { commit } => match head {
            Some(head) if head != *commit => Freshness::RollingStale {
                head: head.to_owned(),
            },
            _ => Freshness::Fresh,
        },
        CapturedEditionIdentity::Dated { .. } => Freshness::Fresh,
    }
}

/// A published W3C specification version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedVersion {
    /// The version's canonical `/TR/` URI.
    pub uri: String,
}

/// A parsed W3C `/specifications/<shortname>/versions` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionsEnvelope {
    /// The series this envelope describes.
    pub series: Series,
    /// The versions W3C publishes for the series.
    pub versions: Vec<PublishedVersion>,
}

/// Failure to parse a W3C versions envelope.
#[derive(Debug)]
pub struct VersionsParseError(String);

impl std::fmt::Display for VersionsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse W3C versions: {}", self.0)
    }
}

impl std::error::Error for VersionsParseError {}

impl VersionsEnvelope {
    /// Parse a W3C versions response (`_embedded.versions[].uri`).
    ///
    /// # Errors
    /// Returns [`VersionsParseError`] when the body is not valid JSON.
    pub fn parse(series: Series, json: &str) -> Result<Self, VersionsParseError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| VersionsParseError(error.to_string()))?;
        let versions = value
            .pointer("/_embedded/versions")
            .and_then(serde_json::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|version| {
                        version
                            .get("uri")
                            .and_then(serde_json::Value::as_str)
                            .map(|uri| PublishedVersion {
                                uri: uri.to_owned(),
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { series, versions })
    }
}

/// The published versions for `series` that the baked catalog has not captured.
///
/// The baked version inventory is produced by the extraction pipeline; until it
/// lands this returns nothing (conservative — never reports every live version
/// as "new").
#[must_use]
pub const fn unseen_versions(series: Series, live: &VersionsEnvelope) -> Vec<&PublishedVersion> {
    let _ = (series, live);
    Vec::new()
}
