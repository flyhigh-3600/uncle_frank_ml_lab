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
            errors.push(diag(
                "UFP1003",
                "invalid character",
                start,
                i,
                start_line,
                start_column,
            ));
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
                out.push(token(
                    TokenKind::Error,
                    source[start..i].to_owned(),
                    start,
                    i,
                    start_line,
                    start_column,
                ));
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
                out.push(token(
                    TokenKind::Error,
                    source[start..i].to_owned(),
                    start,
                    i,
                    start_line,
                    start_column,
                ));
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
                errors.push(diag(
                    "UFP1004",
                    "malformed numeric literal",
                    start,
                    i,
                    start_line,
                    start_column,
                ));
                out.push(token(
                    TokenKind::Error,
                    text,
                    start,
                    i,
                    start_line,
                    start_column,
                ));
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

/// Lossless-enough syntax tree produced by [`parse_ast`]. `SyntaxNode` keeps
/// delimiters and leaf tokens, so consumers can build typed/lowered IR without
/// reparsing source text, while retaining exact source locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxNode {
    pub kind: SyntaxKind,
    pub span: Span,
    pub token: Option<Token>,
    pub children: Vec<SyntaxNode>,
    pub expression: Option<Expr>,
    pub type_ast: Option<TypeAst>,
    pub annotation_ast: Option<AnnotationAst>,
    pub import_ast: Option<ImportAst>,
    pub constraints: Vec<Expr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxKind {
    Plan,
    Annotation,
    Import,
    Portal,
    Text,
    Input,
    Output,
    Matrix,
    BinaryDecl,
    Procedure,
    Pin,
    Graph,
    Node,
    Pipe,
    Block,
    Let,
    Assign,
    If,
    Return,
    ExpressionStatement,
    Expression,
    Type,
    Constraint,
    List,
    Field,
    Index,
    Call,
    PortMap,
    RunUntil,
    Scope,
    Unary,
    Binary,
    PortChain,
    Literal,
    Identifier,
    Operator,
    Group,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Literal(String),
    Identifier(String),
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    Current,
    Scope {
        depth: usize,
        name: String,
        suffix: Vec<Expr>,
    },
    Unary {
        op: String,
        value: Box<Expr>,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    Index {
        base: Box<Expr>,
        indices: Vec<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    PortMap {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    PortChain {
        links: Vec<Expr>,
    },
    List(Vec<Expr>),
    Structured(Vec<ExprField>),
    Matrix(Vec<Expr>),
    Condition(Box<Expr>),
    RunUntil {
        value: Box<Expr>,
        condition: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprField {
    pub ty: Option<TypeAst>,
    pub name: String,
    pub value: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeAst {
    Named(String),
    Sized {
        name: String,
        bits: Option<String>,
        format: Option<String>,
    },
    UnaryContainer {
        name: String,
        element: Option<Box<TypeAst>>,
    },
    Matrix {
        rank: Option<String>,
        dimensions: Vec<String>,
        dimension_exprs: Vec<Expr>,
        element: Option<Box<TypeAst>>,
    },
    Structured(Vec<(Box<TypeAst>, String)>),
    Condition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportAst {
    pub path: Vec<String>,
    pub from: Option<String>,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationAst {
    pub name: String,
    pub args: Vec<AnnotationArg>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnnotationArg {
    Expression(Expr),
    Name(String),
    Zone(Vec<String>),
}

pub fn parse_annotation(source: &str) -> Result<AnnotationAst, Vec<Diagnostic>> {
    let (tokens, mut errors) = lex(source);
    let mut p = AstParser {
        tokens: &tokens,
        at: 0,
        errors: Vec::new(),
    };
    let start = p.expect("@");
    let name = p.take().text;
    let mut args = Vec::new();
    if p.is("(") {
        p.take();
        while !p.eof() && !p.is(")") {
            if p.tok().kind == TokenKind::Int
                && p.tokens.get(p.at + 1).is_some_and(|t| t.text == ".")
            {
                let mut zone = vec![p.take().text];
                while p.is(".") {
                    p.take();
                    if p.tok().kind == TokenKind::Int {
                        zone.push(p.take().text);
                    } else {
                        p.error("UFP2001", "expected integer in zone path");
                        break;
                    }
                }
                args.push(AnnotationArg::Zone(zone));
            } else if p.tok().kind == TokenKind::Ident
                && p.tokens
                    .get(p.at + 1)
                    .is_some_and(|t| t.text == "," || t.text == ")")
            {
                args.push(AnnotationArg::Name(p.take().text));
            } else {
                args.push(AnnotationArg::Expression(p.expr(0)));
            }
            if p.is(",") {
                p.take();
            } else {
                break;
            }
        }
        p.expect(")");
    }
    if !p.eof() {
        p.error("UFP2002", "unexpected token after annotation");
    }
    let end = p.tokens[p.at.saturating_sub(1)].clone();
    errors.extend(p.errors);
    if errors.is_empty() {
        Ok(AnnotationAst {
            name,
            args,
            span: Span {
                start: start.span.start,
                end: end.span.end,
                line: start.span.line,
                column: start.span.column,
            },
        })
    } else {
        Err(errors)
    }
}

/// Parse one expression using the UFP precedence table.
pub fn parse_expression(source: &str) -> Result<Expr, Vec<Diagnostic>> {
    let (tokens, mut errors) = lex(source);
    let mut p = AstParser {
        tokens: &tokens,
        at: 0,
        errors: Vec::new(),
    };
    let expr = p.expr(0);
    if !p.eof() {
        p.error("UFP2002", "unexpected token after expression");
    }
    errors.extend(p.errors);
    if errors.is_empty() {
        Ok(expr)
    } else {
        Err(errors)
    }
}

/// Parse a UFP type, including matrix shapes and structured field types.
pub fn parse_type(source: &str) -> Result<TypeAst, Vec<Diagnostic>> {
    let (tokens, mut errors) = lex(source);
    let mut p = AstParser {
        tokens: &tokens,
        at: 0,
        errors: Vec::new(),
    };
    let ty = p.type_ast();
    if !p.eof() {
        p.error("UFP2002", "unexpected token after type");
    }
    errors.extend(p.errors);
    if errors.is_empty() {
        Ok(ty)
    } else {
        Err(errors)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstPlan {
    pub declarations: Vec<SyntaxNode>,
}

/// Parse the documented UFP syntax into a tree. The tree is intentionally
/// concrete (operators/delimiters are retained as token nodes) to preserve
/// syntax details such as scope-dot depth and port-map chains.
pub fn parse_ast(source: &str) -> Result<AstPlan, Vec<Diagnostic>> {
    let (ast, errors) = parse_ast_recovering(source);
    if errors.is_empty() {
        Ok(ast)
    } else {
        Err(errors)
    }
}

/// Parse while retaining declarations recovered after syntax errors.
pub fn parse_ast_recovering(source: &str) -> (AstPlan, Vec<Diagnostic>) {
    let (tokens, mut errors) = lex(source);
    let mut parser = AstParser {
        tokens: &tokens,
        at: 0,
        errors: Vec::new(),
    };
    let mut declarations = Vec::new();
    while !parser.eof() {
        let start = parser.at;
        let node = parser.declaration();
        if let Some(node) = node {
            declarations.push(node);
        }
        if parser.at == start {
            parser.error("UFP2002", "unable to parse declaration; skipping token");
            parser.at += 1;
        }
    }
    errors.extend(parser.errors);
    (AstPlan { declarations }, errors)
}

struct AstParser<'a> {
    tokens: &'a [Token],
    at: usize,
    errors: Vec<Diagnostic>,
}
impl<'a> AstParser<'a> {
    fn tok(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn eof(&self) -> bool {
        self.tok().kind == TokenKind::Eof
    }
    fn is(&self, s: &str) -> bool {
        self.tok().text == s
    }
    fn take(&mut self) -> Token {
        let t = self.tok().clone();
        if !self.eof() {
            self.at += 1;
        }
        t
    }
    fn error(&mut self, code: &'static str, message: &str) {
        self.errors.push(Diagnostic {
            code,
            message: message.into(),
            span: self.tok().span,
        });
    }
    fn expr_node(kind: ExprKind, start: &Token, end: &Token) -> Expr {
        Expr {
            kind,
            span: Span {
                start: start.span.start,
                end: end.span.end,
                line: start.span.line,
                column: start.span.column,
            },
        }
    }
    fn expr(&mut self, min_prec: u8) -> Expr {
        let first = self.tok().clone();
        let mut left = if ["-", "~", "not"].contains(&self.tok().text.as_str()) {
            let op = self.take().text;
            let value = self.expr(7);
            Self::expr_node(
                ExprKind::Unary {
                    op,
                    value: Box::new(value.clone()),
                },
                &first,
                &self.tokens[self.at.saturating_sub(1)],
            )
        } else {
            self.primary_expr()
        };
        loop {
            if self.is(".") && min_prec <= 8 {
                self.take();
                if self.is("(") {
                    let args = self.expr_args();
                    let end = self.tokens[self.at.saturating_sub(1)].clone();
                    left = Self::expr_node(
                        ExprKind::PortMap {
                            callee: Box::new(left.clone()),
                            args,
                        },
                        &first,
                        &end,
                    );
                } else if self.is(".") {
                    // A chain of port-qualified values (a.b -> c.d) is retained
                    // as a port-chain node; `->` also participates below.
                    break;
                } else {
                    let name = self.take();
                    left = Self::expr_node(
                        ExprKind::Field {
                            base: Box::new(left.clone()),
                            name: name.text.clone(),
                        },
                        &first,
                        &name,
                    );
                }
                continue;
            }
            if self.is("[") && min_prec <= 8 {
                let indices = self.expr_args_close("[", "]");
                let end = self.tokens[self.at.saturating_sub(1)].clone();
                left = Self::expr_node(
                    ExprKind::Index {
                        base: Box::new(left.clone()),
                        indices,
                    },
                    &first,
                    &end,
                );
                continue;
            }
            if self.is("(") && min_prec <= 8 {
                let args = self.expr_args();
                let end = self.tokens[self.at.saturating_sub(1)].clone();
                left = Self::expr_node(
                    ExprKind::Call {
                        callee: Box::new(left.clone()),
                        args,
                    },
                    &first,
                    &end,
                );
                continue;
            }
            if self.is("run") && min_prec <= 1 {
                self.take();
                self.expect("(");
                let value_start = self.at;
                let mut depth = 0i32;
                while !self.eof() {
                    if depth == 0 && self.is("until") {
                        break;
                    }
                    match self.tok().text.as_str() {
                        "(" | "[" | "{" => depth += 1,
                        ")" | "]" | "}" => depth -= 1,
                        _ => {}
                    }
                    self.take();
                }
                let value_end = self.at;
                let mut value_tokens = self.tokens[value_start..value_end].to_vec();
                value_tokens.push(self.tokens.last().unwrap().clone());
                let mut value_parser = AstParser {
                    tokens: &value_tokens,
                    at: 0,
                    errors: Vec::new(),
                };
                let value = value_parser.expr(0);
                self.expect("until");
                self.expect("(");
                let condition = self.expr(0);
                self.expect(")");
                let end = self.expect(")");
                left = Self::expr_node(
                    ExprKind::RunUntil {
                        value: Box::new(value),
                        condition: Box::new(condition),
                    },
                    &first,
                    &end,
                );
                continue;
            }
            let op = self.tok().text.as_str();
            let prec = match op {
                "or" => 1,
                "and" => 2,
                "==" | "!=" | "=" | "<" | "<=" | ">" | ">=" => 3,
                "+" | "-" => 4,
                "*" | ".*" | "/" | "%" => 5,
                "->" => 0,
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            let op = self.take();
            let right = self.expr(prec + 1);
            let end = self.tokens[self.at.saturating_sub(1)].clone();
            let kind = if op.text == "->" {
                let links = match left.kind.clone() {
                    ExprKind::PortChain { links } => links,
                    _ => vec![left.clone()],
                };
                let mut links = links;
                links.push(right.clone());
                ExprKind::PortChain { links }
            } else {
                ExprKind::Binary {
                    op: op.text,
                    left: Box::new(left),
                    right: Box::new(right),
                }
            };
            left = Self::expr_node(kind, &first, &end);
        }
        left
    }
    fn primary_expr(&mut self) -> Expr {
        let first = self.tok().clone();
        if self.eof() || [")", "]", "}", ",", ";"].contains(&self.tok().text.as_str()) {
            self.error("UFP2001", "expected expression");
            return Self::expr_node(ExprKind::Identifier(String::new()), &first, &first);
        }
        if self.is(".") {
            let mut depth = 0;
            while self.is(".") {
                self.take();
                depth += 1;
            }
            if self.tok().kind == TokenKind::Ident || self.tok().kind == TokenKind::Keyword {
                let name = self.take();
                return Self::expr_node(
                    ExprKind::Scope {
                        depth,
                        name: name.text.clone(),
                        suffix: Vec::new(),
                    },
                    &first,
                    &name,
                );
            }
            return Self::expr_node(
                ExprKind::Current,
                &first,
                &self.tokens[self.at.saturating_sub(1)],
            );
        }
        if self.is("(") {
            self.take();
            let e = self.expr(0);
            self.expect(")");
            return e;
        }
        if self.is("[") {
            let values = self.expr_args_close("[", "]");
            return Self::expr_node(
                ExprKind::List(values),
                &first,
                &self.tokens[self.at.saturating_sub(1)],
            );
        }
        if self.is("structured") {
            self.take();
            self.expect("{");
            let mut fields = Vec::new();
            while !self.eof() && !self.is("}") {
                if self.is("structured")
                    && self.tokens.get(self.at + 1).is_some_and(|t| t.text == "{")
                {
                    let nested = self.primary_expr();
                    let name = if self.tok().kind == TokenKind::Ident {
                        self.take().text
                    } else {
                        String::new()
                    };
                    fields.push(ExprField {
                        ty: None,
                        name,
                        value: Some(nested),
                    });
                    if self.is(",") {
                        self.take();
                    } else {
                        break;
                    }
                    continue;
                }
                let builtin_typed = [
                    "bool",
                    "int",
                    "float",
                    "bfloat",
                    "quant",
                    "double",
                    "string",
                    "binary",
                    "list",
                    "sheet",
                    "matrix",
                    "structured",
                    "condition",
                    "auto",
                ]
                .contains(&self.tok().text.as_str());
                let custom_typed = self.tok().kind == TokenKind::Ident
                    && self.tokens[self.at..]
                        .iter()
                        .take_while(|t| t.text != "=" && t.text != "," && t.text != "}")
                        .skip(1)
                        .any(|t| t.kind == TokenKind::Ident);
                let typed = builtin_typed || custom_typed;
                let ty = if typed { Some(self.type_ast()) } else { None };
                let name = self.take().text;
                let value = if self.is("=") {
                    self.take();
                    Some(self.expr(0))
                } else {
                    None
                };
                fields.push(ExprField { ty, name, value });
                if self.is(",") {
                    self.take();
                } else {
                    break;
                }
            }
            let end = self.expect("}");
            return Self::expr_node(ExprKind::Structured(fields), &first, &end);
        }
        if self.is("condition") {
            self.take();
            self.expect("(");
            let value = self.expr(0);
            let end = self.expect(")");
            return Self::expr_node(ExprKind::Condition(Box::new(value)), &first, &end);
        }
        if self.is("matrix") {
            self.take();
            // Consume rank and dimensions as type syntax, then parse brace or
            // nested-row literal as matrix values.
            if self.is("<") {
                self.take();
                while !self.eof() && !self.is(">") {
                    self.take();
                }
                if self.is(">") {
                    self.take();
                }
            }
            if self.is("[") {
                self.expr_args_close("[", "]");
            }
            if self.is("of") {
                self.take();
                self.type_ast();
            }
            let values = if self.is("{") {
                self.expr_args_close("{", "}")
            } else if self.is("[") {
                self.expr_args_close("[", "]")
            } else {
                vec![]
            };
            return Self::expr_node(
                ExprKind::Matrix(values),
                &first,
                &self.tokens[self.at.saturating_sub(1)],
            );
        }
        let t = self.take();
        if matches!(
            t.kind,
            TokenKind::Int | TokenKind::Float | TokenKind::String | TokenKind::TextBlock
        ) || ["true", "false", "null"].contains(&t.text.as_str())
        {
            Self::expr_node(ExprKind::Literal(t.text.clone()), &first, &t)
        } else {
            Self::expr_node(ExprKind::Identifier(t.text.clone()), &first, &t)
        }
    }
    fn expr_args(&mut self) -> Vec<Expr> {
        self.expr_args_close("(", ")")
    }
    fn expr_args_close(&mut self, open: &str, close: &str) -> Vec<Expr> {
        self.expect(open);
        let mut values = Vec::new();
        while !self.eof() && !self.is(close) {
            let before = self.at;
            if self.tok().kind == TokenKind::Ident
                && self.tokens.get(self.at + 1).is_some_and(|t| t.text == "=")
            {
                let name = self.take();
                self.take();
                let value = self.expr(0);
                values.push(Self::expr_node(
                    ExprKind::NamedArg {
                        name: name.text.clone(),
                        value: Box::new(value.clone()),
                    },
                    &name,
                    &self.tokens[self.at.saturating_sub(1)],
                ));
            } else {
                values.push(self.expr(0));
            }
            if self.is(",") {
                self.take();
            } else if self.at == before {
                self.take();
            } else {
                break;
            }
        }
        self.expect(close);
        values
    }
    fn type_ast(&mut self) -> TypeAst {
        if self.eof() {
            self.error("UFP2001", "expected type");
            return TypeAst::Named(String::new());
        }
        let name = self.take().text;
        match name.as_str() {
            "int" | "float" | "bfloat" | "quant" => {
                let mut bits = None;
                let mut format = None;
                if self.is("<") {
                    self.take();
                    if !self.is(">") {
                        bits = Some(self.take().text);
                        if self.is(",") {
                            self.take();
                            format = Some(self.take().text);
                        }
                    }
                    self.expect(">");
                }
                TypeAst::Sized { name, bits, format }
            }
            "list" | "sheet" => {
                let element = if self.is("<") {
                    self.take();
                    let t = self.type_ast();
                    self.expect(">");
                    Some(Box::new(t))
                } else {
                    None
                };
                TypeAst::UnaryContainer { name, element }
            }
            "matrix" => {
                let rank = if self.is("<") {
                    self.take();
                    let r = self.take().text;
                    self.expect(">");
                    Some(r)
                } else {
                    None
                };
                self.expect("[");
                let mut dimensions = Vec::new();
                let mut dimension_exprs = Vec::new();
                while !self.eof() && !self.is("]") {
                    let start = self.at;
                    let dimension = self.expr(0);
                    let raw = self.tokens[start..self.at]
                        .iter()
                        .map(|token| token.text.as_str())
                        .collect::<String>();
                    dimensions.push(raw);
                    dimension_exprs.push(dimension);
                    if self.is(",") {
                        self.take();
                    } else {
                        break;
                    }
                }
                self.expect("]");
                let element = if self.is("of") {
                    self.take();
                    Some(Box::new(self.type_ast()))
                } else {
                    None
                };
                TypeAst::Matrix {
                    rank,
                    dimensions,
                    dimension_exprs,
                    element,
                }
            }
            "structured" => {
                self.expect("{");
                let mut fields = Vec::new();
                while !self.eof() && !self.is("}") {
                    let ty = self.type_ast();
                    let n = self.take().text;
                    fields.push((Box::new(ty), n));
                    if self.is(",") {
                        self.take();
                    } else {
                        break;
                    }
                }
                self.expect("}");
                TypeAst::Structured(fields)
            }
            "condition" => {
                if self.is("(") {
                    self.expr_args();
                }
                TypeAst::Condition
            }
            _ => {
                let mut qualified = name;
                while self.is(".") {
                    self.take();
                    if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                        qualified.push('.');
                        qualified.push_str(&self.take().text);
                    } else {
                        self.error("UFP2001", "expected type path component");
                        break;
                    }
                }
                TypeAst::Named(qualified)
            }
        }
    }
    fn expect(&mut self, text: &str) -> Token {
        if self.is(text) {
            self.take()
        } else {
            self.error("UFP2001", &format!("expected `{text}`"));
            self.tok().clone()
        }
    }
    fn leaf(&mut self, kind: SyntaxKind) -> SyntaxNode {
        let t = self.take();
        SyntaxNode {
            kind,
            span: t.span,
            token: Some(t),
            children: vec![],
            expression: None,
            type_ast: None,
            annotation_ast: None,
            import_ast: None,
            constraints: vec![],
        }
    }
    fn node(
        kind: SyntaxKind,
        first: &Token,
        last: &Token,
        children: Vec<SyntaxNode>,
    ) -> SyntaxNode {
        SyntaxNode {
            kind,
            span: Span {
                start: first.span.start,
                end: last.span.end,
                line: first.span.line,
                column: first.span.column,
            },
            token: None,
            children,
            expression: None,
            type_ast: None,
            annotation_ast: None,
            import_ast: None,
            constraints: vec![],
        }
    }
    fn declaration(&mut self) -> Option<SyntaxNode> {
        let start = self.tok().clone();
        let mut annotations = Vec::new();
        while self.is("@") {
            annotations.push(self.annotation());
        }
        let kind = match self.tok().text.as_str() {
            "import" => SyntaxKind::Import,
            "portal" => SyntaxKind::Portal,
            "text" => SyntaxKind::Text,
            "input" => SyntaxKind::Input,
            "output" => SyntaxKind::Output,
            "matrix" => SyntaxKind::Matrix,
            "binary" => SyntaxKind::BinaryDecl,
            "procedure" => SyntaxKind::Procedure,
            "node" => SyntaxKind::Node,
            "pipe" => SyntaxKind::Pipe,
            _ => {
                self.error("UFP2002", "expected a top-level declaration");
                self.recover();
                return None;
            }
        };
        let mut children = annotations;
        let mut declaration_expr = None;
        let mut declaration_type = None;
        let mut import_ast = None;
        if kind == SyntaxKind::Procedure {
            children.extend(self.procedure());
        } else if kind == SyntaxKind::Node {
            self.take();
            children.extend(self.node_decl());
        } else if kind == SyntaxKind::Pipe {
            self.take();
            children.extend(self.pipe_decl());
        } else if kind == SyntaxKind::Matrix {
            declaration_type = Some(self.type_ast());
            if self.tok().kind == TokenKind::Ident {
                children.push(self.leaf(SyntaxKind::Identifier));
            }
            self.expect("=");
            let value = self.expr(0);
            declaration_expr = Some(match value.kind {
                ExprKind::List(rows) => Self::expr_node(
                    ExprKind::Matrix(rows),
                    &start,
                    &self.tokens[self.at.saturating_sub(1)],
                ),
                _ => value,
            });
            self.expect(";");
        } else if kind == SyntaxKind::BinaryDecl {
            self.take();
            if self.tok().kind == TokenKind::Ident {
                children.push(self.leaf(SyntaxKind::Identifier));
            }
            self.expect("=");
            declaration_expr = Some(self.expr(0));
            self.expect(";");
        } else if matches!(
            kind,
            SyntaxKind::Input | SyntaxKind::Text | SyntaxKind::Portal
        ) {
            self.take();
            if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                children.push(self.leaf(SyntaxKind::Identifier));
            } else {
                self.error("UFP2001", "expected declaration name");
            }
            self.expect("=");
            declaration_expr = Some(self.expr(0));
            if kind == SyntaxKind::Portal && self.is("as") {
                children.push(self.leaf(SyntaxKind::Operator));
                children.push(self.leaf(SyntaxKind::Identifier));
            }
            self.expect(";");
        } else if kind == SyntaxKind::Output {
            self.take();
            if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                children.push(self.leaf(SyntaxKind::Identifier));
            } else {
                self.error("UFP2001", "expected output name");
            }
            self.expect(";");
        } else if kind == SyntaxKind::Import {
            self.take();
            let mut path = Vec::new();
            if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                path.push(self.take().text);
                while self.is(".") {
                    self.take();
                    if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                        path.push(self.take().text);
                    } else {
                        self.error("UFP2001", "expected path component after `.`");
                        break;
                    }
                }
            } else {
                self.error("UFP2001", "expected import path");
            }
            let from = if self.is("from") {
                self.take();
                if self.tok().kind == TokenKind::String {
                    Some(self.take().text)
                } else {
                    self.error("UFP2001", "expected import source string");
                    None
                }
            } else {
                None
            };
            let alias = if self.is("as") {
                self.take();
                if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                    Some(self.take().text)
                } else {
                    self.error("UFP2001", "expected import alias");
                    None
                }
            } else {
                None
            };
            self.expect(";");
            import_ast = Some(ImportAst { path, from, alias });
        } else {
            children.extend(self.until_semicolon());
        }
        let last = self.tokens[self.at.saturating_sub(1)].clone();
        let mut result = Self::node(kind, &start, &last, children);
        result.expression = declaration_expr;
        result.type_ast = declaration_type;
        result.import_ast = import_ast;
        Some(result)
    }
    fn annotation(&mut self) -> SyntaxNode {
        let first_at = self.at;
        let first = self.take();
        let mut children = vec![];
        if !self.eof() {
            children.push(self.leaf(SyntaxKind::Identifier));
        }
        if self.is("(") {
            children.extend(self.group());
        }
        let last = self.tokens[self.at.saturating_sub(1)].clone();
        let mut result = Self::node(SyntaxKind::Annotation, &first, &last, children);
        let mut ts = self.tokens[first_at..self.at].to_vec();
        ts.push(self.tokens.last().unwrap().clone());
        let mut p = AstParser {
            tokens: &ts,
            at: 0,
            errors: Vec::new(),
        };
        let start = p.take();
        let name = p.take().text;
        let mut args = Vec::new();
        if p.is("(") {
            p.take();
            while !p.eof() && !p.is(")") {
                if p.tok().kind == TokenKind::Int
                    && p.tokens.get(p.at + 1).is_some_and(|t| t.text == ".")
                {
                    let mut path = vec![p.take().text];
                    while p.is(".") {
                        p.take();
                        if p.tok().kind == TokenKind::Int {
                            path.push(p.take().text);
                        } else {
                            p.error("UFP2001", "expected integer in zone path");
                            break;
                        }
                    }
                    args.push(AnnotationArg::Zone(path));
                } else {
                    args.push(AnnotationArg::Expression(p.expr(0)));
                }
                if p.is(",") {
                    p.take();
                } else {
                    break;
                }
            }
            p.expect(")");
        }
        if !p.errors.is_empty() {
            self.errors.extend(p.errors);
        }
        result.annotation_ast = Some(AnnotationAst {
            name,
            args,
            span: Span {
                start: start.span.start,
                end: last.span.end,
                line: start.span.line,
                column: start.span.column,
            },
        });
        result
    }
    fn until_semicolon(&mut self) -> Vec<SyntaxNode> {
        let mut v = Vec::new();
        let mut depth = 0i32;
        while !self.eof() {
            if depth == 0 && self.is(";") {
                v.push(self.leaf(SyntaxKind::Operator));
                break;
            }
            if self.is("(") || self.is("[") || self.is("{") {
                v.extend(self.group());
                continue;
            }
            let t = self.tok().text.clone();
            if ["(", "[", "{"].contains(&t.as_str()) {
                depth += 1;
            }
            if [")", "]", "}"].contains(&t.as_str()) {
                depth -= 1;
            }
            v.push(self.leaf(if self.tok().kind == TokenKind::Ident {
                SyntaxKind::Identifier
            } else {
                SyntaxKind::Expression
            }));
        }
        if self.eof()
            && !v
                .last()
                .is_some_and(|x| x.token.as_ref().is_some_and(|t| t.text == ";"))
        {
            self.error("UFP2001", "expected `;`");
        }
        v
    }
    fn group(&mut self) -> Vec<SyntaxNode> {
        let open = self.take();
        let close = match open.text.as_str() {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            _ => ")",
        };
        let mut children = Vec::new();
        while !self.eof() && !self.is(close) {
            if self.is("(") || self.is("[") || self.is("{") {
                children.extend(self.group());
            } else {
                children.push(self.leaf(if self.tok().kind == TokenKind::Ident {
                    SyntaxKind::Identifier
                } else {
                    SyntaxKind::Expression
                }));
            }
        }
        if self.is(close) {
            self.take();
        } else {
            self.error("UFP2001", &format!("expected `{close}`"));
        }
        children
    }
    fn procedure(&mut self) -> Vec<SyntaxNode> {
        self.take(); // procedure keyword
        let mut out = vec![self.leaf(SyntaxKind::Identifier)];
        if self.is("(") {
            self.take();
            while !self.eof() && !self.is(")") {
                let first = self.tok().clone();
                let mut pin = Vec::new();
                if ["input", "output", "inout"].contains(&self.tok().text.as_str()) {
                    pin.push(self.leaf(SyntaxKind::Operator));
                }
                let ty = self.type_ast();
                let mut pin_node = Self::node(
                    SyntaxKind::Pin,
                    &first,
                    &self.tokens[self.at.saturating_sub(1)],
                    std::mem::take(&mut pin),
                );
                pin_node.type_ast = Some(ty);
                if self.tok().kind == TokenKind::Ident {
                    pin_node.children.push(self.leaf(SyntaxKind::Identifier));
                }
                if self.is("with") {
                    self.take();
                    loop {
                        let before = self.at;
                        pin_node.constraints.push(self.expr(0));
                        if self.at == before || !self.is("and") {
                            break;
                        }
                    }
                }
                while !self.eof() && !self.is(",") && !self.is(")") {
                    self.error("UFP2002", "unexpected token in pin declaration");
                    self.recover_pin();
                }
                let last = self.tokens[self.at.saturating_sub(1)].clone();
                pin_node.span.end = last.span.end;
                out.push(pin_node);
                if self.is(",") {
                    self.take();
                } else {
                    break;
                }
            }
            self.expect(")");
        } else {
            self.error("UFP2001", "expected `(` after procedure name");
        }
        if self.is("graph") {
            out.push(self.leaf(SyntaxKind::Graph));
        }
        if self.is("{") {
            out.push(self.block());
        } else {
            self.error("UFP2001", "expected procedure body");
            self.recover();
        }
        out
    }
    fn block(&mut self) -> SyntaxNode {
        let first = self.expect("{");
        let mut children = Vec::new();
        while !self.eof() && !self.is("}") {
            let before = self.at;
            let kind = match self.tok().text.as_str() {
                "let" => SyntaxKind::Let,
                "if" => SyntaxKind::If,
                "return" => SyntaxKind::Return,
                "pipe" => SyntaxKind::Pipe,
                "node" => SyntaxKind::Node,
                _ => SyntaxKind::ExpressionStatement,
            };
            if kind == SyntaxKind::If {
                let f = self.take();
                let mut c = Vec::new();
                if self.is("(") {
                    let (e, span) = self.parenthesized_expr();
                    let mut n = Self::node(
                        SyntaxKind::Expression,
                        &f,
                        &self.tokens[self.at.saturating_sub(1)],
                        vec![],
                    );
                    n.span = span;
                    n.expression = Some(e);
                    c.push(n);
                } else {
                    self.error("UFP2001", "expected condition");
                }
                if self.is("{") {
                    c.push(self.block());
                }
                while self.is("elif") {
                    c.push(self.leaf(SyntaxKind::Operator));
                    if self.is("(") {
                        let (e, span) = self.parenthesized_expr();
                        let mut n = Self::node(
                            SyntaxKind::Expression,
                            &f,
                            &self.tokens[self.at.saturating_sub(1)],
                            vec![],
                        );
                        n.span = span;
                        n.expression = Some(e);
                        c.push(n);
                    }
                    if self.is("{") {
                        c.push(self.block());
                    }
                }
                if self.is("else") {
                    c.push(self.leaf(SyntaxKind::Operator));
                    if self.is("{") {
                        c.push(self.block());
                    }
                }
                let l = self.tokens[self.at.saturating_sub(1)].clone();
                children.push(Self::node(kind, &f, &l, c));
            } else if kind == SyntaxKind::Pipe {
                let f = self.tok().clone();
                self.take();
                let c = self.pipe_decl();
                let l = self.tokens[self.at.saturating_sub(1)].clone();
                children.push(Self::node(kind, &f, &l, c));
            } else if kind == SyntaxKind::Node {
                let f = self.tok().clone();
                self.take();
                let c = self.node_decl();
                let l = self.tokens[self.at.saturating_sub(1)].clone();
                children.push(Self::node(kind, &f, &l, c));
            } else {
                children.push(self.simple_statement(kind));
            }
            if self.at == before {
                self.error("UFP2002", "unable to parse statement");
                self.take();
            }
        }
        let last = if self.is("}") {
            self.take()
        } else {
            self.error("UFP2001", "expected `}`");
            self.tok().clone()
        };
        Self::node(SyntaxKind::Block, &first, &last, children)
    }
    fn parenthesized_expr(&mut self) -> (Expr, Span) {
        let open = self.expect("(");
        let expr = self.expr(0);
        let close = self.expect(")");
        (
            expr,
            Span {
                start: open.span.start,
                end: close.span.end,
                line: open.span.line,
                column: open.span.column,
            },
        )
    }
    fn simple_statement(&mut self, kind: SyntaxKind) -> SyntaxNode {
        let first = self.tok().clone();
        let mut children = Vec::new();
        let mut ty = None;
        let expr: Option<Expr>;
        if kind == SyntaxKind::Let {
            self.take();
            if self.tok().kind == TokenKind::Ident {
                children.push(self.leaf(SyntaxKind::Identifier));
            }
            if self.is(":") {
                self.take();
                ty = Some(self.type_ast());
            }
            self.expect("=");
            expr = Some(self.expr(0));
        } else if kind == SyntaxKind::Return {
            self.take();
            expr = if self.is(";") {
                None
            } else {
                Some(self.expr(0))
            };
            let semi = self.expect(";");
            let mut node = Self::node(kind, &first, &semi, children);
            node.expression = expr;
            return node;
        } else {
            let lhs = self.expr(0);
            if self.is("=") {
                self.take();
                expr = Some(self.expr(0));
                children.push(SyntaxNode {
                    kind: SyntaxKind::Expression,
                    span: lhs.span,
                    token: None,
                    children: vec![],
                    expression: Some(lhs),
                    type_ast: None,
                    annotation_ast: None,
                    import_ast: None,
                    constraints: vec![],
                });
            } else {
                expr = Some(lhs);
            }
        }
        let semi = self.expect(";");
        let mut node = Self::node(kind, &first, &semi, children);
        node.expression = expr;
        node.type_ast = ty;
        node
    }
    fn node_decl(&mut self) -> Vec<SyntaxNode> {
        let mut children = Vec::new();
        if self.tok().kind == TokenKind::Ident {
            children.push(self.leaf(SyntaxKind::Identifier));
        }
        self.expect(":");
        while !self.eof() && !self.is("(") && !self.is(";") {
            children.push(self.leaf(SyntaxKind::Identifier));
            if self.is(".") {
                children.push(self.leaf(SyntaxKind::Operator));
            }
        }
        if self.is("(") {
            self.take();
            while !self.eof() && !self.is(")") {
                let first = self.tok().clone();
                if matches!(self.tok().kind, TokenKind::Ident | TokenKind::Keyword) {
                    children.push(self.leaf(SyntaxKind::Identifier));
                }
                self.expect("=");
                let value = self.expr(0);
                let last = self.tokens[self.at.saturating_sub(1)].clone();
                let mut binding = Self::node(SyntaxKind::Expression, &first, &last, vec![]);
                binding.expression = Some(value);
                children.push(binding);
                if self.is(",") {
                    self.take();
                } else {
                    break;
                }
            }
            self.expect(")");
        }
        self.expect(";");
        children
    }
    fn pipe_decl(&mut self) -> Vec<SyntaxNode> {
        let start = self.at;
        if self.tok().kind == TokenKind::Ident
            && self.tokens.get(self.at + 1).is_some_and(|t| t.text == ":")
        {
            self.take();
            self.take();
        }
        let value = self.expr(0);
        if !matches!(value.kind, ExprKind::PortChain { .. }) {
            self.error("UFP2001", "expected port chain");
        }
        let last = self.expect(";");
        let first = self.tokens[start].clone();
        let mut node = Self::node(SyntaxKind::Expression, &first, &last, vec![]);
        node.expression = Some(value);
        vec![node]
    }
    fn recover_pin(&mut self) {
        while !self.eof() && !self.is(",") && !self.is(")") {
            self.take();
        }
    }
    fn recover(&mut self) {
        let mut depth = 0usize;
        while !self.eof() {
            if depth == 0 && (self.is(";") || self.is("}")) {
                break;
            }
            match self.tok().text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.take();
        }
        if self.is(";") {
            self.take();
        }
    }
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
        assert_eq!(
            errors.iter().map(|e| e.code).collect::<Vec<_>>(),
            ["UFP1003", "UFP1004", "UFP1004", "UFP1001"]
        );
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

    #[test]
    fn parses_documented_example_corpus_into_ast() {
        let examples = [
            include_str!("../../../examples/generate.ufp"),
            include_str!("../../../examples/gguf.ufp"),
            include_str!("../../../examples/hello.ufp"),
            include_str!("../../../examples/model_layers.ufp"),
            include_str!("../../../examples/portal_demo.ufp"),
            include_str!("../../../examples/predict.ufp"),
            include_str!("../../../examples/quantized.ufp"),
            include_str!("../../../examples/structured.ufp"),
            include_str!("../../../examples/text_io.ufp"),
            include_str!("../../../examples/layers/layer_000.ufp"),
        ];
        for (i, source) in examples.iter().enumerate() {
            parse_ast(source).unwrap_or_else(|errors| panic!("example {i}: {errors:#?}"));
        }
    }

    #[test]
    fn ast_retains_annotations_procedures_pins_statements_and_nodes() {
        let ast = parse_ast("@T(0.1.2) procedure f(input matrix<2>[2,2] x, output float y) { let z = x[0,0]; if (z > 0) { y = z; } return y; } node n : pkg.op(x = f.(x)); pipe n.out -> n.in;").unwrap();
        assert!(ast
            .declarations
            .iter()
            .any(|n| n.kind == SyntaxKind::Procedure));
        assert!(ast.declarations.iter().any(|n| n.kind == SyntaxKind::Node));
        assert!(ast.declarations.iter().any(|n| n.kind == SyntaxKind::Pipe));
        let proc = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Procedure)
            .unwrap();
        assert!(proc
            .children
            .iter()
            .any(|n| n.kind == SyntaxKind::Annotation));
        assert!(proc.children.iter().any(|n| n.kind == SyntaxKind::Pin));
        assert!(proc.children.iter().any(|n| n.kind == SyntaxKind::Block));
        assert!(proc
            .children
            .iter()
            .find(|n| n.kind == SyntaxKind::Pin)
            .unwrap()
            .type_ast
            .is_some());
        assert!(proc.children.iter().any(|n| n.annotation_ast.is_some()));
        let block = proc
            .children
            .iter()
            .find(|n| n.kind == SyntaxKind::Block)
            .unwrap();
        assert!(block.children.iter().any(|n| n.expression.is_some()));
        let node = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Node)
            .unwrap();
        assert!(node.children.iter().any(|n| n.expression.is_some()));
        let pipe = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Pipe)
            .unwrap();
        assert!(pipe.children.iter().any(|n| matches!(
            n.expression.as_ref().map(|e| &e.kind),
            Some(ExprKind::PortChain { .. })
        )));
    }

    #[test]
    fn declaration_and_statement_nodes_carry_recursive_semantics() {
        let ast = parse_ast("matrix<2>[2,2] eye = [[1,0],[0,1]]; binary weights = bytes[0,255]; procedure f(){ let x: list<float> = [1.0, 2.0]; return x; }").unwrap();
        let matrix = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Matrix)
            .unwrap();
        assert!(matches!(matrix.type_ast, Some(TypeAst::Matrix { .. })));
        let ExprKind::Matrix(rows) = &matrix.expression.as_ref().unwrap().kind else {
            panic!("expected matrix AST")
        };
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[0].kind, ExprKind::List(_)));
        let binary = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::BinaryDecl)
            .unwrap();
        assert!(binary.expression.is_some());
        let procedure = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Procedure)
            .unwrap();
        let block = procedure
            .children
            .iter()
            .find(|n| n.kind == SyntaxKind::Block)
            .unwrap();
        let let_stmt = block
            .children
            .iter()
            .find(|n| n.kind == SyntaxKind::Let)
            .unwrap();
        assert!(matches!(
            let_stmt.type_ast,
            Some(TypeAst::UnaryContainer { .. })
        ));
        assert!(let_stmt.expression.is_some());
    }

    #[test]
    fn malformed_semantic_ast_inputs_report_recoverable_diagnostics() {
        assert!(!parse_expression("a +").unwrap_err().is_empty());
        assert!(!parse_type("matrix<2>[2,").unwrap_err().is_empty());
        assert!(!parse_annotation("@T(0.x)").unwrap_err().is_empty());
        let errors =
            parse_ast("procedure bad(input matrix<2>[2,] x) { let x = ; } output ok;").unwrap_err();
        assert!(!errors.is_empty());
        assert!(errors.iter().all(|d| d.span.start <= d.span.end));
    }

    #[test]
    fn imports_outputs_constraints_and_expression_dimensions_are_structured() {
        let ast = parse_ast("import pkg.sub from \"lib.ufp\" as local; output result; procedure f(input matrix<2>[n+1, width*2] x with x.shape = y.shape and x.rank = 2) { return x; }").unwrap();
        let import = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Import)
            .unwrap();
        let import = import.import_ast.as_ref().unwrap();
        assert_eq!(import.path, ["pkg", "sub"]);
        assert_eq!(import.from.as_deref(), Some("lib.ufp"));
        assert_eq!(import.alias.as_deref(), Some("local"));
        let output = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Output)
            .unwrap();
        assert!(output
            .children
            .iter()
            .any(|n| n.token.as_ref().is_some_and(|t| t.text == "result")));
        let procedure = ast
            .declarations
            .iter()
            .find(|n| n.kind == SyntaxKind::Procedure)
            .unwrap();
        let pin = procedure
            .children
            .iter()
            .find(|n| n.kind == SyntaxKind::Pin)
            .unwrap();
        assert_eq!(pin.constraints.len(), 1);
        let TypeAst::Matrix {
            dimension_exprs, ..
        } = pin.type_ast.as_ref().unwrap()
        else {
            panic!("expected matrix type")
        };
        assert_eq!(dimension_exprs.len(), 2);
    }

    #[test]
    fn nested_structured_custom_fields_and_error_recovery_are_stable() {
        let e = parse_expression("structured { My.Record item = null, scalar = 1 }").unwrap();
        let ExprKind::Structured(fields) = e.kind else {
            panic!("expected structured literal")
        };
        assert!(matches!(fields[0].ty, Some(TypeAst::Named(ref n)) if n == "My.Record"));
        let bad = "procedure broken(input matrix<2>[3,] x) { let a = structured { int n = ; } } output recovered;";
        let first = parse_ast(bad).unwrap_err();
        let second = parse_ast(bad).unwrap_err();
        assert_eq!(first, second);
        assert!(!first.is_empty());
        assert!(first.iter().all(|d| d.span.start <= d.span.end));
        let (recovered, diagnostics) = parse_ast_recovering(bad);
        assert_eq!(diagnostics, first);
        assert!(recovered
            .declarations
            .iter()
            .any(|n| n.kind == SyntaxKind::Output));
        assert!(parse_ast("output recovered;").is_ok());
    }

    #[test]
    fn expression_parser_builds_precedence_and_postfix_trees() {
        let e = parse_expression("a + b * -c == d and ready").unwrap();
        let ExprKind::Binary { op, left, .. } = e.kind else {
            panic!("expected binary tree")
        };
        assert_eq!(op, "and");
        let ExprKind::Binary { op, .. } = left.kind else {
            panic!("expected comparison")
        };
        assert_eq!(op, "==");

        let e = parse_expression("f.(x).out[0](y)").unwrap();
        let ExprKind::Call { callee, .. } = e.kind else {
            panic!("expected call")
        };
        assert!(matches!(callee.kind, ExprKind::Index { .. }));

        let e = parse_expression("seed run(token_tools.(tokens, .) until(condition(. == limit)))")
            .unwrap();
        assert!(matches!(e.kind, ExprKind::RunUntil { .. }));
        let scoped = parse_expression("..parent.child").unwrap();
        let ExprKind::Field { base, .. } = scoped.kind else {
            panic!("expected scoped field")
        };
        assert!(matches!(base.kind, ExprKind::Scope { depth: 2, .. }));
        assert!(matches!(
            parse_expression("a.out -> b.in").unwrap().kind,
            ExprKind::PortChain { .. }
        ));
        assert!(matches!(
            parse_expression("[1, 2, 3]").unwrap().kind,
            ExprKind::List(_)
        ));
        assert!(matches!(
            parse_expression("structured { x = 1, y = true }")
                .unwrap()
                .kind,
            ExprKind::Structured(_)
        ));
    }

    #[test]
    fn parses_type_shapes_and_annotation_zones() {
        assert!(
            matches!(parse_type("matrix<2>[128,256] of quant<q4_0>").unwrap(), TypeAst::Matrix { rank: Some(_), dimensions, element: Some(_), .. } if dimensions.len() == 2)
        );
        assert!(
            matches!(parse_type("structured { int count, list<string> names }").unwrap(), TypeAst::Structured(fields) if fields.len() == 2)
        );
        assert!(matches!(
            parse_type("float<16, ieee>").unwrap(),
            TypeAst::Sized {
                bits: Some(_),
                format: Some(_),
                ..
            }
        ));
        let annotation = parse_annotation("@T(0.1.2)").unwrap();
        assert!(
            matches!(annotation.args.as_slice(), [AnnotationArg::Zone(path)] if path == &["0", "1", "2"])
        );
    }
}
