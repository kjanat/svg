//! CSS unit identifiers.
//!
//! The tables list every unit identifier of:
//!
//! - CSS Values 4 §6 "Distance Units: the `<length>` type"
//!   (<https://drafts.csswg.org/css-values-4/#lengths>): absolute,
//!   font-relative and viewport-percentage lengths, including the `sv*`,
//!   `lv*` and `dv*` viewport variants;
//! - CSS Conditional 5 §7 "Container Relative Lengths"
//!   (<https://drafts.csswg.org/css-conditional-5/#container-lengths>);
//! - CSS Values 4 §7 "Other Quantities"
//!   (<https://drafts.csswg.org/css-values-4/#other-units>): angle, time,
//!   frequency and resolution units;
//! - CSS Grid 1 §7.2.4 "Flexible Lengths: the fr unit"
//!   (<https://drafts.csswg.org/css-grid-1/#fr-unit>).
//!
//! Entries are stored in ASCII lowercase; unit identifiers match ASCII
//! case-insensitively (CSS Values 4 §5.4).

/// Absolute length units (CSS Values 4 §6.2).
pub const ABSOLUTE_LENGTH_UNITS: &[&str] = &["cm", "mm", "q", "in", "pt", "pc", "px"];

/// Font-relative length units (CSS Values 4 §6.1.1).
pub const FONT_RELATIVE_LENGTH_UNITS: &[&str] = &[
    "em", "rem", "ex", "rex", "cap", "rcap", "ch", "rch", "ic", "ric", "lh", "rlh",
];

/// Viewport-percentage length units with their small, large and dynamic
/// variants (CSS Values 4 §6.1.2).
pub const VIEWPORT_LENGTH_UNITS: &[&str] = &[
    "vw", "vh", "vi", "vb", "vmin", "vmax", "svw", "svh", "svi", "svb", "svmin", "svmax", "lvw",
    "lvh", "lvi", "lvb", "lvmin", "lvmax", "dvw", "dvh", "dvi", "dvb", "dvmin", "dvmax",
];

/// Container query length units (CSS Conditional 5 §7).
pub const CONTAINER_LENGTH_UNITS: &[&str] = &["cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax"];

/// Angle units (CSS Values 4 §7.1).
pub const ANGLE_UNITS: &[&str] = &["deg", "grad", "rad", "turn"];

/// Duration units (CSS Values 4 §7.2).
pub const TIME_UNITS: &[&str] = &["s", "ms"];

/// Frequency units (CSS Values 4 §7.3).
pub const FREQUENCY_UNITS: &[&str] = &["hz", "khz"];

/// Resolution units (CSS Values 4 §7.4), including the `x` alias of
/// `dppx`.
pub const RESOLUTION_UNITS: &[&str] = &["dpi", "dpcm", "dppx", "x"];

/// Flexible length units (CSS Grid 1 §7.2.4).
pub const FLEX_UNITS: &[&str] = &["fr"];

/// The data type a unit identifier belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCategory {
    /// `<length>`.
    Length,
    /// `<angle>`.
    Angle,
    /// `<time>`.
    Time,
    /// `<frequency>`.
    Frequency,
    /// `<resolution>`.
    Resolution,
    /// `<flex>`.
    Flex,
}

/// The category of `unit`, matched ASCII case-insensitively, or `None` for
/// an identifier no table lists.
#[must_use]
pub fn unit_category(unit: &str) -> Option<UnitCategory> {
    let tables: [(&[&str], UnitCategory); 9] = [
        (ABSOLUTE_LENGTH_UNITS, UnitCategory::Length),
        (FONT_RELATIVE_LENGTH_UNITS, UnitCategory::Length),
        (VIEWPORT_LENGTH_UNITS, UnitCategory::Length),
        (CONTAINER_LENGTH_UNITS, UnitCategory::Length),
        (ANGLE_UNITS, UnitCategory::Angle),
        (TIME_UNITS, UnitCategory::Time),
        (FREQUENCY_UNITS, UnitCategory::Frequency),
        (RESOLUTION_UNITS, UnitCategory::Resolution),
        (FLEX_UNITS, UnitCategory::Flex),
    ];
    tables.into_iter().find_map(|(units, category)| {
        units
            .iter()
            .any(|known| known.eq_ignore_ascii_case(unit))
            .then_some(category)
    })
}

/// Whether `unit` is a `<length>` unit.
#[must_use]
pub fn is_length_unit(unit: &str) -> bool {
    unit_category(unit) == Some(UnitCategory::Length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(units: &[&str], category: UnitCategory) {
        for unit in units {
            assert_eq!(unit_category(unit), Some(category), "{unit}");
        }
    }

    // Claim: "Absolute Lengths: the cm, mm, Q, in, pt, pc, px units"
    // https://drafts.csswg.org/css-values-4/#absolute-lengths
    #[test]
    fn absolute_length_units() {
        all(
            &["cm", "mm", "Q", "in", "pt", "pc", "px"],
            UnitCategory::Length,
        );
        assert_eq!(ABSOLUTE_LENGTH_UNITS.len(), 7);
        assert_eq!(unit_category("pxx"), None);
        assert_eq!(unit_category("p"), None);
    }

    // Claim: "Font-relative Lengths: the em, rem, ex, rex, cap, rcap, ch, rch,
    // ic, ric, lh, rlh units"
    // https://drafts.csswg.org/css-values-4/#font-relative-lengths
    #[test]
    fn font_relative_length_units() {
        all(
            &[
                "em", "rem", "ex", "rex", "cap", "rcap", "ch", "rch", "ic", "ric", "lh", "rlh",
            ],
            UnitCategory::Length,
        );
        assert_eq!(FONT_RELATIVE_LENGTH_UNITS.len(), 12);
        assert_eq!(unit_category("rem2"), None);
    }

    // Claim: "Viewport-percentage Lengths: the *vw, *vh, *vi, *vb, *vmin,
    // *vmax units"
    // https://drafts.csswg.org/css-values-4/#viewport-relative-lengths
    // Claim: "There are four variants of the viewport-percentage length units,
    // corresponding to three (possibly identical) notions of the viewport
    // size." with "(lv*)", "(v*)", "(sv*)" and "(dv*)" prefixes.
    // https://drafts.csswg.org/css-values-4/#viewport-variants
    #[test]
    fn viewport_length_units() {
        for prefix in ["", "s", "l", "d"] {
            for base in ["vw", "vh", "vi", "vb", "vmin", "vmax"] {
                let unit = format!("{prefix}{base}");
                assert_eq!(unit_category(&unit), Some(UnitCategory::Length), "{unit}");
            }
        }
        assert_eq!(VIEWPORT_LENGTH_UNITS.len(), 24);
        assert_eq!(unit_category("xvw"), None);
        assert_eq!(unit_category("vmid"), None);
    }

    // Claim: "Container Relative Lengths: the cqw, cqh, cqi, cqb, cqmin, cqmax
    // units"
    // https://drafts.csswg.org/css-conditional-5/#container-lengths
    #[test]
    fn container_length_units() {
        all(
            &["cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax"],
            UnitCategory::Length,
        );
        assert_eq!(CONTAINER_LENGTH_UNITS.len(), 6);
        assert_eq!(unit_category("cq"), None);
    }

    // Claim: "Angle Units: the <angle> type and deg, grad, rad, turn units"
    // https://drafts.csswg.org/css-values-4/#angles
    #[test]
    fn angle_units() {
        all(&["deg", "grad", "rad", "turn"], UnitCategory::Angle);
        assert_eq!(ANGLE_UNITS.len(), 4);
        assert_eq!(unit_category("degree"), None);
    }

    // Claim: "Duration Units: the <time> type and s, ms units"
    // https://drafts.csswg.org/css-values-4/#time
    #[test]
    fn time_units() {
        all(&["s", "ms"], UnitCategory::Time);
        assert_eq!(TIME_UNITS.len(), 2);
        assert_eq!(unit_category("sec"), None);
        assert_eq!(unit_category("min"), None);
    }

    // Claim: "Frequency Units: the <frequency> type and Hz, kHz units"
    // https://drafts.csswg.org/css-values-4/#frequency
    #[test]
    fn frequency_units() {
        all(&["Hz", "kHz", "hz", "KHZ"], UnitCategory::Frequency);
        assert_eq!(FREQUENCY_UNITS.len(), 2);
        assert_eq!(unit_category("mhz"), None);
    }

    // Claim: "Resolution Units: the <resolution> type and dpi, dpcm, dppx
    // units" https://drafts.csswg.org/css-values-4/#resolution
    // Claim: "Added the x alias to dppx."
    // https://drafts.csswg.org/css-values-4/#changes
    #[test]
    fn resolution_units() {
        all(&["dpi", "dpcm", "dppx", "x"], UnitCategory::Resolution);
        assert_eq!(RESOLUTION_UNITS.len(), 4);
        assert_eq!(unit_category("dpmm"), None);
    }

    // Claim: "A flexible length or <flex> is a dimension with the fr unit"
    // https://drafts.csswg.org/css-grid-1/#fr-unit
    #[test]
    fn flex_units() {
        all(&["fr", "FR"], UnitCategory::Flex);
        assert_eq!(FLEX_UNITS, ["fr"]);
    }

    // Claim: "Like keywords, unit identifiers are ASCII case-insensitive."
    // https://drafts.csswg.org/css-values-4/#dimensions
    #[test]
    fn units_match_ascii_case_insensitively() {
        all(
            &["PX", "Px", "q", "EM", "SVH", "DVMAX", "CQMIN"],
            UnitCategory::Length,
        );
        all(&["DEG", "Turn"], UnitCategory::Angle);
        all(&["MS", "S"], UnitCategory::Time);
        all(&["DPPX", "X"], UnitCategory::Resolution);
        assert!(is_length_unit("Px"));
        assert!(!is_length_unit("deg"));
        // ASCII case-insensitive only: no Unicode case folding.
        assert_eq!(unit_category("\u{212A}hz"), None);
        assert_eq!(unit_category(""), None);
    }
}
