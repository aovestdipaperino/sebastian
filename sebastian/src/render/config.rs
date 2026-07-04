//! `%%{init: ...}%%` directive parsing and the effective render config.

use serde_json::{Map, Value};

/// Effective configuration after applying init directives over defaults.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub theme: String,
    /// `look`: "classic" (default) or "handDrawn" (sketchy shapes +
    /// handwritten font). handDrawn is an opt-in stylization and is not
    /// byte-exact against mmdc (rough.js output is randomized upstream).
    pub look: String,
    /// Raw themeVariables overrides from the directive.
    pub theme_variables: Map<String, Value>,
    /// `flowchart.htmlLabels` (edge/cluster labels via getEffectiveHtmlLabels).
    pub flowchart_html_labels: Option<bool>,
    /// Top-level `htmlLabels` (node labels read only this).
    pub top_html_labels: Option<bool>,
    pub node_spacing: f64,
    pub rank_spacing: f64,
    pub padding: f64,
    pub wrapping_width: f64,
    pub curve: Option<String>,
    /// Computed theme variables (populated by the render entry points;
    /// consumed by shapes that need theme colors).
    pub computed_theme: Map<String, Value>,
    /// Graph direction (`data4Layout.direction`), consumed by forkJoin.
    pub direction: String,
    /// Edge-label font size override (ER labels inherit 14px from CSS).
    pub edge_label_font_size: Option<f64>,
    /// `gitGraph.showCommitLabel` (defaults to true when unset).
    pub git_show_commit_label: Option<bool>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_owned(),
            look: "classic".to_owned(),
            theme_variables: Map::new(),
            flowchart_html_labels: None,
            top_html_labels: None,
            node_spacing: 50.0,
            rank_spacing: 50.0,
            padding: 15.0,
            wrapping_width: 200.0,
            curve: None,
            computed_theme: Map::new(),
            direction: "TB".to_owned(),
            edge_label_font_size: None,
            git_show_commit_label: None,
        }
    }
}

impl RenderConfig {
    /// `getEffectiveHtmlLabels`: flowchart.htmlLabels ?? htmlLabels ?? true.
    #[must_use]
    pub fn effective_html_labels(&self) -> bool {
        self.flowchart_html_labels
            .or(self.top_html_labels)
            .unwrap_or(true)
    }

    /// labelHelper consults only the top-level htmlLabels.
    #[must_use]
    pub fn node_html_labels(&self) -> bool {
        self.top_html_labels.unwrap_or(true)
    }

    /// True when `look: handDrawn` selected the sketchy/handwritten style.
    #[must_use]
    pub fn is_hand_drawn(&self) -> bool {
        self.look == "handDrawn"
    }

    /// Label font size in px from themeVariables (default 16).
    #[must_use]
    pub fn font_size(&self) -> f64 {
        self.theme_variables
            .get("fontSize")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches("px").trim().parse().ok())
            .unwrap_or(16.0)
    }
}

/// Port of `utils.detectInit`: finds `%%{init: {...}}%%` /
/// `%%{initialize: ...}%%` directives and merges their args.
#[must_use]
pub fn detect_init(text: &str) -> RenderConfig {
    let mut config = RenderConfig::default();

    // detectDirective replaces all single quotes with double quotes first.
    let text = text.trim().replace('\'', "\"");

    let mut search = text.as_str();
    while let Some(start) = search.find("%%{") {
        let after = &search[start + 3..];
        let Some(end) = after.find("}%%") else { break };
        let body = &after[..end];
        search = &after[end + 3..];

        let Some(colon) = body.find(':') else {
            continue;
        };
        let directive_type = body[..colon].trim();
        if directive_type != "init" && directive_type != "initialize" {
            continue;
        }
        let args = body[colon + 1..].trim();
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(args) else {
            continue;
        };
        apply_init(&mut config, &map);
    }

    config
}

fn apply_init(config: &mut RenderConfig, map: &Map<String, Value>) {
    if let Some(theme) = map.get("theme").and_then(Value::as_str) {
        theme.clone_into(&mut config.theme);
    }
    if let Some(look) = map.get("look").and_then(Value::as_str) {
        look.clone_into(&mut config.look);
    }
    if let Some(Value::Object(vars)) = map.get("themeVariables") {
        for (k, v) in vars {
            config.theme_variables.insert(k.clone(), v.clone());
        }
    }
    if let Some(Value::Object(flow)) = map.get("flowchart") {
        if let Some(b) = flow.get("htmlLabels").and_then(Value::as_bool) {
            config.flowchart_html_labels = Some(b);
        }
        if let Some(n) = flow.get("nodeSpacing").and_then(Value::as_f64) {
            config.node_spacing = n;
        }
        if let Some(n) = flow.get("rankSpacing").and_then(Value::as_f64) {
            config.rank_spacing = n;
        }
        if let Some(n) = flow.get("padding").and_then(Value::as_f64) {
            config.padding = n;
        }
        if let Some(n) = flow.get("wrappingWidth").and_then(Value::as_f64) {
            config.wrapping_width = n;
        }
        if let Some(c) = flow.get("curve").and_then(Value::as_str) {
            config.curve = Some(c.to_owned());
        }
    }
    if let Some(Value::Object(git)) = map.get("gitGraph")
        && let Some(b) = git.get("showCommitLabel").and_then(Value::as_bool)
    {
        config.git_show_commit_label = Some(b);
    }
    // top-level htmlLabels also exists in mermaid config
    if let Some(b) = map.get("htmlLabels").and_then(Value::as_bool) {
        config.top_html_labels = Some(b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_directive() {
        let cfg = detect_init(
            "%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#ffcc00' }}}%%\ngraph TD\nA-->B\n",
        );
        assert_eq!(cfg.theme, "dark");
        assert_eq!(
            cfg.theme_variables
                .get("primaryColor")
                .and_then(Value::as_str),
            Some("#ffcc00")
        );
    }

    #[test]
    fn parses_html_labels() {
        let cfg =
            detect_init("%%{init: {\"flowchart\": {\"htmlLabels\": false}} }%%\ngraph TD\nA\n");
        assert!(!cfg.effective_html_labels());
        assert!(cfg.node_html_labels());
    }

    #[test]
    fn parses_font_size() {
        let cfg = detect_init(
            "%%{init: {'theme':'base', 'themeVariables': {'fontSize':'18px'}}}%%\ngraph TD\nA\n",
        );
        assert_eq!(cfg.theme, "base");
        assert_eq!(cfg.font_size(), 18.0);
    }
}
