//! The SVG 2 source manifest: which files at a `w3c/svgwg` commit make up the
//! editor's draft.
//!
//! SVG 2 is built with Bikeshed (see [`crate::bikeshed`]). Three files at the
//! pinned commit describe the build, and together they replace the
//! `publish.xml` manifest of the old HTML build:
//!
//! - [`tools/svg2-build/config/svg2.json`][config]: the source directory, the
//!   maturity, the definitions files (with each external module's anchor base)
//!   and the editor's-draft base URL the chapter pages are published under.
//! - [`tools/svg2-build/config/svg2-split.json`][split]: the chapter pages in
//!   order, and which of them are appendices.
//! - [`master/index.bs`][index]: the Bikeshed metadata (status, the `/TR/`
//!   alias) and the `include` of every chapter source, checked against the
//!   page list so a chapter added on only one side fails the run.
//!
//! [config]: https://github.com/w3c/svgwg/blob/main/tools/svg2-build/config/svg2.json
//! [split]: https://github.com/w3c/svgwg/blob/main/tools/svg2-build/config/svg2-split.json
//! [index]: https://github.com/w3c/svgwg/blob/main/master/index.bs

use serde::Deserialize;

use crate::Fallible;
use crate::util::boxed;

/// Upstream repository the catalog derives from.
pub const REPO_SLUG: &str = "w3c/svgwg";
/// Canonical browse URL for the repository (recorded in provenance).
pub const REPO_URL: &str = "https://github.com/w3c/svgwg";
/// The build configuration: sources, definitions and the published base.
const CONFIG_PATH: &str = "tools/svg2-build/config/svg2.json";
/// The chapter page list.
const SPLIT_PATH: &str = "tools/svg2-build/config/svg2-split.json";
/// The Bikeshed root document, relative to the source directory.
const INDEX_FILE: &str = "index.bs";

/// One definitions file the element and attribute facts come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionsModule {
    /// Path from the repository root (`master/definitions.xml`).
    pub path: String,
    /// The anchor base of a module defined in another spec (a CSS draft or an
    /// SVG module), when its definitions do not live in SVG 2 itself.
    pub base: Option<String>,
}

/// The SVG 2 inputs at one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Svg2Sources {
    /// Maturity of the draft (`ED`).
    pub maturity: String,
    /// Base URL the chapter pages are published under; permalinks for facts
    /// defined in SVG 2 itself are minted against it.
    pub editors_draft: String,
    /// The version-less `/TR/` alias (`TR:` metadata), when declared.
    pub latest: Option<String>,
    /// Directory holding the chapter sources (`master`).
    pub source_dir: String,
    /// Definitions files, in build order.
    pub definitions: Vec<DefinitionsModule>,
    /// Chapter page names, in order (`intro`, `conform`, ...).
    pub chapters: Vec<String>,
    /// Appendix page names, in order (`idl`, ..., `changes`).
    pub appendices: Vec<String>,
}

#[derive(Deserialize)]
struct Config {
    source: ConfigSource,
    published_base: String,
}

#[derive(Deserialize)]
struct ConfigSource {
    dir: String,
    maturity: String,
    definitions: Vec<ConfigDefinitions>,
}

#[derive(Deserialize)]
struct ConfigDefinitions {
    href: String,
    base: Option<String>,
}

#[derive(Deserialize)]
struct Split {
    pages: Vec<SplitPage>,
}

#[derive(Deserialize)]
struct SplitPage {
    name: String,
    appendix: bool,
}

impl Svg2Sources {
    /// Fetch and parse the manifest files at `commit`.
    ///
    /// # Errors
    /// Returns an error when a file cannot be fetched or fails [`Self::parse`].
    pub fn fetch(commit: &str) -> Fallible<Self> {
        let config = crate::fetch::raw_file(REPO_SLUG, commit, CONFIG_PATH)?;
        let split = crate::fetch::raw_file(REPO_SLUG, commit, SPLIT_PATH)?;
        let dir = serde_json::from_str::<Config>(&config)
            .map_err(|error| boxed(format!("{CONFIG_PATH}: {error}")))?
            .source
            .dir;
        let index = crate::fetch::raw_file(REPO_SLUG, commit, &format!("{dir}/{INDEX_FILE}"))?;
        Self::parse(&config, &split, &index)
    }

    /// Parse the manifest from the build configuration, the page list and the
    /// root document.
    ///
    /// # Errors
    /// Returns an error when a file is malformed, a required section is empty,
    /// the root document's status disagrees with the configured maturity, or
    /// the page list and the root document's includes differ.
    pub fn parse(config: &str, split: &str, index: &str) -> Fallible<Self> {
        let config: Config = serde_json::from_str(config)
            .map_err(|error| boxed(format!("{CONFIG_PATH}: {error}")))?;
        let split: Split =
            serde_json::from_str(split).map_err(|error| boxed(format!("{SPLIT_PATH}: {error}")))?;
        let metadata = Metadata::parse(index);

        if config.source.definitions.is_empty() {
            return Err(boxed(format!("{CONFIG_PATH}: no definitions files listed")));
        }
        if split.pages.is_empty() {
            return Err(boxed(format!("{SPLIT_PATH}: no pages listed")));
        }
        if let Some(status) = &metadata.status
            && status != &config.source.maturity
        {
            return Err(boxed(format!(
                "{INDEX_FILE} Status `{status}` disagrees with {CONFIG_PATH} maturity `{}`",
                config.source.maturity
            )));
        }
        let pages: Vec<&str> = split.pages.iter().map(|page| page.name.as_str()).collect();
        let included: Vec<&str> = metadata
            .includes
            .iter()
            .filter_map(|path| path.strip_suffix(".bs"))
            .collect();
        if pages != included {
            return Err(boxed(format!(
                "{SPLIT_PATH} pages {pages:?} differ from the chapters {INDEX_FILE} includes \
                 {included:?}"
            )));
        }

        let (appendices, chapters): (Vec<_>, Vec<_>) =
            split.pages.into_iter().partition(|page| page.appendix);
        Ok(Self {
            maturity: config.source.maturity,
            editors_draft: config.published_base,
            latest: metadata.tr,
            source_dir: config.source.dir,
            definitions: config
                .source
                .definitions
                .into_iter()
                .map(|module| DefinitionsModule {
                    path: module.href,
                    base: module.base,
                })
                .collect(),
            chapters: chapters.into_iter().map(|page| page.name).collect(),
            appendices: appendices.into_iter().map(|page| page.name).collect(),
        })
    }

    /// Repository path of a page's Bikeshed source.
    #[must_use]
    pub fn page_path(&self, name: &str) -> String {
        format!("{}/{name}.bs", self.source_dir)
    }

    /// Published URL of a page on the editor's draft.
    #[must_use]
    pub fn page_url(&self, name: &str) -> String {
        format!("{}{name}.html", self.editors_draft)
    }

    /// Every page: the chapters, then the appendices.
    pub fn pages(&self) -> impl Iterator<Item = &str> {
        self.chapters
            .iter()
            .chain(&self.appendices)
            .map(String::as_str)
    }
}

/// What the run reads from `index.bs`.
struct Metadata {
    /// `Status:` (`ED`).
    status: Option<String>,
    /// `TR:` (the version-less `/TR/` alias).
    tr: Option<String>,
    /// Paths of `<pre class=include>` blocks, in order.
    includes: Vec<String>,
}

impl Metadata {
    fn parse(index: &str) -> Self {
        let mut metadata = Self {
            status: None,
            tr: None,
            includes: Vec::new(),
        };
        let mut in_metadata = false;
        for line in index.lines() {
            let line = line.trim();
            if line.starts_with("<pre class=metadata")
                || line.starts_with("<pre class=\"metadata\"")
            {
                in_metadata = true;
                continue;
            }
            if in_metadata {
                if line.starts_with("</pre>") {
                    in_metadata = false;
                } else if let Some(value) = line.strip_prefix("Status:") {
                    metadata.status = Some(value.trim().to_owned());
                } else if let Some(value) = line.strip_prefix("TR:") {
                    metadata.tr = Some(value.trim().to_owned());
                }
                continue;
            }
            if let Some(rest) = line
                .strip_prefix("<pre class=include>")
                .or_else(|| line.strip_prefix("<pre class=\"include\">"))
                && let Some(path) = rest
                    .strip_suffix("</pre>")
                    .and_then(|body| body.trim().strip_prefix("path:"))
            {
                metadata.includes.push(path.trim().to_owned());
            }
        }
        metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
      "source": {
        "dir": "master",
        "maturity": "ED",
        "definitions": [
          {"href": "master/definitions.xml"},
          {"href": "specs/animations/master/definitions.xml",
           "base": "https://w3c.github.io/svgwg/specs/animations/"}
        ]
      },
      "published_base": "https://w3c.github.io/svgwg/svg2-draft/"
    }"#;
    const SPLIT: &str = r#"{"pages": [
      {"name": "intro", "appendix": false},
      {"name": "shapes", "appendix": false},
      {"name": "changes", "appendix": true}
    ]}"#;
    const INDEX: &str = "<pre class=metadata>\nTitle: SVG 2\nStatus: ED\n\
        TR: https://www.w3.org/TR/SVG2/\n</pre>\n<p>front</p>\n\
        <pre class=include>path: intro.bs</pre>\n<pre class=include>path: shapes.bs</pre>\n\
        <pre class=include>path: changes.bs</pre>\n";

    #[test]
    fn parses_the_build_manifest() -> Fallible<()> {
        let sources = Svg2Sources::parse(CONFIG, SPLIT, INDEX)?;
        assert_eq!(sources.maturity, "ED");
        assert_eq!(
            sources.editors_draft,
            "https://w3c.github.io/svgwg/svg2-draft/"
        );
        assert_eq!(
            sources.latest.as_deref(),
            Some("https://www.w3.org/TR/SVG2/")
        );
        assert_eq!(sources.chapters, ["intro", "shapes"]);
        assert_eq!(sources.appendices, ["changes"]);
        assert_eq!(sources.definitions.len(), 2);
        assert_eq!(sources.definitions[0].path, "master/definitions.xml");
        assert_eq!(sources.definitions[0].base, None);
        assert_eq!(
            sources.definitions[1].base.as_deref(),
            Some("https://w3c.github.io/svgwg/specs/animations/")
        );
        assert_eq!(sources.page_path("shapes"), "master/shapes.bs");
        assert_eq!(
            sources.page_url("shapes"),
            "https://w3c.github.io/svgwg/svg2-draft/shapes.html"
        );
        assert_eq!(
            sources.pages().collect::<Vec<_>>(),
            ["intro", "shapes", "changes"]
        );
        Ok(())
    }

    #[test]
    fn rejects_a_page_list_that_differs_from_the_includes() {
        let index = INDEX.replace("<pre class=include>path: shapes.bs</pre>\n", "");
        let Err(error) = Svg2Sources::parse(CONFIG, SPLIT, &index) else {
            panic!("a page missing from index.bs should fail");
        };
        assert!(
            error.to_string().contains("differ"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_status_that_disagrees_with_the_maturity() {
        let index = INDEX.replace("Status: ED", "Status: CR");
        let Err(error) = Svg2Sources::parse(CONFIG, SPLIT, &index) else {
            panic!("a status mismatch should fail");
        };
        assert!(
            error.to_string().contains("disagrees"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_missing_definitions() {
        let config = r#"{"source": {"dir": "master", "maturity": "ED", "definitions": []},
            "published_base": "https://x/"}"#;
        assert!(Svg2Sources::parse(config, SPLIT, INDEX).is_err());
    }
}
