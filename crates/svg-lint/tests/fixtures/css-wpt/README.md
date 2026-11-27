# Web Platform Tests cases for SVG lint

The TSV fixtures adapt input values and validity expectations from
[Web Platform Tests](https://github.com/web-platform-tests/wpt/tree/f008c03359b1b88332b383847535ed3a0640fdf3/css/css-values).
See the accompanying BSD license.

`math.tsv` draws from the progress, round/mod/rem, random, hypot/pow/sqrt,
sign/abs, clamp-length, and calc-mix invalid and serialization tests. Cases
that require CSS-only length slots use `blur()` or `fit-content()` to avoid
SVG's allowances for top-level unitless lengths. Serialization cases check
whether the input parses.

Each row is an expectation, an SVG lint grammar, and a CSS input. The tests
exercise static value parsing. They do not evaluate computed values or run
the original browser test harness.
