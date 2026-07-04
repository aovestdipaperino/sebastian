//! gitGraph support: parser subset, `gitGraphAst.ts` semantics, and a
//! direct port of `gitGraphRenderer.ts` (LR/TB/BT orientations, classic look).

#![allow(
    clippy::assigning_clones,
    clippy::manual_strip,
    clippy::nonminimal_bool
)]
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for gitGraph source.
#[derive(Debug)]
pub struct GitParseError {
    pub message: String,
}

impl std::fmt::Display for GitParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gitGraph parse error: {}", self.message)
    }
}

impl std::error::Error for GitParseError {}

const LAYOUT_OFFSET: f64 = 10.0;
const COMMIT_STEP: f64 = 40.0;
const PX: f64 = 4.0;
const PY: f64 = 2.0;
const THEME_COLOR_LIMIT: usize = 8;

// commitType.
const NORMAL: u8 = 0;
const REVERSE: u8 = 1;
const HIGHLIGHT: u8 = 2;
const MERGE: u8 = 3;
const CHERRY_PICK: u8 = 4;

#[derive(Debug, Clone)]
struct Commit {
    id: String,
    seq: usize,
    ty: u8,
    custom_type: Option<u8>,
    custom_id: bool,
    tags: Vec<String>,
    parents: Vec<String>,
    branch: String,
}

#[derive(Debug, Default)]
struct Db {
    commits: Vec<Commit>, // insertion order == seq order
    branches: indexmap::IndexMap<String, Option<String>>, // name -> head commit id
    branch_order: indexmap::IndexMap<String, Option<f64>>,
    curr_branch: String,
    head: Option<String>,
    seq: usize,
    /// Diagram orientation: "LR" (default), "TB", or "BT".
    dir: String,
}

impl Db {
    fn get(&self, id: &str) -> Option<&Commit> {
        self.commits.iter().find(|c| c.id == id)
    }

    /// Deterministic stand-in for the upstream `random({length: 7})` hex id.
    fn auto_id(&mut self) -> String {
        // Simple LCG over the sequence number, hex-formatted to 7 chars.
        let n = self.seq as u64;
        let h = n
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x1234_5678);
        format!("{}-{:07x}", self.seq, h & 0xFFF_FFFF)
    }

    fn push_commit(&mut self, mut c: Commit) {
        c.seq = self.seq;
        self.seq += 1;
        self.head = Some(c.id.clone());
        self.branches
            .insert(self.curr_branch.clone(), Some(c.id.clone()));
        self.commits.push(c);
    }

    fn branches_as_obj_array(&self) -> Vec<String> {
        let mut arr: Vec<(String, f64)> = self
            .branch_order
            .iter()
            .enumerate()
            .map(|(i, (name, order))| {
                #[allow(clippy::cast_precision_loss)]
                let o = order.unwrap_or_else(|| format!("0.{i}").parse().unwrap_or(0.0));
                (name.clone(), o)
            })
            .collect();
        arr.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        arr.into_iter().map(|(n, _)| n).collect()
    }
}

// ------------------------------------------------------------------ parse --

fn take_quoted_option<'a>(rest: &'a str, key: &str) -> Option<(String, &'a str)> {
    let pat = format!("{key}:");
    let idx = rest.find(&pat)?;
    let after = rest[idx + pat.len()..].trim_start();
    let inner = after.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some((inner[..end].to_owned(), &rest[..idx]))
}

fn parse(source: &str) -> Result<Db, GitParseError> {
    let mut db = Db {
        curr_branch: "main".to_owned(),
        ..Db::default()
    };
    db.branches.insert("main".to_owned(), None);
    db.branch_order.insert("main".to_owned(), Some(0.0));

    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line.starts_with("gitGraph") {
                found_header = true;
                let rest = line["gitGraph".len()..].trim().trim_end_matches(':').trim();
                db.dir = match rest {
                    "" | "LR" => "LR".to_owned(),
                    "TB" | "BT" => rest.to_owned(),
                    other => {
                        return Err(GitParseError {
                            message: format!("unsupported gitGraph orientation: {other}"),
                        });
                    }
                };
                continue;
            }
            return Err(GitParseError {
                message: format!("expected gitGraph header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("commit") {
            let mut rest = rest.trim().to_owned();
            let mut id = String::new();
            let mut tags: Vec<String> = Vec::new();
            let mut ty = NORMAL;
            let mut _msg = String::new();
            loop {
                if let Some((v, before)) = take_quoted_option(&rest, "id") {
                    id = v;
                    let after_start = rest.find("id:").map(|i| {
                        let a = rest[i + 3..].trim_start();
                        let q = a.strip_prefix('"').and_then(|x| x.find('"')).unwrap_or(0);
                        rest.len() - a.len() + q + 2
                    });
                    let mut nr = before.to_owned();
                    if let Some(pos) = after_start {
                        nr.push_str(&rest[pos.min(rest.len())..]);
                    }
                    rest = nr;
                } else if let Some((v, before)) = take_quoted_option(&rest, "tag") {
                    tags.push(v.clone());
                    let i = rest.find("tag:").expect("tag present");
                    let a = rest[i + 4..].trim_start();
                    let q = a.strip_prefix('"').and_then(|x| x.find('"')).unwrap_or(0);
                    let consumed = rest.len() - a.len() + q + 2;
                    let mut nr = before.to_owned();
                    nr.push_str(&rest[consumed.min(rest.len())..]);
                    rest = nr;
                } else if let Some((v, before)) = take_quoted_option(&rest, "msg") {
                    _msg = v.clone();
                    let i = rest.find("msg:").expect("msg present");
                    let a = rest[i + 4..].trim_start();
                    let q = a.strip_prefix('"').and_then(|x| x.find('"')).unwrap_or(0);
                    let consumed = rest.len() - a.len() + q + 2;
                    let mut nr = before.to_owned();
                    nr.push_str(&rest[consumed.min(rest.len())..]);
                    rest = nr;
                } else if let Some(i) = rest.find("type:") {
                    let after = rest[i + 5..].trim_start();
                    let word: String = after
                        .chars()
                        .take_while(char::is_ascii_alphabetic)
                        .collect();
                    ty = match word.as_str() {
                        "REVERSE" => REVERSE,
                        "HIGHLIGHT" => HIGHLIGHT,
                        _ => NORMAL,
                    };
                    let consumed = rest.len() - after.len() + word.len();
                    let before = rest[..i].to_owned();
                    rest = format!("{}{}", before, &rest[consumed.min(rest.len())..]);
                } else {
                    break;
                }
            }
            let id = if id.is_empty() { db.auto_id() } else { id };
            let parents = db.head.iter().cloned().collect();
            let branch = db.curr_branch.clone();
            db.push_commit(Commit {
                id,
                seq: 0,
                ty,
                custom_type: None,
                custom_id: false,
                tags,
                parents,
                branch,
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("branch") {
            let mut parts = rest.split_whitespace();
            let name = parts.next().unwrap_or_default().to_owned();
            let mut order: Option<f64> = None;
            let joined: String = parts.collect::<Vec<_>>().join(" ");
            if let Some(i) = joined.find("order:") {
                order = joined[i + 6..].trim().parse().ok();
            }
            if db.branches.contains_key(&name) {
                return Err(GitParseError {
                    message: format!("Trying to create an existing branch: {name}"),
                });
            }
            db.branches.insert(name.clone(), db.head.clone());
            db.branch_order.insert(name.clone(), order);
            db.curr_branch = name;
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("checkout")
            .or_else(|| line.strip_prefix("switch"))
        {
            let name = rest.trim().to_owned();
            if !db.branches.contains_key(&name) {
                return Err(GitParseError {
                    message: format!("checkout of unknown branch: {name}"),
                });
            }
            db.head = db.branches[&name].clone();
            db.curr_branch = name;
            continue;
        }
        if let Some(rest) = line.strip_prefix("merge") {
            let mut parts = rest.split_whitespace();
            let other = parts.next().unwrap_or_default().to_owned();
            let remainder: String = parts.collect::<Vec<_>>().join(" ");
            let mut custom_id = String::new();
            let mut tags: Vec<String> = Vec::new();
            if let Some((v, _)) = take_quoted_option(&remainder, "id") {
                custom_id = v;
            }
            let mut scan = remainder.as_str();
            while let Some(i) = scan.find("tag:") {
                let a = scan[i + 4..].trim_start();
                if let Some(inner) = a.strip_prefix('"')
                    && let Some(e) = inner.find('"')
                {
                    tags.push(inner[..e].to_owned());
                    scan = &inner[e + 1..];
                } else {
                    break;
                }
            }
            let other_head = db.branches.get(&other).cloned().flatten();
            let verified = other_head.unwrap_or_default();
            let id = if custom_id.is_empty() {
                db.auto_id()
            } else {
                custom_id.clone()
            };
            let parents: Vec<String> = match &db.head {
                Some(h) => vec![h.clone(), verified],
                None => Vec::new(),
            };
            let branch = db.curr_branch.clone();
            db.push_commit(Commit {
                id,
                seq: 0,
                ty: MERGE,
                custom_type: None,
                custom_id: !custom_id.is_empty(),
                tags,
                parents,
                branch,
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("cherry-pick") {
            let (source_id, _) = take_quoted_option(rest, "id").ok_or_else(|| GitParseError {
                message: format!("cherry-pick needs an id: {line}"),
            })?;
            let mut tags: Vec<String> = Vec::new();
            let mut scan = rest;
            while let Some(i) = scan.find("tag:") {
                let a = scan[i + 4..].trim_start();
                if let Some(inner) = a.strip_prefix('"')
                    && let Some(e) = inner.find('"')
                {
                    tags.push(inner[..e].to_owned());
                    scan = &inner[e + 1..];
                } else {
                    break;
                }
            }
            let source = db.get(&source_id).cloned().ok_or_else(|| GitParseError {
                message: format!("cherry-pick source not found: {source_id}"),
            })?;
            let id = db.auto_id();
            let parents: Vec<String> = match &db.head {
                Some(h) => vec![h.clone(), source.id.clone()],
                None => Vec::new(),
            };
            let tags = if tags.is_empty() {
                vec![format!("cherry-pick:{}", source.id)]
            } else {
                tags
            };
            let branch = db.curr_branch.clone();
            db.push_commit(Commit {
                id,
                seq: 0,
                ty: CHERRY_PICK,
                custom_type: None,
                custom_id: false,
                tags,
                parents,
                branch,
            });
            continue;
        }
        return Err(GitParseError {
            message: format!("unsupported gitGraph statement: {line}"),
        });
    }
    if !found_header {
        return Err(GitParseError {
            message: "missing gitGraph header".to_owned(),
        });
    }
    Ok(db)
}

// ------------------------------------------------------------------- bbox --

/// Union accumulator over Blink `FloatRect`s (f32 x/y/w/h cascade).
struct Acc {
    rect: crate::render::bbox::Rect,
}

impl Acc {
    fn new() -> Self {
        Acc {
            rect: crate::render::bbox::Rect::from_geometry(0.0, 0.0, -1.0, -1.0),
        }
    }
    fn add_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let r = crate::render::bbox::Rect::from_geometry(x, y, w, h);
        self.rect.union_with(&r);
    }
    /// Lines union even with zero area.
    fn add_line(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let r = crate::render::bbox::Rect::from_line_geometry(x, y, w, h);
        self.rect.union_with(&r);
    }
    fn add_bounds(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.add_rect(x0, y0, x1 - x0, y1 - y0);
    }
    fn x0(&self) -> f64 {
        self.rect.min_x
    }
    fn y0(&self) -> f64 {
        self.rect.min_y
    }
    fn width(&self) -> f64 {
        self.rect.width()
    }
    fn height(&self) -> f64 {
        self.rect.height()
    }
}

// ---------------------------------------------------------------- drawing --

/// Renders gitGraph source to a complete SVG document string.
///
/// # Errors
/// Returns a [`GitParseError`] when the source is not a valid gitGraph.
#[allow(clippy::too_many_lines)]
pub fn render_gitgraph(source: &str, id: &str) -> Result<String, GitParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let measurer = crate::text::TextMeasurer::new();
    let rotate_commit_label = true;
    // Blink unions bboxes hierarchically (per-group f32 RectF cascade).
    let mut acc_branches = Acc::new();
    let mut acc_arrows = Acc::new();
    let mut acc_bullets = Acc::new();
    let mut acc_labels = Acc::new();

    // Text bbox helpers (Trebuchet, getBBox semantics: ceil-1/64 advance and
    // the integer line box).
    let text_dims = |t: &str, fs: f64| -> (f64, f64) {
        (measurer.measure_width(t, fs), measurer.ink_height(t, fs))
    };

    let branches = db.branches_as_obj_array();
    let dir = db.dir.as_str();
    let is_vert = dir == "TB" || dir == "BT";

    // Branch positions. `setBranchPosition`: pos += 50 + (rotate?40) +
    // (TB/BT ? bbox.width/2 : 0).
    let mut branch_pos: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    {
        let mut pos = 0.0;
        for (index, name) in branches.iter().enumerate() {
            branch_pos.insert(name.clone(), (pos, index));
            let bw_half = if is_vert {
                text_dims(name, 16.0).0 / 2.0
            } else {
                0.0
            };
            pos += 50.0 + if rotate_commit_label { 40.0 } else { 0.0 } + bw_half;
        }
    }

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_gitgraph_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    // drawCommits(modifyGraph=false): position pass.
    let mut commit_pos: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();
    let mut max_pos = 0.0f64;
    // BT reverses the seq-sorted commit order for positioning.
    let ordered: Vec<&Commit> = if dir == "BT" {
        db.commits.iter().rev().collect()
    } else {
        db.commits.iter().collect()
    };
    {
        let mut pos = if is_vert { 30.0 } else { 0.0 }; // defaultPos = 30
        for c in &ordered {
            let pos_with_offset = pos + LAYOUT_OFFSET;
            let (branch_p, _) = branch_pos[&c.branch];
            if is_vert {
                // x = branch pos, y = posWithOffset
                commit_pos.insert(c.id.clone(), (branch_p, pos_with_offset));
            } else {
                commit_pos.insert(c.id.clone(), (pos_with_offset, branch_p - 2.0));
            }
            pos += COMMIT_STEP + LAYOUT_OFFSET;
            if pos > max_pos {
                max_pos = pos;
            }
        }
    }
    // The first drawCommits appends empty groups.
    let gb0 = append(&svg, "g");
    set_attr(&gb0, "class", "commit-bullets");
    let gl0 = append(&svg, "g");
    set_attr(&gl0, "class", "commit-labels");

    // drawBranches.
    let mut lanes: Vec<f64> = Vec::new();
    {
        let g = append(&svg, "g");
        for (index, name) in branches.iter().enumerate() {
            let color_idx = index % THEME_COLOR_LIMIT;
            let (pos, _) = branch_pos[name];
            let spine_y = if is_vert { pos } else { pos - 2.0 };
            let line = append(&g, "line");
            if dir == "TB" {
                set_attr(&line, "x1", js_num(pos));
                set_attr(&line, "y1", "30");
                set_attr(&line, "x2", js_num(pos));
                set_attr(&line, "y2", js_num(max_pos));
                acc_branches.add_line(pos, 30.0, 0.0, max_pos - 30.0);
            } else if dir == "BT" {
                set_attr(&line, "x1", js_num(pos));
                set_attr(&line, "y1", js_num(max_pos));
                set_attr(&line, "x2", js_num(pos));
                set_attr(&line, "y2", "30");
                acc_branches.add_line(pos, 30.0, 0.0, max_pos - 30.0);
            } else {
                set_attr(&line, "x1", "0");
                set_attr(&line, "y1", js_num(spine_y));
                set_attr(&line, "x2", js_num(max_pos));
                set_attr(&line, "y2", js_num(spine_y));
                acc_branches.add_line(0.0, spine_y, max_pos, 0.0);
            }
            set_attr(&line, "class", format!("branch branch{color_idx}"));
            lanes.push(spine_y);

            let bkg = append(&g, "rect");
            let branch_label = append(&g, "g");
            set_attr(&branch_label, "class", "branchLabel");
            let label = append(&branch_label, "g");
            set_attr(&label, "class", format!("label branch-label{color_idx}"));
            let text = append(&label, "text");
            let tspan = append(&text, "tspan");
            set_attr(&tspan, "xml:space", "preserve");
            set_attr(&tspan, "dy", "1em");
            set_attr(&tspan, "x", "0");
            set_attr(&tspan, "class", "row");
            set_text(&tspan, name);
            let (bw, bh) = text_dims(name, 16.0);

            set_attr(&bkg, "class", format!("branchLabelBkg label{color_idx}"));
            set_attr(&bkg, "style", "");
            set_attr(&bkg, "rx", "4");
            set_attr(&bkg, "ry", "4");
            if is_vert {
                let by = if dir == "BT" { max_pos } else { 0.0 };
                let bx = pos - bw / 2.0 - 10.0;
                set_attr(&bkg, "x", js_num(bx));
                set_attr(&bkg, "y", js_num(by));
                set_attr(&bkg, "width", js_num(bw + 18.0));
                set_attr(&bkg, "height", js_num(bh + 4.0));
                set_attr(
                    &label,
                    "transform",
                    format!(
                        "translate({}, {})",
                        js_num(pos - bw / 2.0 - 5.0),
                        js_num(by)
                    ),
                );
                acc_branches.add_rect(bx, by, bw + 18.0, bh + 4.0);
            } else {
                let bx = -bw - 4.0 - if rotate_commit_label { 30.0 } else { 0.0 };
                set_attr(&bkg, "x", js_num(bx));
                set_attr(&bkg, "y", js_num(-bh / 2.0 + 10.0));
                set_attr(&bkg, "width", js_num(bw + 18.0));
                set_attr(&bkg, "height", js_num(bh + 4.0));
                let ltx = -bw - 14.0 - if rotate_commit_label { 30.0 } else { 0.0 };
                let lty = spine_y - bh / 2.0 - 2.0;
                set_attr(
                    &label,
                    "transform",
                    format!("translate({}, {})", js_num(ltx), js_num(lty)),
                );
                let bkg_ty = spine_y - 12.0;
                set_attr(
                    &bkg,
                    "transform",
                    format!("translate(-19, {})", js_num(bkg_ty)),
                );
                acc_branches.add_rect(bx - 19.0, -bh / 2.0 + 10.0 + bkg_ty, bw + 18.0, bh + 4.0);
            }
        }
    }

    // drawArrows.
    {
        let g = append(&svg, "g");
        set_attr(&g, "class", "commit-arrows");
        let find_lane = |lanes: &mut Vec<f64>, y1: f64, y2: f64| -> f64 {
            fn rec(lanes: &mut Vec<f64>, y1: f64, y2: f64, depth: u32) -> f64 {
                let candidate = y1 + (y1 - y2).abs() / 2.0;
                if depth > 5 {
                    return candidate;
                }
                if lanes.iter().all(|l| (l - candidate).abs() >= 10.0) {
                    lanes.push(candidate);
                    return candidate;
                }
                let diff = (y1 - y2).abs();
                rec(lanes, y1, y2 - diff / 5.0, depth + 1)
            }
            rec(lanes, y1, y2, 0)
        };
        for c in &db.commits {
            for parent in &c.parents {
                let Some(pa) = db.get(parent) else { continue };
                let p1 = commit_pos[&pa.id];
                let p2 = commit_pos[&c.id];
                // shouldRerouteArrow
                let commit_b_is_furthest = if is_vert { p1.0 < p2.0 } else { p1.1 < p2.1 };
                let branch_to_get_curve = if commit_b_is_furthest {
                    &c.branch
                } else {
                    &pa.branch
                };
                let needs_reroute = db
                    .commits
                    .iter()
                    .any(|x| x.seq > pa.seq && x.seq < c.seq && x.branch == *branch_to_get_curve);
                let mut color_idx = branch_pos[&c.branch].1;
                if c.ty == MERGE && c.parents.first() != Some(&pa.id) {
                    color_idx = branch_pos[&pa.branch].1;
                }
                let merge_nonfirst = c.ty == MERGE && c.parents.first() != Some(&pa.id);
                let line_def = if is_vert {
                    let tok = |v: f64| js_num(v);
                    let (p1x, p1y, p2x, p2y) = (p1.0, p1.1, p2.0, p2.1);
                    if needs_reroute {
                        let (radius, offset) = (10.0, 10.0);
                        let arc = "A 10 10, 0, 0, 0,".to_owned();
                        let arc2 = "A 10 10, 0, 0, 1,".to_owned();
                        let line_x = if p1x < p2x {
                            find_lane(&mut lanes, p1x, p2x)
                        } else {
                            find_lane(&mut lanes, p2x, p1x)
                        };
                        let sign = if dir == "TB" { 1.0 } else { -1.0 };
                        if p1x < p2x {
                            let (first_arc, second_arc) = if dir == "TB" {
                                (arc2.clone(), arc.clone())
                            } else {
                                (arc.clone(), arc2.clone())
                            };
                            [
                                "M".to_owned(),
                                tok(p1x),
                                tok(p1y),
                                "L".to_owned(),
                                tok(line_x - radius),
                                tok(p1y),
                                first_arc,
                                tok(line_x),
                                tok(p1y + sign * offset),
                                "L".to_owned(),
                                tok(line_x),
                                tok(p2y - sign * radius),
                                second_arc,
                                tok(line_x + offset),
                                tok(p2y),
                                "L".to_owned(),
                                tok(p2x),
                                tok(p2y),
                            ]
                            .join(" ")
                        } else {
                            color_idx = branch_pos[&pa.branch].1;
                            let (first_arc, second_arc) = if dir == "TB" {
                                (arc.clone(), arc2.clone())
                            } else {
                                (arc2.clone(), arc.clone())
                            };
                            [
                                "M".to_owned(),
                                tok(p1x),
                                tok(p1y),
                                "L".to_owned(),
                                tok(line_x + radius),
                                tok(p1y),
                                first_arc,
                                tok(line_x),
                                tok(p1y + sign * offset),
                                "L".to_owned(),
                                tok(line_x),
                                tok(p2y - sign * radius),
                                second_arc,
                                tok(line_x - offset),
                                tok(p2y),
                                "L".to_owned(),
                                tok(p2x),
                                tok(p2y),
                            ]
                            .join(" ")
                        }
                    } else {
                        let (radius, offset) = (20.0, 20.0);
                        let arc = "A 20 20, 0, 0, 0,".to_owned();
                        let arc2 = "A 20 20, 0, 0, 1,".to_owned();
                        let sign = if dir == "TB" { 1.0 } else { -1.0 };
                        if (p1x - p2x).abs() < f64::EPSILON {
                            [
                                "M".to_owned(),
                                tok(p1x),
                                tok(p1y),
                                "L".to_owned(),
                                tok(p2x),
                                tok(p2y),
                            ]
                            .join(" ")
                        } else if p1x < p2x {
                            if merge_nonfirst {
                                let a = if dir == "TB" {
                                    arc.clone()
                                } else {
                                    arc2.clone()
                                };
                                [
                                    "M".to_owned(),
                                    tok(p1x),
                                    tok(p1y),
                                    "L".to_owned(),
                                    tok(p1x),
                                    tok(p2y - sign * radius),
                                    a,
                                    tok(p1x + offset),
                                    tok(p2y),
                                    "L".to_owned(),
                                    tok(p2x),
                                    tok(p2y),
                                ]
                                .join(" ")
                            } else {
                                let a = if dir == "TB" {
                                    arc2.clone()
                                } else {
                                    arc.clone()
                                };
                                [
                                    "M".to_owned(),
                                    tok(p1x),
                                    tok(p1y),
                                    "L".to_owned(),
                                    tok(p2x - radius),
                                    tok(p1y),
                                    a,
                                    tok(p2x),
                                    tok(p1y + sign * offset),
                                    "L".to_owned(),
                                    tok(p2x),
                                    tok(p2y),
                                ]
                                .join(" ")
                            }
                        } else if merge_nonfirst {
                            let a = if dir == "TB" {
                                arc2.clone()
                            } else {
                                arc.clone()
                            };
                            [
                                "M".to_owned(),
                                tok(p1x),
                                tok(p1y),
                                "L".to_owned(),
                                tok(p1x),
                                tok(p2y - sign * radius),
                                a,
                                tok(p1x - offset),
                                tok(p2y),
                                "L".to_owned(),
                                tok(p2x),
                                tok(p2y),
                            ]
                            .join(" ")
                        } else {
                            let a = if dir == "TB" {
                                arc.clone()
                            } else {
                                arc2.clone()
                            };
                            [
                                "M".to_owned(),
                                tok(p1x),
                                tok(p1y),
                                "L".to_owned(),
                                tok(p2x + radius),
                                tok(p1y),
                                a,
                                tok(p2x),
                                tok(p1y + sign * offset),
                                "L".to_owned(),
                                tok(p2x),
                                tok(p2y),
                            ]
                            .join(" ")
                        }
                    }
                } else if needs_reroute {
                    let radius = 10.0;
                    let offset = 10.0;
                    let arc = "A 10 10, 0, 0, 0,";
                    let arc2 = "A 10 10, 0, 0, 1,";
                    let line_y = if p1.1 < p2.1 {
                        find_lane(&mut lanes, p1.1, p2.1)
                    } else {
                        find_lane(&mut lanes, p2.1, p1.1)
                    };
                    if p1.1 < p2.1 {
                        format!(
                            "M {} {} L {} {} {} {} {} L {} {} {} {} {} L {} {}",
                            js_num(p1.0),
                            js_num(p1.1),
                            js_num(p1.0),
                            js_num(line_y - radius),
                            arc,
                            js_num(p1.0 + offset),
                            js_num(line_y),
                            js_num(p2.0 - radius),
                            js_num(line_y),
                            arc2,
                            js_num(p2.0),
                            js_num(line_y + offset),
                            js_num(p2.0),
                            js_num(p2.1)
                        )
                    } else {
                        color_idx = branch_pos[&pa.branch].1;
                        format!(
                            "M {} {} L {} {} {} {} {} L {} {} {} {} {} L {} {}",
                            js_num(p1.0),
                            js_num(p1.1),
                            js_num(p1.0),
                            js_num(line_y + radius),
                            arc2,
                            js_num(p1.0 + offset),
                            js_num(line_y),
                            js_num(p2.0 - radius),
                            js_num(line_y),
                            arc,
                            js_num(p2.0),
                            js_num(line_y - offset),
                            js_num(p2.0),
                            js_num(p2.1)
                        )
                    }
                } else {
                    let radius = 20.0;
                    let offset = 20.0;
                    let arc = "A 20 20, 0, 0, 0,";
                    let arc2 = "A 20 20, 0, 0, 1,";
                    if p1.1 < p2.1 {
                        if c.ty == MERGE && c.parents.first() != Some(&pa.id) {
                            format!(
                                "M {} {} L {} {} {} {} {} L {} {}",
                                js_num(p1.0),
                                js_num(p1.1),
                                js_num(p2.0 - radius),
                                js_num(p1.1),
                                arc2,
                                js_num(p2.0),
                                js_num(p1.1 + offset),
                                js_num(p2.0),
                                js_num(p2.1)
                            )
                        } else {
                            format!(
                                "M {} {} L {} {} {} {} {} L {} {}",
                                js_num(p1.0),
                                js_num(p1.1),
                                js_num(p1.0),
                                js_num(p2.1 - radius),
                                arc,
                                js_num(p1.0 + offset),
                                js_num(p2.1),
                                js_num(p2.0),
                                js_num(p2.1)
                            )
                        }
                    } else if p1.1 > p2.1 {
                        if c.ty == MERGE && c.parents.first() != Some(&pa.id) {
                            format!(
                                "M {} {} L {} {} {} {} {} L {} {}",
                                js_num(p1.0),
                                js_num(p1.1),
                                js_num(p2.0 - radius),
                                js_num(p1.1),
                                arc,
                                js_num(p2.0),
                                js_num(p1.1 - offset),
                                js_num(p2.0),
                                js_num(p2.1)
                            )
                        } else {
                            format!(
                                "M {} {} L {} {} {} {} {} L {} {}",
                                js_num(p1.0),
                                js_num(p1.1),
                                js_num(p1.0),
                                js_num(p2.1 + radius),
                                arc2,
                                js_num(p1.0 + offset),
                                js_num(p2.1),
                                js_num(p2.0),
                                js_num(p2.1)
                            )
                        }
                    } else {
                        format!(
                            "M {} {} L {} {}",
                            js_num(p1.0),
                            js_num(p1.1),
                            js_num(p2.0),
                            js_num(p2.1)
                        )
                    }
                };
                let path = append(&g, "path");
                set_attr(&path, "d", line_def.clone());
                set_attr(
                    &path,
                    "class",
                    format!("arrow arrow{}", color_idx % THEME_COLOR_LIMIT),
                );
                // bbox: all coordinates in the path.
                for tok in line_def
                    .split([' ', ','])
                    .filter(|t| !t.is_empty() && t.parse::<f64>().is_ok())
                    .collect::<Vec<_>>()
                    .chunks(1)
                {
                    let _ = tok;
                }
                acc_arrows.add_line(
                    p1.0.min(p2.0),
                    p1.1.min(p2.1),
                    (p2.0 - p1.0).abs(),
                    (p2.1 - p1.1).abs(),
                );
            }
        }
    }

    // drawCommits(modifyGraph=true).
    let g_bullets = append(&svg, "g");
    set_attr(&g_bullets, "class", "commit-bullets");
    let g_labels = append(&svg, "g");
    set_attr(&g_labels, "class", "commit-labels");
    let show_commit_label = config.git_show_commit_label.unwrap_or(true);
    {
        for c in &ordered {
            let (x, y) = commit_pos[&c.id];
            let pos = x - LAYOUT_OFFSET; // LR label anchor (unused when vert)
            let pos_with_offset = x;
            let branch_index = branch_pos[&c.branch].1;
            let symbol = c.custom_type.unwrap_or(c.ty);
            let type_class = match symbol {
                REVERSE => "commit-reverse",
                HIGHLIGHT => "commit-highlight",
                MERGE => "commit-merge",
                CHERRY_PICK => "commit-cherry-pick",
                _ => "commit-normal",
            };
            let color_idx = branch_index % THEME_COLOR_LIMIT;
            // Bullet.
            if symbol == HIGHLIGHT {
                let r1 = append(&g_bullets, "rect");
                set_attr(&r1, "x", js_num(x - 10.0));
                set_attr(&r1, "y", js_num(y - 10.0));
                set_attr(&r1, "width", "20");
                set_attr(&r1, "height", "20");
                set_attr(
                    &r1,
                    "class",
                    format!(
                        "commit {} commit-highlight{color_idx} {type_class}-outer",
                        c.id
                    ),
                );
                let r2 = append(&g_bullets, "rect");
                set_attr(&r2, "x", js_num(x - 6.0));
                set_attr(&r2, "y", js_num(y - 6.0));
                set_attr(&r2, "width", "12");
                set_attr(&r2, "height", "12");
                set_attr(
                    &r2,
                    "class",
                    format!("commit {} commit{color_idx} {type_class}-inner", c.id),
                );
                acc_bullets.add_rect(x - 10.0, y - 10.0, 20.0, 20.0);
            } else if symbol == CHERRY_PICK {
                let circle = append(&g_bullets, "circle");
                set_attr(&circle, "cx", js_num(x));
                set_attr(&circle, "cy", js_num(y));
                set_attr(&circle, "r", "10");
                set_attr(&circle, "class", format!("commit {} {type_class}", c.id));
                for dx in [-3.0, 3.0] {
                    let c2 = append(&g_bullets, "circle");
                    set_attr(&c2, "cx", js_num(x + dx));
                    set_attr(&c2, "cy", js_num(y + 2.0));
                    set_attr(&c2, "r", "2.75");
                    set_attr(&c2, "fill", "#fff");
                    set_attr(&c2, "class", format!("commit {} {type_class}", c.id));
                }
                for dx in [3.0, -3.0] {
                    let l = append(&g_bullets, "line");
                    set_attr(&l, "x1", js_num(x + dx));
                    set_attr(&l, "y1", js_num(y + 1.0));
                    set_attr(&l, "x2", js_num(x));
                    set_attr(&l, "y2", js_num(y - 5.0));
                    set_attr(&l, "stroke", "#fff");
                    set_attr(&l, "class", format!("commit {} {type_class}", c.id));
                }
                acc_bullets.add_rect(x - 10.0, y - 10.0, 20.0, 20.0);
            } else {
                let circle = append(&g_bullets, "circle");
                set_attr(&circle, "cx", js_num(x));
                set_attr(&circle, "cy", js_num(y));
                set_attr(&circle, "r", "10");
                set_attr(
                    &circle,
                    "class",
                    format!("commit {} commit{color_idx}", c.id),
                );
                if symbol == MERGE {
                    let c2 = append(&g_bullets, "circle");
                    set_attr(&c2, "cx", js_num(x));
                    set_attr(&c2, "cy", js_num(y));
                    set_attr(&c2, "r", "6");
                    set_attr(
                        &c2,
                        "class",
                        format!("commit {type_class} {} commit{color_idx}", c.id),
                    );
                }
                if symbol == REVERSE {
                    let cross = append(&g_bullets, "path");
                    set_attr(
                        &cross,
                        "d",
                        format!(
                            "M {},{}L{},{}M{},{}L{},{}",
                            js_num(x - 5.0),
                            js_num(y - 5.0),
                            js_num(x + 5.0),
                            js_num(y + 5.0),
                            js_num(x - 5.0),
                            js_num(y + 5.0),
                            js_num(x + 5.0),
                            js_num(y - 5.0)
                        ),
                    );
                    set_attr(
                        &cross,
                        "class",
                        format!("commit {type_class} {} commit{color_idx}", c.id),
                    );
                }
                acc_bullets.add_rect(x - 10.0, y - 10.0, 20.0, 20.0);
            }

            // Label (TB/BT): text + bkg each rotated -45 about the commit.
            if show_commit_label
                && is_vert
                && symbol != CHERRY_PICK
                && ((c.custom_id && c.ty == MERGE) || c.ty != MERGE)
            {
                let (cx, cy) = (x, y);
                let (bw, bh) = text_dims(&c.id, 10.0);
                let wrapper = append(&g_labels, "g");
                let label_bkg = append(&wrapper, "rect");
                set_attr(&label_bkg, "class", "commit-label-bkg");
                let bx = cx - (bw + 4.0 * PX + 5.0);
                let by = cy - 12.0;
                let (bkw, bkh) = (bw + 2.0 * PY, bh + 2.0 * PY);
                set_attr(&label_bkg, "x", js_num(bx));
                set_attr(&label_bkg, "y", js_num(by));
                set_attr(&label_bkg, "width", js_num(bkw));
                set_attr(&label_bkg, "height", js_num(bkh));
                let text = append(&wrapper, "text");
                set_attr(&text, "x", js_num(cx - (bw + 4.0 * PX)));
                set_attr(&text, "y", js_num(cy + bh - 12.0));
                set_attr(&text, "class", "commit-label");
                set_text(&text, &c.id);
                if rotate_commit_label {
                    let rot = format!("rotate({}, {}, {})", js_num(-45.0), js_num(cx), js_num(cy));
                    set_attr(&text, "transform", &rot);
                    set_attr(&label_bkg, "transform", &rot);
                    // getBBox: bkg corners mapped through rotate(-45, cx, cy).
                    let rad = (-45.0f64).to_radians();
                    let (sa, ca) = (rad.sin(), rad.cos());
                    let e = cx - (ca * cx - sa * cy);
                    let f = cy - (sa * cx + ca * cy);
                    let (mut x0, mut y0, mut x1, mut y1) = (
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    );
                    for (px, py) in [
                        (bx, by),
                        (bx + bkw, by),
                        (bx, by + bkh),
                        (bx + bkw, by + bkh),
                    ] {
                        #[allow(clippy::cast_possible_truncation)]
                        let mx = f64::from((ca * px - sa * py + e) as f32);
                        #[allow(clippy::cast_possible_truncation)]
                        let my = f64::from((sa * px + ca * py + f) as f32);
                        x0 = x0.min(mx);
                        y0 = y0.min(my);
                        x1 = x1.max(mx);
                        y1 = y1.max(my);
                    }
                    acc_labels.add_bounds(x0, y0, x1, y1);
                } else {
                    acc_labels.add_rect(bx, by, bkw, bkh);
                }
            }

            // Label (LR).
            if show_commit_label
                && !is_vert
                && symbol != CHERRY_PICK
                && ((c.custom_id && c.ty == MERGE) || c.ty != MERGE)
            {
                let wrapper = append(&g_labels, "g");
                let label_bkg = append(&wrapper, "rect");
                set_attr(&label_bkg, "class", "commit-label-bkg");
                let text = append(&wrapper, "text");
                set_attr(&text, "x", js_num(pos));
                set_attr(&text, "y", js_num(y + 25.0));
                set_attr(&text, "class", "commit-label");
                set_text(&text, &c.id);
                let (bw, bh) = text_dims(&c.id, 10.0);
                set_attr(&label_bkg, "x", js_num(pos_with_offset - bw / 2.0 - PY));
                set_attr(&label_bkg, "y", js_num(y + 13.5));
                set_attr(&label_bkg, "width", js_num(bw + 2.0 * PY));
                set_attr(&label_bkg, "height", js_num(bh + 2.0 * PY));
                set_attr(&text, "x", js_num(pos_with_offset - bw / 2.0));
                if rotate_commit_label {
                    let r_x = -7.5 - ((bw + 10.0) / 25.0) * 9.5;
                    let r_y = 10.0 + (bw / 25.0) * 8.5;
                    set_attr(
                        &wrapper,
                        "transform",
                        format!(
                            "translate({}, {}) rotate({}, {}, {})",
                            js_num(r_x),
                            js_num(r_y),
                            js_num(-45.0),
                            js_num(pos),
                            js_num(y)
                        ),
                    );
                    // bbox: bkg rect corners through the wrapper transform.
                    let (rx0, ry0) = (pos_with_offset - bw / 2.0 - PY, y + 13.5);
                    let (rw, rh) = (bw + 2.0 * PY, bh + 2.0 * PY);
                    // Blink maps the child rect corners through the
                    // transform in double precision, then stores the mapped
                    // bounds as an f32 FloatRect.
                    // Blink composes the transform list into one affine
                    // matrix (doubles), maps the rect corners, and stores
                    // the bounds as an f32 FloatRect.
                    let rad = (-45.0f64).to_radians();
                    let (sa, ca) = (rad.sin(), rad.cos());
                    // translate(r_x, r_y) . rotate(-45, pos, y)
                    let e = pos.mul_add(-ca, y.mul_add(sa, r_x + pos));
                    let f = pos.mul_add(-sa, y.mul_add(-ca, r_y + y));
                    let mut bx0 = f64::INFINITY;
                    let mut by0 = f64::INFINITY;
                    let mut bx1 = f64::NEG_INFINITY;
                    let mut by1 = f64::NEG_INFINITY;
                    for (cx, cy) in [
                        (rx0, ry0),
                        (rx0 + rw, ry0),
                        (rx0, ry0 + rh),
                        (rx0 + rw, ry0 + rh),
                    ] {
                        // Each mapped point narrows to an f32 FloatPoint.
                        #[allow(clippy::cast_possible_truncation)]
                        let rxp = f64::from((ca * cx - sa * cy + e) as f32);
                        #[allow(clippy::cast_possible_truncation)]
                        let ryp = f64::from((sa * cx + ca * cy + f) as f32);
                        bx0 = bx0.min(rxp);
                        by0 = by0.min(ryp);
                        bx1 = bx1.max(rxp);
                        by1 = by1.max(ryp);
                    }
                    acc_labels.add_bounds(bx0, by0, bx1, by1);
                } else {
                    acc_labels.add_rect(
                        pos_with_offset - bw / 2.0 - PY,
                        y + 13.5,
                        bw + 2.0 * PY,
                        bh + 2.0 * PY,
                    );
                }
            }

            // Tags.
            if !c.tags.is_empty() {
                let mut y_offset = 0.0f64;
                let mut max_w = 0.0f64;
                let mut max_h = 0.0f64;
                struct TagEl {
                    rect: Element,
                    hole: Element,
                    text: Element,
                    y_offset: f64,
                }
                let mut tag_els: Vec<TagEl> = Vec::new();
                let mut tags_rev = c.tags.clone();
                tags_rev.reverse();
                for tag_value in &tags_rev {
                    let rect = append(&g_labels, "polygon");
                    let hole = append(&g_labels, "circle");
                    let text = append(&g_labels, "text");
                    set_attr(&text, "y", js_num(y - 16.0 - y_offset));
                    set_attr(&text, "class", "tag-label");
                    set_text(&text, tag_value);
                    let (tw, th) = text_dims(tag_value, 10.0);
                    max_w = max_w.max(tw);
                    max_h = max_h.max(th);
                    set_attr(&text, "x", js_num(pos_with_offset - tw / 2.0));
                    tag_els.push(TagEl {
                        rect,
                        hole,
                        text: text.clone(),
                        y_offset,
                    });
                    y_offset += 20.0;
                }
                for te in &tag_els {
                    let h2 = max_h / 2.0;
                    let ly = y - 19.2 - te.y_offset;
                    // Common (LR) points/hole set first, then overridden for vert.
                    set_attr(&te.rect, "class", "tag-label-bkg");
                    set_attr(
                        &te.rect,
                        "points",
                        format!(
                            "\n      {},{}  \n      {},{}\n      {},{}\n      {},{}\n      {},{}\n      {},{}",
                            js_num(pos - max_w / 2.0 - PX / 2.0),
                            js_num(ly + PY),
                            js_num(pos - max_w / 2.0 - PX / 2.0),
                            js_num(ly - PY),
                            js_num(pos_with_offset - max_w / 2.0 - PX),
                            js_num(ly - h2 - PY),
                            js_num(pos_with_offset + max_w / 2.0 + PX),
                            js_num(ly - h2 - PY),
                            js_num(pos_with_offset + max_w / 2.0 + PX),
                            js_num(ly + h2 + PY),
                            js_num(pos_with_offset - max_w / 2.0 - PX),
                            js_num(ly + h2 + PY)
                        ),
                    );
                    set_attr(&te.hole, "cy", js_num(ly));
                    set_attr(&te.hole, "cx", js_num(pos - max_w / 2.0 + PX / 2.0));
                    set_attr(&te.hole, "r", "1.5");
                    set_attr(&te.hole, "class", "tag-hole");
                    if is_vert {
                        let (cx, cy) = (x, y);
                        let raw_pos = cy - LAYOUT_OFFSET;
                        let y_origin = raw_pos + te.y_offset;
                        let pts = [
                            (cx, y_origin + 2.0),
                            (cx, y_origin - 2.0),
                            (cx + LAYOUT_OFFSET, y_origin - h2 - 2.0),
                            (cx + LAYOUT_OFFSET + max_w + 4.0, y_origin - h2 - 2.0),
                            (cx + LAYOUT_OFFSET + max_w + 4.0, y_origin + h2 + 2.0),
                            (cx + LAYOUT_OFFSET, y_origin + h2 + 2.0),
                        ];
                        let pts_str = pts
                            .iter()
                            .map(|(px, py)| format!("{},{}", js_num(*px), js_num(*py)))
                            .collect::<Vec<_>>()
                            .join("\n        ");
                        set_attr(&te.rect, "points", pts_str);
                        let rot = format!(
                            "translate(12,12) rotate(45, {},{})",
                            js_num(cx),
                            js_num(raw_pos)
                        );
                        set_attr(&te.rect, "transform", &rot);
                        set_attr(&te.hole, "cx", js_num(cx + PX / 2.0));
                        set_attr(&te.hole, "cy", js_num(y_origin));
                        set_attr(&te.hole, "transform", &rot);
                        set_attr(&te.text, "x", js_num(cx + 5.0));
                        set_attr(&te.text, "y", js_num(y_origin + 3.0));
                        set_attr(
                            &te.text,
                            "transform",
                            format!(
                                "translate(14,14) rotate(45, {},{})",
                                js_num(cx),
                                js_num(raw_pos)
                            ),
                        );
                        // getBBox: polygon corners mapped through the transform.
                        let rad = 45.0f64.to_radians();
                        let (sa, ca) = (rad.sin(), rad.cos());
                        let (mut x0, mut y0, mut x1, mut y1) = (
                            f64::INFINITY,
                            f64::INFINITY,
                            f64::NEG_INFINITY,
                            f64::NEG_INFINITY,
                        );
                        for (px, py) in pts {
                            let dx = px - cx;
                            let dy = py - raw_pos;
                            #[allow(clippy::cast_possible_truncation)]
                            let mx = f64::from((ca.mul_add(dx, -(sa * dy)) + cx + 12.0) as f32);
                            #[allow(clippy::cast_possible_truncation)]
                            let my = f64::from((sa.mul_add(dx, ca * dy) + raw_pos + 12.0) as f32);
                            x0 = x0.min(mx);
                            y0 = y0.min(my);
                            x1 = x1.max(mx);
                            y1 = y1.max(my);
                        }
                        acc_labels.add_bounds(x0, y0, x1, y1);
                    } else {
                        acc_labels.add_rect(
                            pos - max_w / 2.0 - PX / 2.0,
                            ly - h2 - PY,
                            pos_with_offset + max_w / 2.0 + PX - (pos - max_w / 2.0 - PX / 2.0),
                            (h2 + PY) * 2.0,
                        );
                    }
                }
            }
        }
    }

    // setupGraphViewbox: padding 8; the root bbox unions the group bboxes
    // in document order.
    let mut acc = Acc::new();
    acc.rect.union_with(&acc_branches.rect);
    acc.rect.union_with(&acc_arrows.rect);
    acc.rect.union_with(&acc_bullets.rect);
    acc.rect.union_with(&acc_labels.rect);
    let padding = 8.0;
    let width = acc.width() + 2.0 * padding;
    let height = acc.height() + 2.0 * padding;
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(acc.x0() - padding),
            js_num(acc.y0() - padding),
            js_num(width),
            js_num(height)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "gitGraph");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
