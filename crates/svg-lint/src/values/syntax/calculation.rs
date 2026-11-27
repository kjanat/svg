//! Syntax and dimensional types of CSS calculations.
//! <https://drafts.csswg.org/css-values-4/#calc-syntax>

use super::{
    notation::is_math_function,
    parser::{BlockKind, ComponentValue, Function},
    tokenizer::Token,
    types::{NumericType, UnitlessMode},
    units::{UnitCategory, unit_category},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Type([i64; 7]);

impl Type {
    const NUMBER: Self = Self([0; 7]);

    const fn dimension(index: usize) -> Self {
        let mut result = Self::NUMBER;
        result.0[index] = 1;
        result
    }

    fn is_numeric_value(self) -> bool {
        self == Self::NUMBER || (0..self.0.len()).any(|index| self == Self::dimension(index))
    }

    fn is_proportional(self) -> bool {
        self == Self::NUMBER || self == Self::dimension(PERCENT)
    }

    fn product(self, other: Self, divide: bool) -> Self {
        Self(std::array::from_fn(|index| {
            if divide {
                self.0[index] - other.0[index]
            } else {
                self.0[index] + other.0[index]
            }
        }))
    }
}

const LENGTH: usize = 0;
const ANGLE: usize = 1;
const TIME: usize = 2;
const FREQUENCY: usize = 3;
const RESOLUTION: usize = 4;
const FLEX: usize = 5;
const PERCENT: usize = 6;

fn consistent(left: Type, right: Type) -> Option<Type> {
    (left == right).then_some(left)
}

pub fn matches(
    function: &Function<'_>,
    expected: NumericType,
    unitless: UnitlessMode,
    keyword: impl Fn(&ComponentValue<'_>) -> bool,
) -> bool {
    let (expected, percent_hint) = match expected {
        NumericType::Number | NumericType::Integer => (Type::NUMBER, None),
        NumericType::Percentage => (Type::dimension(PERCENT), None),
        NumericType::Length => (Type::dimension(LENGTH), None),
        NumericType::LengthPercentage => (Type::dimension(LENGTH), Some(LENGTH)),
        NumericType::Angle => (Type::dimension(ANGLE), None),
        NumericType::AnglePercentage => (Type::dimension(ANGLE), Some(ANGLE)),
        NumericType::Time => (Type::dimension(TIME), None),
        NumericType::TimePercentage => (Type::dimension(TIME), Some(TIME)),
        NumericType::Frequency => (Type::dimension(FREQUENCY), None),
        NumericType::FrequencyPercentage => (Type::dimension(FREQUENCY), Some(FREQUENCY)),
        NumericType::Resolution => (Type::dimension(RESOLUTION), None),
        NumericType::Flex => (Type::dimension(FLEX), None),
    };
    Calculation {
        percent_hint,
        keyword: &keyword,
    }
    .function(function)
    .is_some_and(|result| {
        result == expected
            || (unitless == UnitlessMode::PresentationAttribute
                && result == Type::NUMBER
                && [Type::dimension(LENGTH), Type::dimension(ANGLE)].contains(&expected))
    })
}

pub fn valid_function(
    function: &Function<'_>,
    keyword: impl Fn(&ComponentValue<'_>) -> bool,
) -> bool {
    [None, Some(LENGTH), Some(ANGLE), Some(TIME), Some(FREQUENCY)]
        .into_iter()
        .any(|percent_hint| {
            Calculation {
                percent_hint,
                keyword: &keyword,
            }
            .function(function)
            .is_some_and(Type::is_numeric_value)
        })
}

type Keyword<'a> = dyn Fn(&ComponentValue<'_>) -> bool + 'a;

struct Calculation<'a> {
    percent_hint: Option<usize>,
    keyword: &'a Keyword<'a>,
}

impl Calculation<'_> {
    fn value(&self, value: &ComponentValue<'_>) -> Option<Type> {
        match value {
            ComponentValue::Token(Token::Number(_)) => Some(Type::NUMBER),
            ComponentValue::Token(Token::Percentage(_)) => {
                Some(Type::dimension(self.percent_hint.unwrap_or(PERCENT)))
            }
            ComponentValue::Token(Token::Dimension { unit, .. }) => {
                let index = match unit_category(unit)? {
                    UnitCategory::Length => LENGTH,
                    UnitCategory::Angle => ANGLE,
                    UnitCategory::Time => TIME,
                    UnitCategory::Frequency => FREQUENCY,
                    UnitCategory::Resolution => RESOLUTION,
                    UnitCategory::Flex => FLEX,
                };
                Some(Type::dimension(index))
            }
            ComponentValue::Token(Token::Ident(name))
                if ["e", "pi", "infinity", "-infinity", "nan"]
                    .iter()
                    .any(|keyword| name.eq_ignore_ascii_case(keyword))
                    || (self.keyword)(value) =>
            {
                Some(Type::NUMBER)
            }
            ComponentValue::Block(block) if block.kind == BlockKind::Paren => {
                self.sum(&block.value)
            }
            ComponentValue::Function(function) if is_math_function(&function.name) => {
                self.function(function)
            }
            ComponentValue::Function(function)
                if ["sibling-index", "sibling-count"]
                    .iter()
                    .any(|name| function.name.eq_ignore_ascii_case(name))
                    && function.value.iter().all(ComponentValue::is_whitespace) =>
            {
                Some(Type::NUMBER)
            }
            ComponentValue::Function(function)
                if ["anchor", "anchor-size"]
                    .iter()
                    .any(|name| function.name.eq_ignore_ascii_case(name)) =>
            {
                Some(Type::dimension(LENGTH))
            }
            ComponentValue::Function(function)
                if ["media-progress", "container-progress"]
                    .iter()
                    .any(|name| function.name.eq_ignore_ascii_case(name)) =>
            {
                Some(Type::NUMBER)
            }
            _ => None,
        }
    }

    fn sum(&self, values: &[ComponentValue<'_>]) -> Option<Type> {
        let mut values = trim(values);
        let mut result = self.product(&mut values)?;
        while !values.is_empty() {
            // Both sides of a binary plus or minus require whitespace. A sign
            // absorbed into a number token is part of the operand instead.
            if !values.first().is_some_and(ComponentValue::is_whitespace) {
                return None;
            }
            let [ComponentValue::Token(Token::Delim('+' | '-')), rest @ ..] = trim_start(values)
            else {
                return None;
            };
            if !rest.first().is_some_and(ComponentValue::is_whitespace) {
                return None;
            }
            values = trim_start(rest);
            result = consistent(result, self.product(&mut values)?)?;
        }
        Some(result)
    }

    fn product(&self, values: &mut &[ComponentValue<'_>]) -> Option<Type> {
        let (first, rest) = values.split_first()?;
        let mut result = self.value(first)?;
        *values = rest;
        loop {
            let [
                ComponentValue::Token(Token::Delim(operator @ ('*' | '/'))),
                rest @ ..,
            ] = trim_start(values)
            else {
                return Some(result);
            };
            let (next, rest) = trim_start(rest).split_first()?;
            result = result.product(self.value(next)?, *operator == '/');
            *values = rest;
        }
    }

    fn function(&self, function: &Function<'_>) -> Option<Type> {
        // Keep empty groups so a trailing comma fails the arity/syntax check.
        let mut args: Vec<_> = function
            .value
            .split(|value| matches!(value, ComponentValue::Token(Token::Comma)))
            .map(trim)
            .collect();
        let name = function.name.to_ascii_lowercase();
        if name == "calc-mix" {
            return common(args.into_iter().map(|arg| {
                self.sum(arg).or_else(|| {
                    let (weight, sum) = arg.split_last()?;
                    let weight_type = Calculation {
                        percent_hint: None,
                        keyword: self.keyword,
                    }
                    .value(weight)?;
                    (weight_type == Type::dimension(PERCENT)
                        && sum.last().is_some_and(ComponentValue::is_whitespace))
                    .then(|| self.sum(sum))
                    .flatten()
                })
            }));
        }
        if name == "calc-interpolate" {
            let (source, stops) = args.split_first()?;
            return self.interpolate(source, stops);
        }
        let strategy = args
            .first()
            .and_then(|arg| single_ident(arg))
            .map(str::to_ascii_lowercase);
        let line_width = name == "round" && strategy.as_deref() == Some("line-width");
        if name == "round"
            && matches!(
                strategy.as_deref(),
                Some("nearest" | "up" | "down" | "to-zero" | "line-width")
            )
            || name == "random" && args.first().is_some_and(|arg| self.random_key(arg))
        {
            args.remove(0);
        }

        if name == "progress"
            && let Some(first) = args.first_mut()
            && let [modifier, value @ ..] = *first
            && modifier
                .ident()
                .is_some_and(|name| name.eq_ignore_ascii_case("no-clamp"))
        {
            *first = trim_start(value);
        }

        let count = args.len();
        let mut values = args.into_iter().enumerate().map(|(index, arg)| {
            if name == "clamp"
                && index != 1
                && single_ident(arg).is_some_and(|name| name.eq_ignore_ascii_case("none"))
            {
                return None;
            }
            Some(self.sum(arg))
        });
        let number = Type::NUMBER;
        let angle = Type::dimension(ANGLE);
        match (name.as_str(), count) {
            ("calc" | "abs", 1) => values.next()??,
            ("min" | "max" | "hypot", 1..) | ("mod" | "rem", 2) | ("random", 2..=3) => {
                common(values.map(Option::flatten))
            }
            ("clamp", 3) => common(values.flatten()),
            ("round", 1..=2) => {
                let ty = common(values.map(Option::flatten))?;
                if line_width {
                    (ty == Type::dimension(LENGTH)).then_some(ty)
                } else {
                    (count == 2 || ty == number).then_some(ty)
                }
            }
            ("sin" | "cos" | "tan", 1) => [number, angle]
                .contains(&values.next()???)
                .then_some(number),
            ("asin" | "acos" | "atan", 1) => (values.next()??? == number).then_some(angle),
            ("atan2", 2) => common(values.map(Option::flatten)).map(|_| angle),
            ("pow", 2) | ("sqrt" | "exp", 1) | ("log", 1..=2) => values
                .all(|ty| ty.flatten() == Some(number))
                .then_some(number),
            ("sign", 1) => common(values.map(Option::flatten)).map(|_| number),
            ("progress", 3) => common(values.map(Option::flatten))
                .filter(|ty| ty.is_numeric_value())
                .map(|_| number),
            _ => None,
        }
    }

    // The input positions have their own context. They are either
    // proportional or all use the same dimension.
    fn interpolate(
        &self,
        source: &[ComponentValue<'_>],
        stops: &[&[ComponentValue<'_>]],
    ) -> Option<Type> {
        let input = Self {
            percent_hint: None,
            keyword: self.keyword,
        };
        let source_type = input.interpolation_source(source)?;
        let absolute = (!source_type.is_proportional()).then_some(source_type);
        let mut position_type = absolute;
        let mut has_absolute_stop = false;
        let mut outputs = Vec::new();
        let mut needs_stop = true;
        for stop in stops {
            let Some(colon) = stop
                .iter()
                .position(|value| matches!(value, ComponentValue::Token(Token::Colon)))
            else {
                if needs_stop || !matches!(stop, [value] if is_easing(value)) {
                    return None;
                }
                needs_stop = true;
                continue;
            };
            let positions = stop[..colon].iter().filter(|value| !value.is_whitespace());
            if !(1..=2).contains(&positions.clone().count()) {
                return None;
            }
            for position in positions {
                let ty = input.value(position)?;
                if !ty.is_numeric_value() {
                    return None;
                }
                if !ty.is_proportional() {
                    if position_type.is_some_and(|current| current != ty) {
                        return None;
                    }
                    position_type = Some(ty);
                    has_absolute_stop = true;
                }
            }
            outputs.push(self.sum(&stop[colon + 1..])?);
            needs_stop = false;
        }
        if needs_stop || (absolute.is_some() && !has_absolute_stop) {
            return None;
        }
        common(outputs.into_iter().map(Some))
    }

    fn interpolation_source(&self, source: &[ComponentValue<'_>]) -> Option<Type> {
        let mut source_type = None;
        let mut default_easing = false;
        let mut progress_easing = false;
        let mut values = source.iter().filter(|value| !value.is_whitespace());
        while let Some(value) = values.next() {
            if value
                .ident()
                .is_some_and(|name| name.eq_ignore_ascii_case("by"))
            {
                if progress_easing || !values.next().is_some_and(is_easing) {
                    return None;
                }
                progress_easing = true;
            } else if is_easing(value) {
                if default_easing {
                    return None;
                }
                default_easing = true;
            } else {
                if source_type.is_some() {
                    return None;
                }
                source_type = self.value(value).filter(|ty| ty.is_numeric_value());
                if source_type.is_none() {
                    let timeline = value.ident().is_some_and(|name| name.starts_with("--"))
                        || matches!(value, ComponentValue::Function(function) if ["scroll", "view"].iter().any(|name| function.name.eq_ignore_ascii_case(name)));
                    if !timeline {
                        return None;
                    }
                    source_type = Some(Type::NUMBER);
                }
            }
        }
        source_type
    }

    fn random_key(&self, value: &[ComponentValue<'_>]) -> bool {
        let entries: Vec<_> = value
            .iter()
            .filter(|value| !value.is_whitespace())
            .collect();
        match entries.as_slice() {
            [first, value]
                if first
                    .ident()
                    .is_some_and(|name| name.eq_ignore_ascii_case("fixed")) =>
            {
                match value {
                    ComponentValue::Token(Token::Number(number)) => {
                        (0.0..=1.0).contains(&number.value)
                    }
                    _ => self.value(value) == Some(Type::NUMBER),
                }
            }
            [entry]
                if entry
                    .ident()
                    .is_some_and(|name| name.eq_ignore_ascii_case("auto")) =>
            {
                true
            }
            entries => {
                let mut dashed = false;
                let mut element = false;
                let mut property = false;
                !entries.is_empty()
                    && entries.iter().all(|entry| {
                        let Some(name) = entry.ident() else {
                            return false;
                        };
                        let seen = if name.starts_with("--") {
                            &mut dashed
                        } else if name.eq_ignore_ascii_case("element-scoped") {
                            &mut element
                        } else if name.eq_ignore_ascii_case("property-scoped")
                            || name.eq_ignore_ascii_case("property-index-scoped")
                            || name.to_ascii_lowercase().starts_with("ua-")
                        {
                            &mut property
                        } else {
                            return false;
                        };
                        !std::mem::replace(seen, true)
                    })
            }
        }
    }
}

fn is_easing(value: &ComponentValue<'_>) -> bool {
    match value {
        ComponentValue::Token(Token::Ident(name)) => [
            "linear",
            "ease",
            "ease-in",
            "ease-out",
            "ease-in-out",
            "step-start",
            "step-end",
        ]
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword)),
        ComponentValue::Function(function) => {
            function.closed
                && ["linear", "cubic-bezier", "steps"]
                    .iter()
                    .any(|keyword| function.name.eq_ignore_ascii_case(keyword))
        }
        _ => false,
    }
}

fn single_ident<'v>(values: &'v [ComponentValue<'_>]) -> Option<&'v str> {
    match values {
        [value] => value.ident(),
        _ => None,
    }
}

fn common(mut types: impl Iterator<Item = Option<Type>>) -> Option<Type> {
    let first = types.next()??;
    types.try_fold(first, |left, right| consistent(left, right?))
}

fn trim<'v, 'a>(values: &'v [ComponentValue<'a>]) -> &'v [ComponentValue<'a>] {
    let values = trim_start(values);
    let end = values
        .iter()
        .rposition(|value| !value.is_whitespace())
        .map_or(0, |end| end + 1);
    &values[..end]
}

fn trim_start<'v, 'a>(values: &'v [ComponentValue<'a>]) -> &'v [ComponentValue<'a>] {
    let start = values
        .iter()
        .position(|value| !value.is_whitespace())
        .unwrap_or(values.len());
    &values[start..]
}
