//! Recursive-descent parser for the flowchart grammar (port of `flow.jison`
//! productions plus the `flowDb` callbacks the jison actions invoke).

use super::db::{DocItem, FlowDb, FlowText, LinkInfo};
use super::lexer::{Lexer, Tok};

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "flowchart parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses flowchart source into a populated [`FlowDb`].
///
/// # Errors
///
/// Returns [`ParseError`] when the source doesn't match the flowchart
/// grammar.
pub fn parse(input: &str) -> Result<FlowDb, ParseError> {
    let chars: Vec<char> = input.chars().collect();
    let tokens = Lexer::new(&chars).tokenize();
    let mut parser = Parser {
        tokens,
        pos: 0,
        db: FlowDb::new(),
    };
    parser.parse_start()?;
    Ok(parser.db)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
    db: FlowDb,
}

impl Parser {
    fn peek(&self) -> &Tok {
        self.tokens.get(self.pos).unwrap_or(&Tok::Eof)
    }

    fn next(&mut self) -> Tok {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Tok::Eof);
        self.pos += 1;
        tok
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Tok::Space) {
            self.pos += 1;
        }
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Tok::Space | Tok::Newline | Tok::Semi) {
            self.pos += 1;
        }
    }

    fn skip_to_end_of_line(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.pos += 1;
        }
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: format!("{} (at token {:?})", message.into(), self.peek()),
        }
    }

    fn parse_start(&mut self) -> Result<(), ParseError> {
        self.skip_separators();
        match self.next() {
            Tok::Graph => {}
            other => {
                return Err(ParseError {
                    message: format!("expected flowchart/graph header, got {other:?}"),
                });
            }
        }
        match self.peek().clone() {
            Tok::Dir(dir) => {
                self.pos += 1;
                self.db.set_direction(&dir);
            }
            Tok::NoDir => {
                self.pos += 1;
                self.db.set_direction("TB");
            }
            _ => {
                self.db.set_direction("TB");
            }
        }
        self.parse_document(false)?;
        Ok(())
    }

    /// Parses statements until EOF (or `end` when inside a subgraph).
    fn parse_document(&mut self, in_subgraph: bool) -> Result<Vec<DocItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_separators();
            match self.peek() {
                Tok::Eof => {
                    if in_subgraph {
                        return Err(self.err("unexpected EOF inside subgraph"));
                    }
                    return Ok(items);
                }
                Tok::End if in_subgraph => {
                    self.pos += 1;
                    return Ok(items);
                }
                _ => {}
            }
            let stmt_items = self.parse_statement()?;
            items.extend(stmt_items);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_statement(&mut self) -> Result<Vec<DocItem>, ParseError> {
        match self.peek().clone() {
            Tok::Direction(dir) => {
                self.pos += 1;
                let value = if dir == "TD" { "TB".to_owned() } else { dir };
                Ok(vec![DocItem::Dir(value)])
            }
            Tok::AccTitle | Tok::AccDescr => {
                self.pos += 1;
                // The value token follows.
                if matches!(self.peek(), Tok::Str(_)) {
                    self.pos += 1;
                }
                Ok(vec![])
            }
            Tok::Click(_) => {
                // Interactivity is not rendered; consume the line.
                self.skip_to_end_of_line();
                Ok(vec![])
            }
            Tok::Subgraph => {
                self.pos += 1;
                self.parse_subgraph()
            }
            Tok::ClassDef => {
                self.pos += 1;
                self.skip_spaces();
                let ids = self.parse_id_string()?;
                self.skip_spaces();
                let styles = self.parse_styles_opt();
                self.db.add_class(&ids, &styles);
                Ok(vec![])
            }
            Tok::Class => {
                self.pos += 1;
                self.skip_spaces();
                let ids = self.parse_id_string()?;
                self.skip_spaces();
                let class_name = self.parse_id_string()?;
                self.db.set_class(&ids, &class_name);
                Ok(vec![])
            }
            Tok::Style => {
                self.pos += 1;
                self.skip_spaces();
                let id = self.parse_id_string()?;
                self.skip_spaces();
                let styles = self.parse_styles_opt();
                self.db.add_vertex(&id, None, None, &styles, &[], None);
                Ok(vec![])
            }
            Tok::LinkStyle => {
                self.pos += 1;
                self.skip_spaces();
                let positions: Vec<Option<usize>> = if matches!(self.peek(), Tok::Default) {
                    self.pos += 1;
                    vec![None]
                } else {
                    let mut list = Vec::new();
                    loop {
                        match self.next() {
                            Tok::Num(n) => {
                                list.push(Some(n.parse::<usize>().map_err(|_| ParseError {
                                    message: format!("invalid linkStyle index {n}"),
                                })?));
                            }
                            other => {
                                return Err(ParseError {
                                    message: format!("expected linkStyle index, got {other:?}"),
                                });
                            }
                        }
                        if matches!(self.peek(), Tok::Comma) {
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    list
                };
                self.skip_spaces();
                if matches!(self.peek(), Tok::Interpolate) {
                    self.pos += 1;
                    self.skip_spaces();
                    let curve = self.parse_id_string()?;
                    self.db.update_link_interpolate(&positions, &curve);
                    self.skip_spaces();
                    if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::Eof) {
                        let styles = self.parse_styles_opt();
                        self.db.update_link(&positions, &styles);
                    }
                } else {
                    let styles = self.parse_styles_opt();
                    self.db.update_link(&positions, &styles);
                }
                Ok(vec![])
            }
            _ => self.parse_vertex_statement(),
        }
    }

    fn parse_subgraph(&mut self) -> Result<Vec<DocItem>, ParseError> {
        self.skip_spaces();
        // textNoTags up to optional [title] or end of line.
        let mut id_text = String::new();
        let mut id_type = "text".to_owned();
        loop {
            match self.peek().clone() {
                Tok::NodeString(s) | Tok::UnicodeText(s) | Tok::Num(s) | Tok::Str(s) => {
                    self.pos += 1;
                    id_text.push_str(&s);
                }
                Tok::MdStr(s) => {
                    self.pos += 1;
                    id_text.push_str(&s);
                    "markdown".clone_into(&mut id_type);
                }
                Tok::Space => {
                    self.pos += 1;
                    if !id_text.is_empty() {
                        id_text.push(' ');
                    }
                }
                Tok::Minus => {
                    self.pos += 1;
                    id_text.push('-');
                }
                Tok::Colon => {
                    self.pos += 1;
                    id_text.push(':');
                }
                Tok::Down => {
                    self.pos += 1;
                    id_text.push('v');
                }
                _ => break,
            }
        }
        let id_text = id_text.trim_end().to_owned();

        let title = if matches!(self.peek(), Tok::Sqs) {
            self.pos += 1;
            let text = self.parse_text_until(&Tok::Sqe)?;
            Some(text)
        } else {
            None
        };

        // separator
        self.skip_separators();
        let items = self.parse_document(true)?;

        let id_obj = if id_text.is_empty() {
            None
        } else {
            Some(FlowText {
                text: id_text,
                label_type: id_type,
            })
        };
        let sg_id = match (&id_obj, &title) {
            (Some(id), Some(title)) => self.db.add_subgraph(Some(id), &items, Some(title)),
            (Some(id), None) => {
                // id doubles as title; if it contains whitespace it is a
                // title only (mirrors flowDb `_id === _title` check).
                if id.text.contains(char::is_whitespace) {
                    let title = id.clone();
                    self.db.add_subgraph(None, &items, Some(&title))
                } else {
                    self.db.add_subgraph(Some(id), &items, Some(id))
                }
            }
            (None, Some(title)) => self.db.add_subgraph(None, &items, Some(title)),
            (None, None) => self.db.add_subgraph(None, &items, None),
        };
        Ok(vec![DocItem::Node(sg_id)])
    }

    /// stylesOpt: comma-separated style strings until end of line.
    fn parse_styles_opt(&mut self) -> Vec<String> {
        let mut styles = Vec::new();
        let mut current = String::new();
        loop {
            match self.peek().clone() {
                Tok::Comma => {
                    self.pos += 1;
                    styles.push(std::mem::take(&mut current));
                }
                Tok::NodeString(s) | Tok::Num(s) | Tok::UnicodeText(s) => {
                    self.pos += 1;
                    current.push_str(&s);
                }
                Tok::Colon => {
                    self.pos += 1;
                    current.push(':');
                }
                Tok::Space => {
                    self.pos += 1;
                    if !current.is_empty() {
                        current.push(' ');
                    }
                }
                Tok::Brkt => {
                    self.pos += 1;
                    current.push('#');
                }
                Tok::Minus => {
                    self.pos += 1;
                    current.push('-');
                }
                Tok::Style => {
                    self.pos += 1;
                    current.push_str("style");
                }
                Tok::Default => {
                    self.pos += 1;
                    current.push_str("default");
                }
                // Semi/Newline/Eof — or anything unexpected — ends the list.
                _ => break,
            }
        }
        if !current.is_empty() {
            styles.push(current);
        }
        styles
    }

    fn parse_id_string(&mut self) -> Result<String, ParseError> {
        let mut id = String::new();
        loop {
            match self.peek().clone() {
                Tok::NodeString(s) | Tok::Num(s) | Tok::UnicodeText(s) => {
                    self.pos += 1;
                    id.push_str(&s);
                }
                Tok::Down => {
                    self.pos += 1;
                    id.push('v');
                }
                Tok::Minus => {
                    self.pos += 1;
                    id.push('-');
                }
                Tok::Default => {
                    self.pos += 1;
                    id.push_str("default");
                }
                Tok::Comma if !id.is_empty() => {
                    // Comma joins multi-target class statements (`class A,B x`).
                    self.pos += 1;
                    id.push(',');
                }
                Tok::Colon => {
                    self.pos += 1;
                    id.push(':');
                }
                Tok::Amp => {
                    self.pos += 1;
                    id.push('&');
                }
                Tok::Brkt => {
                    self.pos += 1;
                    id.push('#');
                }
                Tok::Mult => {
                    self.pos += 1;
                    id.push('*');
                }
                _ => break,
            }
        }
        if id.is_empty() {
            return Err(self.err("expected identifier"));
        }
        Ok(id)
    }

    /// Collects text tokens until `end_tok` (consumed).
    fn parse_text_until(&mut self, end_tok: &Tok) -> Result<FlowText, ParseError> {
        let mut text = String::new();
        let mut label_type = "text".to_owned();
        loop {
            let tok = self.next();
            if tok == *end_tok {
                break;
            }
            match tok {
                Tok::Text(s) | Tok::NodeString(s) | Tok::Num(s) | Tok::UnicodeText(s) => {
                    text.push_str(&s);
                }
                Tok::Str(s) => {
                    text.push_str(&s);
                    "string".clone_into(&mut label_type);
                }
                Tok::MdStr(s) => {
                    text.push_str(&s);
                    "markdown".clone_into(&mut label_type);
                }
                Tok::Space => text.push(' '),
                Tok::TagStart => text.push('<'),
                Tok::TagEnd => text.push('>'),
                Tok::Colon => text.push(':'),
                Tok::Minus => text.push('-'),
                Tok::Amp => text.push('&'),
                Tok::Comma => text.push(','),
                Tok::Semi => text.push(';'),
                Tok::Brkt => text.push('#'),
                Tok::Mult => text.push('*'),
                Tok::Down => text.push('v'),
                Tok::Up => text.push('^'),
                Tok::Eof => return Err(self.err("unterminated node text")),
                other => {
                    return Err(ParseError {
                        message: format!("unexpected token in text: {other:?}"),
                    });
                }
            }
        }
        // The 'string' label type maps to 'text' downstream (flow.jison sets
        // type:'string' but flowDb sanitizes to text rendering).
        if label_type == "string" {
            "text".clone_into(&mut label_type);
        }
        Ok(FlowText { text, label_type })
    }

    /// vertexStatement: node groups joined by links.
    fn parse_vertex_statement(&mut self) -> Result<Vec<DocItem>, ParseError> {
        let mut all_nodes: Vec<String> = Vec::new();
        let mut prev_group = self.parse_node_group()?;
        all_nodes.extend(prev_group.iter().cloned());

        loop {
            self.skip_spaces();
            match self.peek().clone() {
                Tok::Link(_) | Tok::StartLink(_) | Tok::LinkId(_) => {
                    let link = self.parse_link()?;
                    self.skip_spaces();
                    let group = self.parse_node_group()?;
                    self.db.add_link(&prev_group, &group, &link);
                    all_nodes.extend(group.iter().cloned());
                    prev_group = group;
                }
                Tok::ShapeData(_) => {
                    let data = self.collect_shape_data();
                    let last = prev_group.last().cloned().unwrap_or_default();
                    self.apply_shape_data(&last, &data);
                }
                _ => break,
            }
        }
        Ok(all_nodes.into_iter().map(DocItem::Node).collect())
    }

    fn collect_shape_data(&mut self) -> String {
        let mut data = String::new();
        while let Tok::ShapeData(s) = self.peek().clone() {
            self.pos += 1;
            data.push_str(&s);
        }
        data
    }

    /// Minimal `@{...}` metadata support: shape and label keys.
    fn apply_shape_data(&mut self, id: &str, data: &str) {
        for part in data.split(',') {
            let mut kv = part.splitn(2, ':');
            let key = kv.next().unwrap_or("").trim();
            let value = kv.next().unwrap_or("").trim().trim_matches('"');
            match key {
                "shape" => {
                    if let Some(v) = self.db.vertices.get_mut(id) {
                        v.vertex_type = Some(value.to_owned());
                    }
                }
                "label" => {
                    if let Some(v) = self.db.vertices.get_mut(id) {
                        v.text = Some(value.to_owned());
                    }
                }
                _ => {}
            }
        }
    }

    /// node: styledVertex (spaceList AMP spaceList styledVertex)*
    fn parse_node_group(&mut self) -> Result<Vec<String>, ParseError> {
        let mut group = vec![self.parse_styled_vertex()?];
        loop {
            let save = self.pos;
            self.skip_spaces();
            // Optional shape data attached to previous vertex.
            if matches!(self.peek(), Tok::ShapeData(_)) {
                let data = self.collect_shape_data();
                let last = group.last().cloned().unwrap_or_default();
                self.apply_shape_data(&last, &data);
                self.skip_spaces();
            }
            if matches!(self.peek(), Tok::Amp) {
                self.pos += 1;
                self.skip_spaces();
                group.push(self.parse_styled_vertex()?);
            } else {
                self.pos = save;
                break;
            }
        }
        Ok(group)
    }

    fn parse_styled_vertex(&mut self) -> Result<String, ParseError> {
        let id = self.parse_vertex()?;
        if matches!(self.peek(), Tok::StyleSeparator) {
            self.pos += 1;
            let class_name = self.parse_id_string()?;
            self.db.set_class(&id, &class_name);
        }
        Ok(id)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_vertex(&mut self) -> Result<String, ParseError> {
        let id = self.parse_id_string()?;
        match self.peek().clone() {
            Tok::Sqs => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::Sqe)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("square"), &[], &[], None);
            }
            Tok::DoubleCircleStart => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::DoubleCircleEnd)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("doublecircle"), &[], &[], None);
            }
            Tok::Ps => {
                self.pos += 1;
                if matches!(self.peek(), Tok::Ps) {
                    // circle: (( text ))
                    self.pos += 1;
                    let text = self.parse_text_until(&Tok::Pe)?;
                    if !matches!(self.next(), Tok::Pe) {
                        return Err(self.err("expected ))"));
                    }
                    self.db
                        .add_vertex(&id, Some(&text), Some("circle"), &[], &[], None);
                } else {
                    let text = self.parse_text_until(&Tok::Pe)?;
                    self.db
                        .add_vertex(&id, Some(&text), Some("round"), &[], &[], None);
                }
            }
            Tok::EllipseStart => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::EllipseEnd)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("ellipse"), &[], &[], None);
            }
            Tok::StadiumStart => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::StadiumEnd)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("stadium"), &[], &[], None);
            }
            Tok::SubroutineStart => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::SubroutineEnd)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("subroutine"), &[], &[], None);
            }
            Tok::CylinderStart => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::CylinderEnd)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("cylinder"), &[], &[], None);
            }
            Tok::DiamondStart => {
                self.pos += 1;
                if matches!(self.peek(), Tok::DiamondStart) {
                    self.pos += 1;
                    let text = self.parse_text_until(&Tok::DiamondStop)?;
                    if !matches!(self.next(), Tok::DiamondStop) {
                        return Err(self.err("expected }}"));
                    }
                    self.db
                        .add_vertex(&id, Some(&text), Some("hexagon"), &[], &[], None);
                } else {
                    let text = self.parse_text_until(&Tok::DiamondStop)?;
                    self.db
                        .add_vertex(&id, Some(&text), Some("diamond"), &[], &[], None);
                }
            }
            Tok::TagEnd => {
                self.pos += 1;
                let text = self.parse_text_until(&Tok::Sqe)?;
                self.db
                    .add_vertex(&id, Some(&text), Some("odd"), &[], &[], None);
            }
            Tok::TrapStart => {
                self.pos += 1;
                let mut text = FlowText::default();
                let mut label_type = "text".to_owned();
                loop {
                    match self.next() {
                        Tok::TrapEnd => {
                            self.db
                                .add_vertex(&id, Some(&text), Some("trapezoid"), &[], &[], None);
                            break;
                        }
                        Tok::InvTrapEnd => {
                            self.db.add_vertex(
                                &id,
                                Some(&text),
                                Some("lean_right"),
                                &[],
                                &[],
                                None,
                            );
                            break;
                        }
                        Tok::Text(s) | Tok::Str(s) => text.text.push_str(&s),
                        Tok::MdStr(s) => {
                            text.text.push_str(&s);
                            "markdown".clone_into(&mut label_type);
                        }
                        Tok::Space => text.text.push(' '),
                        Tok::Eof => return Err(self.err("unterminated trapezoid")),
                        other => {
                            return Err(ParseError {
                                message: format!("unexpected token in trapezoid: {other:?}"),
                            });
                        }
                    }
                }
                let _ = label_type;
            }
            Tok::InvTrapStart => {
                self.pos += 1;
                let mut text = FlowText::default();
                loop {
                    match self.next() {
                        Tok::InvTrapEnd => {
                            self.db.add_vertex(
                                &id,
                                Some(&text),
                                Some("inv_trapezoid"),
                                &[],
                                &[],
                                None,
                            );
                            break;
                        }
                        Tok::TrapEnd => {
                            self.db
                                .add_vertex(&id, Some(&text), Some("lean_left"), &[], &[], None);
                            break;
                        }
                        Tok::Text(s) | Tok::Str(s) | Tok::MdStr(s) => text.text.push_str(&s),
                        Tok::Space => text.text.push(' '),
                        Tok::Eof => return Err(self.err("unterminated trapezoid")),
                        other => {
                            return Err(ParseError {
                                message: format!("unexpected token in trapezoid: {other:?}"),
                            });
                        }
                    }
                }
            }
            _ => {
                self.db.add_vertex(&id, None, None, &[], &[], None);
            }
        }
        Ok(id)
    }

    fn parse_link(&mut self) -> Result<LinkInfo, ParseError> {
        let link_id = if let Tok::LinkId(id) = self.peek().clone() {
            self.pos += 1;
            Some(id)
        } else {
            None
        };
        match self.next() {
            Tok::Link(raw) => {
                let mut info = self.db.destruct_link(&raw, None);
                info.id = link_id;
                self.skip_spaces();
                if matches!(self.peek(), Tok::Pipe) {
                    self.pos += 1;
                    let text = self.parse_text_until(&Tok::Pipe)?;
                    info.text = Some(text);
                    self.skip_spaces();
                }
                Ok(info)
            }
            Tok::StartLink(start_raw) => {
                let mut text = FlowText::default();
                let end_raw;
                loop {
                    match self.next() {
                        Tok::EdgeText(s) | Tok::Str(s) | Tok::UnicodeText(s) => {
                            text.text.push_str(&s);
                        }
                        Tok::MdStr(s) => {
                            text.text.push_str(&s);
                            "markdown".clone_into(&mut text.label_type);
                        }
                        Tok::Link(raw) => {
                            end_raw = raw;
                            break;
                        }
                        Tok::Eof => return Err(self.err("unterminated edge text")),
                        other => {
                            return Err(ParseError {
                                message: format!("unexpected token in edge text: {other:?}"),
                            });
                        }
                    }
                }
                if text.label_type.is_empty() {
                    "text".clone_into(&mut text.label_type);
                }
                let mut info = self.db.destruct_link(&end_raw, Some(&start_raw));
                info.text = Some(text);
                info.id = link_id;
                Ok(info)
            }
            other => Err(ParseError {
                message: format!("expected link, got {other:?}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_flowchart() {
        let db = parse("flowchart TD\n    A[Start] --> B{Is it?}\n    B -->|Yes| C[OK]\n    B -->|No| D[End]\n").expect("parse");
        assert_eq!(db.get_direction(), "TB");
        assert_eq!(db.vertices.len(), 4);
        assert_eq!(db.edges.len(), 3);
        assert_eq!(db.vertices["A"].text.as_deref(), Some("Start"));
        assert_eq!(db.vertices["A"].vertex_type.as_deref(), Some("square"));
        assert_eq!(db.vertices["B"].vertex_type.as_deref(), Some("diamond"));
        assert_eq!(db.vertices["A"].dom_id, "flowchart-A-0");
        assert_eq!(db.vertices["B"].dom_id, "flowchart-B-1");
        assert_eq!(db.vertices["C"].dom_id, "flowchart-C-3");
        assert_eq!(db.vertices["D"].dom_id, "flowchart-D-5");
        assert_eq!(db.edges[0].id, "L_A_B_0");
        assert_eq!(db.edges[1].text, "Yes");
        assert_eq!(db.edges[1].edge_type.as_deref(), Some("arrow_point"));
    }

    #[test]
    fn parses_edge_text_and_types() {
        let db = parse("graph LR\nA -- label --> B\nB === C\nC -.-> D\nD <--> E\nE --- F\n")
            .expect("parse");
        assert_eq!(db.get_direction(), "LR");
        assert_eq!(db.edges[0].text, "label");
        assert_eq!(db.edges[0].edge_type.as_deref(), Some("arrow_point"));
        assert_eq!(db.edges[1].stroke.as_deref(), Some("thick"));
        assert_eq!(db.edges[1].edge_type.as_deref(), Some("arrow_open"));
        assert_eq!(db.edges[2].stroke.as_deref(), Some("dotted"));
        assert_eq!(db.edges[3].edge_type.as_deref(), Some("double_arrow_point"));
        assert_eq!(db.edges[4].edge_type.as_deref(), Some("arrow_open"));
    }

    #[test]
    fn parses_subgraphs_and_classes() {
        let src = "flowchart TB\n  subgraph one [One]\n  A --> B\n  end\n  classDef red fill:#f00,stroke:#333\n  class A red\n  C:::red --> one\n";
        let db = parse(src).expect("parse");
        assert_eq!(db.sub_graphs.len(), 1);
        assert_eq!(db.sub_graphs[0].id, "one");
        assert_eq!(db.sub_graphs[0].title, "One");
        assert_eq!(db.sub_graphs[0].nodes, vec!["A", "B"]);
        assert!(db.classes.contains_key("red"));
        assert_eq!(db.vertices["A"].classes, vec!["red"]);
        assert_eq!(db.vertices["C"].classes, vec!["red"]);
    }

    #[test]
    fn parses_ampersand_groups() {
        let db = parse("graph TD\nA & B --> C & D\n").expect("parse");
        assert_eq!(db.edges.len(), 4);
        assert_eq!(db.edges[0].start, "A");
        assert_eq!(db.edges[0].end, "C");
        assert_eq!(db.edges[3].start, "B");
        assert_eq!(db.edges[3].end, "D");
    }

    #[test]
    fn parses_all_shapes() {
        let src = "graph TD\na[sq]\nb(round)\nc((circle))\nd{diam}\ne{{hex}}\nf([stadium])\ng[[sub]]\nh[(cyl)]\ni>odd]\nj[/lr/]\nk[\\ll\\]\nl[/trap\\]\nm[\\inv/]\nn(((dc)))\n";
        let db = parse(src).expect("parse");
        let ty = |id: &str| db.vertices[id].vertex_type.clone().unwrap();
        assert_eq!(ty("a"), "square");
        assert_eq!(ty("b"), "round");
        assert_eq!(ty("c"), "circle");
        assert_eq!(ty("d"), "diamond");
        assert_eq!(ty("e"), "hexagon");
        assert_eq!(ty("f"), "stadium");
        assert_eq!(ty("g"), "subroutine");
        assert_eq!(ty("h"), "cylinder");
        assert_eq!(ty("i"), "odd");
        assert_eq!(ty("j"), "lean_right");
        assert_eq!(ty("k"), "lean_left");
        assert_eq!(ty("l"), "trapezoid");
        assert_eq!(ty("m"), "inv_trapezoid");
        assert_eq!(ty("n"), "doublecircle");
    }
}
