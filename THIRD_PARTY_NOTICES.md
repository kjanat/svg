# Third-party notices

This repository vendors a small number of third-party assets under their own
license, separate from this project's [MIT license](./LICENSE).

## Baseline status icons

- **Files:**
  - `crates/svg-language-server/assets/baseline-high.svg` ("widely available")
  - `crates/svg-language-server/assets/baseline-low.svg` ("newly available")
  - `crates/svg-language-server/assets/baseline-limited.svg` ("limited
    availability")
  - `workers/svg-compat/static/badges/baseline-widely.svg`
  - `workers/svg-compat/static/badges/baseline-newly.svg`
  - `workers/svg-compat/static/badges/baseline-limited.svg`
- **Source:**
  [web-platform-dx/web-features](https://github.com/web-platform-dx/web-features)
  Baseline logos, distributed via
  <https://web-platform-dx.github.io/name-and-logo-usage-guidelines/>
- **Copyright:** Google LLC. "Baseline" and its logos are trademarks of Google.
- **License:**
  [Creative Commons Attribution-NoDerivatives 4.0 International (CC BY-ND
  4.0)](https://creativecommons.org/licenses/by-nd/4.0/)
- **Notes:** The guidelines prohibit modifying, distorting, or recoloring the
  logos — the colors themselves indicate Baseline support level. The two
  vendored copies above are pixel-equivalent (same paths/colors, minified
  differently for each build target); do not hand-edit one without checking the
  other stays visually identical. See
  [issue #36](https://github.com/kjanat/svg/issues/36) for open follow-up work
  on this (re-vendoring from the canonical source, deduplicating the two
  copies).
