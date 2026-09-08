//! Presentation-only hover preferences. Compatibility storage and diagnostics are independent.

use serde::Deserialize;

/// Independently selectable hover blocks, in their normal display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Description,
    Status,
    Values,
    Baseline,
    Discouraged,
    Browsers,
    BrowserDetails,
    WebFeaturesSupport,
    Sources,
    Links,
}

/// Individual fields within the browser details section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDetail {
    Notes,
    PartialImplementation,
    Prefix,
    AlternativeName,
    Flags,
    VersionRemoved,
    VersionLast,
    ImplementationLinks,
}

/// Settings under `svg.hover`. Browser IDs are the identifiers published by BCD.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HoverSettings {
    pub browsers: Vec<String>,
    pub sections: Vec<Section>,
    pub browser_details: Vec<BrowserDetail>,
    pub browser_history: bool,
}

impl Default for HoverSettings {
    fn default() -> Self {
        Self {
            browsers: ["chrome", "edge", "firefox", "safari"]
                .map(str::to_owned)
                .to_vec(),
            sections: vec![
                Section::Description,
                Section::Status,
                Section::Values,
                Section::Baseline,
                Section::Discouraged,
                Section::Browsers,
                Section::BrowserDetails,
                Section::Sources,
                Section::Links,
            ],
            browser_details: vec![
                BrowserDetail::Notes,
                BrowserDetail::PartialImplementation,
                BrowserDetail::Prefix,
                BrowserDetail::AlternativeName,
                BrowserDetail::Flags,
                BrowserDetail::VersionRemoved,
            ],
            browser_history: false,
        }
    }
}

impl HoverSettings {
    pub fn shows(&self, section: Section) -> bool {
        self.sections.contains(&section)
    }

    pub fn from_config(config: &serde_json::Value) -> Result<Self, serde_json::Error> {
        let value = config.get("svg").unwrap_or(config).get("hover");
        let mut settings: Self = value.map_or_else(
            || Ok(Self::default()),
            |v| serde_json::from_value(v.clone()),
        )?;
        // Preserve requested order, without repeating products.
        let mut seen = std::collections::HashSet::new();
        settings.browsers.retain(|id| seen.insert(id.clone()));
        Ok(settings)
    }
}

pub fn browser_label(id: &str) -> &str {
    match id {
        "chrome" => "Chrome",
        "chrome_android" => "Chrome for Android",
        "edge" => "Edge",
        "firefox" => "Firefox",
        "firefox_android" => "Firefox for Android",
        "safari" => "Safari",
        "safari_ios" => "Safari on iOS",
        "ie" => "Internet Explorer",
        "opera" => "Opera",
        "opera_android" => "Opera for Android",
        "samsunginternet_android" => "Samsung Internet for Android",
        "webview_android" => "Android WebView",
        "webview_ios" => "iOS WebView",
        "oculus" => "Meta Quest Browser",
        "bun" => "Bun",
        "deno" => "Deno",
        "nodejs" => "Node.js",
        _ => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_preferences_are_reported_instead_of_silently_ignored() {
        for hover in [
            serde_json::json!({"sections":["typo"]}),
            serde_json::json!({"browsers":"chrome"}),
            serde_json::json!({"browser_history":"yes"}),
            serde_json::json!({"template":"custom"}),
        ] {
            assert!(
                HoverSettings::from_config(&serde_json::json!({"svg":{"hover":hover}})).is_err()
            );
        }
    }
}
