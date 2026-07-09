//! Resolve an npm package to a pinned [`CatalogPackageSource`].

use serde_json::Value;

use crate::catalog::CatalogPackageSource;
use crate::util::boxed;
use crate::{Fallible, fetch};

/// Build a [`CatalogPackageSource`] for `package`'s `path`, pinned to the npm
/// `latest` dist-tag resolved at run time.
pub fn package_source(package: &str, path: &str) -> Fallible<CatalogPackageSource> {
    let version = npm_latest_version(package)?;
    let url = format!("https://unpkg.com/{package}@{version}/{path}");
    Ok(CatalogPackageSource {
        name: package.to_owned(),
        version,
        url,
    })
}

/// Resolve the npm `latest` dist-tag for `package`.
///
/// NOTE: this makes regeneration output depend on whatever the package is
/// "latest" at run time, so two runs on different days can differ. The crate has
/// no version-pinning convention; the resolved version is recorded into the
/// catalog's package provenance so a given committed catalog stays reproducible
/// from its own metadata.
/// TODO: pin the npm sources to an explicit version if fully deterministic
/// regeneration from a clean checkout is required.
fn npm_latest_version(package: &str) -> Fallible<String> {
    let registry_package = package.replace('/', "%2f");
    let url = format!("https://registry.npmjs.org/{registry_package}");
    let json: Value = serde_json::from_str(&fetch::url_text(&url, "application/json")?)?;
    let version = json
        .pointer("/dist-tags/latest")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("npm package metadata missing dist-tags.latest"))?;
    Ok(version.to_owned())
}
