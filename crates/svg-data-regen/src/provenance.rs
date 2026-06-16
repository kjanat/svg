//! Typed provenance for a regeneration run.
//!
//! Captures the exact upstream commit a run fetched plus the permalink base
//! URLs distilled from `publish.xml`. This is what later phases stamp into the
//! derived data so every fact can be traced to a pinned commit and minted into
//! a permalink.

use crate::discover::VersionLink;

/// The permalink base URLs a regeneration derives from.
///
/// Distilled from `publish.xml`'s `<versions>` block into the specific editions
/// permalinks point at. Each is optional because not every edition's manifest
/// declares every alias.
#[derive(Debug, Clone)]
pub struct BaseUrls {
    /// Rolling editor's-draft render (`<cvs>`): the GitHub-pages base.
    pub editors_draft: Option<String>,
    /// Current dated `/TR/` edition (`<this>`).
    pub dated: Option<String>,
    /// Version-less `/TR/` draft alias (`<latest>`).
    pub latest: Option<String>,
    /// Version-less `/TR/` REC alias (`<latestrec>`).
    pub latest_rec: Option<String>,
}

impl BaseUrls {
    /// Distil the typed permalink bases from the raw `<versions>` links.
    pub fn from_links(links: &[VersionLink]) -> Self {
        let find = |name: &str| {
            links
                .iter()
                .find(|link| link.name == name)
                .map(|link| link.href.clone())
        };
        Self {
            editors_draft: find("cvs"),
            dated: find("this"),
            latest: find("latest"),
            latest_rec: find("latestrec"),
        }
    }
}

/// The exact upstream snapshot a regeneration fetched, plus permalink bases.
///
/// Everything needed to reproduce the run (slug + ref + SHA) and to mint
/// permalinks (the dated/draft base URLs and the commit date).
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
    /// The permalink base URLs declared by the manifest.
    pub base_urls: BaseUrls,
}
