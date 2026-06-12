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
}

/// A message/event row from the db.
#[derive(Debug, Clone)]
pub struct SeqMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub message: String,
    /// `LINETYPE` value; `None` for plain `addMessage` entries (unused here).
    pub ty: i32,
    pub placement: Option<i32>,
    pub activate: bool,
}

/// Parsed database.
#[derive(Debug, Default)]
pub struct SequenceDb {
    pub actors: indexmap::IndexMap<String, Actor>,
    pub messages: Vec<SeqMessage>,
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
        self.actors.insert(
            id.to_owned(),
            Actor {
                name: id.to_owned(),
                description: description.unwrap_or_else(|| id.to_owned()),
                prev_actor: prev,
                ..Actor::default()
            },
        );
    }

    fn add_signal(&mut self, from: &str, to: &str, message: &str, ty: i32) {
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: from.to_owned(),
            to: to.to_owned(),
            message: message.to_owned(),
            ty,
            placement: None,
            activate: false,
        });
    }
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

        if let Some(rest) =
            strip_keyword(line, "participant").or_else(|| strip_keyword(line, "actor"))
        {
            // participant X [as Alias]
            let rest = rest.trim();
            if let Some((id, alias)) = split_as(rest) {
                db.add_actor(id.trim(), Some(alias.trim().to_owned()));
            } else {
                db.add_actor(rest, None);
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
                activate: false,
            });
            continue;
        }

        if let Some(rest) = strip_keyword(line, "loop") {
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                from: String::new(),
                to: String::new(),
                message: parse_message(rest),
                ty: LOOP_START,
                placement: None,
                activate: false,
            });
            continue;
        }
        if lower == "end" {
            let id = db.messages.len().to_string();
            db.messages.push(SeqMessage {
                id,
                from: String::new(),
                to: String::new(),
                message: String::new(),
                ty: LOOP_END,
                placement: None,
                activate: false,
            });
            continue;
        }
        if lower.starts_with("autonumber") {
            continue;
        }
        if let Some(rest) = strip_keyword(line, "activate") {
            let id = db.messages.len().to_string();
            let actor = rest.trim().to_owned();
            db.add_actor(&actor, None);
            db.messages.push(SeqMessage {
                id,
                from: actor,
                to: String::new(),
                message: String::new(),
                ty: ACTIVE_START,
                placement: None,
                activate: false,
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
                placement: None,
                activate: false,
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
            let (activate_plus, to_clean) = if let Some(stripped) = to_raw.strip_prefix('+') {
                (true, stripped.trim())
            } else if let Some(stripped) = to_raw.strip_prefix('-') {
                (false, stripped.trim())
            } else {
                (false, to_raw)
            };
            let _ = activate_plus;
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
            db.add_signal(from, to_clean, &text, ty);
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
