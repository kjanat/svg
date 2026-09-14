//! Braille-dot sketches of path data, for hover previews.
//!
//! The sketch is text rather than an image, so it survives editors that cannot
//! render images in a hover popup (Helix, terminal clients) and needs no
//! rasterizer process. Geometry comes from the `tree-sitter-svg-path` grammar
//! the rest of the workspace already parses `d` with, not from a second
//! hand-rolled path parser.
//!
//! A braille cell carries 2x4 dots, and dots in a monospace cell are close to
//! square, so a cell grid twice as wide as it is tall yields a roughly square
//! dot grid.

use tree_sitter::{Node, Parser};

/// Dot columns in one braille cell.
const CELL_COLUMNS: usize = 2;
/// Dot rows in one braille cell.
const CELL_ROWS: usize = 4;
/// Sketch width in braille cells.
const GRID_COLUMNS: usize = 32;
/// Sketch height in braille cells.
const GRID_ROWS: usize = 8;
/// Flattening resolution of a single Bezier segment.
const CURVE_STEPS: u32 = 24;
/// Largest arc sweep covered by one flattened chord.
const ARC_STEP: f64 = std::f64::consts::FRAC_PI_8;
/// Cap on the path data a hover will parse at all. Sketching runs on the
/// request path with its own parse of the value, so the work has to stay
/// bounded by something other than the document's size; artwork past this is
/// a silhouette nobody can read anyway.
///
/// This is the only bound flattening needs. The densest point generator per
/// byte is a repeated `T` argument (`"1 1 "`, four bytes, 24 flattened points),
/// so path data within the cap yields at most a few hundred thousand points —
/// bounded work, and no truncation to misreport.
const MAX_PATH_DATA_BYTES: usize = 32 * 1024;
/// First code point of the Unicode braille patterns block.
const BRAILLE_BASE: u32 = 0x2800;
/// Blank braille cell, which keeps column width uniform where a space would not.
const BRAILLE_BLANK: char = '\u{2800}';
/// Dot bit per position inside a cell, indexed by column then row.
const DOT_BITS: [[u8; CELL_ROWS]; CELL_COLUMNS] =
    [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// Segment node kinds this module draws, mirroring the `svg_path` grammar's
/// `path_segment` alternatives.
///
/// `segment_kinds_match_the_grammar` pins the list to the kinds the grammar
/// actually declares, and `every_segment_kind_draws` proves each one reaches a
/// dispatch arm, so a grammar addition or rename fails loudly here instead of
/// silently dropping that segment from every sketch.
const SEGMENT_KINDS: &[&str] = &[
    "moveto_segment",
    "closepath_segment",
    "implicit_lineto_segment",
    "lineto_segment",
    "horizontal_lineto_segment",
    "vertical_lineto_segment",
    "curveto_segment",
    "smooth_curveto_segment",
    "quadratic_bezier_curveto_segment",
    "smooth_quadratic_bezier_curveto_segment",
    "elliptical_arc_segment",
];

/// A point in user units.
#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    const fn shifted(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// Mirror `self` through `pivot`, as the smooth curve commands require.
    fn mirrored(self, pivot: Self) -> Self {
        Self::new(
            2.0f64.mul_add(pivot.x, -self.x),
            2.0f64.mul_add(pivot.y, -self.y),
        )
    }
}

/// A rendered sketch and the facts worth printing beside it.
pub struct Sketch {
    /// Braille rows, top to bottom, joined by newlines.
    pub art: String,
    /// Drawing commands, counting implicit repeats as separate commands.
    pub commands: usize,
    /// Subpaths, one per `M`/`m` plus any resumed after a close, including
    /// subpaths that draw nothing.
    pub subpaths: usize,
    /// Width of the drawn geometry in user units.
    pub width: f64,
    /// Height of the drawn geometry in user units.
    pub height: f64,
}

/// The spelling of a `d_attribute`'s name, for callers that need to decide
/// whether it applies to the element that carries it.
pub fn attribute_name<'a>(attribute: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    descendant_of_kind_any(attribute, &["d_attribute_name"])?
        .utf8_text(source)
        .ok()
}

/// Sketch the path data owned by a `d_attribute` node, if it draws anything.
///
/// The value is taken whole from between its quotes and XML-decoded, rather
/// than read off the `path_data_payload` token: that token excludes `&`, so a
/// value carrying a character reference (`d="M0 0&#32;L10 10"`, which XML
/// resolves to whitespace before SVG ever sees it) splits into a payload plus
/// an error node, and reading the payload alone would sketch only the prefix.
pub fn sketch_for_attribute(attribute: Node<'_>, source: &[u8]) -> Option<Sketch> {
    let quoted = descendant_of_kind_any(
        attribute,
        &["double_quoted_path_data", "single_quoted_path_data"],
    )?;
    let raw = quoted.utf8_text(source).ok()?;
    let inner = raw
        .strip_prefix(['"', '\''])
        .and_then(|value| value.strip_suffix(['"', '\'']))?;
    // Bound the work before decoding, not after: unescaping an oversized value
    // would scan and allocate all of it before `flatten` ever saw the length.
    // Decoding only ever shrinks, so the raw length is a sound upper bound.
    if inner.len() > MAX_PATH_DATA_BYTES {
        return None;
    }
    let decoded = quick_xml::escape::unescape(inner).ok()?;
    sketch(&decoded)
}

/// Sketch raw path data, or return `None` when it does not parse or draws nothing.
pub fn sketch(path_data: &str) -> Option<Sketch> {
    let outline = flatten(path_data)?;
    render(&outline)
}

/// Flattened geometry of one `d` value.
struct Outline {
    polylines: Vec<Vec<Point>>,
    commands: usize,
    /// Counted as the path grammar defines them rather than taken from
    /// `polylines`, which deliberately omits subpaths that draw nothing.
    subpaths: usize,
}

/// Parse `path_data` and reduce every command to polylines in user units.
///
/// Path data that does not parse cleanly yields `None`: a half-typed path would
/// otherwise sketch whichever fragment happened to survive error recovery, which
/// is worse than showing nothing. So does path data whose arithmetic leaves the
/// finite range, or that is too large to parse on the request path.
fn flatten(path_data: &str) -> Option<Outline> {
    if path_data.len() > MAX_PATH_DATA_BYTES {
        return None;
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg_path::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(path_data.as_bytes(), None)?;
    let root = tree.root_node();
    if root.has_error() || has_non_finite_number(root, path_data.as_bytes()) {
        return None;
    }

    let mut pen = Pen::default();
    pen.walk(root, path_data.as_bytes());
    pen.finish()
}

/// Which coordinate a `H`/`V` command supplies.
#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

/// Walks path segments, tracking the state the SVG path grammar is defined against.
#[derive(Default)]
struct Pen {
    polylines: Vec<Vec<Point>>,
    current: Vec<Point>,
    cursor: Point,
    subpath_start: Point,
    cubic_reflection: Option<Point>,
    quadratic_reflection: Option<Point>,
    last_command: u8,
    commands: usize,
    subpaths: usize,
    /// Whether a subpath is currently open. A close ends one, and the next
    /// drawing command begins another even without an intervening moveto.
    subpath_open: bool,
    /// Set when the path turns out not to be drawable after all: a computed
    /// point leaves the finite range (literals are checked before flattening,
    /// but `M1e308 0 l1e308 1` overflows from finite operands), or a bare
    /// coordinate pair continues a command that cannot take one.
    invalid: bool,
    /// The absolute point the first coordinate named, subtracted from every
    /// absolute coordinate after it. Curve and arc arithmetic then happens
    /// near zero, where a coordinate carries its full precision, instead of
    /// at whatever magnitude the artwork happens to sit at. Only differences
    /// leave this module, so the shift is invisible from outside.
    origin: Option<Point>,
}

impl Pen {
    fn walk(&mut self, node: Node<'_>, source: &[u8]) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if SEGMENT_KINDS.contains(&child.kind()) {
                self.segment(child, source);
            } else {
                self.walk(child, source);
            }
        }
    }

    fn segment(&mut self, node: Node<'_>, source: &[u8]) {
        let kind = node.kind();
        if kind == "implicit_lineto_segment" {
            // The grammar accepts a bare coordinate pair anywhere in its segment
            // sequence, but the SVG path grammar only allows one continuing a
            // moveto or lineto: `M0 0 Z 10 10` parses cleanly and is invalid.
            if !matches!(self.last_command, b'M' | b'm' | b'L' | b'l') {
                self.invalid = true;
                return;
            }
            // Pairs trailing a moveto or lineto inherit that command's relativity.
            let relative = self.last_command.is_ascii_lowercase();
            self.line_pairs(node, source, relative);
            return;
        }

        let Some(letter) = command_letter(node, source) else {
            return;
        };
        self.last_command = letter;
        let relative = letter.is_ascii_lowercase();

        match kind {
            "moveto_segment" => self.moveto(node, source, relative),
            "closepath_segment" => self.close(),
            "lineto_segment" => self.line_pairs(node, source, relative),
            "horizontal_lineto_segment" => {
                self.axis_lines(node, source, relative, Axis::Horizontal);
            }
            "vertical_lineto_segment" => self.axis_lines(node, source, relative, Axis::Vertical),
            "curveto_segment" => self.cubics(node, source, relative),
            "smooth_curveto_segment" => self.smooth_cubics(node, source, relative),
            "quadratic_bezier_curveto_segment" => self.quadratics(node, source, relative),
            "smooth_quadratic_bezier_curveto_segment" => {
                self.smooth_quadratics(node, source, relative);
            }
            "elliptical_arc_segment" => self.arcs(node, source, relative),
            _ => {}
        }
    }

    fn moveto(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut started = false;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "path_coordinate_pair" {
                continue;
            }
            let Some(pair) = read_pair(child, source) else {
                continue;
            };
            let target = self.resolve(pair, relative);
            if started {
                // Trailing pairs after a moveto are implicit linetos (SVG 2 9.3.3).
                self.line_to(target);
            } else {
                self.begin_subpath(target);
                started = true;
            }
            self.commands += 1;
        }
    }

    fn line_pairs(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "path_coordinate_pair" {
                continue;
            }
            let Some(pair) = read_pair(child, source) else {
                continue;
            };
            let target = self.resolve(pair, relative);
            self.line_to(target);
            self.commands += 1;
        }
    }

    fn axis_lines(&mut self, node: Node<'_>, source: &[u8], relative: bool, axis: Axis) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "path_coordinate" {
                continue;
            }
            let Some(value) = read_number(child, source) else {
                continue;
            };
            let target = match (axis, relative) {
                (Axis::Horizontal, true) => self.cursor.shifted(value, 0.0),
                (Axis::Vertical, true) => self.cursor.shifted(0.0, value),
                // One axis is absolute and the other holds, so only the named
                // coordinate crosses into the local frame; localizing the pair
                // would shift the held axis a second time.
                (Axis::Horizontal, false) => {
                    Point::new(self.localize(Point::new(value, 0.0)).x, self.cursor.y)
                }
                (Axis::Vertical, false) => {
                    Point::new(self.cursor.x, self.localize(Point::new(0.0, value)).y)
                }
            };
            self.line_to(target);
            self.commands += 1;
        }
    }

    fn cubics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for argument in node.children(&mut cursor) {
            if argument.kind() != "curveto_argument" {
                continue;
            }
            let [first, second, end] = coordinate_pairs(argument, source)[..] else {
                continue;
            };
            // Every control point of a relative curve is relative to the same
            // starting point, so all three resolve before the cursor moves.
            let first = self.resolve(first, relative);
            let second = self.resolve(second, relative);
            let end = self.resolve(end, relative);
            self.cubic_to(first, second, end);
            self.commands += 1;
        }
    }

    fn smooth_cubics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for argument in node.children(&mut cursor) {
            if argument.kind() != "smooth_curveto_argument" {
                continue;
            }
            let [second, end] = coordinate_pairs(argument, source)[..] else {
                continue;
            };
            let first = self
                .cubic_reflection
                .map_or(self.cursor, |control| control.mirrored(self.cursor));
            let second = self.resolve(second, relative);
            let end = self.resolve(end, relative);
            self.cubic_to(first, second, end);
            self.commands += 1;
        }
    }

    fn quadratics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for argument in node.children(&mut cursor) {
            if argument.kind() != "quadratic_bezier_curveto_argument" {
                continue;
            }
            let [control, end] = coordinate_pairs(argument, source)[..] else {
                continue;
            };
            let control = self.resolve(control, relative);
            let end = self.resolve(end, relative);
            self.quadratic_to(control, end);
            self.commands += 1;
        }
    }

    fn smooth_quadratics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "path_coordinate_pair" {
                continue;
            }
            let Some(pair) = read_pair(child, source) else {
                continue;
            };
            let control = self
                .quadratic_reflection
                .map_or(self.cursor, |previous| previous.mirrored(self.cursor));
            let end = self.resolve(pair, relative);
            self.quadratic_to(control, end);
            self.commands += 1;
        }
    }

    fn arcs(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        let mut cursor = node.walk();
        for argument in node.children(&mut cursor) {
            if argument.kind() != "elliptical_arc_argument" {
                continue;
            }
            let Some(arc) = read_arc(argument, source) else {
                continue;
            };
            let end = self.resolve(arc.end, relative);
            self.arc_to(&arc, end);
            self.commands += 1;
        }
    }

    fn cubic_to(&mut self, first: Point, second: Point, end: Point) {
        let start = self.cursor;
        for step in 1..=CURVE_STEPS {
            let t = f64::from(step) / f64::from(CURVE_STEPS);
            let u = 1.0 - t;
            self.push(blend(&[
                (u * u * u, start),
                (3.0 * u * u * t, first),
                (3.0 * u * t * t, second),
                (t * t * t, end),
            ]));
        }
        self.cursor = end;
        self.cubic_reflection = Some(second);
        self.quadratic_reflection = None;
    }

    fn quadratic_to(&mut self, control: Point, end: Point) {
        let start = self.cursor;
        for step in 1..=CURVE_STEPS {
            let t = f64::from(step) / f64::from(CURVE_STEPS);
            let u = 1.0 - t;
            self.push(blend(&[
                (u * u, start),
                (2.0 * u * t, control),
                (t * t, end),
            ]));
        }
        self.cursor = end;
        self.quadratic_reflection = Some(control);
        self.cubic_reflection = None;
    }

    /// Flatten an elliptical arc, following the endpoint-to-center conversion
    /// in SVG 2 appendix B.2.4.
    fn arc_to(&mut self, arc: &ArcCommand, end: Point) {
        let start = self.cursor;
        // The command still counts as an arc even where it draws nothing, so a
        // following S/T must fall back to the current point rather than reflect
        // the control point of whatever curve preceded this arc.
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
        if arc.radii.x < 0.0 || arc.radii.y < 0.0 {
            // SVG 2 9.3.8 calls a negative radius an error. Taking its absolute
            // value would quietly turn invalid data into a plausible curve.
            // The error is in the command, not in the geometry it would have
            // drawn, so it stands even where the arc paints nothing.
            self.invalid = true;
            return;
        }
        if identical(start.x, end.x) && identical(start.y, end.y) {
            // SVG 2 9.3.8 omits the arc only when the endpoints are identical.
            // A tolerance here would discard real geometry expressed in small
            // units, where the whole path is narrower than the tolerance.
            return;
        }
        let (mut rx, mut ry) = (arc.radii.x, arc.radii.y);
        if rx <= 0.0 || ry <= 0.0 {
            // Only an exactly zero radius degrades to a straight line (SVG 2
            // 9.3.8). Merely small radii are scaled up to reach the endpoints
            // by the correction below, and drawing those as a line would
            // flatten a real curve.
            self.line_to(end);
            return;
        }

        // Reduce before converting: past a few degrees of magnitude the
        // radian value's own ulp exceeds a full turn, so `sin_cos` can no
        // longer recover the orientation. SVG angles are modulo a turn, and
        // the remainder of a representable degree value is exact.
        let phi = (arc.rotation % 360.0).to_radians();
        let (sin_phi, cos_phi) = phi.sin_cos();
        let delta = Point::new(start.x - end.x, start.y - end.y);
        let local = Point::new(
            cos_phi.mul_add(delta.x, sin_phi * delta.y),
            (-sin_phi).mul_add(delta.x, cos_phi * delta.y),
        );

        // Reduce the difference against its own largest component before any
        // norm is taken. `A1e308 1e308 0 0 1 1.3e308 1.3e308` is a shallow arc
        // on radii that need no correction at all, but the hypotenuse of its
        // endpoints is 1.8e308 and leaves the range, and everything derived
        // from it then does too. Measured this way nothing can overflow, and
        // one component stays exactly ±1 so nothing can underflow to a
        // direction that does not exist either.
        let scale = local.x.abs().max(local.y.abs());
        if scale <= 0.0 {
            // Distinct endpoints always differ by something, so this is only
            // reachable if the difference itself left the range.
            self.line_to(end);
            return;
        }
        let unit = Point::new(local.x / scale, local.y / scale);

        // Take the direction by multiplying rather than dividing. The ray
        // through `(dx/rx, dy/ry)` is the ray through `(dx*ry, dy*rx)`, and
        // from a bounded vector those products cannot leave the range for any
        // positive finite radii — where the quotients can leave it at both
        // ends. `A5e-324 5e-324 0 0 1 1 0` sends each quotient to infinity,
        // and the direction between two infinities is NaN; `A1e200 1` with
        // endpoints 1e-200 apart sends both to zero, which is a direction
        // that does not exist. Neither arc is degenerate — the first is an
        // ordinary half circle once the radii are corrected, the second a
        // near-complete ellipse 2e200 across.
        let bearing = Point::new(unit.x * ry, unit.y * rx);
        let along = bearing.x.abs().max(bearing.y.abs());
        let aligned = Point::new(bearing.x / along, bearing.y / along);
        let length = aligned.x.hypot(aligned.y);
        let toward = Point::new(aligned.x / length, aligned.y / length);

        // Measure the difference in radii. This is SVG 2 B.2.4 divided through
        // by rx*ry, which keeps every quantity below bounded by one and so
        // removes the squares the raw form needs. Those squares are where
        // extreme magnitudes break: `A5e-201 5e-201` underflows them to zero
        // and vanishes into a zero denominator, and an aspect ratio like
        // `A1e-200 1` overflows them to infinity and poisons the rest in NaN.
        // Dividing through the smaller radius rather than each one's own keeps
        // this norm bounded whatever the aspect ratio.
        let smaller = rx.min(ry);
        let shaped = Point::new(unit.x * (smaller / rx), unit.y * (smaller / ry));
        let magnitude = shaped.x.hypot(shaped.y);

        // Grow radii too small to join the endpoints. SVG 2 B.2.5 scales both
        // by the factor that puts the endpoints on the ellipse, which is this
        // half-difference measured in radii; the endpoints then sit exactly
        // one radius from the centre, whatever the factor was. Scaling each
        // radius through `smaller` rather than multiplying it by that factor
        // keeps the product finite where the factor itself is not, and a
        // common factor leaves the direction alone either way. Dividing by
        // `smaller` before restoring the difference's own scale keeps the
        // comparison itself in range when no correction is needed at all.
        let offset = if (scale / smaller) * magnitude / 2.0 > 1.0 {
            let half_span = scale * magnitude / 2.0;
            rx = (rx / smaller) * half_span;
            ry = (ry / smaller) * half_span;
            toward
        } else {
            Point::new(unit.x * (scale / rx) / 2.0, unit.y * (scale / ry) / 2.0)
        };

        // At most one now, and zero only where halving the difference
        // underflowed, which `toward` already covers.
        let span = offset.x.hypot(offset.y);
        if span <= 0.0 && !arc.large {
            // A small arc over an offset below the representable range is
            // shorter than the chord it subtends, and that chord is a line.
            self.line_to(end);
            return;
        }
        let sign = if arc.large == arc.sweep { -1.0 } else { 1.0 };

        // How far the centre lies from the midpoint, along the perpendicular
        // of `toward`, in radii. The spec's factor is this divided by `span`,
        // and that quotient is what breaks: at a span of `5e-311` it is
        // `2e310`, past the finite range, while every product it appears in
        // stays within one. Keeping the two apart lets the arc survive a span
        // the factor cannot express, and reduces to `sign * toward` where the
        // span is zero, which is the limit of a chord shrinking to nothing.
        let rise = sign * span.mul_add(-span, 1.0).max(0.0).sqrt();
        let arm = Point::new(rise * toward.x, rise * toward.y);

        // The sweep angles are radius-relative too, so they follow from the
        // same offsets instead of quotients of extreme numbers.
        let from = Point::new(offset.x - arm.y, offset.y + arm.x);
        let to = Point::new(-offset.x - arm.y, -offset.y + arm.x);
        let theta = angle_between(Point::new(1.0, 0.0), from);
        let mut sweep = angle_between(from, to);
        if !arc.sweep && sweep > 0.0 {
            sweep -= std::f64::consts::TAU;
        } else if arc.sweep && sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }
        if arc.large && sweep == 0.0 {
            // The endpoints are closer than the radii can express, so the two
            // ends of the sweep are the same point and the angle between them
            // is zero either way. A large arc over an offset that small is a
            // full turn; a zero sweep is never a large arc otherwise.
            sweep = if arc.sweep {
                std::f64::consts::TAU
            } else {
                -std::f64::consts::TAU
            };
        }

        // Walk out from the start rather than out from the centre. A point on
        // the arc is the centre plus a radius, and for radii far larger than
        // the arc itself those two nearly cancel: `A1e160 1e160 0 0 1 1 0`
        // sweeps about 1e-160, which vanishes when added to an angle near
        // -pi/2, and what survives is the error in that angle's own cosine,
        // magnified by 1e160 into a sample at 6e143. Measured from the start,
        // the same step is the difference of two cosines, which the half-angle
        // identity states as a product: the tiny sweep stays a factor instead
        // of being added to something large, and the radius multiplies a
        // quantity that is small precisely because the sweep is.
        let steps = arc_steps(sweep);
        for step in 1..=steps {
            let at = f64::from(step) / f64::from(steps);
            let half = sweep * at / 2.0;
            let chord = 2.0 * half.sin();
            let (sin_mid, cos_mid) = (theta + half).sin_cos();
            let local = Point::new(-(rx * (chord * sin_mid)), ry * (chord * cos_mid));
            self.push(Point::new(
                cos_phi.mul_add(local.x, -(sin_phi * local.y)) + start.x,
                sin_phi.mul_add(local.x, cos_phi * local.y) + start.y,
            ));
        }
        self.cursor = end;
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    fn resolve(&mut self, pair: Point, relative: bool) -> Point {
        if self.origin.is_none() {
            // The first coordinate fixes the frame, in whichever form names
            // it. A path opening with `m` displaces from the implied origin,
            // and it is where that displacement lands — not the zero it
            // started from — that the rest of the path is drawn around.
            let anchor = if relative {
                self.cursor.shifted(pair.x, pair.y)
            } else {
                pair
            };
            self.origin = Some(anchor);
            return Point::default();
        }
        if relative {
            // The cursor already sits in the local frame, so a displacement
            // needs no shift.
            self.cursor.shifted(pair.x, pair.y)
        } else {
            self.localize(pair)
        }
    }

    /// Move an absolute coordinate into the local frame, fixing that frame on
    /// the first coordinate the path names. Subtracting two nearby doubles is
    /// exact, so a shape keeps every digit it had relative to its own origin;
    /// all it loses is the offset it shares with every other point.
    fn localize(&mut self, absolute: Point) -> Point {
        let origin = *self.origin.get_or_insert(absolute);
        Point::new(absolute.x - origin.x, absolute.y - origin.y)
    }

    /// A moveto paints nothing on its own, so it only moves the cursor. The
    /// point enters the polyline when a drawing command seeds it in `push`,
    /// which keeps an unused `M` out of both the raster and the bounds.
    fn begin_subpath(&mut self, at: Point) {
        if !self.accepts(at) {
            return;
        }
        self.flush();
        self.cursor = at;
        self.subpath_start = at;
        self.subpaths += 1;
        self.subpath_open = true;
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    fn line_to(&mut self, to: Point) {
        self.push(to);
        self.cursor = to;
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    fn close(&mut self) {
        self.commands += 1;
        if !self.current.is_empty() {
            let start = self.subpath_start;
            self.push(start);
            self.flush();
        }
        self.cursor = self.subpath_start;
        self.subpath_open = false;
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    /// Append a point, seeding the polyline when drawing resumes after a close.
    fn push(&mut self, to: Point) {
        if !self.accepts(to) {
            return;
        }
        if self.current.is_empty() {
            if !self.subpath_open {
                self.subpaths += 1;
                self.subpath_open = true;
            }
            self.current.push(self.cursor);
        }
        self.current.push(to);
        self.cursor = to;
    }

    /// Every point that reaches a polyline passes through here, so overflow
    /// anywhere in the coordinate arithmetic invalidates the whole sketch
    /// rather than collapsing it to a dot with an infinite reported extent.
    fn accepts(&mut self, point: Point) -> bool {
        // Both frames have to hold the point. The local one is where the
        // arithmetic happened, and the absolute one is where the document says
        // the point is: `M1e308 0 l1e308 1` is a clean line 1e308 long as
        // measured from its own start, and an endpoint at 2e308 that no
        // renderer can place. Checking only the local frame would draw it.
        let origin = self.origin.unwrap_or_default();
        if point.x.is_finite()
            && point.y.is_finite()
            && (origin.x + point.x).is_finite()
            && (origin.y + point.y).is_finite()
        {
            return true;
        }
        self.invalid = true;
        false
    }

    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.polylines.push(std::mem::take(&mut self.current));
        }
    }

    fn finish(mut self) -> Option<Outline> {
        if self.invalid {
            return None;
        }
        self.flush();
        Some(Outline {
            polylines: self.polylines,
            commands: self.commands,
            subpaths: self.subpaths,
        })
    }
}

/// Arc parameters as written in the path data, before endpoint conversion.
struct ArcCommand {
    radii: Point,
    rotation: f64,
    large: bool,
    sweep: bool,
    end: Point,
}

fn read_arc(node: Node<'_>, source: &[u8]) -> Option<ArcCommand> {
    let radii_node = first_child_of_kind(node, "elliptical_arc_radii")?;
    let [rx, ry] = children_numbers(radii_node, source, "path_coordinate")[..] else {
        return None;
    };
    let rotation_node = first_child_of_kind(node, "path_rotation")?;
    Some(ArcCommand {
        radii: Point::new(rx, ry),
        rotation: read_number(rotation_node, source)?,
        large: read_flag(node, source, "path_arc_flag")?,
        sweep: read_flag(node, source, "path_sweep_flag")?,
        end: read_pair(first_child_of_kind(node, "path_coordinate_pair")?, source)?,
    })
}

fn read_flag(node: Node<'_>, source: &[u8], kind: &str) -> Option<bool> {
    let flag = first_child_of_kind(node, kind)?;
    Some(flag.utf8_text(source).ok()?.trim() == "1")
}

fn first_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn command_letter(node: Node<'_>, source: &[u8]) -> Option<u8> {
    let command = node.child_by_field_name("command")?;
    command.utf8_text(source).ok()?.trim().bytes().next()
}

/// Numbers held directly by `node`. Only ever called on fixed-arity argument
/// nodes, so the result stays small regardless of how long the path data is.
fn children_numbers(node: Node<'_>, source: &[u8], kind: &str) -> Vec<f64> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .filter_map(|child| read_number(child, source))
        .collect()
}

/// Coordinate pairs held directly by `node`, under the same arity caveat.
fn coordinate_pairs(node: Node<'_>, source: &[u8]) -> Vec<Point> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.kind() == "path_coordinate_pair")
        .filter_map(|pair| read_pair(pair, source))
        .collect()
}

fn read_pair(node: Node<'_>, source: &[u8]) -> Option<Point> {
    let [x, y] = children_numbers(node, source, "path_coordinate")[..] else {
        return None;
    };
    Some(Point::new(x, y))
}

fn read_number(node: Node<'_>, source: &[u8]) -> Option<f64> {
    let text = node.utf8_text(source).ok()?.trim();
    let value = text.parse::<f64>().ok().filter(|value| value.is_finite())?;
    // The exponent runs off the bottom of the range as readily as off the top,
    // and `1e-999` collapsing to zero misreports the geometry just as surely
    // as `1e999` reaching infinity does. Only the digits before the exponent
    // decide whether the number was written as a zero: `0e5` is one, and
    // `1e-999` is not.
    let written_zero = text
        .split(['e', 'E'])
        .next()
        .is_none_or(|mantissa| !mantissa.bytes().any(|digit| matches!(digit, b'1'..=b'9')));
    (value != 0.0 || written_zero).then_some(value)
}

/// Path number syntax permits exponents that leave the `f64` range at either
/// end: `1e999` parses to infinity, which would poison the bounds and scale
/// into `NaN` and report an extent of `inf` units, and `1e-999` parses to a
/// zero that would draw a distinct endpoint as a dot. Such a value invalidates
/// the whole sketch rather than silently dropping one segment, matching how a
/// parse error is handled.
fn has_non_finite_number(node: Node<'_>, source: &[u8]) -> bool {
    if node.kind() == "path_number" {
        return read_number(node, source).is_none();
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| has_non_finite_number(child, source))
}

fn descendant_of_kind_any<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    if kinds.contains(&node.kind()) {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| descendant_of_kind_any(child, kinds))
}

/// Weighted sum of points, used to evaluate Bezier basis functions.
fn blend(terms: &[(f64, Point)]) -> Point {
    Point::new(
        terms.iter().map(|(weight, point)| weight * point.x).sum(),
        terms.iter().map(|(weight, point)| weight * point.y).sum(),
    )
}

/// Signed angle from `from` to `to`, as SVG 2 appendix B.2.4 defines it.
///
/// Taken as `atan2(cross, dot)` rather than a signed `acos` of the normalized
/// dot product: for a shallow sweep the cosine rounds to exactly one and `acos`
/// collapses the angle to zero, which would flatten an arc whose ends are close
/// together — the near-complete circle of `A1e8 1e8 0 1 1 1 0` among them.
fn angle_between(from: Point, to: Point) -> f64 {
    let cross = from.x.mul_add(to.y, -(from.y * to.x));
    let dot = from.x.mul_add(to.x, from.y * to.y);
    cross.atan2(dot)
}

/// Numeric equality, spelled through `partial_cmp` so it reads as deliberate
/// and does not trip `float_cmp`. Unlike `total_cmp` this treats `-0.0` and
/// `0.0` as the same coordinate, which is how SVG compares them; NaN cannot
/// reach here because non-finite literals are rejected before flattening.
fn identical(left: f64, right: f64) -> bool {
    left.partial_cmp(&right) == Some(std::cmp::Ordering::Equal)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "chord count is bounded by TAU / ARC_STEP before truncation"
)]
#[expect(
    clippy::cast_sign_loss,
    reason = "sweep magnitude is non-negative after abs()"
)]
fn arc_steps(sweep: f64) -> u32 {
    let chords = (sweep.abs() / ARC_STEP).ceil();
    (chords as u32).max(2)
}

/// Rasterize flattened polylines onto the braille grid.
fn render(outline: &Outline) -> Option<Sketch> {
    let (min, max) = bounds(&outline.polylines)?;
    let width = max.x - min.x;
    let height = max.y - min.y;
    // Individually finite extremes can still span more than f64 can hold, and
    // an infinite extent would collapse the scale and misreport the size.
    if !width.is_finite() || !height.is_finite() {
        return None;
    }

    let dots_x = i32::try_from(GRID_COLUMNS * CELL_COLUMNS).ok()?;
    let dots_y = i32::try_from(GRID_ROWS * CELL_ROWS).ok()?;
    // Place points by their position within the geometry rather than by an
    // absolute units-per-dot factor: dividing by the extent first keeps the
    // arithmetic in [0, 1], where an extent as small as `L1e-310 1e-310`
    // cannot overflow the factor into infinity and collapse onto one dot.
    let span = width.max(height);
    let scale = fit_scale(width / span, height / span, dots_x, dots_y);

    let mut grid = Grid::default();
    for polyline in &outline.polylines {
        let mut previous: Option<(i32, i32)> = None;
        for point in polyline {
            let current = (
                dot_index((point.x - min.x) / span * scale, dots_x),
                dot_index((point.y - min.y) / span * scale, dots_y),
            );
            match previous {
                Some(from) => grid.line(from, current),
                None => grid.set(current),
            }
            previous = Some(current);
        }
    }

    Some(Sketch {
        art: grid.into_art()?,
        commands: outline.commands,
        subpaths: outline.subpaths,
        width,
        height,
    })
}

fn bounds(polylines: &[Vec<Point>]) -> Option<(Point, Point)> {
    let mut points = polylines.iter().flatten();
    let first = points.next()?;
    let mut min = *first;
    let mut max = *first;
    for point in points {
        min = Point::new(min.x.min(point.x), min.y.min(point.y));
        max = Point::new(max.x.max(point.x), max.y.max(point.y));
    }
    Some((min, max))
}

/// Dots per unit of the geometry's longest side, preserving aspect ratio and
/// never overflowing the grid. Both extents arrive relative to that side, so
/// the larger is exactly one and its candidate is always finite.
fn fit_scale(width: f64, height: f64, dots_x: i32, dots_y: i32) -> f64 {
    let horizontal = if width > 0.0 {
        f64::from(dots_x - 1) / width
    } else {
        f64::INFINITY
    };
    let vertical = if height > 0.0 {
        f64::from(dots_y - 1) / height
    } else {
        f64::INFINITY
    };
    let scale = horizontal.min(vertical);
    // A single point, or a path with no extent, has no meaningful scale.
    if scale.is_finite() { scale } else { 1.0 }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "value is rounded and clamped into the dot grid before truncation"
)]
fn dot_index(value: f64, limit: i32) -> i32 {
    value.round().clamp(0.0, f64::from(limit - 1)) as i32
}

/// Dot bitmap, one byte of braille dots per cell.
struct Grid {
    cells: Vec<u8>,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            cells: vec![0; GRID_COLUMNS * GRID_ROWS],
        }
    }
}

impl Grid {
    fn set(&mut self, (x, y): (i32, i32)) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        let (Some(column), Some(row)) = (x.checked_div(CELL_COLUMNS), y.checked_div(CELL_ROWS))
        else {
            return;
        };
        let Some(cell) = self.cells.get_mut(row * GRID_COLUMNS + column) else {
            return;
        };
        if let Some(bit) = DOT_BITS
            .get(x % CELL_COLUMNS)
            .and_then(|bits| bits.get(y % CELL_ROWS))
        {
            *cell |= *bit;
        }
    }

    /// Plot a dot line with Bresenham's algorithm.
    fn line(&mut self, from: (i32, i32), to: (i32, i32)) {
        let (mut x, mut y) = from;
        let step_x = if from.0 < to.0 { 1 } else { -1 };
        let step_y = if from.1 < to.1 { 1 } else { -1 };
        let span_x = (to.0 - x).abs();
        let span_y = -(to.1 - y).abs();
        let mut error = span_x + span_y;

        loop {
            self.set((x, y));
            if x == to.0 && y == to.1 {
                return;
            }
            let doubled = 2 * error;
            if doubled >= span_y {
                error += span_y;
                x += step_x;
            }
            if doubled <= span_x {
                error += span_x;
                y += step_y;
            }
        }
    }

    /// Render to text, cropped to the cells that actually carry dots.
    fn into_art(self) -> Option<String> {
        let rows: Vec<&[u8]> = self.cells.chunks(GRID_COLUMNS).collect();
        let first_row = rows
            .iter()
            .position(|row| row.iter().any(|cell| *cell != 0))?;
        let last_row = rows
            .iter()
            .rposition(|row| row.iter().any(|cell| *cell != 0))?;
        let last_column = rows
            .iter()
            .filter_map(|row| row.iter().rposition(|cell| *cell != 0))
            .max()?;

        let art = rows
            .get(first_row..=last_row)?
            .iter()
            .map(|row| {
                row.iter()
                    .take(last_column + 1)
                    .map(|cell| {
                        char::from_u32(BRAILLE_BASE + u32::from(*cell)).unwrap_or(BRAILLE_BLANK)
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        Some(art)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn art(path_data: &str) -> Result<String, Box<dyn std::error::Error>> {
        Ok(sketch(path_data).ok_or("path data should sketch")?.art)
    }

    #[test]
    fn braille_is_the_only_output_alphabet() -> TestResult {
        let art = art("M0 0 H10 V10 H0 Z")?;
        assert!(
            art.chars()
                .all(|c| c == '\n' || ('\u{2800}'..='\u{28FF}').contains(&c)),
            "sketch should be braille and newlines only: {art:?}"
        );
        Ok(())
    }

    #[test]
    fn relative_and_absolute_forms_sketch_identically() -> TestResult {
        assert_eq!(
            art("M0 0 L10 0 L10 10 L0 10 Z")?,
            art("m0 0 l10 0 l0 10 l-10 0 z")?
        );
        Ok(())
    }

    #[test]
    fn trailing_moveto_pairs_draw_implicit_linetos() -> TestResult {
        assert_eq!(art("M0 0 10 0 10 10")?, art("M0 0 L10 0 L10 10")?);
        Ok(())
    }

    #[test]
    fn smooth_curve_mirrors_the_previous_control_point() -> TestResult {
        // The S control point reflects (8,0) through (10,10), giving (12,20).
        assert_eq!(
            art("M0 0 C2 0 8 0 10 10 S18 20 20 20")?,
            art("M0 0 C2 0 8 0 10 10 C12 20 18 20 20 20")?
        );
        Ok(())
    }

    #[test]
    fn arc_bulges_away_from_its_chord() -> TestResult {
        let sketch = sketch("M0 0 A10 10 0 0 1 20 0").ok_or("arc should sketch")?;
        assert!(
            sketch.height > 9.0,
            "a semicircular arc should rise about one radius, got {}",
            sketch.height
        );
        assert!(
            sketch.art.lines().count() > 1,
            "an arc should not flatten onto a single row: {}",
            sketch.art
        );
        Ok(())
    }

    #[test]
    fn zero_radius_arc_degrades_to_a_line() -> TestResult {
        assert_eq!(art("M0 0 A0 0 0 0 1 20 0")?, art("M0 0 L20 0")?);
        Ok(())
    }

    #[test]
    fn counts_subpaths_and_commands() -> TestResult {
        let sketch = sketch("M0 0 H10 Z M0 20 H10 Z").ok_or("path should sketch")?;
        assert_eq!(sketch.subpaths, 2);
        assert_eq!(sketch.commands, 6);
        Ok(())
    }

    #[test]
    fn reports_geometry_extent_in_user_units() -> TestResult {
        let sketch = sketch("M5 5 H25 V15 H5 Z").ok_or("path should sketch")?;
        assert!(identical(sketch.width, 20.0), "width {}", sketch.width);
        assert!(identical(sketch.height, 10.0), "height {}", sketch.height);
        Ok(())
    }

    #[test]
    fn unparsable_or_empty_path_data_has_no_sketch() {
        assert!(sketch("M0 0 L").is_none(), "truncated command");
        assert!(sketch("   ").is_none(), "whitespace-only path data");
        assert!(sketch("").is_none(), "empty path data");
        assert!(sketch("not a path").is_none(), "prose");
    }

    #[test]
    fn move_only_subpaths_draw_nothing() -> TestResult {
        // A moveto paints nothing in SVG, so it must not raster a phantom dot,
        assert!(sketch("M5 5").is_none(), "a lone moveto draws nothing");
        // nor stretch the bounds of the geometry that follows it.
        assert_eq!(art("M10000 10000 M0 0 L10 0")?, art("M0 0 L10 0")?);
        Ok(())
    }

    #[test]
    fn small_arc_radii_are_scaled_rather_than_flattened() -> TestResult {
        // Radii too small to span the endpoints are scaled up to reach them
        // (SVG 2 B.2.5), so this is a semicircle, not the straight line an
        // approximate zero test would draw.
        let sketch = sketch("M0 0 A1e-13 1e-13 0 0 1 20 0").ok_or("tiny arc should sketch")?;
        assert!(
            sketch.height > 9.0,
            "corrected radii should still bulge about one radius, got {}",
            sketch.height
        );
        assert_eq!(
            art("M0 0 A1e-13 1e-13 0 0 1 20 0")?,
            art("M0 0 A10 10 0 0 1 20 0")?
        );
        Ok(())
    }

    #[test]
    fn character_references_in_the_attribute_value_are_decoded() -> TestResult {
        // `&` cannot appear in a `path_data_payload`, so the host grammar splits
        // such a value into a payload plus an error node.
        let source = br#"<svg><path d="M0 0&#32;L10 10"/></svg>"#;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        let tree = parser.parse(source, None).ok_or("host parse")?;
        let attribute =
            descendant_of_kind_any(tree.root_node(), &["d_attribute"]).ok_or("d attribute node")?;

        let decoded = sketch_for_attribute(attribute, source)
            .ok_or("an entity-carrying value should still sketch")?;
        let plain = sketch("M0 0 L10 10").ok_or("reference sketch")?;
        assert_eq!(
            decoded.art, plain.art,
            "`&#32;` is whitespace, so the sketch should match the plain form"
        );
        assert_eq!(decoded.commands, plain.commands);
        Ok(())
    }

    #[test]
    fn sketch_is_cropped_to_the_drawn_cells() -> TestResult {
        let art = art("M0 0 H10")?;
        assert_eq!(art.lines().count(), 1, "a flat line needs one row: {art}");
        Ok(())
    }

    /// One path per entry in `SEGMENT_KINDS`, keyed by the kind it must produce.
    const SEGMENT_SAMPLES: &[(&str, &str)] = &[
        // A moveto paints nothing alone, so its sample needs a drawing command.
        ("moveto_segment", "M5 5 L6 6"),
        ("closepath_segment", "M0 0 L10 10 Z"),
        ("implicit_lineto_segment", "M0 0 10 10"),
        ("lineto_segment", "M0 0 L10 10"),
        ("horizontal_lineto_segment", "M0 0 H10"),
        ("vertical_lineto_segment", "M0 0 V10"),
        ("curveto_segment", "M0 0 C2 0 8 0 10 10"),
        ("smooth_curveto_segment", "M0 0 C2 0 8 0 10 10 S18 20 20 20"),
        ("quadratic_bezier_curveto_segment", "M0 0 Q5 10 10 0"),
        (
            "smooth_quadratic_bezier_curveto_segment",
            "M0 0 Q5 10 10 0 T20 0",
        ),
        ("elliptical_arc_segment", "M0 0 A10 10 0 0 1 20 0"),
    ];

    fn parse(path_data: &str) -> Option<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_svg_path::LANGUAGE.into())
            .ok()?;
        parser.parse(path_data.as_bytes(), None)
    }

    fn contains_kind(node: Node<'_>, kind: &str) -> bool {
        node.kind() == kind || {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|child| contains_kind(child, kind))
        }
    }

    #[test]
    fn segment_kinds_match_the_grammar() {
        let language: tree_sitter::Language = tree_sitter_svg_path::LANGUAGE.into();
        let mut declared: Vec<&str> = (0..language.node_kind_count())
            .filter_map(|id| {
                let id = u16::try_from(id).ok()?;
                let kind = language.node_kind_for_id(id)?;
                // `path_segment` is the choice wrapper over the concrete
                // segments, not a segment that carries coordinates itself.
                let concrete = kind.ends_with("_segment") && kind != "path_segment";
                (language.node_kind_is_named(id) && concrete).then_some(kind)
            })
            .collect();
        declared.sort_unstable();
        declared.dedup();

        let mut handled = SEGMENT_KINDS.to_vec();
        handled.sort_unstable();
        assert_eq!(
            handled, declared,
            "SEGMENT_KINDS drifted from the svg_path grammar; a segment kind missing here is \
             silently skipped by every sketch"
        );
    }

    #[test]
    fn every_segment_kind_draws() -> TestResult {
        let mut sampled: Vec<&str> = SEGMENT_SAMPLES.iter().map(|(kind, _)| *kind).collect();
        sampled.sort_unstable();
        let mut handled = SEGMENT_KINDS.to_vec();
        handled.sort_unstable();
        assert_eq!(handled, sampled, "every segment kind needs a sample path");

        for (kind, path_data) in SEGMENT_SAMPLES {
            let tree = parse(path_data).ok_or("sample should parse")?;
            assert!(
                contains_kind(tree.root_node(), kind),
                "{path_data} should produce a {kind} node"
            );
            let sketch = sketch(path_data).ok_or("sample should sketch")?;
            assert!(
                !sketch.art.is_empty(),
                "{kind} reached no dispatch arm: {path_data}"
            );
        }
        Ok(())
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        // `1e999` parses to f64::INFINITY, which would poison bounds and scale.
        assert!(sketch("M0 0 L1e999 1").is_none(), "infinite coordinate");
        assert!(sketch("M0 0 H1e400").is_none(), "infinite axis coordinate");
    }

    #[test]
    fn omitted_arc_still_resets_smooth_curve_reflection() -> TestResult {
        // The zero-length arc draws nothing, but it is still the command before
        // `S`, so the S control point is the current point, not a reflection of
        // the earlier cubic's second control point.
        assert_eq!(
            art("M0 0 C2 0 8 0 10 10 A10 10 0 0 1 10 10 S18 20 20 20")?,
            art("M0 0 C2 0 8 0 10 10 C10 10 18 20 20 20")?
        );
        Ok(())
    }

    #[test]
    fn coordinate_overflow_is_rejected() {
        // Every literal here is finite; the arithmetic is what leaves the
        // range, so the parse-time check alone does not catch these. The local
        // frame keeps a distant shape's own precision, and must not be allowed
        // to make a point the document cannot hold look reachable.
        for path_data in [
            "M1e308 0 l1e308 1",
            "M0 0 L1e308 0 l1e308 0",
            "M1e308 0 c0 0 0 0 1e308 1",
            "M1e308 1e308 a1 1 0 0 1 1e308 1e308",
            "M0 0 l1e308 0 l1e308 0",
            "M-1e308 0 L1e308 0",
        ] {
            assert!(
                sketch(path_data).is_none(),
                "overflowing arithmetic should not sketch: {path_data}"
            );
        }
    }

    #[test]
    fn distant_origins_keep_their_detail() -> TestResult {
        // A 100-unit heart drawn 1e16 units from the origin. Every coordinate
        // there lands on a multiple of 2, so accumulating the shape in place
        // quantises it; in a frame fixed on its own first point the arithmetic
        // is identical to drawing it at the origin.
        const TAIL: &str = concat!(
            " c-25 -25 -45 -40 -45 -60 a25 25 0 0 1 45 -15",
            " a25 25 0 0 1 45 15 c0 20 -20 35 -45 60 z"
        );
        assert_eq!(
            art(&format!("M50 90{TAIL}"))?,
            art(&format!("M1e16 1e16{TAIL}"))?
        );
        assert_eq!(
            art(&format!("M50 90{TAIL}"))?,
            art(&format!("M1e17 -1e17{TAIL}"))?
        );
        Ok(())
    }

    #[test]
    fn a_relative_opening_moveto_anchors_the_frame_where_it_lands() -> TestResult {
        // `m` displaces from the implied origin, so the shape begins where it
        // lands, not at the zero it counted from. Anchoring the frame at that
        // zero would leave the cursor in absolute coordinates and lose exactly
        // what the frame exists to keep: this line measures 96 units instead
        // of 100, and curves at the same offset visibly distort.
        assert_eq!(
            art("m1e17 1e17 l100 0 l0 50 l-100 0 z")?,
            art("M1e17 1e17 l100 0 l0 50 l-100 0 z")?
        );
        let relative = sketch("m1e17 1e17 l100 0").ok_or("relative opening")?;
        assert_eq!((relative.width, relative.height), (100.0, 0.0));
        Ok(())
    }

    #[test]
    fn a_far_flung_shape_is_still_measured_where_it_lies() -> TestResult {
        // The frame moves the arithmetic, not the reported geometry: a shape
        // is as wide as its own coordinates say, wherever it sits.
        let near = sketch("M0 0 h100 v50 h-100 z").ok_or("near")?;
        let far = sketch("M1e16 1e16 h100 v50 h-100 z").ok_or("far")?;
        assert_eq!((near.width, near.height), (far.width, far.height));
        Ok(())
    }

    /// Build well-formed path data of roughly `bytes` length.
    fn linetos(bytes: usize) -> String {
        let segment = " L1 1";
        let mut path_data = String::from("M0 0");
        while path_data.len() + segment.len() <= bytes {
            path_data.push_str(segment);
        }
        path_data
    }

    #[test]
    fn oversized_path_data_is_not_sketched() {
        let oversized = linetos(MAX_PATH_DATA_BYTES + 64);
        assert!(
            oversized.len() > MAX_PATH_DATA_BYTES,
            "fixture should exceed the parse cap"
        );
        assert!(
            sketch(&oversized).is_none(),
            "path data past the parse cap should not reach the parser"
        );

        let within = linetos(MAX_PATH_DATA_BYTES - 64);
        assert!(
            sketch(&within).is_some(),
            "path data under the cap should still sketch"
        );
    }

    #[test]
    fn dense_path_within_the_byte_cap_is_drawn_whole() -> TestResult {
        // The densest point generator per byte, well past what the old point
        // cap allowed: the sketch must cover all of it rather than silently
        // render a prefix and report the prefix's command count as the total.
        let groups = 6_000;
        let mut path_data = String::from("M0 0 T");
        for _ in 0..groups {
            path_data.push_str("1 1 ");
        }
        assert!(
            path_data.len() <= MAX_PATH_DATA_BYTES,
            "fixture must stay under the parse cap"
        );
        let sketch = sketch(&path_data).ok_or("dense path should sketch")?;
        assert_eq!(
            sketch.commands,
            groups + 1,
            "every command should be drawn, not just those before a cap"
        );
        Ok(())
    }

    #[test]
    fn subpaths_are_counted_as_the_path_grammar_defines_them() -> TestResult {
        // An empty subpath draws nothing but is still a subpath,
        let empty_first = sketch("M0 0 M10 10 L20 20").ok_or("sketch")?;
        assert_eq!(empty_first.subpaths, 2);
        // and a drawing command after a close begins one without a moveto.
        let after_close = sketch("M0 0 L10 0 Z L20 20").ok_or("sketch")?;
        assert_eq!(after_close.subpaths, 2);
        let two_closed = sketch("M0 0 H10 Z M0 20 H10 Z").ok_or("sketch")?;
        assert_eq!(two_closed.subpaths, 2);
        let one = sketch("M0 0 H10").ok_or("sketch")?;
        assert_eq!(one.subpaths, 1);
        Ok(())
    }

    #[test]
    fn implicit_coordinates_after_a_non_repeatable_command_are_invalid() {
        // The path grammar accepts these, but a bare pair only continues a
        // moveto or lineto, so the data is in error and must not be sketched.
        assert!(sketch("M0 0 Z 10 10").is_none(), "pair after a closepath");
        // The forms that legitimately carry trailing coordinates still draw:
        // a pair continuing a moveto or lineto, and the repeats that the other
        // commands take themselves (`H10 20 20` is three horizontal linetos,
        // not an implicit one).
        assert!(sketch("M0 0 10 10").is_some(), "pair after a moveto");
        assert!(sketch("M0 0 L5 5 10 10").is_some(), "pair after a lineto");
        assert!(sketch("M0 0 H10 20 20").is_some(), "repeated H coordinates");
    }

    #[test]
    fn signed_zero_endpoints_are_the_same_point() {
        // SVG compares coordinates numerically, where -0 equals 0, so this arc
        // is omitted and leaves a path that draws nothing.
        assert!(
            sketch("M0 0 A10 10 0 0 1 -0 0").is_none(),
            "an arc between numerically equal endpoints should be omitted"
        );
    }

    #[test]
    fn arcs_in_small_units_survive_the_correction() -> TestResult {
        // Squaring these magnitudes underflows to zero, which would collapse
        // the denominator and flatten the arc into its chord.
        let tiny = sketch("M0 0 A5e-201 5e-201 0 0 1 1e-200 0").ok_or("tiny arc")?;
        assert!(
            tiny.height > 0.0,
            "the arc should still bulge off its chord, got {}",
            tiny.height
        );
        // Same shape at a sane magnitude, as a check that the rescaling did not
        // change the geometry it produces.
        assert_eq!(
            art("M0 0 A5e-201 5e-201 0 0 1 1e-200 0")?,
            art("M0 0 A5 5 0 0 1 10 0")?
        );
        Ok(())
    }

    #[test]
    fn tiny_geometry_fills_the_grid_like_its_unit_scaled_twin() -> TestResult {
        // Both extents are subnormal, so an absolute units-per-dot factor
        // overflows and collapses the diagonal onto a single dot.
        assert_eq!(art("M0 0 L1e-310 1e-310")?, art("M0 0 L1 1")?);
        Ok(())
    }

    #[test]
    fn extreme_arc_aspect_ratios_still_draw() -> TestResult {
        // Squaring the radii overflows at this ratio; the correction scales
        // them to 0.5 and 5e199 and the arc remains drawable.
        let sketch = sketch("M0 0 A1e-200 1 0 0 1 1 0").ok_or("lopsided arc should sketch")?;
        assert!(
            sketch.height > 0.0,
            "the corrected arc should bulge off its chord, got {}",
            sketch.height
        );
        Ok(())
    }

    #[test]
    fn negative_arc_radii_are_invalid() {
        // A negative radius is an error, not something to correct into a curve.
        assert!(sketch("M0 0 A-10 10 0 0 1 20 0").is_none(), "negative rx");
        assert!(sketch("M0 0 A10 -10 0 0 1 20 0").is_none(), "negative ry");
        assert!(sketch("M0 0 A10 10 0 0 1 20 0").is_some(), "positive radii");
    }

    #[test]
    fn negative_radii_are_rejected_even_where_the_arc_draws_nothing() {
        // The arc returns to its current point, so it paints nothing either
        // way, but a negative radius is still an error and the sketch of a
        // path containing one would be a guess at what was meant.
        assert!(
            sketch("M0 0 L10 0 A-1 1 0 0 1 10 0").is_none(),
            "negative rx on a coincident arc"
        );
        assert!(
            sketch("M0 0 L10 0 A1 -1 0 0 1 10 0").is_none(),
            "negative ry on a coincident arc"
        );
    }

    #[test]
    fn arcs_on_vast_radii_keep_a_finite_centre() -> TestResult {
        // The endpoints sit 5e-161 radii apart, so the centre is about 2e160
        // radii along the chord normal. Reaching it through the square of
        // that offset, or through the factor times the radius, overflows on
        // the way to a result that is representable.
        let sketch = sketch("M0 0 A1e160 1e160 0 1 1 1 0").ok_or("vast arc should sketch")?;
        assert!(
            sketch.height > 1.0e160,
            "a near-complete circle should span about a diameter, got {}",
            sketch.height
        );
        Ok(())
    }

    #[test]
    fn arcs_spanning_the_top_of_the_range_still_draw() -> TestResult {
        // Endpoints 1.3e308 apart on each axis: their hypotenuse is 1.8e308
        // and leaves the range, though the arc itself needs no correction —
        // the half-span in radii is about 0.919 — and every point on it is
        // representable. The same arc an order of magnitude down is the same
        // picture, and the render normalises, so they must rasterise alike.
        assert_eq!(
            art("M0 0 A1e308 1e308 0 0 1 1.3e308 1.3e308")?,
            art("M0 0 A1e307 1e307 0 0 1 1.3e307 1.3e307")?
        );
        let vast = sketch("M0 0 A1e308 1e308 0 0 1 1.3e308 1.3e308").ok_or("vast arc")?;
        assert!(
            vast.width > 1.0e308 && vast.height > 1.0e308,
            "the arc should span its endpoints, got {} x {}",
            vast.width,
            vast.height
        );
        Ok(())
    }

    #[test]
    fn radii_far_above_their_separation_still_carry_a_direction() -> TestResult {
        // Endpoints 1e-200 apart on a 1e200 radius. Both endpoint-to-radius
        // quotients underflow to zero, which is a direction that does not
        // exist, yet the large-arc flag describes a near-complete ellipse
        // 2e200 across. The magnitude is genuinely zero here; the bearing is
        // not, and it is the bearing the centre needs.
        let wide = sketch("M0 0 A1e200 1 0 1 1 1e-200 0").ok_or("wide arc")?;
        assert!(
            wide.width > 1.0e200 && (wide.height - 2.0).abs() < 1.0e-9,
            "should span a diameter by a diameter, got {} x {}",
            wide.width,
            wide.height
        );

        // The same arc with its radii transposed turns the ellipse on its side.
        let tall = sketch("M0 0 A1 1e200 0 1 1 0 1e-200").ok_or("tall arc")?;
        assert!(
            tall.height > 1.0e200 && (tall.width - 2.0).abs() < 1.0e-9,
            "the transposed arc should stand up, got {} x {}",
            tall.width,
            tall.height
        );

        // Without the large-arc flag the shorter way round over an offset that
        // small really is the chord, so this must not inflate too.
        let short = sketch("M0 0 A1e200 1 0 0 1 1e-200 0").ok_or("short arc")?;
        assert!(
            short.width < 1.0e-100 && short.height == 0.0,
            "the small arc should stay its chord, got {} x {}",
            short.width,
            short.height
        );
        Ok(())
    }

    #[test]
    fn radii_far_below_their_separation_are_grown_not_lost() -> TestResult {
        // Radii of the smallest subnormal, a whole unit of separation apart.
        // Each endpoint-to-radius ratio is infinite, and a direction between
        // two infinities is NaN, but the spec grows radii that cannot reach
        // until they do: these become 0.5, and the arc an ordinary half circle.
        let sketch = sketch("M0 0 A5e-324 5e-324 0 0 1 1 0").ok_or("subnormal radii")?;
        assert!(
            (sketch.width - 1.0).abs() < 1.0e-9 && (sketch.height - 0.5).abs() < 1.0e-9,
            "the corrected arc should span its chord by a radius, got {} x {}",
            sketch.width,
            sketch.height
        );
        Ok(())
    }

    #[test]
    fn a_shallow_arc_on_vast_radii_stays_beside_its_chord() -> TestResult {
        // One unit of arc on a 1e160 radius: a line, to any eye. Sampled from
        // the centre, the sweep of about 1e-160 disappears into an angle near
        // -pi/2 and the radius magnifies that angle's own rounding into a
        // sample at 6e143, reported as an enormous line pointing nowhere.
        let sketch = sketch("M0 0 A1e160 1e160 0 0 1 1 0").ok_or("shallow vast arc")?;
        assert!(
            (sketch.width - 1.0).abs() < 1.0e-9 && sketch.height < 1.0e-9,
            "the arc should hug its one-unit chord, got {} x {}",
            sketch.width,
            sketch.height
        );
        Ok(())
    }

    #[test]
    fn arcs_over_subnormal_offsets_still_close_their_circle() -> TestResult {
        // The same unit circle, its two endpoints pushed closer together than
        // any dot could show. At 1e-310 the spec's centre factor is 2e310 and
        // leaves the finite range; at 5e-324 halving the difference underflows
        // to nothing. Both are the limit of the case above them, so all four
        // draw the circle the large-arc flag asks for.
        let expected = art("M0 0 A1 1 0 1 1 1e-300 0")?;
        for endpoint in ["1e-307", "1e-310", "5e-324"] {
            assert_eq!(
                art(&format!("M0 0 A1 1 0 1 1 {endpoint} 0"))?,
                expected,
                "large arc to {endpoint}"
            );
        }
        Ok(())
    }

    #[test]
    fn subnormal_offsets_do_not_inflate_a_small_arc() -> TestResult {
        // The limit only applies to the large-arc flag. Without it the arc is
        // the shorter way round, which over an offset this small is nothing.
        let sketch = sketch("M0 0 A1 1 0 0 1 5e-324 0").ok_or("small arc")?;
        assert_eq!(
            (sketch.width, sketch.height),
            (f64::from_bits(1), 0.0),
            "the chord itself, not a unit circle"
        );
        Ok(())
    }

    #[test]
    fn an_exact_semicircle_is_not_rounded_up_to_a_turn() -> TestResult {
        // Where the chord is a diameter both flags describe the same half
        // turn, and the sweep between the endpoints is a true pi rather than
        // the zero that marks an underflow. It must stay a half circle.
        for flags in ["1 1", "0 1", "1 0", "0 0"] {
            let sketch = sketch(&format!("M0 0 A1 1 0 {flags} 2 0")).ok_or("semicircle")?;
            assert!(
                (sketch.width - 2.0).abs() < 1.0e-9 && (sketch.height - 1.0).abs() < 1.0e-9,
                "flags {flags} should span a diameter by a radius, got {} x {}",
                sketch.width,
                sketch.height
            );
        }
        Ok(())
    }

    #[test]
    fn shallow_arcs_keep_their_sweep() -> TestResult {
        // The endpoints are 1 unit apart on a 1e8 radius, so the normalized
        // vectors differ by ~1e-8 radians and their cosine rounds to exactly
        // one. With the large-arc flag set this is very nearly a full circle.
        let sketch = sketch("M0 0 A100000000 100000000 0 1 1 1 0").ok_or("shallow arc")?;
        assert!(
            sketch.height > 1.0e8,
            "a near-complete circle should span about a diameter, got {}",
            sketch.height
        );
        Ok(())
    }

    #[test]
    fn huge_arc_rotations_reduce_to_their_equivalent() -> TestResult {
        // 1e20 degrees is 280 degrees, but converting it to radians first
        // leaves a value whose ulp is larger than a full turn.
        assert_eq!(
            art("M0 0 A100 20 1e20 0 1 100 0")?,
            art("M0 0 A100 20 280 0 1 100 0")?
        );
        Ok(())
    }

    #[test]
    fn numbers_that_underflow_to_zero_are_rejected() {
        // `1e-999` is a distinct endpoint written down, not the origin; a
        // sketch of it would be a dot standing for geometry that is somewhere
        // else. A zero that was written as one is still a zero.
        assert!(sketch("M0 0 L1e-999 0").is_none(), "underflowing lineto");
        assert!(sketch("M0 0 H1e-999").is_none(), "underflowing coordinate");
        assert!(sketch("M0 0 L5e-324 0").is_some(), "the smallest subnormal");
        assert!(
            sketch("M0 0 L0e5 0 L1 1").is_some(),
            "a zero written as one"
        );
    }

    #[test]
    fn bounds_overflow_is_rejected() {
        // Both endpoints are finite and accepted, but their span is not.
        assert!(
            sketch("M-1e308 0 L1e308 0").is_none(),
            "an infinite extent should not sketch"
        );
    }

    #[test]
    fn arcs_between_near_but_distinct_endpoints_still_draw() -> TestResult {
        // Endpoints a tenth of a picometre apart are distinct, so the arc is
        // not omitted; a tolerance here would drop the whole path.
        let sketch = sketch("M0 0 A5e-14 5e-14 0 0 1 1e-13 0").ok_or("tiny arc should sketch")?;
        assert!(
            sketch.height > 0.0,
            "the arc should bulge off its chord, got {}",
            sketch.height
        );
        Ok(())
    }
}
