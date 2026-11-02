//! Typed provenance for a regeneration run.
//!
//! Captures the exact upstream commit a run fetched plus the permalink base
//! URLs the SVG 2 build declares (see [`crate::svgwg`]). This is what later
//! phases stamp into the derived data so every fact can be traced to a pinned
//! commit and minted into a permalink.

/// The exact upstream snapshot a regeneration fetched, plus permalink bases.
///
/// Everything needed to reproduce the run (slug + ref + SHA) and to mint
/// permalinks (the draft base URL and the commit date).
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Canonical browse URL of the upstream repository.
    pub repository: String,
    /// The ref that was resolved (branch name or an explicit SHA/tag).
    pub reference: String,
    /// The full commit SHA the run pinned to.
    pub commit_sha: String,
    /// The commit's committer date (the deterministic document date).
    pub commit_date: String,
    /// The spec maturity at that commit (`ED`, `CR`, ...).
    pub maturity: String,
    /// Base URL of the editor's-draft chapter pages (`published_base`).
    pub editors_draft: String,
    /// The version-less `/TR/` alias, when the build declares one.
    pub latest: Option<String>,
}
