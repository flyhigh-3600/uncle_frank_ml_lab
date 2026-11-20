//! Syntax front-end for Uncle Frank's Plan (`.ufp`).
//!
//! This crate owns source spans, lexing, the shallow plan AST, and diagnostics.
//! Expression/type parsing is deliberately a later layer; procedure bodies are
//! retained as balanced token blocks until UF-103 gives them a typed AST.

use std::fmt;

pub const UFP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Keyword,
    Int,
    Float,
    String,
    TextBlock,
    Punct(&'static str),
    Eof,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.span.line, self.span.column, self.code, self.message
        )
    }
}

const KEYWORDS: &[&str] = &[
    "procedure",
    "node",
    "pipe",
    "portal",
    "import",
    "text",
    "graph",
    "input",
    "output",
    "inout",
    "with",
    "and",
    "or",
    "not",
    "if",
    "elif",
    "else",
    "return",
    "let",
    "run",
    "until",
    "structured",
    "condition",
    "matrix",
    "list",
    "sheet",
    "string",
    "binary",
    "bool",
    "int",
    "float",
    "bfloat",
    "quant",
    "double",
    "true",
    "false",
    "null",
    "auto",
    "as",
    "from",
    "of",
];

pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let (mut i, mut line, mut column) = (0, 1, 1);
    while i < bytes.len() {
        let start = i;
        let start_line = line;
        let start_column = column;
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            advance(source, &mut i, &mut line, &mut column);
            continue;
        }
        // Keep UTF-8 byte spans valid while reporting columns in characters.
        if !b.is_ascii() {
            let width = source[i..].chars().next().map(char::len_utf8).unwrap_or(1);
            i += width;
            column += 1;
            errors.push(diag("UFP1003", "invalid character", start, i, start_line, start_column));
            continue;
        }
        if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                advance(source, &mut i, &mut line, &mut column);
            }
            continue;
        }
        if bytes.get(i..i + 2) == Some(b"/*") {
            advance(source, &mut i, &mut line, &mut column);
            advance(source, &mut i, &mut line, &mut column);
            let mut closed = false;
            while i < bytes.len() {
                if bytes.get(i..i + 2) == Some(b"*/") {
                    advance(source, &mut i, &mut line, &mut column);
                    advance(source, &mut i, &mut line, &mut column);
                    closed = true;
                    break;
                }
                advance(source, &mut i, &mut line, &mut column);
            }
            if !closed {
                errors.push(diag(
                    "UFP1002",
                    "unterminated block comment",
                    start,
                    i,
                    start_line,
                    start_column,
                ));
            }
            continue;
        }
        if bytes.get(i..i + 3) == Some(b"\"\"\"") {
            for _ in 0..3 {
                advance(source, &mut i, &mut line, &mut column);
            }
            let content_start = i;
            let mut closed = false;
            while i < bytes.len() {
                if bytes.get(i..i + 3) == Some(b"\"\"\"") {
                    let text = source[content_start..i].to_owned();
                    for _ in 0..3 {
                        advance(source, &mut i, &mut line, &mut column);
                    }
                    out.push(token(
                        TokenKind::TextBlock,
                        text,
                        start,
                        i,
                        start_line,
                        start_column,
                    ));
                    closed = true;
                    break;
                }
                advance(source, &mut i, &mut line, &mut column);
            }
            if !closed {
                errors.push(diag(
                    "UFP1006",
                    "unterminated text block",
                    start,
                    i,
                    start_line,
                    start_column,
                ));
                out.push(token(TokenKind::Error, source[start..i].to_owned(), start, i, start_line, start_column));
            }
            continue;
        }
        if b == b'"' {
            advance(source, &mut i, &mut line, &mut column);
            let content_start = i;
            let mut escaped = false;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\n' && !escaped {
                    break;
                }
                if bytes[i] == b'"' && !escaped {
                    let text = source[content_start..i].to_owned();
                    advance(source, &mut i, &mut line, &mut column);
                    out.push(token(
                        TokenKind::String,
                        text,
                        start,
                        i,
                        start_line,
                        start_column,
                    ));
                    closed = true;
                    break;
                }
                escaped = bytes[i] == b'\\' && !escaped;
                if bytes[i] != b'\\' {
                    escaped = false;
                }
                advance(source, &mut i, &mut line, &mut column);
            }
            if !closed {
                errors.push(diag(
                    "UFP1001",
                    "unterminated string",
                    start,
                    i,
                    start_line,
                    start_column,
                ));
                out.push(token(TokenKind::Error, source[start..i].to_owned(), start, i, start_line, start_column));
            }
            continue;
        }
        if is_ident_start(b) {
            advance(source, &mut i, &mut line, &mut column);
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                advance(source, &mut i, &mut line, &mut column);
            }
            let text = source[start..i].to_owned();
            let kind = if KEYWORDS.contains(&text.as_str()) {
                TokenKind::Keyword
            } else {
                TokenKind::Ident
            };
            out.push(token(kind, text, start, i, start_line, start_column));
            continue;
        }
        if b.is_ascii_digit() {
            // Zone paths deliberately tokenize 0.1.2 as INT DOT INT DOT INT,
            // not as a float followed by a dot. The annotation grammar reserves
            // this tiny context-sensitive lexical rule.
            let zone_start = source[..start].rfind("@T(");
            let in_zone_path = zone_start.is_some_and(|at| !source[at + 3..start].contains(')'));
            advance(source, &mut i, &mut line, &mut column);
            let mut malformed_number = false;
            if b == b'0' && matches!(bytes.get(i), Some(b'x' | b'X')) {
                advance(source, &mut i, &mut line, &mut column);
                let digits_start = i;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    advance(source, &mut i, &mut line, &mut column);
                }
                malformed_number = i == digits_start;
            } else {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    advance(source, &mut i, &mut line, &mut column);
                }
                if !in_zone_path
                    && matches!(bytes.get(i), Some(b'.'))
                    && bytes.get(i + 1).is_some_and(|x| x.is_ascii_digit())
                {
                    advance(source, &mut i, &mut line, &mut column);
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        advance(source, &mut i, &mut line, &mut column);
                    }
                }
                if !in_zone_path && matches!(bytes.get(i), Some(b'e' | b'E')) {
                    advance(source, &mut i, &mut line, &mut column);
                    if matches!(bytes.get(i), Some(b'+' | b'-')) {
                        advance(source, &mut i, &mut line, &mut column);
                    }
                    let exponent_start = i;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        advance(source, &mut i, &mut line, &mut column);
                    }
                    malformed_number = i == exponent_start;
                }
            }
            let text = source[start..i].to_owned();
            if malformed_number {
                errors.push(diag("UFP1004", "malformed numeric literal", start, i, start_line, start_column));
                out.push(token(TokenKind::Error, text, start, i, start_line, start_column));
                continue;
            }
            let kind = if !in_zone_path && text.contains(['.', 'e', 'E']) {
                TokenKind::Float
            } else {
                TokenKind::Int
            };
            out.push(token(kind, text, start, i, start_line, start_column));
            continue;
        }
        let punct = if bytes.get(i..i + 2) == Some(b"->") {
            Some("->")
        } else if bytes.get(i..i + 2) == Some(b".*") {
            Some(".*")
        } else if bytes.get(i..i + 2) == Some(b"==") {
            Some("==")
        } else if bytes.get(i..i + 2) == Some(b"!=") {
            Some("!=")
        } else if bytes.get(i..i + 2) == Some(b"<=") {
            Some("<=")
        } else if bytes.get(i..i + 2) == Some(b">=") {
            Some(">=")
        } else {
            match b {
                b'(' => Some("("),
                b')' => Some(")"),
                b'{' => Some("{"),
                b'}' => Some("}"),
                b'[' => Some("["),
                b']' => Some("]"),
                b',' => Some(","),
                b';' => Some(";"),
                b':' => Some(":"),
                b'.' => Some("."),
                b'@' => Some("@"),
                b'+' => Some("+"),
                b'-' => Some("-"),
                b'*' => Some("*"),
                b'/' => Some("/"),
                b'%' => Some("%"),
                b'=' => Some("="),
                b'<' => Some("<"),
                b'>' => Some(">"),
                b'~' => Some("~"),
                b'&' => Some("&"),
                b'|' => Some("|"),
                b'^' => Some("^"),
                _ => None,
            }
        };
        if let Some(p) = punct {
            for _ in 0..p.len() {
                advance(source, &mut i, &mut line, &mut column);
            }
            out.push(token(
                TokenKind::Punct(p),
                p.to_owned(),
                start,
                i,
                start_line,
                start_column,
            ));
        } else {
            advance(source, &mut i, &mut line, &mut column);
            errors.push(diag(
                "UFP1003",
                "invalid character",
                start,
                i,
                start_line,
                start_column,
            ));
        }
    }
    out.push(token(TokenKind::Eof, String::new(), i, i, line, column));
    (out, errors)
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}
fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}
fn advance(source: &str, i: &mut usize, line: &mut usize, column: &mut usize) {
    if source.as_bytes()[*i] == b'\n' {
        *line += 1;
        *column = 1;
    } else {
        *column += 1;
    }
    *i += 1;
}
fn token(
    kind: TokenKind,
    text: String,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
) -> Token {
    Token {
        kind,
        text,
        span: Span {
            start,
            end,
            line,
            column,
        },
    }
}
fn diag(
    code: &'static str,
    message: &str,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span: Span {
            start,
            end,
            line,
            column,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub items: Vec<Item>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Input {
        name: String,
        value: String,
        span: Span,
    },
    Output {
        name: String,
        span: Span,
    },
    Text {
        name: String,
        path: String,
        span: Span,
    },
    Portal {
        name: String,
        path: String,
        alias: Option<String>,
        span: Span,
    },
    Pipe {
        from: Endpoint,
        to: Endpoint,
        span: Span,
    },
    Other {
        keyword: String,
        span: Span,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub node: String,
    pub port: String,
}

pub fn parse_plan(source: &str) -> Result<Plan, Vec<Diagnostic>> {
    let (tokens, mut errors) = lex(source);
    let mut p = Parser {
        tokens: &tokens,
        at: 0,
        errors: Vec::new(),
    };
    let mut items = Vec::new();
    while !p.at_eof() {
        match p.peek().text.as_str() {
            "input" => {
                if let Some(item) = p.input() {
                    items.push(item);
                }
            }
            "output" => {
                if let Some(item) = p.output() {
                    items.push(item);
                }
            }
            "text" => {
                if let Some(item) = p.text() {
                    items.push(item);
                }
            }
            "portal" => {
                if let Some(item) = p.portal() {
                    items.push(item);
                }
            }
            "pipe" => {
                if let Some(item) = p.pipe() {
                    items.push(item);
                }
            }
            "matrix" | "binary" => {
                if let Some(item) = p.named_data() {
                    items.push(item);
                }
            }
            "procedure" | "import" | "node" | "@" => {
                let t = p.bump().clone();
                items.push(Item::Other {
                    keyword: t.text,
                    span: t.span,
                });
                p.skip_declaration();
            }
            _ => {
                p.error("UFP2002", "expected a top-level declaration");
                p.skip_declaration();
            }
        }
    }
    errors.extend(p.errors);
    if errors.is_empty() {
        Ok(Plan { items })
    } else {
        Err(errors)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    errors: Vec<Diagnostic>,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn at_eof(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }
    fn bump(&mut self) -> &Token {
        let t = &self.tokens[self.at];
        if !self.at_eof() {
            self.at += 1;
        }
        t
    }
    fn take(&mut self, text: &str) -> Option<Token> {
        if self.peek().text == text {
            Some(self.bump().clone())
        } else {
            self.error("UFP2001", &format!("expected `{text}`"));
            None
        }
    }
    fn name(&mut self) -> Option<Token> {
        if self.peek().kind == TokenKind::Ident {
            Some(self.bump().clone())
        } else {
            self.error("UFP2001", "expected identifier");
            None
        }
    }
    fn word(&mut self) -> Option<Token> {
        if matches!(self.peek().kind, TokenKind::Ident | TokenKind::Keyword) {
            Some(self.bump().clone())
        } else {
            self.error("UFP2001", "expected name");
            None
        }
    }
    fn text_value(&mut self) -> Option<Token> {
        if matches!(self.peek().kind, TokenKind::String | TokenKind::TextBlock) {
            Some(self.bump().clone())
        } else {
            self.error("UFP2001", "expected text literal");
            None
        }
    }
    fn error(&mut self, code: &'static str, message: &str) {
        self.errors.push(Diagnostic {
            code,
            message: message.into(),
            span: self.peek().span,
        });
    }
    fn input(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let name = self.name()?;
        self.take("=")?;
        let value = self.text_value()?;
        self.take(";")?;
        Some(Item::Input {
            name: name.text,
            value: value.text,
            span: first.span,
        })
    }
    fn output(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let name = self.name()?;
        self.take(";")?;
        Some(Item::Output {
            name: name.text,
            span: first.span,
        })
    }
    fn text(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let name = self.name()?;
        self.take("=")?;
        let path = self.text_value()?;
        self.take(";")?;
        Some(Item::Text {
            name: name.text,
            path: path.text,
            span: first.span,
        })
    }
    fn portal(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let name = self.name()?;
        self.take("=")?;
        let path = self.text_value()?;
        let alias = if self.peek().text == "as" {
            self.bump();
            Some(self.name()?.text)
        } else {
            None
        };
        self.take(";")?;
        Some(Item::Portal {
            name: name.text,
            path: path.text,
            alias,
            span: first.span,
        })
    }
    fn endpoint(&mut self) -> Option<Endpoint> {
        let node = self.name()?;
        self.take(".")?;
        let port = self.word()?;
        Some(Endpoint {
            node: node.text,
            port: port.text,
        })
    }
    fn pipe(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let from = self.endpoint()?;
        self.take("->")?;
        let to = self.endpoint()?;
        self.take(";")?;
        Some(Item::Pipe {
            from,
            to,
            span: first.span,
        })
    }
    /// Parse enough of a named matrix/binary declaration to retain it at the
    /// plan level. Its expression/type AST is intentionally deferred to UF-103.
    fn named_data(&mut self) -> Option<Item> {
        let first = self.bump().clone();
        let mut saw_equals = false;
        let mut depth = 0usize;
        while !self.at_eof() {
            let t = self.bump();
            match t.text.as_str() {
                "{" | "(" | "[" | "<" => depth += 1,
                "}" | ")" | "]" | ">" => depth = depth.saturating_sub(1),
                "=" if depth == 0 => saw_equals = true,
                ";" if depth == 0 => break,
                _ => {}
            }
        }
        if !saw_equals {
            self.errors.push(Diagnostic {
                code: "UFP2001",
                message: "expected `=` in named data declaration".into(),
                span: first.span,
            });
            return None;
        }
        Some(Item::Other {
            keyword: first.text,
            span: first.span,
        })
    }
    fn skip_declaration(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            let t = self.bump();
            match t.text.as_str() {
                "{" | "(" | "[" => depth += 1,
                "}" | ")" | "]" => depth = depth.saturating_sub(1),
                ";" if depth == 0 => break,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexes_text_terminal_and_zone() {
        let (t, e) = lex("input x = \"\"\"hi\\n\"\"\"; @T(0.1.2)");
        assert!(e.is_empty());
        assert!(t.iter().any(|x| x.kind == TokenKind::TextBlock));
        assert_eq!(t.iter().filter(|x| x.text == ".").count(), 2);
    }
    #[test]
    fn parses_terminal_pipe() {
        let plan = parse_plan("input a = \"x\"; output b; pipe a.text -> b.text;").unwrap();
        assert_eq!(plan.items.len(), 3);
    }
    #[test]
    fn accepts_named_nested_matrix() {
        let plan = parse_plan("matrix[2,2] layer = [[1.0, 0.0], [0.0, 1.0]];").unwrap();
        assert_eq!(plan.items.len(), 1);
    }
    #[test]
    fn malformed_lexemes_have_valid_spans_and_recover() {
        let source = "é 0x 2e+ \"unfinished\noutput ok;";
        let (tokens, errors) = lex(source);
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["UFP1003", "UFP1004", "UFP1004", "UFP1001"]);
        for error in &errors {
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
            assert!(error.span.end >= error.span.start);
        }
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Error));
        assert!(tokens.iter().any(|t| t.text == "output"));
    }

    #[test]
    fn unterminated_block_comment_is_diagnosed_at_opener() {
        let (tokens, errors) = lex("/* bad");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "UFP1002");
        assert_eq!((errors[0].span.start, errors[0].span.end), (0, 6));
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
    }
}
