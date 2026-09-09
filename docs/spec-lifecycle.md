# Specification lifecycle audit

Reviewed for [#50](https://github.com/kjanat/svg/issues/50), using SVGWG commit
`c403ca46ad045ebdaeda148bae69f814fb744db7`, the dated SVG 2 CR of 2018-10-04,
and the SVG 1.1 Recommendations of 2003-01-14 and 2011-08-16.

## Classification

| Feature                                   | SVG 1.1 editions | SVG 2 CR 2018                  | Pinned SVG 2 draft             |
| ----------------------------------------- | ---------------- | ------------------------------ | ------------------------------ |
| `glyph-orientation-vertical`              | Present          | Present, explicitly obsolete   | Present, explicitly obsolete   |
| `glyph-orientation-horizontal`, `kerning` | Present          | Explicitly removed             | Explicitly removed             |
| `xlink:href`, `xlink:title`, `xml:space`  | Present          | Present, explicitly deprecated | Present, explicitly deprecated |
| `type` on `style`                         | Present          | Present                        | Present, explicitly obsolete   |
| `type` on `animateTransform`              | Present          | Present                        | Present; no spec deprecation   |
| `baseProfile`, `version`                  | Present          | Absent                         | Absent                         |
| SVG font elements and `tref`              | Present          | Absent                         | Absent                         |
| `font-stretch`                            | Present          | Present                        | Present; no spec deprecation   |

Each explicit declaration is emitted in the relevant snapshot with a source URL
and anchor. Attribute declarations also retain their bearer when local to an
element. Membership remains separate: obsolete legacy behavior is different from
a feature being unavailable in a profile.

Sources for the main declarations are the dated and pinned editions of
[text](https://www.w3.org/TR/2018/CR-SVG2-20181004/text.html),
[linking](https://www.w3.org/TR/2018/CR-SVG2-20181004/linking.html), and
[document structure](https://www.w3.org/TR/2018/CR-SVG2-20181004/struct.html).
The later `style/type` declaration comes from the
[pinned styling chapter](https://github.com/w3c/svgwg/blob/c403ca46ad045ebdaeda148bae69f814fb744db7/master/styling.html#L82).
The generator discovers and scans chapters from each publication, rather than
loading this table as data.

## Distinctions preserved

- Obsolete SVG 1.1 `writing-mode` **values** do not obsolete the whole property.
- The `font-stretch` legacy name alias of `font-width` in
  [CSS Fonts 4](https://drafts.csswg.org/css-fonts-4/#propdef-font-stretch) does
  not imply removal or deprecation. The SVG snapshots still list `font-stretch`.
  First-class alias/replacement metadata remains separate work.
- Deprecated DOM interfaces and methods are outside the element/attribute
  catalog. Prospective requirements, examples and change logs are not current
  normative lifecycle declarations.
- `zoomAndPan` is absent from the SVG 2 inventory. A reference to its obsolete
  behavior in DOM prose does not restore it as a present authoring attribute.
- Current BCD deprecation and WebDX discouragement do not rewrite historical
  specification membership or explicit SVG declarations.

The source scan found no explicit element/attribute deprecation declarations in
the two SVG 1.1 publications. This does not assert that every DOM API or
individual value is current, or that every historical feature works in modern
browsers. The generated membership overlays retain older features independently
of browser support.

## Runtime precedence

For a feature supported in the selected profile, lint and completion use the
same calculation:

1. Explicit spec deprecation or obsoletion remains authoritative.
2. Refreshed browser flags replace bundled flags, including an empty record.
3. BCD deprecation takes precedence over experimental annotation.
4. Spec experimental status survives clearing browser flags.
5. Bundled browser flags annotate the latest profile. A historical profile keeps
   its spec status unless a runtime record explicitly supplies advice.

Attribute context is selected before applying that policy. An exact `style/type`
declaration does not affect `animateTransform/type`, and an exact runtime record
takes precedence over the global attribute fallback. WebDX advice stays in its
own feature and compatibility-key context.

## Reproduction and remaining scope

```sh
cargo run -p svg-data-regen -- c403ca46ad045ebdaeda148bae69f814fb744db7 --recorded-packages
cargo test -p svg-data-regen
cargo test -p svg-language-server --test spec_lifecycle
just verify
```

The package option keeps BCD 8.0.13, Web Features 3.36.0 and Webref CSS 8.7.3
when run against this catalog. It does not pin live external specification
pages; see [the pipeline](../PIPELINE.md).

Full per-fact provenance, foreign grammar references, timing grammar, and
broader structural/value review reports remain in
[#51](https://github.com/kjanat/svg/issues/51).
