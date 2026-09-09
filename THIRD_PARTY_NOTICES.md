# Third-party notices

Vendored third-party artwork uses its own license, separate from this project's
[MIT license](./LICENSE).

## Baseline status icons

- **Canonical files:** the six `baseline-*-icon*.svg` files in
  [`crates/svg-language-server/assets/`](crates/svg-language-server/assets/BASELINE.md).
- **Generated copies:** the same filenames in
  `workers/svg-compat/static/badges/`, copied byte-for-byte from those
  originals.
- **Source:** the SVG downloads linked from
  [WebDX's official usage guidelines](https://web-platform-dx.github.io/name-and-logo-usage-guidelines/),
  vendored from
  [website revision `fc968a06896869a3ec9a11f471507922c90ada45`](https://github.com/web-platform-dx/web-platform-dx.github.io/tree/fc968a06896869a3ec9a11f471507922c90ada45/src/assets/img).
  The adjacent `baseline-icons.json` records the revision and SHA-256 hashes.
- **Copyright and trademarks:** Google LLC. Baseline and its logos are
  trademarks of Google. Their use does not imply sponsorship or endorsement.
- **License:**
  [Creative Commons Attribution-NoDerivatives 4.0 International (CC BY-ND 4.0)](https://creativecommons.org/licenses/by-nd/4.0/).
- **Modifications:** none. The official light and dark files retain their exact
  upstream bytes. Display containers select a variant and scale proportionally;
  they do not recolor, distort, or edit the logo geometry.

Use Widely Available for `"high"` (or Baseline as a concept), Newly Available
for `"low"`, and Limited availability for `false`. Missing/unrecognized status
gets no Baseline icon. Where WebDX discourages a feature, the default display
explains that advice instead of showing a badge.

The [asset maintenance guide](crates/svg-language-server/assets/BASELINE.md)
documents synchronization and verification. Never maintain separate artwork in
the worker or edit the SVGs to implement a theme.
