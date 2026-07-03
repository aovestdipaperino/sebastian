//! Default-theme CSS emission.
//!
//! The stylesheet is the exact output of mermaid 11.15.0's `styles.ts` for
//! flowcharts with the default theme, captured from mmdc and parameterized by
//! the diagram id.

const TEMPLATE: &str = include_str!("default_theme.css");

/// CSS for a flowchart with the default theme, scoped to `#{id}`.
#[must_use]
pub fn flowchart_css(id: &str) -> String {
    TEMPLATE.replace("{ID}", &format!("#{id}"))
}

/// Converts a hex color to Chrome CSSOM's `rgb(r, g, b)` serialization.
fn hex_to_rgb(value: &str) -> Option<String> {
    let hex = value.strip_prefix('#')?;
    let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
    match digits.len() {
        3 => Some(format!(
            "rgb({}, {}, {})",
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17
        )),
        6 => Some(format!(
            "rgb({}, {}, {})",
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5]
        )),
        _ => None,
    }
}

const COLOR_PROPS: &[&str] = &["fill", "stroke", "color", "background-color", "bgFill"];

#[must_use]
pub fn cssom_color_value(prop: &str, value: &str) -> String {
    cssom_value(prop, value)
}

fn cssom_value(prop: &str, value: &str) -> String {
    if COLOR_PROPS.contains(&prop) {
        if let Some(rgb) = hex_to_rgb(value) {
            return rgb;
        }
        if let Some(rgb) = hsl_to_rgb(value) {
            return rgb;
        }
    }
    value.to_owned()
}

/// CSSOM serialization of `hsl(h, s%, l%)` strings as `rgb(r, g, b)`.
fn hsl_to_rgb(value: &str) -> Option<String> {
    let inner = value
        .trim()
        .strip_prefix("hsl(")
        .and_then(|v| v.strip_suffix(')'))?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].parse().ok()?;
    let sp: f64 = parts[1].strip_suffix('%')?.parse().ok()?;
    let lp: f64 = parts[2].strip_suffix('%')?.parse().ok()?;
    let s = sp / 100.0;
    let l = lp / 100.0;
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0f64.mul_add(l, -1.0)).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to8 = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0);
    Some(format!("rgb({}, {}, {})", to8(r1), to8(g1), to8(b1)))
}

/// Font-family override applied to every label in `look: handDrawn` mode, for
/// the handwritten/Excalidraw appearance. Uses a handwritten system-font stack
/// (Trebuchet metrics still drive layout, so node sizes are approximate).
#[must_use]
pub fn hand_drawn_font_css(id: &str) -> String {
    let font = "\"Comic Sans MS\", \"Chalkboard SE\", \"Bradley Hand\", cursive";
    format!(
        "#{id} .nodeLabel,#{id} .edgeLabel,#{id} .label text,#{id} span,#{id} p{{font-family:{font}!important;}}"
    )
}

/// classDef CSS (mermaidAPI `createCssStyles` + stylis serialization).
#[must_use]
pub fn class_defs_css(
    id: &str,
    html_labels: bool,
    classes: &[(String, Vec<String>, Vec<String>)],
) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    let format_decls = |styles: &[String]| -> String {
        styles
            .iter()
            .filter(|s| !s.trim().is_empty())
            .fold(String::new(), |mut decls, s| {
                let mut parts = s.splitn(2, ':');
                let prop = parts.next().unwrap_or("").trim();
                let value = parts.next().unwrap_or("").trim();
                let _ = write!(decls, "{prop}:{}!important;", cssom_value(prop, value));
                decls
            })
    };
    // createCssStyles: htmlLabels targets the foreignObject contents (`> *`
    // serializes as `>*` via CSSOM, plus `span`); without html labels the
    // styles target the SVG shape elements directly. The `>` is XML-escaped
    // later, but our style text is escaped during serialization, so emit the
    // raw `>` here.
    let elements: &[&str] = if html_labels {
        &[">*", " span"]
    } else {
        &[" rect", " polygon", " ellipse", " circle", " path"]
    };
    for (name, styles, text_styles) in classes {
        if !styles.is_empty() {
            let decls = format_decls(styles);
            for element in elements {
                let _ = write!(out, "#{id} .{name}{element}{{{decls}}}");
            }
        }
        if !text_styles.is_empty() {
            // mermaidAPI maps `color` to `fill` for tspan rules.
            let mapped: Vec<String> = text_styles
                .iter()
                .map(|s| s.replacen("color", "fill", 1))
                .collect();
            let decls = format_decls(&mapped);
            let _ = write!(out, "#{id} .{name} tspan{{{decls}}}");
        }
    }
    out
}

use serde_json::Map;
use serde_json::Value;

/// `fade(color, 0.5)` from flowchart styles.ts.
fn fade(color: &str, opacity: f64) -> String {
    let r = super::khroma::channel(color, 'r');
    let g = super::khroma::channel(color, 'g');
    let b = super::khroma::channel(color, 'b');
    super::khroma::rgba(r, g, b, opacity)
}

/// Generates the full flowchart stylesheet for the computed theme variables,
/// matching mermaid's stylis-compiled output byte for byte.
#[allow(clippy::too_many_lines)]
/// Emits one stylis-compiled rule (comma-space minification outside parens).
fn rule(o: &mut String, i: &str, selector_suffixes: &[&str], decls: &str) {
    let mut minified = String::with_capacity(decls.len());
    let mut depth = 0usize;
    let mut after_comma = false;
    for c in decls.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if after_comma && c == ' ' && depth == 0 {
            continue;
        }
        after_comma = c == ',';
        minified.push(c);
    }
    let sel: Vec<String> = selector_suffixes
        .iter()
        .map(|s| format!("{i}{s}"))
        .collect();
    let _ = std::fmt::Write::write_fmt(o, format_args!("{}{{{minified}}}", sel.join(",")));
}

/// The diagram-independent stylesheet head (font, error, edge thickness,
/// markers, svg/p resets).
fn css_prefix(o: &mut String, i: &str, vars: &Map<String, Value>) {
    let v = |key: &str| super::themes::get(vars, key);
    let font_family = v("fontFamily");
    let font_size = v("fontSize");
    let text_color = v("textColor");
    let error_bkg = v("errorBkgColor");
    let error_text = v("errorTextColor");
    let line_color = v("lineColor");
    let sw = stroke_width(vars);
    rule(
        o,
        i,
        &[""],
        &format!("font-family:{font_family};font-size:{font_size};fill:{text_color};"),
    );
    *o += "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}";
    *o += "@keyframes dash{to{stroke-dashoffset:0;}}";
    rule(
        o,
        i,
        &[" .edge-animation-slow"],
        "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;",
    );
    rule(
        o,
        i,
        &[" .edge-animation-fast"],
        "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;",
    );
    rule(o, i, &[" .error-icon"], &format!("fill:{error_bkg};"));
    rule(
        o,
        i,
        &[" .error-text"],
        &format!("fill:{error_text};stroke:{error_text};"),
    );
    rule(
        o,
        i,
        &[" .edge-thickness-normal"],
        &format!("stroke-width:{sw}px;"),
    );
    rule(o, i, &[" .edge-thickness-thick"], "stroke-width:3.5px;");
    rule(o, i, &[" .edge-pattern-solid"], "stroke-dasharray:0;");
    rule(
        o,
        i,
        &[" .edge-thickness-invisible"],
        "stroke-width:0;fill:none;",
    );
    rule(o, i, &[" .edge-pattern-dashed"], "stroke-dasharray:3;");
    rule(o, i, &[" .edge-pattern-dotted"], "stroke-dasharray:2;");
    rule(
        o,
        i,
        &[" .marker"],
        &format!("fill:{line_color};stroke:{line_color};"),
    );
    rule(o, i, &[" .marker.cross"], &format!("stroke:{line_color};"));
    rule(
        o,
        i,
        &[" svg"],
        &format!("font-family:{font_family};font-size:{font_size};"),
    );
    rule(o, i, &[" p"], "margin:0;");
}

/// `strokeWidth || 1` from the theme variables.
fn stroke_width(vars: &Map<String, Value>) -> String {
    vars.get("strokeWidth")
        .and_then(Value::as_f64)
        .map_or_else(|| "1".to_owned(), crate::svg::js_num)
}

/// The shared neo/look tail (`.node .neo-node` through `:root`).
fn css_suffix(o: &mut String, i: &str, id: &str, vars: &Map<String, Value>) {
    let v = |key: &str| super::themes::get(vars, key);
    let font_family = v("fontFamily");
    let node_border = v("nodeBorder");
    let sw = stroke_width(vars);
    let use_gradient = super::themes::get_bool(vars, "useGradient");
    let drop_shadow = v("dropShadow");
    let neo_stroke = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        node_border.clone()
    };
    let neo_filter = if drop_shadow.is_empty() {
        "none".to_owned()
    } else {
        drop_shadow.clone()
    };
    rule(
        o,
        i,
        &[" .node .neo-node"],
        &format!("stroke:{node_border};"),
    );
    rule(
        o,
        i,
        &[
            " [data-look=\"neo\"].node rect",
            " [data-look=\"neo\"].cluster rect",
            " [data-look=\"neo\"].node polygon",
        ],
        &format!("stroke:{neo_stroke};filter:{neo_filter};"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].node path"],
        &format!("stroke:{neo_stroke};stroke-width:{sw}px;"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].node .outer-path"],
        &format!("filter:{neo_filter};"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].node .neo-line path"],
        &format!("stroke:{node_border};filter:none;"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].node circle"],
        &format!("stroke:{neo_stroke};filter:{neo_filter};"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].node circle .state-start"],
        "fill:#000000;",
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].icon-shape .icon"],
        &format!("fill:{neo_stroke};filter:{neo_filter};"),
    );
    rule(
        o,
        i,
        &[" [data-look=\"neo\"].icon-shape .icon-neo path"],
        &format!("stroke:{neo_stroke};filter:{neo_filter};"),
    );
    rule(
        o,
        i,
        &[" :root"],
        &format!("--mermaid-font-family:{font_family};"),
    );
}

/// The timeline stylesheet (port of `diagrams/timeline/styles.js`,
/// classic non-redux sections).
#[must_use]
pub fn themed_timeline_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(&mut o, &i, &[" .edge"], "stroke-width:3;");
    for n in 0..12i32 {
        let s = n - 1;
        let c_scale = v(&format!("cScale{n}"));
        let c_label = v(&format!("cScaleLabel{n}"));
        let c_inv = v(&format!("cScaleInv{n}"));
        let sw = 17 - 3 * n;
        rule(
            &mut o,
            &i,
            &[
                &format!(" .section-{s} rect"),
                &format!(" .section-{s} path"),
                &format!(" .section-{s} circle"),
                &format!(" .section-{s} path"),
            ],
            &format!("fill:{c_scale};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .section-{s} text")],
            &format!("fill:{c_label};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .node-icon-{s}")],
            &format!("font-size:40px;color:{c_label};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .section-edge-{s}")],
            &format!("stroke:{c_scale};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .edge-depth-{s}")],
            &format!("stroke-width:{sw};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .section-{s} line")],
            &format!("stroke:{c_inv};stroke-width:3;"),
        );
        rule(
            &mut o,
            &i,
            &[" .lineWrapper line"],
            &format!("stroke:{c_label};"),
        );
        rule(
            &mut o,
            &i,
            &[" .disabled", " .disabled circle", " .disabled text"],
            &format!("fill:{};", {
                let t = v("tertiaryColor");
                if t.is_empty() {
                    "lightgray".to_owned()
                } else {
                    t
                }
            }),
        );
        rule(
            &mut o,
            &i,
            &[" .disabled text"],
            &format!("fill:{};", {
                let c = v("clusterBorder");
                if c.is_empty() {
                    "#efefef".to_owned()
                } else {
                    c
                }
            }),
        );
    }
    rule(
        &mut o,
        &i,
        &[
            " .section-root rect",
            " .section-root path",
            " .section-root circle",
        ],
        &format!("fill:{};", v("git0")),
    );
    rule(
        &mut o,
        &i,
        &[" .section-root text"],
        &format!("fill:{};", v("gitBranchLabel0")),
    );
    rule(
        &mut o,
        &i,
        &[" .icon-container"],
        "height:100%;display:flex;justify-content:center;align-items:center;",
    );
    rule(&mut o, &i, &[" .edge"], "fill:none;");
    rule(&mut o, &i, &[" .eventWrapper"], "filter:brightness(120%);");
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The quadrantChart stylesheet: no diagram-specific rules.
#[must_use]
pub fn themed_quadrant_css(id: &str, vars: &Map<String, Value>) -> String {
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The user-journey stylesheet (port of `diagrams/user-journey/styles.js`).
#[must_use]
pub fn themed_journey_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    let font = v("fontFamily");
    let text_color = v("textColor");
    rule(
        &mut o,
        &i,
        &[" .label"],
        &format!("font-family:{font};color:{};", {
            let t = v("nodeTextColor");
            if t.is_empty() { text_color.clone() } else { t }
        }),
    );
    rule(&mut o, &i, &[" .mouth"], "stroke:#666;");
    rule(&mut o, &i, &[" line"], &format!("stroke:{text_color};"));
    rule(
        &mut o,
        &i,
        &[" .legend"],
        &format!("fill:{text_color};font-family:{font};"),
    );
    rule(&mut o, &i, &[" .label text"], &format!("fill:{};", "#333"));
    rule(
        &mut o,
        &i,
        &[" .label"],
        &format!("color:{};", {
            let t = v("nodeTextColor");
            if t.is_empty() { text_color.clone() } else { t }
        }),
    );
    rule(
        &mut o,
        &i,
        &[" .face"],
        &format!("fill:{};stroke:#999;", {
            let f = v("faceColor");
            if f.is_empty() {
                "#FFF8DC".to_owned()
            } else {
                f
            }
        }),
    );
    rule(
        &mut o,
        &i,
        &[
            " .node rect",
            " .node circle",
            " .node ellipse",
            " .node polygon",
            " .node path",
        ],
        &format!(
            "fill:{};stroke:{};stroke-width:1px;",
            v("mainBkg"),
            v("nodeBorder")
        ),
    );
    rule(&mut o, &i, &[" .node .label"], "text-align:center;");
    rule(&mut o, &i, &[" .node.clickable"], "cursor:pointer;");
    rule(
        &mut o,
        &i,
        &[" .arrowheadPath"],
        &format!("fill:{};", v("arrowheadColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .edgePath .path"],
        &format!("stroke:{};stroke-width:1.5px;", v("lineColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .flowchart-link"],
        &format!("stroke:{};fill:none;", v("lineColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel"],
        &format!(
            "background-color:{};text-align:center;",
            v("edgeLabelBackground")
        ),
    );
    rule(&mut o, &i, &[" .edgeLabel rect"], "opacity:0.5;");
    rule(
        &mut o,
        &i,
        &[" .cluster text"],
        &format!("fill:{};", v("titleColor")),
    );
    rule(
        &mut o,
        &i,
        &[" div.mermaidTooltip"],
        &format!(
            "position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{font};font-size:12px;background:{};border:1px solid {};border-radius:2px;pointer-events:none;z-index:100;",
            v("tertiaryColor"),
            v("border2")
        ),
    );
    for n in 0..8 {
        let fill = v(&format!("fillType{n}"));
        rule(
            &mut o,
            &i,
            &[&format!(" .task-type-{n}"), &format!(" .section-type-{n}")],
            &format!("fill:{fill};"),
        );
    }
    rule(
        &mut o,
        &i,
        &[" .label-icon"],
        "display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;",
    );
    rule(
        &mut o,
        &i,
        &[" .node .label-icon path"],
        "fill:currentColor;stroke:revert;stroke-width:revert;",
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The gitGraph stylesheet (port of `diagrams/git/styles.js`).
#[must_use]
pub fn themed_gitgraph_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    let font = v("fontFamily");
    rule(
        &mut o,
        &i,
        &[" .commit-id", " .commit-msg", " .branch-label"],
        "fill:lightgrey;color:lightgrey;font-family:'trebuchet ms',verdana,arial,sans-serif;font-family:var(--mermaid-font-family);",
    );
    let _ = font;
    for n in 0..12 {
        let git = v(&format!("git{}", n % 8));
        let inv = v(&format!("gitInv{}", n % 8));
        let label = v(&format!("gitBranchLabel{}", n % 8));
        rule(
            &mut o,
            &i,
            &[&format!(" .branch-label{n}")],
            &format!("fill:{label};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .commit{n}")],
            &format!("stroke:{git};fill:{git};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .commit-highlight{n}")],
            &format!("stroke:{inv};fill:{inv};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .label{n}")],
            &format!("fill:{git};"),
        );
        rule(
            &mut o,
            &i,
            &[&format!(" .arrow{n}")],
            &format!("stroke:{git};"),
        );
    }
    rule(
        &mut o,
        &i,
        &[" .branch"],
        &format!(
            "stroke-width:1;stroke:{};stroke-dasharray:2;",
            v("gitBranchLabelColor_line")
                .is_empty()
                .then_some(())
                .map_or_else(|| v("lineColor"), |()| v("lineColor"))
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .commit-label"],
        &format!("font-size:10px;fill:{};", v("commitLabelColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .commit-label-bkg"],
        &format!(
            "font-size:10px;fill:{};opacity:0.5;",
            v("commitLabelBackground")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .tag-label"],
        &format!("font-size:10px;fill:{};", v("tagLabelColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .tag-label-bkg"],
        &format!(
            "fill:{};stroke:{};",
            v("tagLabelBackground"),
            v("tagLabelBorder")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .tag-hole"],
        &format!("fill:{};", v("textColor")),
    );
    let main_bkg = v("mainBkg");
    rule(
        &mut o,
        &i,
        &[" .commit-merge"],
        &format!("stroke:{main_bkg};fill:{main_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .commit-reverse"],
        &format!("stroke:{main_bkg};fill:{main_bkg};stroke-width:3;"),
    );
    rule(
        &mut o,
        &i,
        &[" .commit-highlight-inner"],
        &format!("stroke:{main_bkg};fill:{main_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .arrow"],
        "stroke-width:8;stroke-linecap:round;fill:none;",
    );
    rule(
        &mut o,
        &i,
        &[" .gitTitleText"],
        &format!("text-anchor:middle;font-size:18px;fill:{};", v("textColor")),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The gantt stylesheet (port of `diagrams/gantt/styles.js`).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn themed_gantt_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    let font = v("fontFamily");
    let text_color = v("textColor");
    let section_bkg = v("sectionBkgColor");
    let alt_section_bkg = v("altSectionBkgColor");
    let section_bkg2 = v("sectionBkgColor2");
    let exclude_bkg = v("excludeBkgColor");
    let task_border = v("taskBorderColor");
    let task_bkg = v("taskBkgColor");
    let task_text = v("taskTextColor");
    let task_text_dark = v("taskTextDarkColor");
    let task_text_outside = v("taskTextOutsideColor");
    let task_text_clickable = v("taskTextClickableColor");
    let active_border = v("activeTaskBorderColor");
    let active_bkg = v("activeTaskBkgColor");
    let grid_color = v("gridColor");
    let done_bkg = v("doneTaskBkgColor");
    let done_border = v("doneTaskBorderColor");
    let crit_border = v("critBorderColor");
    let crit_bkg = v("critBkgColor");
    let today_line = v("todayLineColor");
    let vert_line = v("vertLineColor");
    let title_color = v("titleColor");

    rule(
        &mut o,
        &i,
        &[" .mermaid-main-font"],
        &format!("font-family:{font};"),
    );
    rule(
        &mut o,
        &i,
        &[" .exclude-range"],
        &format!("fill:{exclude_bkg};"),
    );
    rule(&mut o, &i, &[" .section"], "stroke:none;opacity:0.2;");
    rule(&mut o, &i, &[" .section0"], &format!("fill:{section_bkg};"));
    rule(
        &mut o,
        &i,
        &[" .section2"],
        &format!("fill:{section_bkg2};"),
    );
    rule(
        &mut o,
        &i,
        &[" .section1", " .section3"],
        &format!("fill:{alt_section_bkg};opacity:0.2;"),
    );
    for n in 0..4 {
        rule(
            &mut o,
            &i,
            &[&format!(" .sectionTitle{n}")],
            &format!("fill:{title_color};"),
        );
    }
    rule(
        &mut o,
        &i,
        &[" .sectionTitle"],
        &format!("text-anchor:start;font-family:{font};"),
    );
    rule(
        &mut o,
        &i,
        &[" .grid .tick"],
        &format!("stroke:{grid_color};opacity:0.8;shape-rendering:crispEdges;"),
    );
    rule(
        &mut o,
        &i,
        &[" .grid .tick text"],
        &format!("font-family:{font};fill:{text_color};"),
    );
    rule(&mut o, &i, &[" .grid path"], "stroke-width:0;");
    rule(
        &mut o,
        &i,
        &[" .today"],
        &format!("fill:none;stroke:{today_line};stroke-width:2px;"),
    );
    rule(&mut o, &i, &[" .task"], "stroke-width:2;");
    rule(
        &mut o,
        &i,
        &[" .taskText"],
        &format!("text-anchor:middle;font-family:{font};"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutsideRight"],
        &format!("fill:{task_text_dark};text-anchor:start;font-family:{font};"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutsideLeft"],
        &format!("fill:{task_text_dark};text-anchor:end;"),
    );
    rule(&mut o, &i, &[" .task.clickable"], "cursor:pointer;");
    rule(
        &mut o,
        &i,
        &[" .taskText.clickable"],
        &format!("cursor:pointer;fill:{task_text_clickable}!important;font-weight:bold;"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutsideLeft.clickable"],
        &format!("cursor:pointer;fill:{task_text_clickable}!important;font-weight:bold;"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutsideRight.clickable"],
        &format!("cursor:pointer;fill:{task_text_clickable}!important;font-weight:bold;"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskText0", " .taskText1", " .taskText2", " .taskText3"],
        &format!("fill:{task_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .task0", " .task1", " .task2", " .task3"],
        &format!("fill:{task_bkg};stroke:{task_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutside0", " .taskTextOutside2"],
        &format!("fill:{task_text_outside};"),
    );
    rule(
        &mut o,
        &i,
        &[" .taskTextOutside1", " .taskTextOutside3"],
        &format!("fill:{task_text_outside};"),
    );
    rule(
        &mut o,
        &i,
        &[" .active0", " .active1", " .active2", " .active3"],
        &format!("fill:{active_bkg};stroke:{active_border};"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .activeText0",
            " .activeText1",
            " .activeText2",
            " .activeText3",
        ],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[" .done0", " .done1", " .done2", " .done3"],
        &format!("stroke:{done_border};fill:{done_bkg};stroke-width:2;"),
    );
    rule(
        &mut o,
        &i,
        &[" .doneText0", " .doneText1", " .doneText2", " .doneText3"],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .doneText0.taskTextOutsideLeft",
            " .doneText0.taskTextOutsideRight",
            " .doneText1.taskTextOutsideLeft",
            " .doneText1.taskTextOutsideRight",
            " .doneText2.taskTextOutsideLeft",
            " .doneText2.taskTextOutsideRight",
            " .doneText3.taskTextOutsideLeft",
            " .doneText3.taskTextOutsideRight",
        ],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[" .crit0", " .crit1", " .crit2", " .crit3"],
        &format!("stroke:{crit_border};fill:{crit_bkg};stroke-width:2;"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .activeCrit0",
            " .activeCrit1",
            " .activeCrit2",
            " .activeCrit3",
        ],
        &format!("stroke:{crit_border};fill:{active_bkg};stroke-width:2;"),
    );
    rule(
        &mut o,
        &i,
        &[" .doneCrit0", " .doneCrit1", " .doneCrit2", " .doneCrit3"],
        &format!(
            "stroke:{crit_border};fill:{done_bkg};stroke-width:2;cursor:pointer;shape-rendering:crispEdges;"
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .milestone"],
        "transform:rotate(45deg) scale(0.8,0.8);",
    );
    rule(&mut o, &i, &[" .milestoneText"], "font-style:italic;");
    rule(
        &mut o,
        &i,
        &[
            " .doneCritText0",
            " .doneCritText1",
            " .doneCritText2",
            " .doneCritText3",
        ],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .doneCritText0.taskTextOutsideLeft",
            " .doneCritText0.taskTextOutsideRight",
            " .doneCritText1.taskTextOutsideLeft",
            " .doneCritText1.taskTextOutsideRight",
            " .doneCritText2.taskTextOutsideLeft",
            " .doneCritText2.taskTextOutsideRight",
            " .doneCritText3.taskTextOutsideLeft",
            " .doneCritText3.taskTextOutsideRight",
        ],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(&mut o, &i, &[" .vert"], &format!("stroke:{vert_line};"));
    rule(
        &mut o,
        &i,
        &[" .vertText"],
        &format!("font-size:15px;text-anchor:middle;fill:{vert_line}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .activeCritText0",
            " .activeCritText1",
            " .activeCritText2",
            " .activeCritText3",
        ],
        &format!("fill:{task_text_dark}!important;"),
    );
    rule(
        &mut o,
        &i,
        &[" .titleText"],
        &format!("text-anchor:middle;font-size:18px;fill:{title_color};font-family:{font};"),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The xychart stylesheet: no diagram-specific rules, just the shared
/// prefix and suffix.
#[must_use]
pub fn themed_xychart_css(id: &str, vars: &Map<String, Value>) -> String {
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The erDiagram stylesheet (port of `diagrams/er/styles.ts`).
#[must_use]
pub fn themed_er_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" .entityBox"],
        &format!("fill:{};stroke:{};", v("mainBkg"), v("nodeBorder")),
    );
    let tertiary = v("tertiaryColor");
    rule(
        &mut o,
        &i,
        &[" .relationshipLabelBox"],
        &format!("fill:{tertiary};opacity:0.7;background-color:{tertiary};"),
    );
    rule(&mut o, &i, &[" .relationshipLabelBox rect"], "opacity:0.5;");
    rule(
        &mut o,
        &i,
        &[" .labelBkg"],
        &format!("background-color:{};", fade(&tertiary, 0.5)),
    );
    let edge_label_bg = v("edgeLabelBackground");
    rule(
        &mut o,
        &i,
        &[" .edgeLabel"],
        &format!("background-color:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label rect"],
        &format!("fill:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label text"],
        &format!("fill:{};", v("textColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label"],
        &format!("fill:{};font-size:14px;", v("nodeBorder")),
    );
    let node_text = {
        let t = v("nodeTextColor");
        if t.is_empty() { v("textColor") } else { t }
    };
    rule(
        &mut o,
        &i,
        &[" .label"],
        &format!("font-family:{};color:{node_text};", v("fontFamily")),
    );
    rule(
        &mut o,
        &i,
        &[" .edge-pattern-dashed"],
        "stroke-dasharray:8,8;",
    );
    rule(
        &mut o,
        &i,
        &[
            " .node rect",
            " .node circle",
            " .node ellipse",
            " .node polygon",
        ],
        &format!(
            "fill:{};stroke:{};stroke-width:1px;",
            v("mainBkg"),
            v("nodeBorder")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .relationshipLine"],
        &format!("stroke:{};stroke-width:1px;fill:none;", v("lineColor")),
    );
    rule(
        &mut o,
        &i,
        &[" .marker"],
        &format!(
            "fill:none!important;stroke:{}!important;stroke-width:1;",
            v("lineColor")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" [data-look=neo].labelBkg"],
        &format!("background-color:{};", fade(&tertiary, 0.5)),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The pie chart stylesheet (port of `diagrams/pie/pieStyles.ts`).
#[must_use]
pub fn themed_pie_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" .pieCircle"],
        &format!(
            "stroke:{};stroke-width:{};opacity:{};",
            v("pieStrokeColor"),
            v("pieStrokeWidth"),
            v("pieOpacity")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .pieOuterCircle"],
        &format!(
            "stroke:{};stroke-width:{};fill:none;",
            v("pieOuterStrokeColor"),
            v("pieOuterStrokeWidth")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .pieTitleText"],
        &format!(
            "text-anchor:middle;font-size:{};fill:{};font-family:{};",
            v("pieTitleTextSize"),
            v("pieTitleTextColor"),
            v("fontFamily")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .slice"],
        &format!(
            "font-family:{};fill:{};font-size:{};",
            v("fontFamily"),
            v("pieSectionTextColor"),
            v("pieSectionTextSize")
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .legend text"],
        &format!(
            "fill:{};font-family:{};font-size:{};",
            v("pieLegendTextColor"),
            v("fontFamily"),
            v("pieLegendTextSize")
        ),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The class diagram stylesheet (port of `diagrams/class/styles.js`).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn themed_class_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let font_family = v("fontFamily");
    let node_border = v("nodeBorder");
    let text_color = v("textColor");
    let class_text = v("classText");
    let main_bkg = v("mainBkg");
    let cluster_bkg = v("clusterBkg");
    let cluster_border = v("clusterBorder");
    let line_color = v("lineColor");
    let note_text = v("noteTextColor");
    let edge_label_bg = v("edgeLabelBackground");

    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" g.classGroup text"],
        &format!("fill:{node_border};stroke:none;font-family:{font_family};font-size:10px;"),
    );
    rule(
        &mut o,
        &i,
        &[" g.classGroup text .title"],
        "font-weight:bolder;",
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label text"],
        &format!("fill:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label span"],
        &format!("color:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label span p"],
        "background-color:transparent;",
    );
    rule(
        &mut o,
        &i,
        &[" .cluster rect"],
        &format!("fill:{cluster_bkg};stroke:{cluster_border};stroke-width:1px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster text"],
        &format!("fill:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster span"],
        &format!("color:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .nodeLabel", " .edgeLabel"],
        &format!("color:{class_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .noteLabel .nodeLabel", " .noteLabel .edgeLabel"],
        &format!("color:{note_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label rect"],
        &format!("fill:{main_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .label text"],
        &format!("fill:{class_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .labelBkg"],
        &format!("background:{main_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label span"],
        &format!("background:{main_bkg};"),
    );
    rule(&mut o, &i, &[" .classTitle"], "font-weight:bolder;");
    rule(
        &mut o,
        &i,
        &[
            " .node rect",
            " .node circle",
            " .node ellipse",
            " .node polygon",
            " .node path",
        ],
        &format!("fill:{main_bkg};stroke:{node_border};stroke-width:1;"),
    );
    rule(
        &mut o,
        &i,
        &[" .divider"],
        &format!("stroke:{node_border};stroke-width:1;"),
    );
    rule(&mut o, &i, &[" g.clickable"], "cursor:pointer;");
    rule(
        &mut o,
        &i,
        &[" g.classGroup rect"],
        &format!("fill:{main_bkg};stroke:{node_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" g.classGroup line"],
        &format!("stroke:{node_border};stroke-width:1;"),
    );
    rule(
        &mut o,
        &i,
        &[" .classLabel .box"],
        &format!("stroke:none;stroke-width:0;fill:{main_bkg};opacity:0.5;"),
    );
    rule(
        &mut o,
        &i,
        &[" .classLabel .label"],
        &format!("fill:{node_border};font-size:10px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .relation"],
        &format!("stroke:{line_color};stroke-width:1;fill:none;"),
    );
    rule(&mut o, &i, &[" .dashed-line"], "stroke-dasharray:3;");
    rule(&mut o, &i, &[" .dotted-line"], "stroke-dasharray:1 2;");
    for marker in ["composition", "dependency"] {
        for end in ["Start", "End"] {
            rule(
                &mut o,
                &i,
                &[
                    &format!(" [id$=\"-{marker}{end}\"]"),
                    &format!(" .{marker}"),
                ],
                &format!(
                    "fill:{line_color}!important;stroke:{line_color}!important;stroke-width:1;"
                ),
            );
        }
    }
    for marker in ["extension", "aggregation"] {
        for end in ["Start", "End"] {
            rule(
                &mut o,
                &i,
                &[
                    &format!(" [id$=\"-{marker}{end}\"]"),
                    &format!(" .{marker}"),
                ],
                &format!(
                    "fill:transparent!important;stroke:{line_color}!important;stroke-width:1;"
                ),
            );
        }
    }
    for end in ["Start", "End"] {
        rule(
            &mut o,
            &i,
            &[&format!(" [id$=\"-lollipop{end}\"]"), " .lollipop"],
            &format!("fill:{main_bkg}!important;stroke:{line_color}!important;stroke-width:1;"),
        );
    }
    rule(
        &mut o,
        &i,
        &[" .edgeTerminals"],
        "font-size:11px;line-height:initial;",
    );
    rule(
        &mut o,
        &i,
        &[" .classTitleText"],
        &format!("text-anchor:middle;font-size:18px;fill:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel[data-look=\"neo\"]"],
        &format!("background-color:{edge_label_bg};text-align:center;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel[data-look=\"neo\"] p"],
        &format!("background-color:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel[data-look=\"neo\"] rect"],
        &format!("opacity:0.5;background-color:{edge_label_bg};fill:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .label-icon"],
        "display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;",
    );
    rule(
        &mut o,
        &i,
        &[" .node .label-icon path"],
        "fill:currentColor;stroke:revert;stroke-width:revert;",
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The sequence diagram stylesheet (port of `diagrams/sequence/styles.js`).
#[must_use]
pub fn themed_sequence_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let actor_border = v("actorBorder");
    let actor_bkg = v("actorBkg");
    let sw = stroke_width(vars);
    let drop_shadow = {
        let d = v("dropShadow");
        if d.is_empty() { "none".to_owned() } else { d }
    };
    let note_border = v("noteBorderColor");
    let note_bkg = v("noteBkgColor");
    let actor_text = v("actorTextColor");
    let actor_line = v("actorLineColor");
    let signal = v("signalColor");
    let signal_text = v("signalTextColor");
    let seq_num = v("sequenceNumberColor");
    let label_box_border = v("labelBoxBorderColor");
    let label_box_bkg = v("labelBoxBkgColor");
    let label_text = v("labelTextColor");
    let loop_text = v("loopTextColor");
    let note_text = v("noteTextColor");
    let note_font_weight = {
        let w = v("noteFontWeight");
        if w.is_empty() { "normal".to_owned() } else { w }
    };
    let activation_bkg = v("activationBkgColor");
    let activation_border = v("activationBorderColor");
    let node_border = v("nodeBorder");

    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" .actor"],
        &format!("stroke:{actor_border};fill:{actor_bkg};stroke-width:{sw};"),
    );
    rule(
        &mut o,
        &i,
        &[" rect.actor.outer-path[data-look=\"neo\"]"],
        &format!("filter:{drop_shadow};"),
    );
    rule(
        &mut o,
        &i,
        &[" rect.note[data-look=\"neo\"]"],
        &format!("stroke:{note_border};fill:{note_bkg};filter:{drop_shadow};"),
    );
    rule(
        &mut o,
        &i,
        &[" text.actor>tspan"],
        &format!("fill:{actor_text};stroke:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .actor-line"],
        &format!("stroke:{actor_line};"),
    );
    rule(
        &mut o,
        &i,
        &[" .innerArc"],
        "stroke-width:1.5;stroke-dasharray:none;",
    );
    rule(
        &mut o,
        &i,
        &[" .messageLine0"],
        &format!("stroke-width:1.5;stroke-dasharray:none;stroke:{signal};"),
    );
    rule(
        &mut o,
        &i,
        &[" .messageLine1"],
        &format!("stroke-width:1.5;stroke-dasharray:2,2;stroke:{signal};"),
    );
    rule(
        &mut o,
        &i,
        &[" [id$=\"-arrowhead\"] path"],
        &format!("fill:{signal};stroke:{signal};"),
    );
    rule(
        &mut o,
        &i,
        &[" .sequenceNumber"],
        &format!("fill:{seq_num};"),
    );
    rule(
        &mut o,
        &i,
        &[" [id$=\"-sequencenumber\"]"],
        &format!("fill:{signal};"),
    );
    rule(
        &mut o,
        &i,
        &[" [id$=\"-crosshead\"] path"],
        &format!("fill:{signal};stroke:{signal};"),
    );
    rule(
        &mut o,
        &i,
        &[" .messageText"],
        &format!("fill:{signal_text};stroke:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .labelBox"],
        &format!("stroke:{label_box_border};fill:{label_box_bkg};filter:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .labelText", " .labelText>tspan"],
        &format!("fill:{label_text};stroke:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .loopText", " .loopText>tspan"],
        &format!("fill:{loop_text};stroke:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .sectionTitle", " .sectionTitle>tspan"],
        &format!("fill:{loop_text};stroke:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .loopLine"],
        &format!(
            "stroke-width:2px;stroke-dasharray:2,2;stroke:{label_box_border};fill:{label_box_border};"
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .note"],
        &format!("stroke:{note_border};fill:{note_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .noteText", " .noteText>tspan"],
        &format!("fill:{note_text};stroke:none;font-weight:{note_font_weight};"),
    );
    rule(
        &mut o,
        &i,
        &[" .activation0"],
        &format!("fill:{activation_bkg};stroke:{activation_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" .activation1"],
        &format!("fill:{activation_bkg};stroke:{activation_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" .activation2"],
        &format!("fill:{activation_bkg};stroke:{activation_border};"),
    );
    rule(&mut o, &i, &[" .actorPopupMenu"], "position:absolute;");
    rule(
        &mut o,
        &i,
        &[" .actorPopupMenuPanel"],
        &format!(
            "position:absolute;fill:{actor_bkg};box-shadow:0px 8px 16px 0px rgba(0,0,0,0.2);filter:drop-shadow(3px 5px 2px rgb(0 0 0 / 0.4));"
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .actor-man circle", " line"],
        &format!("fill:{actor_bkg};stroke-width:2px;"),
    );
    rule(
        &mut o,
        &i,
        &[" g rect.rect"],
        &format!("filter:{drop_shadow};stroke:{node_border};"),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

pub fn themed_flowchart_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let font_family = v("fontFamily");
    let text_color = v("textColor");
    let line_color = v("lineColor");
    let stroke_width = vars
        .get("strokeWidth")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let sw = crate::svg::js_num(stroke_width);
    let node_text = {
        let n = v("nodeTextColor");
        if n.is_empty() { text_color.clone() } else { n }
    };
    let title_color = v("titleColor");
    let main_bkg = v("mainBkg");
    let node_border = v("nodeBorder");
    let arrowhead = v("arrowheadColor");
    let edge_label_bg = v("edgeLabelBackground");
    let label_bkg_faded = fade(&edge_label_bg, 0.5);
    let cluster_bkg = v("clusterBkg");
    let cluster_border = v("clusterBorder");
    let tertiary = v("tertiaryColor");
    let border2 = v("border2");
    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" .label"],
        &format!("font-family:{font_family};color:{node_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label text"],
        &format!("fill:{title_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label span"],
        &format!("color:{title_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label span p"],
        "background-color:transparent;",
    );
    rule(
        &mut o,
        &i,
        &[" .label text", " span"],
        &format!("fill:{node_text};color:{node_text};"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .node rect",
            " .node circle",
            " .node ellipse",
            " .node polygon",
            " .node path",
        ],
        &format!("fill:{main_bkg};stroke:{node_border};stroke-width:{sw}px;"),
    );
    rule(
        &mut o,
        &i,
        &[
            " .rough-node .label text",
            " .node .label text",
            " .image-shape .label",
            " .icon-shape .label",
        ],
        "text-anchor:middle;",
    );
    rule(
        &mut o,
        &i,
        &[" .node .katex path"],
        "fill:#000;stroke:#000;stroke-width:1px;",
    );
    rule(
        &mut o,
        &i,
        &[
            " .rough-node .label",
            " .node .label",
            " .image-shape .label",
            " .icon-shape .label",
        ],
        "text-align:center;",
    );
    rule(&mut o, &i, &[" .node.clickable"], "cursor:pointer;");
    rule(
        &mut o,
        &i,
        &[" .root .anchor path"],
        &format!("fill:{line_color}!important;stroke-width:0;stroke:{line_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .arrowheadPath"],
        &format!("fill:{arrowhead};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgePath .path"],
        &format!("stroke:{line_color};stroke-width:{sw}px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .flowchart-link"],
        &format!("stroke:{line_color};fill:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel"],
        &format!("background-color:{edge_label_bg};text-align:center;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel p"],
        &format!("background-color:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel rect"],
        &format!("opacity:0.5;background-color:{edge_label_bg};fill:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .labelBkg"],
        &format!("background-color:{label_bkg_faded};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster rect"],
        &format!("fill:{cluster_bkg};stroke:{cluster_border};stroke-width:1px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster text"],
        &format!("fill:{title_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster span"],
        &format!("color:{title_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" div.mermaidTooltip"],
        &format!(
            "position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{font_family};font-size:12px;background:{tertiary};border:1px solid {border2};border-radius:2px;pointer-events:none;z-index:100;"
        ),
    );
    rule(
        &mut o,
        &i,
        &[" .flowchartTitleText"],
        &format!("text-anchor:middle;font-size:18px;fill:{text_color};"),
    );
    rule(&mut o, &i, &[" rect.text"], "fill:none;stroke-width:0;");
    rule(
        &mut o,
        &i,
        &[" .icon-shape", " .image-shape"],
        &format!("background-color:{edge_label_bg};text-align:center;"),
    );
    rule(
        &mut o,
        &i,
        &[" .icon-shape p", " .image-shape p"],
        &format!("background-color:{edge_label_bg};padding:2px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .icon-shape .label rect", " .image-shape .label rect"],
        &format!("opacity:0.5;background-color:{edge_label_bg};fill:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .label-icon"],
        "display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;",
    );
    rule(
        &mut o,
        &i,
        &[" .node .label-icon path"],
        "fill:currentColor;stroke:revert;stroke-width:revert;",
    );
    css_suffix(&mut o, &i, id, vars);
    o
}

/// The stateDiagram stylesheet (port of `diagrams/state/styles.js` through
/// stylis, wrapped in the shared prefix/suffix).
#[must_use]
pub fn themed_statediagram_css(id: &str, vars: &Map<String, Value>) -> String {
    let v = |key: &str| super::themes::get(vars, key);
    let or = |primary: &str, fallback: &str| {
        let p = v(primary);
        if p.is_empty() { v(fallback) } else { p }
    };
    let transition = v("transitionColor");
    let node_border = v("nodeBorder");
    let text_color = v("textColor");
    let state_label = v("stateLabelColor");
    let main_bkg = v("mainBkg");
    let line_color = v("lineColor");
    let sw = stroke_width(vars);
    let background = v("background");
    let note_border = v("noteBorderColor");
    let note_bkg = v("noteBkgColor");
    let note_text = v("noteTextColor");
    let label_background = v("labelBackgroundColor");
    let edge_label_bg = v("edgeLabelBackground");
    let transition_label = or("transitionLabelColor", "tertiaryTextColor");
    let special = v("specialStateColor");
    let inner_end = v("innerEndBackground");
    let composite_bg = or("compositeBackground", "background");
    let state_bkg = or("stateBkg", "mainBkg");
    let state_border = or("stateBorder", "nodeBorder");
    let composite_title_bg = v("compositeTitleBackground");
    let alt_background = {
        let a = v("altBackground");
        if a.is_empty() {
            "#efefef".to_owned()
        } else {
            a
        }
    };
    let use_gradient = super::themes::get_bool(vars, "useGradient");
    let radius = v("radius");
    let drop_shadow = v("dropShadow");
    let neo_cluster_stroke = if use_gradient {
        format!("url({id}-gradient)")
    } else {
        state_border.clone()
    };
    let neo_filter = if drop_shadow.is_empty() {
        "none".to_owned()
    } else {
        drop_shadow.replace("url(#drop-shadow)", &format!("url({id}-drop-shadow)"))
    };

    let i = format!("#{id}");
    let mut o = String::new();
    css_prefix(&mut o, &i, vars);
    rule(
        &mut o,
        &i,
        &[" defs [id$=\"-barbEnd\"]"],
        &format!("fill:{transition};stroke:{transition};"),
    );
    rule(
        &mut o,
        &i,
        &[" g.stateGroup text"],
        &format!("fill:{node_border};stroke:none;font-size:10px;"),
    );
    rule(
        &mut o,
        &i,
        &[" g.stateGroup text"],
        &format!("fill:{text_color};stroke:none;font-size:10px;"),
    );
    rule(
        &mut o,
        &i,
        &[" g.stateGroup .state-title"],
        &format!("font-weight:bolder;fill:{state_label};"),
    );
    rule(
        &mut o,
        &i,
        &[" g.stateGroup rect"],
        &format!("fill:{main_bkg};stroke:{node_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" g.stateGroup line"],
        &format!("stroke:{line_color};stroke-width:{sw};"),
    );
    rule(
        &mut o,
        &i,
        &[" .transition"],
        &format!("stroke:{transition};stroke-width:{sw};fill:none;"),
    );
    rule(
        &mut o,
        &i,
        &[" .stateGroup .composit"],
        &format!("fill:{background};border-bottom:1px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .stateGroup .alt-composit"],
        "fill:#e0e0e0;border-bottom:1px;",
    );
    rule(
        &mut o,
        &i,
        &[" .state-note"],
        &format!("stroke:{note_border};fill:{note_bkg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .state-note text"],
        &format!("fill:{note_text};stroke:none;font-size:10px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .stateLabel .box"],
        &format!("stroke:none;stroke-width:0;fill:{main_bkg};opacity:0.5;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label rect"],
        &format!("fill:{label_background};opacity:0.5;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel"],
        &format!("background-color:{edge_label_bg};text-align:center;"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel p"],
        &format!("background-color:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel rect"],
        &format!("opacity:0.5;background-color:{edge_label_bg};fill:{edge_label_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .edgeLabel .label text"],
        &format!("fill:{transition_label};"),
    );
    rule(
        &mut o,
        &i,
        &[" .label div .edgeLabel"],
        &format!("color:{transition_label};"),
    );
    rule(
        &mut o,
        &i,
        &[" .stateLabel text"],
        &format!("fill:{state_label};font-size:10px;font-weight:bold;"),
    );
    rule(
        &mut o,
        &i,
        &[" .node circle.state-start"],
        &format!("fill:{special};stroke:{special};"),
    );
    rule(
        &mut o,
        &i,
        &[" .node .fork-join"],
        &format!("fill:{special};stroke:{special};"),
    );
    rule(
        &mut o,
        &i,
        &[" .node circle.state-end"],
        &format!("fill:{inner_end};stroke:{background};stroke-width:1.5;"),
    );
    rule(
        &mut o,
        &i,
        &[" .end-state-inner"],
        &format!("fill:{composite_bg};stroke-width:1.5;"),
    );
    rule(
        &mut o,
        &i,
        &[" .node rect"],
        &format!("fill:{state_bkg};stroke:{state_border};stroke-width:{sw}px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .node polygon"],
        &format!("fill:{main_bkg};stroke:{state_border};stroke-width:{sw}px;"),
    );
    rule(
        &mut o,
        &i,
        &[" [id$=\"-barbEnd\"]"],
        &format!("fill:{line_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-cluster rect"],
        &format!("fill:{composite_title_bg};stroke:{state_border};stroke-width:{sw}px;"),
    );
    rule(
        &mut o,
        &i,
        &[" .cluster-label", " .nodeLabel"],
        &format!("color:{state_label};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-cluster rect.outer"],
        "rx:5px;ry:5px;",
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-state .divider"],
        &format!("stroke:{state_border};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-state .title-state"],
        "rx:5px;ry:5px;",
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-cluster.statediagram-cluster .inner"],
        &format!("fill:{composite_bg};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-cluster.statediagram-cluster-alt .inner"],
        &format!("fill:{alt_background};"),
    );
    rule(&mut o, &i, &[" .statediagram-cluster .inner"], "rx:0;ry:0;");
    rule(
        &mut o,
        &i,
        &[" .statediagram-state rect.basic"],
        "rx:5px;ry:5px;",
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-state rect.divider"],
        &format!("stroke-dasharray:10,10;fill:{alt_background};"),
    );
    rule(&mut o, &i, &[" .note-edge"], "stroke-dasharray:5;");
    for _ in 0..2 {
        rule(
            &mut o,
            &i,
            &[" .statediagram-note rect"],
            &format!("fill:{note_bkg};stroke:{note_border};stroke-width:1px;rx:0;ry:0;"),
        );
    }
    rule(
        &mut o,
        &i,
        &[" .statediagram-note text"],
        &format!("fill:{note_text};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagram-note .nodeLabel"],
        &format!("color:{note_text};"),
    );
    rule(&mut o, &i, &[" .statediagram .edgeLabel"], "color:red;");
    rule(
        &mut o,
        &i,
        &[" [id$=\"-dependencyStart\"]", " [id$=\"-dependencyEnd\"]"],
        &format!("fill:{line_color};stroke:{line_color};stroke-width:{sw};"),
    );
    rule(
        &mut o,
        &i,
        &[" .statediagramTitleText"],
        &format!("text-anchor:middle;font-size:18px;fill:{text_color};"),
    );
    rule(
        &mut o,
        &i,
        &[" [data-look=\"neo\"].statediagram-cluster rect"],
        &format!("fill:{main_bkg};stroke:{neo_cluster_stroke};stroke-width:{sw};"),
    );
    rule(
        &mut o,
        &i,
        &[" [data-look=\"neo\"].statediagram-cluster rect.outer"],
        &format!("rx:{radius}px;ry:{radius}px;filter:{neo_filter};"),
    );
    css_suffix(&mut o, &i, id, vars);
    o
}
