//! packet-beta support: a direct port of the langium packet grammar
//! (`packages/parser/src/language/packet`), `db.ts` (`populate` /
//! `getNextFittingBlock`), and `renderer.ts`.
//!
//! The layout is pure arithmetic — no text measurement — so the only subtlety
//! is matching mermaid's block-wrapping logic and d3 number formatting.

#![allow(clippy::assigning_clones)]
use crate::svg::{append, js_num, serialize, set_attr, set_text};

/// A parse error for packet source.
#[derive(Debug)]
pub struct PacketParseError {
    pub message: String,
}

impl std::fmt::Display for PacketParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "packet parse error: {}", self.message)
    }
}

impl std::error::Error for PacketParseError {}

// Fixed defaults from `DEFAULT_CONFIG.packet` (config.schema.yaml). `showBits`
// is true, so `getConfig` bumps `paddingY` by 10.
const ROW_HEIGHT: f64 = 32.0;
const PADDING_X: f64 = 5.0;
const PADDING_Y: f64 = 15.0;
const BIT_WIDTH: f64 = 32.0;
const BITS_PER_ROW: i64 = 32;
const MAX_PACKET_SIZE: usize = 10_000;

/// A laid-out block within a word: inclusive bit range and its label.
#[derive(Clone)]
struct Block {
    start: i64,
    end: i64,
    label: String,
}

/// A raw block statement as parsed, before contiguity/wrapping resolution.
struct RawBlock {
    start: Option<i64>,
    end: Option<i64>,
    bits: Option<i64>,
    label: String,
}

struct Db {
    title: String,
    words: Vec<Vec<Block>>,
}

/// Strip the surrounding quotes of a langium STRING and unescape `\x` -> `x`.
fn unquote(s: &str) -> String {
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_block_line(line: &str) -> Result<RawBlock, PacketParseError> {
    let (range, label_part) = line.split_once(':').ok_or_else(|| PacketParseError {
        message: format!("bad packet block (missing ':'): {line}"),
    })?;
    let label_part = label_part.trim();
    if !((label_part.starts_with('"') && label_part.ends_with('"') && label_part.len() >= 2)
        || (label_part.starts_with('\'') && label_part.ends_with('\'') && label_part.len() >= 2))
    {
        return Err(PacketParseError {
            message: format!("packet block label must be a quoted string: {line}"),
        });
    }
    let label = unquote(label_part);
    let range = range.trim();
    let bad = |_| PacketParseError {
        message: format!("bad packet block range: {line}"),
    };
    if let Some(bits) = range.strip_prefix('+') {
        let bits: i64 = bits.trim().parse().map_err(bad)?;
        Ok(RawBlock {
            start: None,
            end: None,
            bits: Some(bits),
            label,
        })
    } else if let Some((s, e)) = range.split_once('-') {
        Ok(RawBlock {
            start: Some(s.trim().parse().map_err(bad)?),
            end: Some(e.trim().parse().map_err(bad)?),
            bits: None,
            label,
        })
    } else {
        Ok(RawBlock {
            start: Some(range.trim().parse().map_err(bad)?),
            end: None,
            bits: None,
            label,
        })
    }
}

/// Port of `getNextFittingBlock`: split a block at the row boundary if it
/// overflows the current row.
fn next_fitting_block(block: &Block, row: i64) -> (Block, Option<Block>) {
    if block.end < row * BITS_PER_ROW {
        return (block.clone(), None);
    }
    let row_end = row * BITS_PER_ROW - 1;
    let row_start = row * BITS_PER_ROW;
    (
        Block {
            start: block.start,
            end: row_end,
            label: block.label.clone(),
        },
        Some(Block {
            start: row_start,
            end: block.end,
            label: block.label.clone(),
        }),
    )
}

fn parse(source: &str) -> Result<Db, PacketParseError> {
    let mut title = String::new();
    let mut raw_blocks: Vec<RawBlock> = Vec::new();
    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line == "packet" || line == "packet-beta" {
                found_header = true;
                continue;
            }
            return Err(PacketParseError {
                message: format!("expected packet header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("title")
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            title = rest.trim().to_owned();
            continue;
        }
        raw_blocks.push(parse_block_line(line)?);
    }
    if !found_header {
        return Err(PacketParseError {
            message: "missing packet header".to_owned(),
        });
    }

    // Port of `populate`.
    let mut words: Vec<Vec<Block>> = Vec::new();
    let mut last_bit: i64 = -1;
    let mut word: Vec<Block> = Vec::new();
    let mut row: i64 = 1;
    for rb in raw_blocks {
        if let (Some(s), Some(e)) = (rb.start, rb.end)
            && e < s
        {
            return Err(PacketParseError {
                message: format!(
                    "Packet block {s} - {e} is invalid. End must be greater than start."
                ),
            });
        }
        let s = rb.start.unwrap_or(last_bit + 1);
        if s != last_bit + 1 {
            return Err(PacketParseError {
                message: format!(
                    "Packet block {s} - {} is not contiguous. It should start from {}.",
                    rb.end.unwrap_or(s),
                    last_bit + 1
                ),
            });
        }
        if rb.bits == Some(0) {
            return Err(PacketParseError {
                message: format!("Packet block {s} is invalid. Cannot have a zero bit field."),
            });
        }
        let e = rb.end.unwrap_or_else(|| s + rb.bits.unwrap_or(1) - 1);
        last_bit = e;

        let mut cur = Block {
            start: s,
            end: e,
            label: rb.label,
        };
        while word.len() <= (BITS_PER_ROW + 1) as usize && words.len() < MAX_PACKET_SIZE {
            let (block, next) = next_fitting_block(&cur, row);
            let block_end = block.end;
            word.push(block);
            if block_end + 1 == row * BITS_PER_ROW {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                row += 1;
            }
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
    }
    if !word.is_empty() {
        words.push(word);
    }

    Ok(Db { title, words })
}

fn draw_word(group: &crate::svg::Element, word: &[Block], row_number: usize, hand_drawn: bool) {
    let word_y = row_number as f64 * (ROW_HEIGHT + PADDING_Y) + PADDING_Y;
    for block in word {
        let block_x = (block.start % BITS_PER_ROW) as f64 * BIT_WIDTH + 1.0;
        let width = (block.end - block.start + 1) as f64 * BIT_WIDTH - PADDING_X;

        let rect = append(group, "rect");
        set_attr(&rect, "x", js_num(block_x));
        set_attr(&rect, "y", js_num(word_y));
        set_attr(&rect, "width", js_num(width));
        set_attr(&rect, "height", js_num(ROW_HEIGHT));
        set_attr(&rect, "class", "packetBlock");
        if hand_drawn {
            set_attr(&rect, "style", "stroke:none");
            crate::render::handdrawn::hd_overlay_rect(
                group,
                block_x,
                word_y,
                width,
                ROW_HEIGHT,
                "",
                "packetBlock",
            );
        }

        let label = append(group, "text");
        set_attr(&label, "x", js_num(block_x + width / 2.0));
        set_attr(&label, "y", js_num(word_y + ROW_HEIGHT / 2.0));
        set_attr(&label, "class", "packetLabel");
        set_attr(&label, "dominant-baseline", "middle");
        set_attr(&label, "text-anchor", "middle");
        set_text(&label, &block.label);

        // showBits is always true for the default config.
        let is_single = block.end == block.start;
        let bit_number_y = word_y - 2.0;
        let start_text = append(group, "text");
        set_attr(
            &start_text,
            "x",
            js_num(block_x + if is_single { width / 2.0 } else { 0.0 }),
        );
        set_attr(&start_text, "y", js_num(bit_number_y));
        set_attr(&start_text, "class", "packetByte start");
        set_attr(&start_text, "dominant-baseline", "auto");
        set_attr(
            &start_text,
            "text-anchor",
            if is_single { "middle" } else { "start" },
        );
        set_text(&start_text, &block.start.to_string());

        if !is_single {
            let end_text = append(group, "text");
            set_attr(&end_text, "x", js_num(block_x + width));
            set_attr(&end_text, "y", js_num(bit_number_y));
            set_attr(&end_text, "class", "packetByte end");
            set_attr(&end_text, "dominant-baseline", "auto");
            set_attr(&end_text, "text-anchor", "end");
            set_text(&end_text, &block.end.to_string());
        }
    }
}

/// Renders mermaid packet-beta source to a complete SVG document string.
///
/// # Errors
/// Returns a [`PacketParseError`] when the source is not a valid packet diagram.
pub fn render_packet(source: &str, id: &str) -> Result<String, PacketParseError> {
    let config = crate::render::config::detect_init(source);
    let hand_drawn = config.is_hand_drawn();
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;

    let total_row_height = ROW_HEIGHT + PADDING_Y;
    let title_present = !db.title.is_empty();
    let svg_height = total_row_height * (db.words.len() as f64 + 1.0)
        - if title_present { 0.0 } else { ROW_HEIGHT };
    let svg_width = BIT_WIDTH * BITS_PER_ROW as f64 + 2.0;

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(svg_width), js_num(svg_height)),
    );
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(svg_width)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "packet");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_packet_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    for (row_number, word) in db.words.iter().enumerate() {
        let group = append(&svg, "g");
        draw_word(&group, word, row_number, hand_drawn);
    }

    let title = append(&svg, "text");
    if !db.title.is_empty() {
        set_text(&title, &db.title);
    }
    set_attr(&title, "x", js_num(svg_width / 2.0));
    set_attr(&title, "y", js_num(svg_height - total_row_height / 2.0));
    set_attr(&title, "dominant-baseline", "middle");
    set_attr(&title, "text-anchor", "middle");
    set_attr(&title, "class", "packetTitle");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
