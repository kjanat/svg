//! Migrate compatibility facts using the catalog's recorded package versions.
//! Specification-derived fields and source package versions remain unchanged.

use std::path::Path;

use serde_json::Value;

use crate::{
    Fallible,
    catalog::{CatalogCompatFacts, CatalogPackageSource},
    compat, schema, write_json,
};

pub fn run() -> Fallible<()> {
    let dir = crate::catalog_data_dir()?;
    let mut manifest = read(&dir.join("catalog.json"))?;
    let core_path = crate::resolve_data_ref_for_write(&dir, required(&manifest["core"], "href")?)?;
    let compat_path =
        crate::resolve_data_ref_for_write(&dir, required(&manifest["compat"], "href")?)?;
    let mut core = read(&core_path)?;
    let mut provenance = read(&compat_path)?;
    let facts = compat::fetch_compat_catalog_from_sources(
        source(&provenance["browser_compat_data"])?,
        source(&provenance["web_features"])?,
    )?;

    for element in core["elements"]
        .as_array_mut()
        .ok_or("catalog elements must be an array")?
    {
        let facts = facts.elements.get(required(element, "name")?);
        replace_facts(element, facts)?;
    }
    for attribute in core["attributes"]
        .as_array_mut()
        .ok_or("catalog attributes must be an array")?
    {
        let facts = facts.attributes.get(required(attribute, "name")?);
        replace_facts(
            attribute,
            facts.and_then(compat::CompatAttribute::common_facts),
        )?;
        if let Some(contexts) = attribute
            .get_mut("element_compat")
            .and_then(Value::as_array_mut)
        {
            for context in contexts {
                let facts =
                    facts.and_then(|facts| facts.element_facts.get(context["element"].as_str()?));
                replace_facts(context, facts)?;
            }
        }
    }
    if let Some(features) = provenance
        .get_mut("unmodeled_features")
        .and_then(Value::as_array_mut)
    {
        for feature in features {
            let key = required(feature, "compat_key")?;
            let facts = facts
                .provenance
                .unmodeled_features
                .iter()
                .find(|item| item.compat_key == key)
                .map(|item| &item.facts);
            replace_facts(feature, facts)?;
        }
    }

    // Advance every component's contract together, retaining reference paths and spec provenance.
    let mut unchanged = Vec::new();
    for key in ["graph", "tree_sitter"] {
        unchanged.push(required(&manifest[key], "href")?.to_owned());
    }
    for snapshot in manifest["snapshots"]
        .as_array()
        .ok_or("catalog snapshots must be an array")?
    {
        unchanged.push(required(snapshot, "href")?.to_owned());
    }
    let mut documents = vec![(core_path, core), (compat_path, provenance)];
    for href in unchanged {
        let path = crate::resolve_data_ref_for_write(&dir, &href)?;
        documents.push((path.clone(), read(&path)?));
    }
    manifest["schema_version"] = Value::from(schema::CATALOG_SCHEMA_VERSION);
    documents.push((dir.join("catalog.json"), manifest));
    for (path, mut value) in documents {
        value["schema_version"] = Value::from(schema::CATALOG_SCHEMA_VERSION);
        write_json(&path, &value)?;
    }
    for schema in schema::catalog_schema_documents()? {
        std::fs::write(dir.join(schema.file_name), schema.json)?;
    }
    println!(
        "Refreshed compatibility metadata from the recorded package versions (schema v{}).",
        schema::CATALOG_SCHEMA_VERSION
    );
    Ok(())
}

fn read(path: &Path) -> Fallible<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn required<'a>(value: &'a Value, key: &str) -> Fallible<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field: {key}").into())
}

fn source(value: &Value) -> Fallible<CatalogPackageSource> {
    Ok(CatalogPackageSource {
        name: required(value, "name")?.to_owned(),
        version: required(value, "version")?.to_owned(),
        url: required(value, "url")?.to_owned(),
    })
}

fn replace_facts(value: &mut Value, facts: Option<&CatalogCompatFacts>) -> Fallible<()> {
    let object = value
        .as_object_mut()
        .ok_or("catalog fact must be an object")?;
    if let Some(support) = facts.and_then(|facts| facts.browser_support.as_ref()) {
        object.insert("browser_support".to_owned(), serde_json::to_value(support)?);
    } else {
        object.shift_remove("browser_support");
    }
    if let Some(baseline) = facts.and_then(|facts| facts.baseline.as_ref()) {
        object.insert("baseline".to_owned(), serde_json::to_value(baseline)?);
    } else {
        object.shift_remove("baseline");
    }
    if let Some(facts) = facts.filter(|facts| !facts.discouraged.is_empty()) {
        object.insert(
            "discouraged".to_owned(),
            serde_json::to_value(&facts.discouraged)?,
        );
    } else {
        object.shift_remove("discouraged");
    }
    Ok(())
}
