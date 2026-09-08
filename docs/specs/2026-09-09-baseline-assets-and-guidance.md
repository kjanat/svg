# Baseline assets and user guidance

Issues: [#36](https://github.com/kjanat/svg/issues/36),
[#37](https://github.com/kjanat/svg/issues/37).

## Goal

Use canonical Baseline artwork and explain the compatibility information where
people encounter it: the README, editor documentation, dashboard, and CLI.

## Design

- Vendor the six official light/dark status icons, without byte changes, from a
  pinned revision of the WebDX website. Keep the canonical files inside the
  language-server crate so Cargo packages remain self-contained.
- Record the upstream revision and SHA-256 hashes. Generate identical worker
  copies with an explicit synchronization command. Offline verification rejects
  changed originals, stale copies, missing files, and unexpected SVGs.
- The dashboard selects an official variant with `picture`. The LSP embeds the
  unmodified images inside an 18 by 10 SVG container; the container chooses the
  theme and preserves the original aspect ratio. No logo paths or colors change.
- Keep license and attribution beside the packaged assets and in the root
  notices. Prevent Git newline conversion and formatter changes to vendored
  SVGs.
- Explain upstream statuses, optional milestones, source versions, actual
  startup-refresh defaults, source-specific fallback, contextual facts,
  attribute summaries, project advice, and the browser overview's limits.

## Verification

Compare all six assets with the pinned upstream bytes. Check synchronization
offline in `just verify` and CI. Test status-to-icon mapping, theme selection,
neutral unknown states, discouragement, static serving, and the LSP container's
size and embedded bytes. Inspect light/dark rendering, check Cargo package
contents, run the repository preflight, and wait for PR checks.

## Non-goals

No new Baseline algorithm, metadata schema, browser-selection setting, release,
or changes to PR #47. Issue #45 remains separate release runtime coverage work.
