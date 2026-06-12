//! SVG `<text>`/`<tspan>` labels (`htmlLabels: false` path), porting
//! `createFormattedText`, `nonMarkdownToLines`, and `splitLineToFitWidth`.

use crate::svg::{Element, append, js_num, set_attr, set_text_append};
use crate::text::TextMeasurer;

/// Bounding box of an SVG text element as Chrome's `getBBox()` reports it.
#[derive(Debug, Clone, Copy, Default)]
pub struct SvgBBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `decodeHTMLEntities` from createText.ts — only amp/lt/gt.
fn decode_entities_minimal(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// `nonMarkdownToLines`: lines split on `\n`, literal `\n`, `<br>`; words by
/// the `<[^>]+>|[^\s<>]+` pattern on the trimmed line.
#[must_use]
pub fn non_markdown_to_lines(text: &str) -> Vec<Vec<String>> {
    // Split on \\n, \n, or <br/> variants.
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1) == Some(&'n') {
            lines.push(std::mem::take(&mut current));
            i += 2;
            continue;
        }
        if chars[i] == '\n' {
            lines.push(std::mem::take(&mut current));
            i += 1;
            continue;
        }
        if chars[i] == '<' {
            let rest: String = chars[i..].iter().take(8).collect::<String>().to_lowercase();
            if rest.starts_with("<br")
                && let Some(close) = chars[i..].iter().position(|&c| c == '>')
            {
                let tag: String = chars[i + 1..i + close].iter().collect();
                let name = tag.trim_start_matches('/').trim();
                if name.eq_ignore_ascii_case("br")
                    || name.to_lowercase().starts_with("br ")
                    || name.to_lowercase().starts_with("br/")
                    || name.eq_ignore_ascii_case("br/")
                {
                    lines.push(std::mem::take(&mut current));
                    i += close + 1;
                    continue;
                }
            }
        }
        current.push(chars[i]);
        i += 1;
    }
    lines.push(current);

    lines
        .iter()
        .map(|line| {
            let line = line.trim();
            // /<[^>]+>|[^\s<>]+/g
            let mut words = Vec::new();
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '<' {
                    if let Some(close) = chars[i + 1..].iter().position(|&c| c == '>')
                        && close > 0
                    {
                        let word: String = chars[i..=i + close + 1].iter().collect();
                        words.push(word);
                        i += close + 2;
                        continue;
                    }
                    // '<' with no closing '>' is excluded by the char class.
                    i += 1;
                    continue;
                }
                if chars[i].is_whitespace() || chars[i] == '>' {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && chars[i] != '<'
                    && chars[i] != '>'
                {
                    i += 1;
                }
                words.push(chars[start..i].iter().collect());
            }
            words
        })
        .collect()
}

/// Rendered inner-tspan contents for a line: first word plain, others with a
/// leading space (updateTextContentAndStyles).
fn tspan_contents(words: &[String]) -> Vec<String> {
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let decoded = decode_entities_minimal(w);
            if i == 0 {
                decoded
            } else {
                format!(" {decoded}")
            }
        })
        .collect()
}

/// `getComputedTextLength` of a line: per-tspan advances summed (no kerning
/// across tspan boundaries), full float precision.
fn line_width(measurer: &TextMeasurer, words: &[String], font_size: f64) -> f64 {
    tspan_contents(words)
        .iter()
        .map(|t| measurer.measure_advance_svg(t, font_size))
        .sum()
}

/// `splitLineToFitWidth` port.
fn split_line_to_fit(
    measurer: &TextMeasurer,
    line: &[String],
    width: f64,
    font_size: f64,
) -> Vec<Vec<String>> {
    let check_fit = |candidate: &[String]| line_width(measurer, candidate, font_size) <= width;
    let mut words: std::collections::VecDeque<String> = line.iter().cloned().collect();
    let mut lines: Vec<Vec<String>> = Vec::new();
    let mut new_line: Vec<String> = Vec::new();

    loop {
        if words.is_empty() {
            if !new_line.is_empty() {
                lines.push(new_line);
            }
            return lines;
        }
        let next_word = words.pop_front().expect("non-empty");
        let mut line_with_next = new_line.clone();
        line_with_next.push(next_word.clone());

        if check_fit(&line_with_next) {
            new_line = line_with_next;
            continue;
        }
        if !new_line.is_empty() {
            lines.push(std::mem::take(&mut new_line));
            words.push_front(next_word);
        } else if !next_word.is_empty() {
            // Split the word character by character.
            let chars: Vec<char> = next_word.chars().collect();
            let mut used = String::new();
            let mut idx = 0;
            while idx < chars.len() {
                let mut candidate = used.clone();
                candidate.push(chars[idx]);
                if check_fit(std::slice::from_ref(&candidate)) {
                    used = candidate;
                    idx += 1;
                } else {
                    break;
                }
            }
            if used.is_empty() && idx < chars.len() {
                // First character does not fit; take it anyway.
                used.push(chars[idx]);
                idx += 1;
            }
            let rest: String = chars[idx..].iter().collect();
            lines.push(vec![used]);
            if !rest.is_empty() {
                words.push_front(rest);
            }
        }
    }
}

pub struct FormattedText {
    /// The element createText returns (labelGroup with background, or the
    /// bare text element when no background was added).
    pub label_element: Element,
    /// The labelGroup wrapper (left in place even when the text is returned).
    pub label_group: Element,
    /// The `<text>` element.
    pub text_element: Element,
    /// `textElement.getBBox()`.
    pub text_bbox: SvgBBox,
    /// `labelElement.getBBox()` (includes the background rect when present).
    pub label_bbox: SvgBBox,
    pub has_background: bool,
}

impl std::fmt::Debug for FormattedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormattedText")
            .field("text_bbox", &self.text_bbox)
            .finish_non_exhaustive()
    }
}

/// Chrome's integer font metrics for the label font at `font_size`.
fn font_metrics(font_size: f64) -> (f64, f64) {
    // Trebuchet MS hhea: ascender 1923, descender -455, upem 2048; Chrome
    // rounds to integer CSS pixels.
    let ascent = (1923.0 * font_size / 2048.0).round();
    let descent = (455.0 * font_size / 2048.0).round();
    (ascent, descent)
}

/// Port of `createFormattedText` (non-html labels).
pub fn create_formatted_text(
    parent: &Element,
    raw_text: &str,
    measurer: &TextMeasurer,
    font_size: f64,
    width: f64,
    add_background: bool,
    center_text: bool,
) -> FormattedText {
    // createText: text.replace(/<br\s*\/?>/g, '<br/>') happens before
    // nonMarkdownToLines; our splitter handles all br forms directly.
    let add_background = add_background && !raw_text.is_empty();
    let structured = non_markdown_to_lines(raw_text);

    let label_group = append(parent, "g");
    let bkg = append(&label_group, "rect");
    set_attr(&bkg, "class", "background");
    set_attr(&bkg, "style", "stroke: none");
    let text_el = append(&label_group, "text");
    set_attr(&text_el, "y", "-10.1");
    if center_text {
        set_attr(&text_el, "text-anchor", "middle");
    }

    let mut line_index: usize = 0;
    let mut max_width: f64 = 0.0;
    for line in &structured {
        let fits = line_width(measurer, line, font_size) <= width;
        let prepared: Vec<Vec<String>> = if fits {
            vec![line.clone()]
        } else {
            split_line_to_fit(measurer, line, width, font_size)
        };
        for prepared_line in &prepared {
            let tspan = append(&text_el, "tspan");
            set_attr(&tspan, "class", "text-outer-tspan row");
            set_attr(&tspan, "x", "0");
            #[allow(clippy::cast_precision_loss)]
            let y = line_index as f64 * 1.1 - 0.1;
            set_attr(&tspan, "y", format!("{}em", js_num(y)));
            set_attr(&tspan, "dy", "1.1em");
            if center_text {
                set_attr(&tspan, "text-anchor", "middle");
            }
            for content in tspan_contents(prepared_line) {
                let inner = append(&tspan, "tspan");
                set_attr(&inner, "font-style", "normal");
                set_attr(&inner, "class", "text-inner-tspan");
                set_attr(&inner, "font-weight", "normal");
                set_text_append(&inner, &content);
            }
            max_width = max_width.max(line_width(measurer, prepared_line, font_size));
            line_index += 1;
        }
    }

    // textElement.getBBox(): f32, anchored at 0 (or centered), with Chrome's
    // integer ascent/descent and 1.1em line advance.
    let f32q = |v: f64| f64::from(v as f32);
    let (ascent, descent) = font_metrics(font_size);
    let text_bbox = if line_index == 0 || max_width == 0.0 {
        SvgBBox::default()
    } else {
        let first_baseline = font_size; // (-0.1 + 1.1) em
        #[allow(clippy::cast_precision_loss)]
        let last_baseline = font_size * (1.0 + 1.1 * (line_index as f64 - 1.0));
        SvgBBox {
            x: if center_text {
                f32q(-max_width / 2.0)
            } else {
                0.0
            },
            y: f32q(first_baseline - ascent),
            width: f32q(max_width),
            height: f32q(last_baseline + descent - (first_baseline - ascent)),
        }
    };

    if add_background {
        let padding = 2.0;
        set_attr(&bkg, "x", js_num(text_bbox.x - padding));
        set_attr(&bkg, "y", js_num(text_bbox.y - padding));
        set_attr(&bkg, "width", js_num(text_bbox.width + 2.0 * padding));
        set_attr(&bkg, "height", js_num(text_bbox.height + 2.0 * padding));
        let label_bbox = SvgBBox {
            x: text_bbox.x - padding,
            y: text_bbox.y - padding,
            width: text_bbox.width + 2.0 * padding,
            height: text_bbox.height + 2.0 * padding,
        };
        FormattedText {
            label_element: label_group.clone(),
            label_group,
            text_element: text_el,
            text_bbox,
            label_bbox,
            has_background: true,
        }
    } else {
        FormattedText {
            label_element: text_el.clone(),
            label_group,
            text_element: text_el,
            text_bbox,
            label_bbox: text_bbox,
            has_background: false,
        }
    }
}
