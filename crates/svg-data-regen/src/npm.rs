//! Resolve an npm package to a pinned [`CatalogPackageSource`].

use serde_json::Value;

use crate::catalog::CatalogPackageSource;
use crate::util::boxed;
use crate::{Fallible, fetch};

/// Build a [`CatalogPackageSource`] using the recorded version when requested,
/// otherwise the npm `latest` dist-tag resolved at run time.
pub fn package_source(package: &str, path: &str) -> Fallible<CatalogPackageSource> {
    let version = if std::env::args().any(|arg| arg == "--recorded-packages") {
        recorded_version(package)?
    } else {
        npm_latest_version(package)?
    };
    let url = format!("https://unpkg.com/{package}@{version}/{path}");
    Ok(CatalogPackageSource {
        name: package.to_owned(),
        version,
        url,
    })
}

/// Resolve the npm `latest` dist-tag for `package`.
///
/// Use `--recorded-packages` to reuse committed package versions during a
/// generator change without upgrading unrelated compatibility/grammar inputs.
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

fn recorded_version(package: &str) -> Fallible<String> {
    let (file, pointer) = match package {
        "@mdn/browser-compat-data" => ("catalog.compat.json", "/browser_compat_data"),
        "web-features" => ("catalog.compat.json", "/web_features"),
        "@webref/css" => ("catalog.tree-sitter.json", "/sources/webref_css"),
        _ => return Err(boxed(format!("no recorded package source for {package}"))),
    };
    let json: Value = serde_json::from_str(&std::fs::read_to_string(
        crate::catalog_data_dir()?.join(file),
    )?)?;
    let source = json
        .pointer(pointer)
        .ok_or_else(|| boxed(format!("{file}: missing {pointer}")))?;
    if source["name"].as_str() != Some(package) {
        return Err(boxed(format!("{file}{pointer}: package name mismatch")));
    }
    source["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| boxed(format!("{file}{pointer}: missing version")))
}
