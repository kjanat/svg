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
/// Cap on flattened points, so pathological path data cannot stall a hover.
const MAX_POINTS: usize = 100_000;
/// First code point of the Unicode braille patterns block.
const BRAILLE_BASE: u32 = 0x2800;
/// Blank braille cell, which keeps column width uniform where a space would not.
const BRAILLE_BLANK: char = '\u{2800}';
/// Dot bit per position inside a cell, indexed by column then row.
const DOT_BITS: [[u8; CELL_ROWS]; CELL_COLUMNS] =
    [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// Segment node kinds produced by the `svg_path` grammar.
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
    /// Subpaths, one per `M`/`m` plus any started after a close.
    pub subpaths: usize,
    /// Width of the drawn geometry in user units.
    pub width: f64,
    /// Height of the drawn geometry in user units.
    pub height: f64,
}

/// Sketch the path data owned by a `d_attribute` node, if it draws anything.
pub fn sketch_for_attribute(attribute: Node<'_>, source: &[u8]) -> Option<Sketch> {
    let payload = descendant_of_kind(attribute, "path_data_payload")?;
    sketch(payload.utf8_text(source).ok()?)
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
}

/// Parse `path_data` and reduce every command to polylines in user units.
///
/// Path data that does not parse cleanly yields `None`: a half-typed path would
/// otherwise sketch whichever fragment happened to survive error recovery, which
/// is worse than showing nothing.
fn flatten(path_data: &str) -> Option<Outline> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_svg_path::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(path_data.as_bytes(), None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut pen = Pen::default();
    pen.walk(root, path_data.as_bytes());
    Some(pen.finish())
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
    points: usize,
}

impl Pen {
    fn walk(&mut self, node: Node<'_>, source: &[u8]) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if self.points >= MAX_POINTS {
                return;
            }
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
        let mut pairs = coordinate_pairs(node, source).into_iter();
        let Some(first) = pairs.next() else {
            return;
        };
        let start = self.resolve(first, relative);
        self.begin_subpath(start);
        self.commands += 1;
        // Trailing pairs after a moveto are implicit linetos (SVG 2 section 9.3.3).
        for pair in pairs {
            let target = self.resolve(pair, relative);
            self.line_to(target);
            self.commands += 1;
        }
    }

    fn line_pairs(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        for pair in coordinate_pairs(node, source) {
            let target = self.resolve(pair, relative);
            self.line_to(target);
            self.commands += 1;
        }
    }

    fn axis_lines(&mut self, node: Node<'_>, source: &[u8], relative: bool, axis: Axis) {
        for value in children_numbers(node, source, "path_coordinate") {
            let target = match (axis, relative) {
                (Axis::Horizontal, true) => self.cursor.shifted(value, 0.0),
                (Axis::Horizontal, false) => Point::new(value, self.cursor.y),
                (Axis::Vertical, true) => self.cursor.shifted(0.0, value),
                (Axis::Vertical, false) => Point::new(self.cursor.x, value),
            };
            self.line_to(target);
            self.commands += 1;
        }
    }

    fn cubics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        for argument in children_of_kind(node, "curveto_argument") {
            let pairs = coordinate_pairs(argument, source);
            let [first, second, end] = pairs[..] else {
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
        for argument in children_of_kind(node, "smooth_curveto_argument") {
            let pairs = coordinate_pairs(argument, source);
            let [second, end] = pairs[..] else {
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
        for argument in children_of_kind(node, "quadratic_bezier_curveto_argument") {
            let pairs = coordinate_pairs(argument, source);
            let [control, end] = pairs[..] else {
                continue;
            };
            let control = self.resolve(control, relative);
            let end = self.resolve(end, relative);
            self.quadratic_to(control, end);
            self.commands += 1;
        }
    }

    fn smooth_quadratics(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        for pair in coordinate_pairs(node, source) {
            let control = self
                .quadratic_reflection
                .map_or(self.cursor, |previous| previous.mirrored(self.cursor));
            let end = self.resolve(pair, relative);
            self.quadratic_to(control, end);
            self.commands += 1;
        }
    }

    fn arcs(&mut self, node: Node<'_>, source: &[u8], relative: bool) {
        for argument in children_of_kind(node, "elliptical_arc_argument") {
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
        if close_enough(start.x, end.x) && close_enough(start.y, end.y) {
            // Coincident endpoints: the arc is omitted entirely.
            return;
        }
        let (mut rx, mut ry) = (arc.radii.x.abs(), arc.radii.y.abs());
        if close_enough(rx, 0.0) || close_enough(ry, 0.0) {
            // A zero radius degrades to a straight line.
            self.line_to(end);
            return;
        }

        let phi = arc.rotation.to_radians();
        let (sin_phi, cos_phi) = phi.sin_cos();
        let half = Point::new((start.x - end.x) / 2.0, (start.y - end.y) / 2.0);
        let local = Point::new(
            cos_phi.mul_add(half.x, sin_phi * half.y),
            (-sin_phi).mul_add(half.x, cos_phi * half.y),
        );

        // Grow radii that are too small to join the endpoints at all.
        let oversize = (local.x * local.x) / (rx * rx) + (local.y * local.y) / (ry * ry);
        if oversize > 1.0 {
            let growth = oversize.sqrt();
            rx *= growth;
            ry *= growth;
        }

        let spread_x = (ry * ry) * (local.x * local.x);
        let spread_y = (rx * rx) * (local.y * local.y);
        let denominator = spread_x + spread_y;
        if denominator <= 0.0 {
            self.line_to(end);
            return;
        }
        let radii_product = (rx * rx) * (ry * ry);
        let numerator = radii_product - spread_y - spread_x;
        let sign = if arc.large == arc.sweep { -1.0 } else { 1.0 };
        let factor = sign * (numerator.max(0.0) / denominator).sqrt();
        let local_center = Point::new(factor * rx * local.y / ry, -factor * ry * local.x / rx);
        let center = Point::new(
            cos_phi.mul_add(local_center.x, -(sin_phi * local_center.y))
                + f64::midpoint(start.x, end.x),
            sin_phi.mul_add(local_center.x, cos_phi * local_center.y)
                + f64::midpoint(start.y, end.y),
        );

        let from = Point::new(
            (local.x - local_center.x) / rx,
            (local.y - local_center.y) / ry,
        );
        let to = Point::new(
            (-local.x - local_center.x) / rx,
            (-local.y - local_center.y) / ry,
        );
        let theta = angle_between(Point::new(1.0, 0.0), from);
        let mut sweep = angle_between(from, to);
        if !arc.sweep && sweep > 0.0 {
            sweep -= std::f64::consts::TAU;
        } else if arc.sweep && sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }

        let steps = arc_steps(sweep);
        for step in 1..=steps {
            let at = f64::from(step) / f64::from(steps);
            let (sin_t, cos_t) = sweep.mul_add(at, theta).sin_cos();
            self.push(Point::new(
                (cos_phi * rx).mul_add(cos_t, -(sin_phi * ry * sin_t)) + center.x,
                (sin_phi * rx).mul_add(cos_t, cos_phi * ry * sin_t) + center.y,
            ));
        }
        self.cursor = end;
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    const fn resolve(&self, pair: Point, relative: bool) -> Point {
        if relative {
            self.cursor.shifted(pair.x, pair.y)
        } else {
            pair
        }
    }

    fn begin_subpath(&mut self, at: Point) {
        self.flush();
        self.cursor = at;
        self.subpath_start = at;
        self.current.push(at);
        self.points += 1;
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
        self.cubic_reflection = None;
        self.quadratic_reflection = None;
    }

    /// Append a point, seeding the polyline when drawing resumes after a close.
    fn push(&mut self, to: Point) {
        if self.current.is_empty() {
            self.current.push(self.cursor);
            self.points += 1;
        }
        self.current.push(to);
        self.cursor = to;
        self.points += 1;
    }

    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.polylines.push(std::mem::take(&mut self.current));
        }
    }

    fn finish(mut self) -> Outline {
        self.flush();
        Outline {
            polylines: self.polylines,
            commands: self.commands,
        }
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
    let radii_node = children_of_kind(node, "elliptical_arc_radii")
        .into_iter()
        .next()?;
    let [rx, ry] = children_numbers(radii_node, source, "path_coordinate")[..] else {
        return None;
    };
    let rotation = children_numbers(node, source, "path_rotation")
        .first()
        .copied()?;
    Some(ArcCommand {
        radii: Point::new(rx, ry),
        rotation,
        large: read_flag(node, source, "path_arc_flag")?,
        sweep: read_flag(node, source, "path_sweep_flag")?,
        end: coordinate_pairs(node, source).into_iter().next()?,
    })
}

fn read_flag(node: Node<'_>, source: &[u8], kind: &str) -> Option<bool> {
    let flag = children_of_kind(node, kind).into_iter().next()?;
    Some(flag.utf8_text(source).ok()?.trim() == "1")
}

fn command_letter(node: Node<'_>, source: &[u8]) -> Option<u8> {
    let command = node.child_by_field_name("command")?;
    command.utf8_text(source).ok()?.trim().bytes().next()
}

fn children_of_kind<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

fn children_numbers(node: Node<'_>, source: &[u8], kind: &str) -> Vec<f64> {
    children_of_kind(node, kind)
        .into_iter()
        .filter_map(|child| read_number(child, source))
        .collect()
}

fn coordinate_pairs(node: Node<'_>, source: &[u8]) -> Vec<Point> {
    children_of_kind(node, "path_coordinate_pair")
        .into_iter()
        .filter_map(|pair| {
            let [x, y] = children_numbers(pair, source, "path_coordinate")[..] else {
                return None;
            };
            Some(Point::new(x, y))
        })
        .collect()
}

fn read_number(node: Node<'_>, source: &[u8]) -> Option<f64> {
    node.utf8_text(source).ok()?.trim().parse().ok()
}

fn descendant_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| descendant_of_kind(child, kind))
}

/// Weighted sum of points, used to evaluate Bezier basis functions.
fn blend(terms: &[(f64, Point)]) -> Point {
    Point::new(
        terms.iter().map(|(weight, point)| weight * point.x).sum(),
        terms.iter().map(|(weight, point)| weight * point.y).sum(),
    )
}

/// Signed angle from `from` to `to`, as SVG 2 appendix B.2.4 defines it.
fn angle_between(from: Point, to: Point) -> f64 {
    let magnitude = from.x.hypot(from.y) * to.x.hypot(to.y);
    if magnitude <= 0.0 {
        return 0.0;
    }
    let cosine = (from.x.mul_add(to.x, from.y * to.y) / magnitude).clamp(-1.0, 1.0);
    let sign = if from.x.mul_add(to.y, -(from.y * to.x)) < 0.0 {
        -1.0
    } else {
        1.0
    };
    sign * cosine.acos()
}

fn close_enough(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-12
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

    let dots_x = i32::try_from(GRID_COLUMNS * CELL_COLUMNS).ok()?;
    let dots_y = i32::try_from(GRID_ROWS * CELL_ROWS).ok()?;
    let scale = fit_scale(width, height, dots_x, dots_y);

    let mut grid = Grid::default();
    for polyline in &outline.polylines {
        let mut previous: Option<(i32, i32)> = None;
        for point in polyline {
            let current = (
                dot_index((point.x - min.x) * scale, dots_x),
                dot_index((point.y - min.y) * scale, dots_y),
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
        subpaths: outline.polylines.len(),
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

/// Scale user units to dots, preserving aspect ratio and never overflowing the grid.
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
        assert!(close_enough(sketch.width, 20.0), "width {}", sketch.width);
        assert!(
            close_enough(sketch.height, 10.0),
            "height {}",
            sketch.height
        );
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
    fn single_point_path_still_sketches_one_dot() -> TestResult {
        let sketch = sketch("M5 5").ok_or("moveto should sketch")?;
        assert_eq!(sketch.art, "\u{2801}");
        Ok(())
    }

    #[test]
    fn sketch_is_cropped_to_the_drawn_cells() -> TestResult {
        let art = art("M0 0 H10")?;
        assert_eq!(art.lines().count(), 1, "a flat line needs one row: {art}");
        Ok(())
    }
}
