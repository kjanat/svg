# SVG-LINT KNOWLEDGE BASE

## OVERVIEW

Structural SVG diagnostics crate. Walks parsed trees, applies suppression
directives, and reports byte-accurate diagnostics for the LSP.

## WHERE TO LOOK

| Task                         | Location                    | Notes                                           |
| ---------------------------- | --------------------------- | ----------------------------------------------- |
| Public lint entrypoints      | `src/lib.rs`                | `lint`, `lint_tree`                             |
| Rule engine / tree walk      | `src/rules/mod.rs`          | Suppression collection + element walk           |
| Suppression handling         | `src/rules/suppressions.rs` | File, next-line, and unused suppression parsing |
| Diagnostic model             | `src/types.rs`              | Codes, severity, payload                        |
| Suppression regression tests | `src/lib.rs`                | Semantic regression suite                       |
| Namespace resolution         | `src/namespaces.rs`         | Scope folding; `resolves_to_svg_namespace`      |

## CONVENTIONS

- Prefer `lint_tree` in callers that already own a parsed tree.
- The rule pipeline pre-collects suppressions and defined ids before walking
  elements.
- Foreign-namespace metadata elements (`{http://www.w3.org/1999/xhtml}link`,
  `{http://www.w3.org/1999/xhtml}meta`) resolve by namespace and are skipped
  from SVG catalog checks; they are deliberately absent from the `svg-data`
  catalog. A bare `link` or `meta` stays in the SVG namespace and is flagged
  unknown.
- Foreign-namespace content under `foreignObject` is exempt from normal SVG
  child checks.
- `resolves_to_svg_namespace` is the shared answer to "is this element SVG?" —
  the language server gates hover and completion on it, so keep it in step with
  the scope folding in `src/rules/mod.rs`.
- Messages and codes are user-facing contract; LSP and integration tests depend
  on them.

## ANTI-PATTERNS

- Do not emit `InvalidChild` for nodes already flagged as `UnknownElement`.
- Do not treat XML infrastructure attrs (`xmlns`, `xml:*`) as unknown SVG attrs.
- Do not narrow the attribute-name kind allowlist without checking other grammar
  consumers.
- Do not break file-level, next-line, or unused-suppression semantics.

## NOTES

- `src/lib.rs` tests are the main semantic regression suite.
- Missing-reference diagnostics depend on definition collection staying aligned
  with `svg-references`.
