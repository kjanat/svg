# DOCS KNOWLEDGE BASE

## OVERVIEW

Dated design history captures intent, scope, and verification for major
features. Active implementation work is tracked in GitHub issues.

## STRUCTURE

```text
docs/
└── specs/  # architecture, scope, testing, non-goals
```

## WHERE TO LOOK

| Task                        | Location                      | Notes                                      |
| --------------------------- | ----------------------------- | ------------------------------------------ |
| Track implementation work   | GitHub issues                 | Current scope and acceptance criteria      |
| Recover architecture intent | `specs/*.md`                  | Goal, constraints, non-goals               |
| Find verification commands  | `../justfile`, `releasing.md` | Current development and release procedures |

## CONVENTIONS

- Filenames are date-prefixed: `YYYY-MM-DD-*`.
- Specs describe goal, architecture, testing, and non-goals before
  implementation.
- Keep unfinished work in GitHub issues. Retire obsolete implementation
  checklists after preserving any remaining requirements there.
- Verification sections are historical guidance; current command truth still
  lives in `justfile`.

## ANTI-PATTERNS

- Check the relevant design spec before changing documented requirements.
- Do not assume docs still match code; verify against source when behavior
  changed.
- Do not treat historical snippets as copy-paste safe without checking current
  APIs and file paths.

## NOTES

- Specs retain the original decisions for features ranging from the initial
  color-only LSP to compatibility data and release verification. Their dated
  scope may have been superseded; verify it against current code.
