//! The SVG Native profile and its constraints.

/// What kind of spec entity a constraint targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// An element.
    Element,
    /// An attribute.
    Attribute,
    /// A property.
    Property,
}

/// The scope an SVG Native conditional-support constraint applies across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintScope {
    /// The attribute/property is supported only on the listed bearer elements.
    Elements {
        /// Allowlisted bearer element names.
        names: &'static [&'static str],
    },
}

/// The SVG Native profile: the restricted subset of SVG that the SVG Native
/// rendering profile supports.
#[derive(Debug, PartialEq, Eq)]
pub struct SvgNative {
    /// Element names not supported by the profile.
    pub unsupported_elements: &'static [&'static str],
    /// Attribute names not supported by the profile.
    pub unsupported_attributes: &'static [&'static str],
    /// Property names not supported by the profile.
    pub unsupported_properties: &'static [&'static str],
}

impl SvgNative {
    /// Whether `name` (of the given kind) is unsupported by the profile.
    #[must_use]
    pub fn is_unsupported(&self, kind: ConstraintKind, name: &str) -> bool {
        let set = match kind {
            ConstraintKind::Element => self.unsupported_elements,
            ConstraintKind::Attribute => self.unsupported_attributes,
            ConstraintKind::Property => self.unsupported_properties,
        };
        set.contains(&name)
    }

    /// The scope `name` (of `kind`) is conditionally restricted to, when SVG
    /// Native supports it only on a subset of bearers.
    #[must_use]
    pub const fn supported_only(
        &self,
        kind: ConstraintKind,
        name: &str,
    ) -> Option<ConstraintScope> {
        let _ = (kind, name);
        // Populated by the extraction pipeline; `None` until it lands.
        None
    }
}

/// The SVG Native profile data (extracted from the SVG Native spec).
#[must_use]
pub fn svg_native() -> &'static SvgNative {
    // Populated by the extraction pipeline; empty constraints until it lands.
    static SVG_NATIVE: SvgNative = SvgNative {
        unsupported_elements: &[],
        unsupported_attributes: &[],
        unsupported_properties: &[],
    };
    &SVG_NATIVE
}
