//! Core runtime types for the SVG specification catalog.
//!
//! These are the public ADTs the catalog is expressed in and that the LSP and
//! linter consume. The concrete data is generated at build time from the
//! extracted, committed structured spec data (see `build.rs`); these types only
//! describe its shape.

/// A canonical SVG specification snapshot the catalog tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpecSnapshotId {
    /// SVG 1.1 First Edition (W3C REC 2003-01-14).
    Svg11Rec20030114,
    /// SVG 1.1 Second Edition (W3C REC 2011-08-16).
    Svg11Rec20110816,
    /// SVG 2 Candidate Recommendation (2018-10-04).
    Svg2Cr20181004,
    /// SVG 2 Editor's Draft (rolling).
    Svg2EditorsDraft,
}

impl SpecSnapshotId {
    /// The most recent snapshot — the default profile when none is declared.
    pub const LATEST: Self = Self::Svg2EditorsDraft;

    /// Stable string identifier (matches the on-disk snapshot directory name).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Svg11Rec20030114 => "Svg11Rec20030114",
            Self::Svg11Rec20110816 => "Svg11Rec20110816",
            Self::Svg2Cr20181004 => "Svg2Cr20181004",
            Self::Svg2EditorsDraft => "Svg2EditorsDraft",
        }
    }
}

/// Spec lifecycle of an element or attribute within a given profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecLifecycle {
    /// Stable in the selected profile.
    Stable,
    /// Present only in a draft / non-stable snapshot.
    Experimental,
    /// Explicitly deprecated by the spec.
    Deprecated,
    /// Removed from later snapshots but known historically.
    Obsolete,
}

/// Structural child-content model of an element.
#[derive(Debug, Clone)]
pub enum ContentModel {
    /// Accepts children from the listed categories unioned with explicit names.
    Children {
        /// Allowed child categories.
        categories: &'static [ElementCategory],
        /// Explicit additional child element names.
        elements: &'static [&'static str],
    },
    /// Accepts exactly the listed child element names.
    ChildrenSet(&'static [&'static str]),
    /// Accepts any element from the SVG namespace.
    AnySvg,
    /// Hosts foreign-namespace content (e.g. HTML in `foreignObject`).
    Foreign,
    /// Must be empty.
    Void,
    /// Primarily character data.
    Text,
}

/// SVG content-model element categories (the spec's own taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementCategory {
    /// Animation elements.
    Animation,
    /// Descriptive elements (`desc`, `title`, `metadata`).
    Descriptive,
    /// Basic shapes.
    Shape,
    /// Structural elements.
    Structural,
    /// Paint-server elements (gradients, pattern).
    PaintServer,
    /// Gradient elements.
    Gradient,
    /// Container elements.
    Container,
    /// Filter primitive elements.
    FilterPrimitive,
    /// Light-source elements.
    LightSource,
    /// Text-content child elements (`tspan`, `textPath`).
    TextContentChild,
}

/// How an attribute's value space is described.
#[derive(Debug, Clone)]
pub enum AttributeValues {
    /// One of the listed keyword values.
    Enum(&'static [&'static str]),
    /// A `<transform-list>`, optionally constrained to named functions.
    Transform(&'static [&'static str]),
    /// A `preserveAspectRatio` value.
    PreserveAspectRatio {
        /// Allowed alignment keywords.
        alignments: &'static [&'static str],
        /// Allowed `meet` / `slice` keywords.
        meet_or_slice: &'static [&'static str],
    },
    /// A CSS/SVG color value.
    Color,
    /// A length value.
    Length,
    /// A URL / fragment reference.
    Url,
    /// A number or percentage.
    NumberOrPercentage,
    /// Free-form text with no constrained grammar.
    FreeText,
}

/// Inexactness qualifier on a baseline / version date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaselineQualifier {
    /// The date is an "on or before" upper bound.
    Before,
    /// The date is an "on or after" lower bound.
    After,
    /// The date is approximate.
    Approximately,
}

/// Web-platform baseline status of a feature (the *browser-compat* axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    /// Widely available across engines (since `since`).
    Widely {
        /// Year it reached widely-available baseline.
        since: u16,
        /// Qualifier when the upstream date was inexact.
        qualifier: Option<BaselineQualifier>,
    },
    /// Newly available (since `since`), not yet widely available.
    Newly {
        /// Year it reached newly-available baseline.
        since: u16,
        /// Qualifier when the upstream date was inexact.
        qualifier: Option<BaselineQualifier>,
    },
    /// Limited availability.
    Limited,
}

/// A runtime flag a browser gates a feature behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserFlag {
    /// Flag/preference name.
    pub name: &'static str,
}

/// Baked support detail for one browser (the *browser-compat* axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserVersion {
    /// Explicit support flag, when the data states one (`false` = unsupported).
    pub supported: Option<bool>,
    /// Whether support is partial.
    pub partial_implementation: bool,
    /// Upstream notes.
    pub notes: &'static [&'static str],
    /// Vendor prefix required, when any.
    pub prefix: Option<&'static str>,
    /// Alternative name the browser ships under, when any.
    pub alternative_name: Option<&'static str>,
    /// Runtime flags gating the feature.
    pub flags: &'static [BrowserFlag],
    /// First version (`"15"`, `"≤37"`), when known.
    pub version_added: Option<&'static str>,
    /// Qualifier on the added version's date inexactness.
    pub version_qualifier: Option<BaselineQualifier>,
    /// Version support was removed in, when any.
    pub version_removed: Option<&'static str>,
    /// Qualifier on the removed version's date inexactness.
    pub version_removed_qualifier: Option<BaselineQualifier>,
}

impl BrowserVersion {
    /// An empty support record: support state unknown, no version, no caveats.
    /// A base for spreading (`..BrowserVersion::EMPTY`) when only a field or two
    /// is known.
    pub const EMPTY: Self = Self {
        supported: None,
        partial_implementation: false,
        notes: &[],
        prefix: None,
        alternative_name: None,
        flags: &[],
        version_added: None,
        version_qualifier: None,
        version_removed: None,
        version_removed_qualifier: None,
    };
}

/// Per-browser support across the four tracked engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// One contributing reason behind a [`CompatVerdict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictReason {
    /// Compat data marks the feature deprecated.
    BcdDeprecated,
    /// Compat data marks the feature experimental.
    BcdExperimental,
    /// The active profile dropped the feature after `last_seen`.
    ProfileObsolete {
        /// Last snapshot the feature was present in.
        last_seen: SpecSnapshotId,
    },
    /// The feature is only in a draft snapshot.
    ProfileExperimental,
    /// Baseline is limited.
    BaselineLimited,
    /// Baseline is newly available.
    BaselineNewly {
        /// Year of newly-available baseline.
        since: u16,
        /// Date-inexactness qualifier.
        qualifier: Option<BaselineQualifier>,
    },
    /// A browser ships a partial implementation.
    PartialImplementationIn(&'static str),
    /// A browser needs a vendor prefix.
    PrefixRequiredIn {
        /// Browser identifier.
        browser: &'static str,
        /// Required prefix literal.
        prefix: &'static str,
    },
    /// A browser gates the feature behind a flag.
    BehindFlagIn(&'static str),
    /// A browser reports no support.
    UnsupportedIn(&'static str),
    /// A browser removed support at a version.
    RemovedIn {
        /// Browser identifier.
        browser: &'static str,
        /// Version support was removed in.
        version: &'static str,
        /// Qualifier on the removal version's date inexactness.
        qualifier: Option<BaselineQualifier>,
    },
}

/// Highest-tier recommendation across a verdict's reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictRecommendation {
    /// Safe to use.
    Safe,
    /// Usable with caution.
    Caution,
    /// Avoid.
    Avoid,
    /// Do not use.
    Forbid,
}

/// A fully-reconciled compatibility verdict for a feature in a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatVerdict {
    /// Highest-tier recommendation across all reasons.
    pub recommendation: VerdictRecommendation,
    /// Static template key for the hover headline.
    pub headline_template: &'static str,
    /// Contributing reasons, sorted by tier.
    pub reasons: &'static [VerdictReason],
}

/// Definition of an SVG element.
#[derive(Debug, Clone)]
pub struct ElementDef {
    /// Element tag name.
    pub name: &'static str,
    /// Short human-readable description (from spec prose).
    pub description: &'static str,
    /// MDN reference URL.
    pub mdn_url: &'static str,
    /// Resolved spec permalink URL for the active profile, when known.
    pub spec_url: Option<&'static str>,
    /// Whether compat data marks the element deprecated.
    pub deprecated: bool,
    /// Whether compat data marks the element experimental.
    pub experimental: bool,
    /// Web-platform baseline status, when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser support data, when known.
    pub browser_support: Option<BrowserSupport>,
    /// Pre-computed compat verdicts per snapshot.
    pub verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
    /// Structural child-content model.
    pub content_model: ContentModel,
    /// Element-specific attribute names.
    pub attrs: &'static [&'static str],
    /// Whether the element accepts SVG global attributes.
    pub global_attrs: bool,
}

/// Definition of an SVG attribute.
#[derive(Debug, Clone)]
pub struct AttributeDef {
    /// Attribute name.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
    /// MDN reference URL.
    pub mdn_url: &'static str,
    /// Resolved spec permalink URL for the active profile, when known.
    pub spec_url: Option<&'static str>,
    /// Whether compat data marks the attribute deprecated.
    pub deprecated: bool,
    /// Whether compat data marks the attribute experimental.
    pub experimental: bool,
    /// Whether the spec marks the attribute animatable.
    pub animatable: bool,
    /// CSS presentation-attribute property name, when applicable.
    pub presentation_attribute: Option<&'static str>,
    /// Web-platform baseline status, when known.
    pub baseline: Option<BaselineStatus>,
    /// Per-browser support data, when known.
    pub browser_support: Option<BrowserSupport>,
    /// Pre-computed compat verdicts per snapshot.
    pub verdicts: &'static [(SpecSnapshotId, CompatVerdict)],
    /// Value space.
    pub values: AttributeValues,
    /// Per-snapshot value overrides, when the value space differs by profile.
    pub value_overrides: &'static [(SpecSnapshotId, AttributeValues)],
    /// Elements the attribute applies to; empty means global.
    pub elements: &'static [&'static str],
}

impl AttributeDef {
    /// The value space for `profile`, honoring per-snapshot overrides.
    #[must_use]
    pub fn values_for_profile(&self, profile: SpecSnapshotId) -> &AttributeValues {
        self.value_overrides
            .iter()
            .find_map(|(snapshot, values)| (*snapshot == profile).then_some(values))
            .unwrap_or(&self.values)
    }
}

/// Outcome of a profile-aware element/attribute lookup.
#[derive(Debug, Clone, Copy)]
pub enum ProfileLookup<T: 'static> {
    /// The feature is present in the profile.
    Present {
        /// The matched definition.
        value: &'static T,
        /// Its lifecycle in this profile.
        lifecycle: SpecLifecycle,
    },
    /// Known to SVG, but not part of this profile; carries the snapshots it
    /// *is* present in.
    UnsupportedInProfile {
        /// Snapshots the feature is present in.
        known_in: &'static [SpecSnapshotId],
    },
    /// Not a known SVG element/attribute at all.
    Unknown,
}

/// An element paired with its lifecycle in a profile (for completion).
#[derive(Debug, Clone, Copy)]
pub struct ProfiledElement {
    /// The element definition.
    pub element: &'static ElementDef,
    /// Its lifecycle in the active profile.
    pub lifecycle: SpecLifecycle,
}

/// An attribute paired with its lifecycle in a profile (for completion).
#[derive(Debug, Clone, Copy)]
pub struct ProfiledAttribute {
    /// The attribute definition.
    pub attribute: &'static AttributeDef,
    /// Its lifecycle in the active profile.
    pub lifecycle: SpecLifecycle,
}

/// Metadata describing a snapshot (id aliases for profile resolution, etc.).
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    /// The snapshot this metadata describes.
    pub snapshot: SpecSnapshotId,
    /// Accepted alias strings that resolve to this snapshot.
    pub aliases: &'static [&'static str],
}
