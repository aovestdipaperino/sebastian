//! Top-level diagram orchestration: detect the mermaid diagram type, parse the
//! source with the matching front-end, and drive the shared [`crate::render`]
//! pipeline.
//!
//! This module sits *above* both [`crate::render`] and the individual diagram
//! front-ends (`flowchart`, `state`, `classdiag`, `sequence`, `timeline`), so
//! the rendering engine never has to depend on a specific diagram parser.

use crate::flowchart::parser::{ParseError, parse};
use crate::render::{self, DiagramChrome, render_unified};

/// Detects the mermaid diagram type of `source` from its header keyword.
#[must_use]
pub fn detect_diagram_type(source: &str) -> &'static str {
    for raw in source.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") || t.starts_with('#') {
            continue;
        }
        if t.starts_with("stateDiagram") {
            return "state";
        }
        if t.starts_with("sequenceDiagram") {
            return "sequence";
        }
        if t == "timeline" || t.starts_with("timeline ") {
            return "timeline";
        }
        if t.starts_with("classDiagram") {
            return "class";
        }
        if t == "pie" || t.starts_with("pie ") || t.starts_with("pie\t") {
            return "pie";
        }
        #[cfg(feature = "mermaid-extensions")]
        if t == "pyramid" || t.starts_with("pyramid ") {
            return "pyramid";
        }
        if t.starts_with("erDiagram") {
            return "er";
        }
        if t.starts_with("requirementDiagram") {
            return "requirement";
        }
        if t.starts_with("C4Context")
            || t.starts_with("C4Container")
            || t.starts_with("C4Component")
            || t.starts_with("C4Dynamic")
            || t.starts_with("C4Deployment")
        {
            return "c4";
        }
        if t.starts_with("xychart-beta") {
            return "xychart";
        }
        if t == "gantt" || t.starts_with("gantt ") {
            return "gantt";
        }
        if t.starts_with("gitGraph") {
            return "gitgraph";
        }
        if t == "journey" || t.starts_with("journey ") {
            return "journey";
        }
        if t.starts_with("quadrantChart") {
            return "quadrant";
        }
        if t == "packet" || t == "packet-beta" || t.starts_with("packet ") {
            return "packet";
        }
        if t.starts_with("radar-beta") {
            return "radar";
        }
        if t == "sankey-beta" || t.starts_with("sankey-beta ") || t == "sankey" {
            return "sankey";
        }
        if t == "block-beta"
            || t.starts_with("block-beta ")
            || t == "block"
            || t.starts_with("block ")
        {
            return "block";
        }
        if t == "treemap-beta"
            || t == "treemap"
            || t.starts_with("treemap-beta ")
            || t.starts_with("treemap ")
        {
            return "treemap";
        }
        if t == "kanban" || t.starts_with("kanban ") || t.starts_with("kanban:") {
            return "kanban";
        }
        if t == "mindmap" || t.starts_with("mindmap ") {
            return "mindmap";
        }
        if t == "architecture-beta" || t.starts_with("architecture-beta ") || t == "architecture" {
            return "architecture";
        }
        return "flowchart";
    }
    "flowchart"
}

/// Renders any supported mermaid diagram to a complete SVG document string.
///
/// # Errors
/// Returns an error when the source cannot be parsed or rendered. Malformed
/// input that trips an internal invariant deep in the pipeline is caught at
/// this boundary and reported as an error rather than a panic.
pub fn render_diagram(source: &str, id: &str) -> Result<String, Box<dyn std::error::Error>> {
    // The render pipeline is panic-free for every valid diagram in the test
    // corpora, but hostile input can still reach internal `panic!`/`expect`
    // invariants. Contain those here so library users and the CLI always get
    // a Result. (The pipeline holds no cross-call state, so unwinding is
    // safe; the hook is restored around the call to keep panics quiet.)
    use std::cell::Cell;
    use std::sync::Once;
    thread_local! {
        static SILENCE_PANICS: Cell<bool> = const { Cell::new(false) };
    }
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !SILENCE_PANICS.with(Cell::get) {
                prev(info);
            }
        }));
    });
    SILENCE_PANICS.with(|s| s.set(true));
    let result = std::panic::catch_unwind(|| render_diagram_inner(source, id));
    SILENCE_PANICS.with(|s| s.set(false));
    result.unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());
        Err(format!("invalid diagram: {msg}").into())
    })
}

fn render_diagram_inner(source: &str, id: &str) -> Result<String, Box<dyn std::error::Error>> {
    // HAND-DRAWN EXTENSION: the handwritten label font applies to every
    // diagram type, so it is injected once here instead of in each renderer.
    // Text metrics must come from the same font, so measurement is switched
    // for the whole render (reset even on error/panic-unwind paths).
    let hand_drawn = render::config::detect_init(source).is_hand_drawn();
    struct ResetHandDrawn;
    impl Drop for ResetHandDrawn {
        fn drop(&mut self) {
            crate::text::set_hand_drawn(false);
        }
    }
    let _reset = hand_drawn.then(|| {
        crate::text::set_hand_drawn(true);
        ResetHandDrawn
    });
    let svg = render_by_type(source, id)?;
    if hand_drawn {
        let css = render::css::hand_drawn_font_css(id);
        // Flowchart injects the override in its own stylesheet already.
        if !svg.contains(&css) {
            return Ok(svg.replacen("</style>", &format!("{css}</style>"), 1));
        }
    } else {
        // Classic look measured with an embedded fallback face (no real
        // fonts on the host): draw with that same face, or viewers that do
        // have the real fonts render differently-sized glyphs than the
        // boxes were measured for. No-op on hosts with the real fonts —
        // byte-exact output is unchanged there. Sequence diagrams measure
        // with Times metrics, everything else with Trebuchet.
        let seq = detect_diagram_type(source) == "sequence";
        let css = if seq && !crate::text::times_available() {
            Some(render::css::fallback_seq_font_css(id))
        } else if !seq && !crate::text::trebuchet_available() {
            Some(render::css::fallback_font_css(id))
        } else {
            None
        };
        if let Some(css) = css {
            return Ok(svg.replacen("</style>", &format!("{css}</style>"), 1));
        }
    }
    Ok(svg)
}

fn render_by_type(source: &str, id: &str) -> Result<String, Box<dyn std::error::Error>> {
    match detect_diagram_type(source) {
        "state" => render_state(source, id).map_err(Into::into),
        "pie" => crate::pie::render_pie(source, id).map_err(Into::into),
        #[cfg(feature = "mermaid-extensions")]
        "pyramid" => crate::pyramid::render_pyramid(source, id).map_err(Into::into),
        "er" => render_er(source, id).map_err(Into::into),
        "requirement" => render_requirement(source, id).map_err(Into::into),
        "c4" => crate::c4::render_c4(source, id).map_err(Into::into),
        "xychart" => crate::xychart::render_xychart(source, id).map_err(Into::into),
        "gantt" => crate::gantt::render_gantt(source, id).map_err(Into::into),
        "gitgraph" => crate::gitgraph::render_gitgraph(source, id).map_err(Into::into),
        "journey" => crate::journey::render_journey(source, id).map_err(Into::into),
        "quadrant" => crate::quadrant::render_quadrant(source, id).map_err(Into::into),
        "packet" => crate::packet::render_packet(source, id).map_err(Into::into),
        "radar" => crate::radar::render_radar(source, id).map_err(Into::into),
        "sankey" => crate::sankey::render_sankey(source, id).map_err(Into::into),
        "block" => crate::block::render_block(source, id).map_err(Into::into),
        "treemap" => crate::treemap::render_treemap(source, id).map_err(Into::into),
        "kanban" => crate::kanban::render_kanban(source, id).map_err(Into::into),
        "mindmap" => crate::mindmap::render_mindmap(source, id).map_err(Into::into),
        "architecture" => crate::architecture::render_architecture(source, id).map_err(Into::into),
        "sequence" => crate::sequence::render_sequence(source, id).map_err(Into::into),
        "timeline" => crate::timeline::render_timeline(source, id).map_err(Into::into),
        "class" => render_class(source, id).map_err(Into::into),
        _ => render_flowchart(source, id).map_err(Into::into),
    }
}

/// Renders mermaid stateDiagram-v2 source to a complete SVG document string.
///
/// # Errors
/// Returns a [`crate::state::StateParseError`] when the source is not a
/// valid state diagram.
pub fn render_state(source: &str, id: &str) -> Result<String, crate::state::StateParseError> {
    let mut config = render::config::detect_init(source);
    let theme_vars = render::themes::theme_variables(&config.theme, &config.theme_variables);
    config.computed_theme.clone_from(&theme_vars);
    let (data, class_list) = crate::state::get_layout_data_and_classes(source, id, &config)?;
    let chrome = DiagramChrome {
        svg_class: "statediagram",
        aria: "stateDiagram",
        diagram_type: "stateDiagram",
        css: format!(
            "{}{}",
            render::css::themed_statediagram_css(id, &theme_vars),
            render::css::class_defs_css(id, config.effective_html_labels(), &class_list),
        ),
    };
    Ok(render_unified(&data, &config, &theme_vars, &chrome, id))
}

/// Renders mermaid erDiagram source to a complete SVG document string.
///
/// # Errors
/// Returns a [`crate::er::ErParseError`] when the source is not a valid
/// er diagram.
pub fn render_er(source: &str, id: &str) -> Result<String, crate::er::ErParseError> {
    let mut config = render::config::detect_init(source);
    let theme_vars = render::themes::theme_variables(&config.theme, &config.theme_variables);
    config.computed_theme.clone_from(&theme_vars);
    config.node_spacing = 140.0;
    config.rank_spacing = 80.0;
    config.edge_label_font_size = Some(14.0);
    let data = crate::er::get_layout_data(source, id)?;
    let chrome = DiagramChrome {
        svg_class: "erDiagram",
        aria: "er",
        diagram_type: "er",
        css: render::css::themed_er_css(id, &theme_vars),
    };
    Ok(render_unified(&data, &config, &theme_vars, &chrome, id))
}

/// Renders mermaid requirementDiagram source to a complete SVG document string.
///
/// This is an **approximate** (non-byte-exact) renderer: requirement/element
/// boxes are laid out and drawn through the shared flowchart node pipeline
/// rather than the byte-exact `requirementBox` shape (whose sizing needs
/// Blink `getBBox()` ink metrics). See [`crate::requirement`].
///
/// # Errors
/// Returns a [`crate::requirement::RequirementParseError`] when the source is
/// not a valid requirement diagram.
pub fn render_requirement(
    source: &str,
    id: &str,
) -> Result<String, crate::requirement::RequirementParseError> {
    let mut config = render::config::detect_init(source);
    let theme_vars = render::themes::theme_variables(&config.theme, &config.theme_variables);
    config.computed_theme.clone_from(&theme_vars);
    let data = crate::requirement::get_layout_data(source, id)?;
    let chrome = DiagramChrome {
        svg_class: "requirementDiagram",
        aria: "requirement",
        diagram_type: "requirement",
        css: render::css::themed_flowchart_css(id, &theme_vars),
    };
    Ok(render_unified(&data, &config, &theme_vars, &chrome, id))
}

/// Renders mermaid classDiagram source to a complete SVG document string.
///
/// # Errors
/// Returns a [`crate::classdiag::ClassParseError`] when the source is not a
/// valid class diagram.
pub fn render_class(source: &str, id: &str) -> Result<String, crate::classdiag::ClassParseError> {
    let mut config = render::config::detect_init(source);
    let theme_vars = render::themes::theme_variables(&config.theme, &config.theme_variables);
    config.computed_theme.clone_from(&theme_vars);
    let data = crate::classdiag::get_layout_data(source, id)?;
    let chrome = DiagramChrome {
        svg_class: "classDiagram",
        aria: "class",
        diagram_type: "class",
        css: render::css::themed_class_css(id, &theme_vars),
    };
    Ok(render_unified(&data, &config, &theme_vars, &chrome, id))
}

/// Renders mermaid flowchart source to a complete SVG document string.
///
/// # Errors
/// Returns a [`ParseError`] when the source is not a valid flowchart.
pub fn render_flowchart(source: &str, id: &str) -> Result<String, ParseError> {
    let mut config = render::config::detect_init(source);
    let theme_vars = render::themes::theme_variables(&config.theme, &config.theme_variables);
    config.computed_theme.clone_from(&theme_vars);
    let db = parse(source)?;
    let data = db.get_data(id, &config);

    let class_list: Vec<(String, Vec<String>, Vec<String>)> = db
        .classes
        .iter()
        .map(|(name, class)| {
            (
                name.clone(),
                class.styles.clone(),
                class.text_styles.clone(),
            )
        })
        .collect();
    let chrome = DiagramChrome {
        svg_class: "flowchart",
        aria: "flowchart-v2",
        diagram_type: "flowchart-v2",
        css: format!(
            "{}{}{}",
            render::css::themed_flowchart_css(id, &theme_vars),
            render::css::class_defs_css(id, config.effective_html_labels(), &class_list),
            if config.is_hand_drawn() {
                render::css::hand_drawn_font_css(id)
            } else {
                String::new()
            }
        ),
    };
    Ok(render_unified(&data, &config, &theme_vars, &chrome, id))
}

/// Convenience: render mermaid `source` straight to a PNG (white background, no
/// overlay). Pairs with [`render_diagram`], which returns the SVG.
///
/// # Errors
/// Returns an error if the diagram fails to render or rasterize.
#[cfg(feature = "raster")]
pub fn render_png(source: &str, id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use crate::render::raster::{RasterOptions, rasterize_svg};
    let svg = render_diagram(source, id)?;
    Ok(rasterize_svg(&svg, &RasterOptions::default())?)
}
