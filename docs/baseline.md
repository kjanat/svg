# Reading SVG compatibility information

Baseline answers a broad question: how established is this feature across
WebDX's defined browser set? SVG validity, individual browser support, and
`svg`'s usage advice answer different questions.

## Status and dates

| Display              | Web Features value      | Meaning                                                          |
| -------------------- | ----------------------- | ---------------------------------------------------------------- |
| Newly Available      | `"low"`                 | Reached the core browser set under upstream policy               |
| Widely Available     | `"high"`                | Also meets upstream criteria for longer-established availability |
| Limited availability | `false`                 | Not designated Baseline, including formally discouraged features |
| Unknown or omitted   | Missing or unrecognized | No recognized status; never interpreted as Limited               |

These statuses come from the
[`web-features` package](https://github.com/web-platform-dx/web-features/blob/main/packages/web-features/README.md).
`svg` does not calculate eligibility from displayed browser versions or dates.
WebDX maintains the
[Baseline definition](https://github.com/web-platform-dx/web-features/blob/main/docs/baseline.md),
including browser scope, age requirements, and editorial rules. Those rules and
the underlying data can change.

The dates are two separate, optional milestones. A feature might become **Newly
Available on 2020-01-15** and **Widely Available on 2022-07-15**. The Widely
date is not its first Baseline date. A known status still appears when its date
is absent. Hover details retain both full dates, approximate or bounded
qualifiers, and uninterpreted upstream values when a date is malformed.

WebDX can formally discourage a feature even when browsers implement it. The
default hover/dashboard presentation replaces its badge with the reason,
supporting references, and alternatives. This advice is feature-scoped and
separate from MDN BCD deprecation and SVG specification lifecycle status.
Missing status gets neutral text or no badge, never a Limited icon.

## Browsers and limits

Detailed browser information comes from
[MDN Browser Compatibility Data](https://github.com/mdn/browser-compat-data). By
default the hover and dashboard show **desktop** Chrome, Edge, Firefox, and
Safari. These are four browser products, not four independent engines. The
Baseline core set also includes Chrome and Firefox on Android and Safari on iOS.
See the
[current supported browser set](https://web-platform-dx.github.io/baseline/).

The hover's
[`svg.hover.browsers` setting](../crates/svg-language-server/README.md#hover-presentation)
can select other products and platforms, including mobile and WebView entries.
Changing this selection does not change Baseline eligibility. All upstream
products and support histories remain in the data; an explicitly selected
product without data displays unknown. Notes, flags, partial implementations,
and removals matter alongside version numbers.

Baseline does not guarantee support in every browser, WebView, non-browser SVG
renderer, assistive technology, or a particular application's deployment
environment. Use the detailed data and tests appropriate to your audience.

## Where the facts come from

Normal Rust builds use the checked-in catalog snapshot, including its recorded
BCD and Web Features package versions. They do not refresh it automatically. The
catalog's compatibility source records and Rust `compat_sources()` expose those
identities. A later release can contain newer facts.

The language server currently **enables startup refresh by default** with
`svg.runtime_compat: true`. Set it to `false` in initialization options for
bundled compatibility data without network requests. The separate
`svg.svgwg_drift_check` is opt-in and also needs to stay `false` for offline
sessions. See
[editor configuration](../crates/svg-language-server/README.md#configuration).
This documents the actual default; selecting a different hover layout does not
enable or disable networking.

At startup, each compatibility source resolves its latest package version and
then downloads that exact version. While that work is pending, the bundled
catalog supplies the answers. The server performs one refresh per session; there
is no periodic or previous-session cache. The hover's `sources` section reports
each package version, URL, selected context, and outcome:

| Outcome          | What the editor uses                                                             |
| ---------------- | -------------------------------------------------------------------------------- |
| Loaded           | Facts from the identified refreshed source                                       |
| No data          | Successful source load found no applicable facts; clears that source's old facts |
| Unknown status   | Retains the raw unrecognized value, without inventing a Baseline tier            |
| Refresh failed   | Bundled facts from that source, explicitly marked stale                          |
| Refresh disabled | Bundled facts, with their recorded source versions                               |

BCD and Web Features can succeed or fail independently. The hover identifies
each source separately, so refreshed browser details alongside a bundled
Baseline do not pretend to be one wholly fresh snapshot. Badges, dates, advice,
and browser notes all derive from the resulting effective facts.

The **dashboard and `svg-compat` CLI** use their pinned npm imports by default.
Their version controls/flags can request other versions or `latest`; the source
table and JSON identify the resolved versions. Both sources must load for a
response. An upstream error fails the request rather than returning the language
server's bundled fallback. Cached responses retain their source identity. The
response's generation time is not a Baseline milestone or a claim that the
source packages are the latest.

## Context and project advice

An attribute can differ between elements. The editor resolves the exact
element-and-attribute context first, then the global attribute key; genuinely
common bundled facts are a fallback where applicable. An explicit per-key Web
Features override is authoritative even when empty or unrecognized.

Dashboard attribute rows and CLI attribute counts are **project-derived
summaries of observed contexts**. They select the least favorable known tier and
the later matching milestone on ties. Their coverage counts report known,
unknown, and missing records separately. Expand “By element” or inspect
`attributes[name].contexts` in JSON for a particular use. A summary may combine
facts from different contexts; it is not an upstream status for the attribute
everywhere, and unlisted elements have no recorded coverage.

The editor's **Caution / use with care** and **Avoid** recommendations are `svg`
policy combining compatibility concerns with independent SVG profile facts.
Newly or Limited can prompt caution; they do not prove invalid SVG. Changing
browser preferences only changes hover presentation, not this policy. For the
exact structured fields and aggregation rules, see
[compatibility metadata v2](compat-metadata-v2.md).

## Artwork and further reading

The icons follow the
[official usage guidelines](https://web-platform-dx.github.io/name-and-logo-usage-guidelines/).
Their [source, license, and maintenance instructions](../THIRD_PARTY_NOTICES.md)
are recorded separately from code licensing. Both light and dark variants use
unmodified upstream artwork.

- [WebDX](https://web-platform-dx.github.io/)
- [Baseline overview and browser set](https://web-platform-dx.github.io/baseline/)
- [Reference Baseline status component](https://github.com/web-platform-dx/baseline-status)

The reference component also presents Web Features data; it does not
independently decide Baseline eligibility.
