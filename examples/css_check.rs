fn main() {
    let vars = mermaid_rust::render::themes::theme_variables("default", &serde_json::Map::new());
    let generated = mermaid_rust::render::css::themed_flowchart_css("my-svg", &vars);
    let captured = mermaid_rust::render::css::flowchart_css("my-svg");
    if generated == captured {
        println!("DEFAULT CSS IDENTICAL");
    } else {
        let pos = generated
            .bytes()
            .zip(captured.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(generated.len().min(captured.len()));
        println!("differs at {pos}");
        println!(
            "gen: {}",
            &generated[pos.saturating_sub(60)..(pos + 80).min(generated.len())]
        );
        println!(
            "cap: {}",
            &captured[pos.saturating_sub(60)..(pos + 80).min(captured.len())]
        );
    }
}
