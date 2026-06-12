//! Tokenizer for the flowchart grammar, ported from `flow.jison`.
//!
//! Jison picks the longest match, with earlier rules winning ties; lexer
//! states mirror the jison `%x` states that matter for flowcharts.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Graph,
    Subgraph,
    End,
    Dir(String),
    NoDir,
    Direction(String),
    AccTitle,
    AccDescr,
    ShapeData(String),
    Style,
    Default,
    LinkStyle,
    Interpolate,
    ClassDef,
    Class,
    Click(String),
    Href,
    LinkTarget(String),
    CallbackName(String),
    CallbackArgs(String),
    LinkId(String),
    Num(String),
    Brkt,
    StyleSeparator,
    Colon,
    Amp,
    Semi,
    Comma,
    Mult,
    /// Complete link token, e.g. `-->`, `==>`, `-.->`, `~~~`.
    Link(String),
    /// Open link start, e.g. `--`, `==`, `-.`.
    StartLink(String),
    EdgeText(String),
    Pipe,
    Ps,
    Pe,
    Sqs,
    Sqe,
    DiamondStart,
    DiamondStop,
    StadiumStart,
    StadiumEnd,
    SubroutineStart,
    SubroutineEnd,
    CylinderStart,
    CylinderEnd,
    DoubleCircleStart,
    DoubleCircleEnd,
    EllipseStart,
    EllipseEnd,
    TrapStart,
    TrapEnd,
    InvTrapStart,
    InvTrapEnd,
    VertexWithPropsStart,
    TagStart,
    TagEnd,
    Up,
    Down,
    Minus,
    NodeString(String),
    UnicodeText(String),
    Str(String),
    MdStr(String),
    Text(String),
    Quote,
    Newline,
    Space,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Initial,
    Dir,
    Str,
    MdStr,
    EdgeText,
    ThickEdgeText,
    DottedEdgeText,
    Text,
    EllipseText,
    TrapText,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ShapeData,
    ShapeDataStr,
    Click,
    CallbackName,
    CallbackArgs,
}

pub struct Lexer<'a> {
    src: &'a [char],
    pos: usize,
    stack: Vec<State>,
    first_graph: bool,
}

impl std::fmt::Debug for Lexer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lexer").field("pos", &self.pos).finish()
    }
}

fn is_node_string_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '"'
                | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '.'
                | '`'
                | '?'
                | '\\'
                | '_'
                | '/'
        )
}

fn is_unicode_text(c: char) -> bool {
    // The jison rule enumerates unicode letter ranges; alphabetic non-ASCII
    // covers the same set for practical purposes.
    !c.is_ascii() && c.is_alphabetic()
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(src: &'a [char]) -> Self {
        Self {
            src,
            pos: 0,
            stack: vec![State::Initial],
            first_graph: true,
        }
    }

    fn state(&self) -> State {
        *self.stack.last().expect("state stack non-empty")
    }

    fn rest(&self) -> &[char] {
        &self.src[self.pos..]
    }

    fn starts_with(&self, s: &str) -> bool {
        let rest = self.rest();
        let chars: Vec<char> = s.chars().collect();
        rest.len() >= chars.len() && rest[..chars.len()] == chars[..]
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.rest().get(offset).copied()
    }

    fn take(&mut self, n: usize) -> String {
        let s: String = self.rest()[..n].iter().collect();
        self.pos += n;
        s
    }

    /// Matches `\s*[xo<]?PAT\s*` where PAT is the link body; returns the
    /// consumed length and the trimmed token text, or None.
    fn match_link(&self, full: bool, kind: char) -> Option<(usize, String)> {
        let rest = self.rest();
        let mut i = 0;
        while i < rest.len() && rest[i].is_whitespace() && rest[i] != '\n' {
            i += 1;
        }
        let start_body = i;
        if i < rest.len() && matches!(rest[i], 'x' | 'o' | '<') {
            i += 1;
        }
        let body_start = i;
        match kind {
            '-' if full => {
                // --+[-xo>]
                while i < rest.len() && rest[i] == '-' {
                    i += 1;
                }
                if i - body_start < 2 {
                    return None;
                }
                // Last consumed '-' may serve as the terminator [-xo>].
                if i < rest.len() && matches!(rest[i], 'x' | 'o' | '>') {
                    i += 1;
                } else if i - body_start >= 3 {
                    // Trailing '-' acts as terminator (e.g. `---`).
                } else {
                    return None;
                }
            }
            '-' => {
                // START_LINK: exactly `--` not followed by another link char
                if !(i + 1 < rest.len() && rest[i] == '-' && rest[i + 1] == '-') {
                    return None;
                }
                i += 2;
                // Must NOT be a full link (`--x`): full link rule has priority
                // by longest match, checked by caller order.
            }
            '=' if full => {
                while i < rest.len() && rest[i] == '=' {
                    i += 1;
                }
                if i - body_start < 2 {
                    return None;
                }
                if i < rest.len() && matches!(rest[i], 'x' | 'o' | '>') {
                    i += 1;
                } else if i - body_start >= 3 {
                } else {
                    return None;
                }
            }
            '=' => {
                if !(i + 1 < rest.len() && rest[i] == '=' && rest[i + 1] == '=') {
                    return None;
                }
                i += 2;
            }
            '.' if full => {
                // [xo<]?-?\.+-[xo>]?
                if i < rest.len() && rest[i] == '-' {
                    i += 1;
                }
                let dots = i;
                while i < rest.len() && rest[i] == '.' {
                    i += 1;
                }
                if i == dots {
                    return None;
                }
                if i < rest.len() && rest[i] == '-' {
                    i += 1;
                } else {
                    return None;
                }
                if i < rest.len() && matches!(rest[i], 'x' | 'o' | '>') {
                    i += 1;
                }
            }
            '.' => {
                // -.
                if !(i + 1 < rest.len() && rest[i] == '-' && rest[i + 1] == '.') {
                    return None;
                }
                i += 2;
            }
            _ => return None,
        }
        let body: String = rest[start_body..i].iter().collect();
        // consume trailing inline whitespace
        while i < rest.len() && rest[i].is_whitespace() && rest[i] != '\n' {
            i += 1;
        }
        Some((i, body))
    }

    #[allow(clippy::too_many_lines)]
    pub fn next_token(&mut self) -> Tok {
        if self.pos >= self.src.len() {
            return Tok::Eof;
        }

        match self.state() {
            State::Str => {
                if self.starts_with("\"") {
                    self.pos += 1;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '"') {
                    n += 1;
                }
                return Tok::Str(self.take(n));
            }
            State::MdStr => {
                if self.starts_with("`\"") {
                    self.pos += 2;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '`' && c != '"') {
                    n += 1;
                }
                return Tok::MdStr(self.take(n));
            }
            State::AccTitle | State::AccDescr => {
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '\n') {
                    n += 1;
                }
                let value = self.take(n);
                self.stack.pop();
                return if self.state() == State::Initial {
                    // Value tokens carry their text; the parser ignores them.
                    Tok::Str(value)
                } else {
                    Tok::Str(value)
                };
            }
            State::AccDescrMultiline => {
                if self.starts_with("}") {
                    self.pos += 1;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '}') {
                    n += 1;
                }
                return Tok::Str(self.take(n));
            }
            State::ShapeData => {
                if self.starts_with("\"") {
                    self.pos += 1;
                    self.stack.push(State::ShapeDataStr);
                    return Tok::ShapeData(String::new());
                }
                if self.starts_with("}") {
                    self.pos += 1;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '}' && c != '"') {
                    n += 1;
                }
                return Tok::ShapeData(self.take(n));
            }
            State::ShapeDataStr => {
                if self.starts_with("\"") {
                    self.pos += 1;
                    self.stack.pop();
                    return Tok::ShapeData(String::new());
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '"') {
                    n += 1;
                }
                let text = self.take(n);
                // jison replaces newline+indent with <br/>
                let re_replaced = text
                    .split('\n')
                    .map(str::trim_start)
                    .collect::<Vec<_>>()
                    .join("<br/>");
                return Tok::ShapeData(re_replaced);
            }
            State::Click => {
                if self.peek(0).is_some_and(|c| c == ' ' || c == '\n') {
                    self.pos += 1;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != ' ' && c != '\n') {
                    n += 1;
                }
                return Tok::Click(self.take(n));
            }
            State::CallbackName => {
                if self.starts_with("(") {
                    // `(...)` args or empty
                    if self.peek(1).is_some_and(|c| c == ')') {
                        self.pos += 2;
                        self.stack.pop();
                        return self.next_token();
                    }
                    self.pos += 1;
                    self.stack.pop();
                    self.stack.push(State::CallbackArgs);
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != '(' && c != '\n') {
                    n += 1;
                }
                return Tok::CallbackName(self.take(n));
            }
            State::CallbackArgs => {
                if self.starts_with(")") {
                    self.pos += 1;
                    self.stack.pop();
                    return self.next_token();
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| c != ')') {
                    n += 1;
                }
                return Tok::CallbackArgs(self.take(n));
            }
            State::Dir => {
                // (\r?\n)*\s*\n → NODIR; \s*XX → DIR
                let rest = self.rest();
                let mut i = 0;
                while i < rest.len() && rest[i].is_whitespace() {
                    if rest[i] == '\n' {
                        self.pos += i + 1;
                        self.stack.pop();
                        return Tok::NoDir;
                    }
                    i += 1;
                }
                for (pat, dir) in [
                    ("LR", "LR"),
                    ("RL", "RL"),
                    ("TB", "TB"),
                    ("BT", "BT"),
                    ("TD", "TD"),
                    ("BR", "BR"),
                    ("<", "<"),
                    (">", ">"),
                    ("^", "^"),
                    ("v", "v"),
                ] {
                    let chars: Vec<char> = pat.chars().collect();
                    if rest[i..].starts_with(&chars[..]) {
                        self.pos += i + chars.len();
                        self.stack.pop();
                        return Tok::Dir(dir.to_owned());
                    }
                }
                // No direction given on same line; treat as NODIR at newline.
                self.pos += i;
                self.stack.pop();
                return self.next_token();
            }
            State::EdgeText | State::ThickEdgeText | State::DottedEdgeText => {
                let kind = match self.state() {
                    State::EdgeText => '-',
                    State::ThickEdgeText => '=',
                    _ => '.',
                };
                if let Some((n, body)) = self.match_link(true, kind) {
                    self.pos += n;
                    self.stack.pop();
                    return Tok::Link(body);
                }
                // EDGE_TEXT: [^-]|-(?!-)  (analogous for = and .)
                let c = self.peek(0).expect("non-empty");
                // Accumulate a run for efficiency, stopping before potential link end.
                let mut n = 0;
                while let Some(c2) = self.peek(n) {
                    let stop = match kind {
                        '-' => c2 == '-' && self.peek(n + 1) == Some('-'),
                        '=' => c2 == '=' && self.peek(n + 1) == Some('='),
                        _ => {
                            (c2 == '.' && self.peek(n + 1) == Some('-'))
                                || (c2 == '-' && self.peek(n + 1) == Some('.'))
                        }
                    };
                    if stop {
                        break;
                    }
                    n += 1;
                }
                if n == 0 {
                    // single char fallthrough
                    let _ = c;
                    n = 1;
                }
                return Tok::EdgeText(self.take(n));
            }
            State::Text => {
                // close tokens first
                for (pat, tok, pop) in [
                    ("])", Tok::StadiumEnd, true),
                    ("]]", Tok::SubroutineEnd, true),
                    (")]", Tok::CylinderEnd, true),
                    (")))", Tok::DoubleCircleEnd, true),
                ] {
                    if self.starts_with(pat) {
                        self.pos += pat.chars().count();
                        if pop {
                            self.stack.pop();
                        }
                        return tok;
                    }
                }
                if self.starts_with("|") {
                    self.pos += 1;
                    self.stack.pop();
                    return Tok::Pipe;
                }
                if self.starts_with(")") {
                    self.pos += 1;
                    self.stack.pop();
                    return Tok::Pe;
                }
                if self.starts_with("]") {
                    self.pos += 1;
                    self.stack.pop();
                    return Tok::Sqe;
                }
                if self.starts_with("}") {
                    self.pos += 1;
                    self.stack.pop();
                    return Tok::DiamondStop;
                }
                if self.starts_with("\"`") {
                    self.pos += 2;
                    self.stack.push(State::MdStr);
                    return self.next_token();
                }
                if self.starts_with("\"") {
                    self.pos += 1;
                    self.stack.push(State::Str);
                    return self.next_token();
                }
                // Openers apply in any state (`<*>` rules in jison).
                for (pat, tok, state) in [
                    ("(-", Tok::EllipseStart, State::EllipseText),
                    ("([", Tok::StadiumStart, State::Text),
                    ("[[", Tok::SubroutineStart, State::Text),
                    ("[(", Tok::CylinderStart, State::Text),
                    ("(((", Tok::DoubleCircleStart, State::Text),
                    ("[/", Tok::TrapStart, State::TrapText),
                    ("[\\", Tok::InvTrapStart, State::TrapText),
                ] {
                    if self.starts_with(pat) {
                        self.pos += pat.chars().count();
                        self.stack.push(state);
                        return tok;
                    }
                }
                if self.starts_with("(") {
                    self.pos += 1;
                    self.stack.push(State::Text);
                    return Tok::Ps;
                }
                if self.starts_with("[") {
                    self.pos += 1;
                    self.stack.push(State::Text);
                    return Tok::Sqs;
                }
                if self.starts_with("{") {
                    self.pos += 1;
                    self.stack.push(State::Text);
                    return Tok::DiamondStart;
                }
                // TEXT: [^\[\]\(\)\{\}\|\"]+
                let mut n = 0;
                while self
                    .peek(n)
                    .is_some_and(|c| !matches!(c, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"'))
                {
                    n += 1;
                }
                if n == 0 {
                    let c = self.take(1);
                    return Tok::Text(c);
                }
                return Tok::Text(self.take(n));
            }
            State::EllipseText => {
                if self.starts_with("-)") || self.starts_with("/)") || self.starts_with("))") {
                    self.pos += 2;
                    self.stack.pop();
                    return Tok::EllipseEnd;
                }
                let mut n = 0;
                while self.peek(n).is_some_and(|c| {
                    !matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
                        && (c != '-' || self.peek(n + 1) != Some(')'))
                }) {
                    n += 1;
                }
                if n == 0 {
                    let c = self.take(1);
                    return Tok::Text(c);
                }
                return Tok::Text(self.take(n));
            }
            State::TrapText => {
                if self.starts_with("\\]") {
                    self.pos += 2;
                    self.stack.pop();
                    return Tok::TrapEnd;
                }
                if self.starts_with("/]") {
                    self.pos += 2;
                    self.stack.pop();
                    return Tok::InvTrapEnd;
                }
                let mut n = 0;
                while let Some(c) = self.peek(n) {
                    if c == '/' || c == '\\' {
                        if self.peek(n + 1) == Some(']') {
                            break;
                        }
                        n += 1;
                        continue;
                    }
                    if matches!(c, '[' | ']' | '(' | ')' | '{' | '}') {
                        break;
                    }
                    n += 1;
                }
                if n == 0 {
                    let c = self.take(1);
                    return Tok::Text(c);
                }
                return Tok::Text(self.take(n));
            }
            State::Initial => {}
        }

        // INITIAL state
        let rest = self.rest();

        // Comments: %% to end of line (mermaid strips them pre-parse, we do it here)
        if self.starts_with("%%") {
            let mut n = 0;
            while self.peek(n).is_some_and(|c| c != '\n') {
                n += 1;
            }
            self.pos += n;
            return self.next_token();
        }

        // Keywords and special prefixes
        if self.starts_with("accTitle") {
            let mut n = "accTitle".len();
            while self.peek(n).is_some_and(char::is_whitespace) {
                n += 1;
            }
            if self.peek(n) == Some(':') {
                n += 1;
                while self.peek(n).is_some_and(|c| c == ' ' || c == '\t') {
                    n += 1;
                }
                self.pos += n;
                self.stack.push(State::AccTitle);
                return Tok::AccTitle;
            }
        }
        if self.starts_with("accDescr") {
            let mut n = "accDescr".len();
            while self.peek(n).is_some_and(char::is_whitespace) {
                n += 1;
            }
            if self.peek(n) == Some(':') {
                n += 1;
                while self.peek(n).is_some_and(|c| c == ' ' || c == '\t') {
                    n += 1;
                }
                self.pos += n;
                self.stack.push(State::AccDescr);
                return Tok::AccDescr;
            }
            if self.peek(n) == Some('{') {
                self.pos += n + 1;
                self.stack.push(State::AccDescrMultiline);
                return Tok::AccDescr;
            }
        }

        if self.starts_with("@{") {
            self.pos += 2;
            self.stack.push(State::ShapeData);
            return Tok::ShapeData(String::new());
        }

        if self.starts_with("call ") || self.starts_with("call\t") {
            self.pos += 5;
            self.stack.push(State::CallbackName);
            return self.next_token();
        }

        if self.starts_with("\"`") {
            self.pos += 2;
            self.stack.push(State::MdStr);
            return self.next_token();
        }
        if self.starts_with("\"") {
            self.pos += 1;
            self.stack.push(State::Str);
            return self.next_token();
        }

        // direction statements (before keywords; jison rule `.*direction\s+TB[^\n]*`)
        {
            // The jison pattern allows leading characters; in practice it is
            // used as a standalone statement, possibly indented.
            let line_start: String = rest.iter().take_while(|&&c| c != '\n').collect();
            let trimmed = line_start.trim_start();
            if let Some(rest) = trimmed.strip_prefix("direction") {
                let after = rest.trim_start();
                for d in ["TB", "BT", "RL", "LR", "TD"] {
                    if after.starts_with(d) {
                        let n = line_start.chars().count();
                        self.pos += n;
                        return Tok::Direction((*d).to_owned());
                    }
                }
            }
        }

        for (kw, tok) in [
            ("style", Tok::Style),
            ("default", Tok::Default),
            ("linkStyle", Tok::LinkStyle),
            ("interpolate", Tok::Interpolate),
            ("classDef", Tok::ClassDef),
            ("class", Tok::Class),
            ("href", Tok::Href),
            ("subgraph", Tok::Subgraph),
            ("_self", Tok::LinkTarget("_self".into())),
            ("_blank", Tok::LinkTarget("_blank".into())),
            ("_parent", Tok::LinkTarget("_parent".into())),
            ("_top", Tok::LinkTarget("_top".into())),
        ] {
            if self.starts_with(kw) {
                // Keyword must not be a prefix of a longer NODE_STRING.
                let len = kw.chars().count();
                let next = self.peek(len);
                let part_of_id = next.is_some_and(is_node_string_char);
                if !part_of_id {
                    if kw == "href" {
                        // jison: "href"[\s]
                        if !next.is_some_and(char::is_whitespace) {
                            continue;
                        }
                        self.pos += len + 1;
                        return Tok::Href;
                    }
                    self.pos += len;
                    return tok;
                }
            }
        }

        if self.starts_with("click") {
            let next = self.peek(5);
            if next.is_some_and(char::is_whitespace) {
                self.pos += 5;
                while self.peek(0).is_some_and(|c| c == ' ' || c == '\t') {
                    self.pos += 1;
                }
                self.stack.push(State::Click);
                return self.next_token();
            }
        }

        for kw in ["flowchart-elk", "flowchart", "graph"] {
            if self.starts_with(kw) {
                let len = kw.chars().count();
                if !self.peek(len).is_some_and(is_node_string_char) {
                    self.pos += len;
                    if self.first_graph {
                        self.first_graph = false;
                        self.stack.push(State::Dir);
                    }
                    return Tok::Graph;
                }
            }
        }
        if self.starts_with("end") {
            // "end"\b\s*
            let next = self.peek(3);
            if !next.is_some_and(is_node_string_char) && next != Some('-') {
                self.pos += 3;
                while self.peek(0).is_some_and(|c| c == ' ' || c == '\t') {
                    self.pos += 1;
                }
                return Tok::End;
            }
        }

        // LINK_ID: [^\s"]+@(?=[^{"])
        {
            let mut n = 0;
            while self
                .peek(n)
                .is_some_and(|c| !c.is_whitespace() && c != '"' && c != '@')
            {
                n += 1;
            }
            if n > 0
                && self.peek(n) == Some('@')
                && self.peek(n + 1).is_some_and(|c| c != '{' && c != '"')
            {
                let id = self.take(n + 1);
                return Tok::LinkId(id);
            }
        }

        // Link patterns (longest match before NODE_STRING/punctuation)
        // Full links first (longest), then start links.
        for kind in ['-', '=', '.'] {
            if let Some((n, body)) = self.match_link(true, kind) {
                self.pos += n;
                return Tok::Link(body);
            }
        }
        if self.starts_with("~~~") {
            let mut n = 3;
            while self.peek(n) == Some('~') {
                n += 1;
            }
            let body = self.take(n);
            while self.peek(0).is_some_and(|c| c.is_whitespace() && c != '\n') {
                self.pos += 1;
            }
            return Tok::Link(body);
        }
        for (kind, state) in [
            ('-', State::EdgeText),
            ('=', State::ThickEdgeText),
            ('.', State::DottedEdgeText),
        ] {
            if let Some((n, body)) = self.match_link(false, kind) {
                self.pos += n;
                self.stack.push(state);
                return Tok::StartLink(body);
            }
        }

        // Shape openers
        for (pat, tok, state) in [
            ("(-", Tok::EllipseStart, Some(State::EllipseText)),
            ("([", Tok::StadiumStart, Some(State::Text)),
            ("[[", Tok::SubroutineStart, Some(State::Text)),
            ("[|", Tok::VertexWithPropsStart, None),
            ("[(", Tok::CylinderStart, Some(State::Text)),
            ("(((", Tok::DoubleCircleStart, Some(State::Text)),
            ("[/", Tok::TrapStart, Some(State::TrapText)),
            ("[\\", Tok::InvTrapStart, Some(State::TrapText)),
        ] {
            if self.starts_with(pat) {
                self.pos += pat.chars().count();
                if let Some(s) = state {
                    self.stack.push(s);
                }
                return tok;
            }
        }

        let c = rest[0];
        match c {
            '>' => {
                self.pos += 1;
                self.stack.push(State::Text);
                return Tok::TagEnd;
            }
            '<' => {
                self.pos += 1;
                return Tok::TagStart;
            }
            '^' => {
                self.pos += 1;
                return Tok::Up;
            }
            '|' => {
                self.pos += 1;
                self.stack.push(State::Text);
                return Tok::Pipe;
            }
            '(' => {
                self.pos += 1;
                self.stack.push(State::Text);
                return Tok::Ps;
            }
            '[' => {
                self.pos += 1;
                self.stack.push(State::Text);
                return Tok::Sqs;
            }
            '{' => {
                self.pos += 1;
                self.stack.push(State::Text);
                return Tok::DiamondStart;
            }
            '#' => {
                self.pos += 1;
                return Tok::Brkt;
            }
            '&' => {
                self.pos += 1;
                return Tok::Amp;
            }
            ';' => {
                self.pos += 1;
                return Tok::Semi;
            }
            ',' => {
                self.pos += 1;
                return Tok::Comma;
            }
            '*' => {
                self.pos += 1;
                return Tok::Mult;
            }
            'v'
                // single 'v' is DOWN only when not part of NODE_STRING
                if !self.peek(1).is_some_and(is_node_string_char) => {
                    self.pos += 1;
                    return Tok::Down;
                }
            _ => {}
        }

        if self.starts_with(":::") {
            self.pos += 3;
            return Tok::StyleSeparator;
        }
        if c == ':' {
            self.pos += 1;
            return Tok::Colon;
        }

        // NUM (digits) — wins over NODE_STRING when the whole match is digits
        if c.is_ascii_digit() {
            let mut n = 0;
            while self.peek(n).is_some_and(|ch| ch.is_ascii_digit()) {
                n += 1;
            }
            // If a longer NODE_STRING match exists, prefer it (longest match).
            let mut m = 0;
            while self.peek(m).is_some_and(|ch| {
                is_node_string_char(ch)
                    || (ch == '-'
                        && self
                            .peek(m + 1)
                            .is_some_and(|c2| c2 != '>' && c2 != '-' && c2 != '.'))
                    || (ch == '=' && self.peek(m + 1) != Some('='))
            }) {
                m += 1;
            }
            if m > n {
                return Tok::NodeString(self.take(m));
            }
            return Tok::Num(self.take(n));
        }

        // NODE_STRING
        if is_node_string_char(c) || c == '-' || c == '=' {
            let mut n = 0;
            while let Some(ch) = self.peek(n) {
                if is_node_string_char(ch) {
                    n += 1;
                } else if ch == '-' {
                    // \-(?=[^\>\-\.])
                    let next = self.peek(n + 1);
                    if next.is_some_and(|c2| c2 != '>' && c2 != '-' && c2 != '.') {
                        n += 1;
                    } else {
                        break;
                    }
                } else if ch == '=' {
                    if self.peek(n + 1) == Some('=') {
                        break;
                    }
                    n += 1;
                } else {
                    break;
                }
            }
            if n > 0 {
                return Tok::NodeString(self.take(n));
            }
        }

        if c == '-' {
            self.pos += 1;
            return Tok::Minus;
        }

        if is_unicode_text(c) {
            let mut n = 0;
            while self.peek(n).is_some_and(is_unicode_text) {
                n += 1;
            }
            return Tok::UnicodeText(self.take(n));
        }

        if c == '\n' || c == '\r' {
            let mut n = 0;
            while self.peek(n).is_some_and(|ch| ch == '\n' || ch == '\r') {
                n += 1;
            }
            self.pos += n;
            return Tok::Newline;
        }
        if c.is_whitespace() {
            self.pos += 1;
            return Tok::Space;
        }
        if c == '"' {
            self.pos += 1;
            return Tok::Quote;
        }

        // Unknown char: skip.
        self.pos += 1;
        self.next_token()
    }

    #[must_use]
    pub fn tokenize(mut self) -> Vec<Tok> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let done = tok == Tok::Eof;
            tokens.push(tok);
            if done {
                break;
            }
        }
        tokens
    }
}
