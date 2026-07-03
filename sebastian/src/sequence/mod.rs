//! sequenceDiagram support: parser (subset of `sequenceDiagram.jison`),
//! database (`sequenceDb.ts`), and renderer (`sequenceRenderer.ts` +
//! `svgDraw.js`).

mod render;

pub use render::render_sequence;

/// A parse error for sequence diagram source.
#[derive(Debug)]
pub struct SeqParseError {
    pub message: String,
}

impl std::fmt::Display for SeqParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sequence diagram parse error: {}", self.message)
    }
}

impl std::error::Error for SeqParseError {}

// LINETYPE constants (sequenceDb.ts).
pub const SOLID: i32 = 0;
pub const DOTTED: i32 = 1;
pub const NOTE: i32 = 2;
pub const SOLID_CROSS: i32 = 3;
pub const DOTTED_CROSS: i32 = 4;
pub const SOLID_OPEN: i32 = 5;
pub const DOTTED_OPEN: i32 = 6;
pub const LOOP_START: i32 = 10;
pub const LOOP_END: i32 = 11;
pub const ALT_START: i32 = 12;
pub const ALT_ELSE: i32 = 13;
pub const ALT_END: i32 = 14;
pub const OPT_START: i32 = 15;
pub const OPT_END: i32 = 16;
pub const PAR_START: i32 = 19;
pub const PAR_AND: i32 = 20;
pub const PAR_END: i32 = 21;
pub const RECT_START: i32 = 22;
pub const RECT_END: i32 = 23;
pub const CRITICAL_START: i32 = 27;
pub const CRITICAL_OPTION: i32 = 28;
pub const CRITICAL_END: i32 = 29;
pub const BREAK_START: i32 = 30;
pub const BREAK_END: i32 = 31;
pub const ACTIVE_START: i32 = 17;
pub const ACTIVE_END: i32 = 18;
pub const SOLID_POINT: i32 = 24;
pub const DOTTED_POINT: i32 = 25;
pub const AUTONUMBER: i32 = 26;
pub const BIDIRECTIONAL_SOLID: i32 = 33;
pub const BIDIRECTIONAL_DOTTED: i32 = 34;

// PLACEMENT constants.
pub const LEFTOF: i32 = 0;
pub const RIGHTOF: i32 = 1;
pub const OVER: i32 = 2;

/// An actor (participant).
#[derive(Debug, Clone, Default)]
pub struct Actor {
    pub name: String,
    pub description: String,
    pub prev_actor: Option<String>,
    pub next_actor: Option<String>,

    // Rendering data.
    pub x: f64,
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub starty: f64,
    pub stopy: Option<f64>,
    pub actor_cnt: Option<usize>,
    /// Index into [`SequenceDb::boxes`] when declared inside a `box`.
    pub box_index: Option<usize>,
    /// `"participant"` or `"actor"` (stick figure).
    pub is_actor_man: bool,
    /// Participant metadata type: database/queue/control/entity/boundary/
    /// collections (empty for plain participants).
    pub meta_type: String,
    /// `link` statements: (label, url) in insertion order.
    pub links: Vec<(String, String)>,
}

/// A message/event row from the db.
#[derive(Debug, Clone, Default)]
pub struct SeqMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub message: String,
    /// `LINETYPE` value; `None` for plain `addMessage` entries (unused here).
    pub ty: i32,
    pub placement: Option<i32>,
    pub activate: bool,
    /// `autonumber` statement payload: (start, step, visible).
    pub autonumber: Option<(f64, f64, bool)>,
}

/// A `box` grouping of participants.
#[derive(Debug, Clone, Default)]
pub struct SeqBox {
    /// Title text; `None` when the box line only carried a color.
    pub name: Option<String>,
    pub fill: String,
    pub actor_keys: Vec<String>,
}

/// Parsed database.
#[derive(Debug, Default)]
pub struct SequenceDb {
    pub actors: indexmap::IndexMap<String, Actor>,
    pub messages: Vec<SeqMessage>,
    pub boxes: Vec<SeqBox>,
    /// `createdActors`: actor id -> index of the creating message.
    pub created_actors: std::collections::HashMap<String, usize>,
    /// `destroyedActors`: actor id -> index of the destroying message.
    pub destroyed_actors: std::collections::HashMap<String, usize>,
    current_box: Option<usize>,
}

impl SequenceDb {
    fn add_actor(&mut self, id: &str, description: Option<String>) {
        if let Some(existing) = self.actors.get_mut(id) {
            if let Some(d) = description {
                existing.description = d;
            }
            return;
        }
        let prev = self.actors.keys().last().cloned();
        if let Some(p) = &prev {
            self.actors[p].next_actor = Some(id.to_owned());
        }
        let box_index = self.current_box;
        if let Some(bi) = box_index {
            self.boxes[bi].actor_keys.push(id.to_owned());
        }
        self.actors.insert(
            id.to_owned(),
            Actor {
                name: id.to_owned(),
                description: description.unwrap_or_else(|| id.to_owned()),
                prev_actor: prev,
                box_index,
                ..Actor::default()
            },
        );
    }

    fn add_signal(&mut self, from: &str, to: &str, message: &str, ty: i32, activate: bool) {
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: from.to_owned(),
            to: to.to_owned(),
            message: message.to_owned(),
            ty,
            activate,
            ..SeqMessage::default()
        });
    }
}

/// CSS named colors (`CSS.supports('color', x)` for the `\w*` capture).
const CSS_NAMED_COLORS: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "transparent",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
    "currentcolor",
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
];

/// `parseBoxData`: leading color token (or `rgb()/hsl()` function) + title.
fn parse_box_data(s: &str) -> (String, Option<String>) {
    let lower = s.to_lowercase();
    let (color, rest) = if ["rgb", "rgba", "hsl", "hsla"].iter().any(|f| {
        lower
            .strip_prefix(f)
            .is_some_and(|r| r.trim_start().starts_with('('))
    }) {
        // `\(.*\)` is greedy: through the last ')'.
        s.rfind(')').map_or((s, ""), |i| (&s[..=i], &s[i + 1..]))
    } else {
        let end = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(s.len());
        (&s[..end], &s[end..])
    };
    let color_ok = if color.is_empty() {
        false
    } else if color.contains('(') {
        true
    } else {
        CSS_NAMED_COLORS.contains(&color.to_lowercase().as_str())
    };
    let (color, title) = if color_ok {
        (color.to_owned(), rest.trim())
    } else {
        ("transparent".to_owned(), s.trim())
    };
    let title = if title.is_empty() {
        None
    } else {
        Some(title.to_owned())
    };
    (color, title)
}

/// `parseMessage`: trim and strip <wrap:/nowrap>: prefixes.
fn parse_message(text: &str) -> String {
    let t = text.trim();
    let t = t
        .strip_prefix("wrap:")
        .or_else(|| t.strip_prefix(":wrap:"))
        .or_else(|| t.strip_prefix("nowrap:"))
        .or_else(|| t.strip_prefix(":nowrap:"))
        .unwrap_or(t);
    t.trim().to_owned()
}

/// Parses sequenceDiagram source.
///
/// # Errors
///
/// Returns [`SeqParseError`] when the source doesn't match the supported
/// grammar subset.
#[allow(clippy::too_many_lines)]
pub fn parse(source: &str) -> Result<SequenceDb, SeqParseError> {
    let mut db = SequenceDb::default();
    let mut found_header = false;
    let mut block_stack: Vec<i32> = Vec::new();
    let mut in_box = false;
    let lines_iter = source.lines().peekable();
    for raw in lines_iter {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found_header {
            if line.starts_with("sequenceDiagram") {
                found_header = true;
                continue;
            }
            return Err(SeqParseError {
                message: format!("expected sequenceDiagram header, got {line:?}"),
            });
        }

        let lower = line.to_lowercase();

        if let Some(rest) = strip_keyword(line, "destroy") {
            let actor = rest.trim().to_owned();
            let idx = db.messages.len();
            db.destroyed_actors.insert(actor, idx);
            continue;
        }
        let (line, created) =
            strip_keyword(line, "create").map_or((line, false), |rest| (rest.trim(), true));
        let participant = strip_keyword(line, "participant").map(|r| (r, false));
        let participant = participant.or_else(|| strip_keyword(line, "actor").map(|r| (r, true)));
        if let Some((rest, is_actor_man)) = participant {
            // participant X [as Alias] [@{ "type": "..." }]
            let mut rest = rest.trim();
            let mut meta_type = String::new();
            if let Some(at) = rest.find("@{") {
                let meta = rest[at + 2..].trim_end_matches('}');
                // Minimal JSON: "type" : "database"
                if let Some(tpos) = meta.find("\"type\"") {
                    let after = &meta[tpos + 6..];
                    if let Some(c) = after.find(':') {
                        let val = after[c + 1..].trim();
                        let val = val.trim_matches(|ch| ch == '"' || ch == ',' || ch == ' ');
                        val.clone_into(&mut meta_type);
                    }
                }
                rest = rest[..at].trim();
            }
            let id = if let Some((id, alias)) = split_as(rest) {
                db.add_actor(id.trim(), Some(alias.trim().to_owned()));
                id.trim().to_owned()
            } else {
                db.add_actor(rest, None);
                rest.to_owned()
            };
            if let Some(a) = db.actors.get_mut(&id) {
                if is_actor_man {
                    a.is_actor_man = true;
                }
                if !meta_type.is_empty() {
                    a.meta_type = meta_type;
                }
            }
            if created {
                let idx = db.messages.len();
                db.created_actors.insert(id, idx);
            }
            continue;
        }

        if let Some(rest) = strip_keyword(line, "link") {
            // link Actor: Label @ URL
            if let Some(colon) = rest.find(':') {
                let actor = rest[..colon].trim().to_owned();
                let after = &rest[colon + 1..];
                if let Some(at) = after.find('@') {
                    let label = after[..at].trim().to_owned();
                    let url = after[at + 1..].trim().to_owned();
                    if let Some(a) = db.actors.get_mut(&actor) {
                        a.links.push((label, url));
                    }
                }
            }
            continue;
        }
        if lower.starts_with("note ") {
            let rest = line[5..].trim();
            let (placement, after) = if let Some(r) = strip_keyword(rest, "over") {
                (OVER, r)
            } else if let Some(r) = rest
                .strip_prefix("left of")
                .or_else(|| rest.strip_prefix("Left of"))
                .or_else(|| rest.strip_prefix("Left Of"))
            {
                (LEFTOF, r)
            } else if let Some(r) = rest
                .strip_prefix("right of")
                .or_else(|| rest.strip_prefix("Right of"))
                .or_else(|| rest.strip_prefix("Right Of"))
            {
                (RIGHTOF, r)
            } else {
                return Err(SeqParseError {
                    message: format!("bad note statement: {line}"),
                });
            };
            let after = after.trim();
            let Some(colon) = after.find(':') else {
                return Err(SeqParseError {
                    message: format!("note missing text: {line}"),
                });
            };
            let actors_part = after[..colon].trim();
            let text = parse_message(&after[colon + 1..]);
            let (from, to) = if placement == OVER {
                if let Some((a, b)) = actors_part.split_once(',') {
                    (a.trim().to_owned(), b.trim().to_owned())
                } else {
                    (actors_part.to_owned(), actors_part.to_owned())
                }
            } else {
                (actors_part.to_owned(), actors_part.to_owned())
            };
            db.add_actor(&from, None);
            if to != from {
                db.add_actor(&to, None);
            }
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                from,
                to,
                message: text,
                ty: NOTE,
                placement: Some(placement),
                ..SeqMessage::default()
            });
            continue;
        }

        if let Some(rest) = strip_keyword(line, "box") {
            let (fill, name) = parse_box_data(rest.trim());
            db.boxes.push(SeqBox {
                name,
                fill,
                actor_keys: Vec::new(),
            });
            db.current_box = Some(db.boxes.len() - 1);
            in_box = true;
            continue;
        }

        let mut block_start = None;
        for (kw, start, end) in [
            ("loop", LOOP_START, LOOP_END),
            ("alt", ALT_START, ALT_END),
            ("opt", OPT_START, OPT_END),
            ("par", PAR_START, PAR_END),
            ("critical", CRITICAL_START, CRITICAL_END),
            ("break", BREAK_START, BREAK_END),
            ("rect", RECT_START, RECT_END),
        ] {
            if let Some(rest) = strip_keyword(line, kw) {
                block_start = Some((rest, start, end));
                break;
            }
        }
        if let Some((rest, start, end)) = block_start {
            block_stack.push(end);
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                message: parse_message(rest),
                ty: start,
                ..SeqMessage::default()
            });
            continue;
        }
        let mut section = None;
        if let Some(rest) = strip_keyword(line, "else") {
            section = Some((rest, ALT_ELSE));
        } else if let Some(rest) = strip_keyword(line, "and") {
            section = Some((rest, PAR_AND));
        } else if let Some(rest) = strip_keyword(line, "option") {
            section = Some((rest, CRITICAL_OPTION));
        }
        if let Some((rest, ty)) = section {
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                message: parse_message(rest),
                ty,
                ..SeqMessage::default()
            });
            continue;
        }
        if lower == "end" {
            if in_box {
                in_box = false;
                db.current_box = None;
                continue;
            }
            let end = block_stack.pop().unwrap_or(LOOP_END);
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                ty: end,
                ..SeqMessage::default()
            });
            continue;
        }
        if lower.starts_with("autonumber") {
            // autonumber [off] [start [step]]
            let rest = line["autonumber".len()..].trim();
            let (visible, nums) = if rest.eq_ignore_ascii_case("off") {
                (false, Vec::new())
            } else {
                (
                    true,
                    rest.split_whitespace()
                        .filter_map(|t| t.parse::<f64>().ok())
                        .collect::<Vec<_>>(),
                )
            };
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                ty: AUTONUMBER,
                autonumber: Some((
                    nums.first().copied().unwrap_or(0.0),
                    nums.get(1).copied().unwrap_or(0.0),
                    visible,
                )),
                ..SeqMessage::default()
            });
            continue;
        }
        if let Some(rest) = strip_keyword(line, "activate") {
            let id = db.messages.len().to_string();
            let actor = rest.trim().to_owned();
            db.messages.push(SeqMessage {
                id,
                from: actor,
                to: String::new(),
                message: String::new(),
                ty: ACTIVE_START,
                ..SeqMessage::default()
            });
            continue;
        }
        if let Some(rest) = strip_keyword(line, "deactivate") {
            let id = db.messages.len().to_string();
            let actor = rest.trim().to_owned();
            db.messages.push(SeqMessage {
                id,
                from: actor,
                to: String::new(),
                message: String::new(),
                ty: ACTIVE_END,
                ..SeqMessage::default()
            });
            continue;
        }
        if lower.starts_with("title")
            || lower.starts_with("acctitle")
            || lower.starts_with("accdescr")
        {
            continue;
        }

        // Message statement: actor ARROW actor : text
        if let Some((from, arrow, to_part)) = split_arrow(line) {
            let (to_raw, text) = match to_part.find(':') {
                Some(c) => (to_part[..c].trim(), parse_message(&to_part[c + 1..])),
                None => (to_part.trim(), String::new()),
            };
            // Trailing +/- activation shorthand on the target.
            #[derive(PartialEq)]
            enum Act {
                None,
                Plus,
                Minus,
            }
            let (act, to_clean) = if let Some(stripped) = to_raw.strip_prefix('+') {
                (Act::Plus, stripped.trim())
            } else if let Some(stripped) = to_raw.strip_prefix('-') {
                (Act::Minus, stripped.trim())
            } else {
                (Act::None, to_raw)
            };
            let ty = match arrow {
                "->>" => SOLID,
                "-->>" => DOTTED,
                "->" => SOLID_OPEN,
                "-->" => DOTTED_OPEN,
                "-x" => SOLID_CROSS,
                "--x" => DOTTED_CROSS,
                "-)" => SOLID_POINT,
                "--)" => DOTTED_POINT,
                "<<->>" => BIDIRECTIONAL_SOLID,
                "<<-->>" => BIDIRECTIONAL_DOTTED,
                _ => {
                    return Err(SeqParseError {
                        message: format!("unsupported arrow {arrow}"),
                    });
                }
            };
            let from = from.trim();
            db.add_actor(from, None);
            db.add_actor(to_clean, None);
            db.add_signal(from, to_clean, &text, ty, act == Act::Plus);
            match act {
                Act::Plus => {
                    let id = db.messages.len().to_string();
                    db.messages.push(SeqMessage {
                        id,
                        from: to_clean.to_owned(),
                        ty: ACTIVE_START,
                        ..SeqMessage::default()
                    });
                }
                Act::Minus => {
                    let id = db.messages.len().to_string();
                    db.messages.push(SeqMessage {
                        id,
                        from: from.to_owned(),
                        ty: ACTIVE_END,
                        ..SeqMessage::default()
                    });
                }
                Act::None => {}
            }
            continue;
        }

        return Err(SeqParseError {
            message: format!("unsupported statement: {line}"),
        });
    }
    if !found_header {
        return Err(SeqParseError {
            message: "missing sequenceDiagram header".to_owned(),
        });
    }
    Ok(db)
}

/// Strips a leading keyword (case-insensitive) followed by whitespace.
fn strip_keyword<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    if line.len() > kw.len()
        && line[..kw.len()].eq_ignore_ascii_case(kw)
        && line.as_bytes()[kw.len()].is_ascii_whitespace()
    {
        Some(&line[kw.len() + 1..])
    } else {
        None
    }
}

/// `participant A as B`.
fn split_as(rest: &str) -> Option<(&str, &str)> {
    let idx = rest.find(" as ")?;
    Some((&rest[..idx], &rest[idx + 4..]))
}

/// Finds the message arrow, longest-first.
fn split_arrow(line: &str) -> Option<(&str, &'static str, &str)> {
    const ARROWS: &[&str] = &[
        "<<-->>", "<<->>", "-->>", "->>", "--x", "--)", "-->", "-x", "-)", "->",
    ];
    let mut best: Option<(usize, &'static str)> = None;
    for a in ARROWS {
        if let Some(i) = line.find(a) {
            match best {
                Some((bi, ba)) => {
                    if i < bi || (i == bi && a.len() > ba.len()) {
                        best = Some((i, a));
                    }
                }
                None => best = Some((i, a)),
            }
        }
    }
    let (i, arrow) = best?;
    Some((&line[..i], arrow, &line[i + arrow.len()..]))
}
