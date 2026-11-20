//! Closed UFP type model and first static checks.
//!
//! The parser will attach these to every AST expression in UF-201/UF-203. The
//! terminal checker is already used by the executable text-plan slice.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use ufp_syntax::{AstPlan, Expr, ExprKind, Item, Plan, Span, SyntaxKind, SyntaxNode, TypeAst};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Bool,
    Int(Option<u8>),
    Float {
        bits: u8,
        variant: Option<Float8>,
    },
    BFloat16,
    String,
    Binary,
    List(Option<Box<Type>>),
    Sheet(Option<Box<Type>>),
    Matrix {
        rank: Option<usize>,
        dims: Vec<usize>,
        elem: Box<Type>,
    },
    Structured(Vec<Field>),
    Condition,
    Auto,
    Quant(String),
    Named(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Float8 {
    E4M3,
    E5M2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}
impl Type {
    pub fn matrix(dims: Vec<usize>, elem: Type) -> Self {
        Self::Matrix {
            rank: Some(dims.len()),
            dims,
            elem: Box::new(elem),
        }
    }
    pub fn valid(&self) -> Result<(), String> {
        match self {
            Self::Int(Some(n)) if ![8, 16, 32, 64].contains(n) => {
                Err(format!("int<{n}> is not supported"))
            }
            Self::Float { bits, variant } if ![8, 16, 32, 64].contains(bits) => {
                Err(format!("float<{bits}> is not supported"))
            }
            Self::Float {
                bits: 8,
                variant: None,
            } => Err("float<8> requires e4m3 or e5m2".into()),
            Self::Float {
                bits,
                variant: Some(_),
            } if *bits != 8 => Err("float variants only apply to float<8>".into()),
            Self::Matrix {
                rank: Some(r),
                dims,
                ..
            } if *r != dims.len() => Err(format!(
                "matrix rank {r} does not match {} dimensions",
                dims.len()
            )),
            Self::Matrix { dims, elem, .. } if dims.contains(&0) => {
                Err("matrix dimensions must be positive".into())
            }
            Self::Matrix { elem, .. } => elem.valid(),
            _ => Ok(()),
        }
    }
}
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => write!(f, "bool"),
            Self::Int(None) => write!(f, "int"),
            Self::Int(Some(n)) => write!(f, "int<{n}>"),
            Self::Float {
                bits: 32,
                variant: None,
            } => write!(f, "float"),
            Self::Float {
                bits,
                variant: None,
            } => write!(f, "float<{bits}>"),
            Self::Float {
                bits,
                variant: Some(v),
            } => write!(
                f,
                "float<{bits}, {}>",
                match v {
                    Float8::E4M3 => "e4m3",
                    Float8::E5M2 => "e5m2",
                }
            ),
            Self::BFloat16 => write!(f, "bfloat<16>"),
            Self::String => write!(f, "string"),
            Self::Binary => write!(f, "binary"),
            Self::List(None) => write!(f, "list"),
            Self::List(Some(t)) => write!(f, "list<{t}>"),
            Self::Sheet(_) => write!(f, "sheet"),
            Self::Matrix { rank, dims, elem } => {
                write!(f, "matrix")?;
                if let Some(r) = rank {
                    write!(f, "<{r}>")?
                };
                write!(
                    f,
                    "[{}] of {elem}",
                    dims.iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
            Self::Structured(_) => write!(f, "structured"),
            Self::Condition => write!(f, "condition"),
            Self::Auto => write!(f, "auto"),
            Self::Quant(q) => write!(f, "quant<{q}>"),
            Self::Named(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

/// Convert parser type syntax into the closed checker type model.
pub fn type_from_ast(ast: &TypeAst) -> Result<Type, String> {
    let ty = match ast {
        TypeAst::Named(name) => match name.as_str() {
            "bool" => Type::Bool,
            "double" => Type::Float {
                bits: 64,
                variant: None,
            },
            "string" => Type::String,
            "binary" => Type::Binary,
            "auto" => Type::Auto,
            name => Type::Named(name.to_owned()),
        },
        TypeAst::Condition => Type::Condition,
        TypeAst::Sized { name, bits, format } => {
            let bits = bits
                .as_deref()
                .map(|s| {
                    s.parse::<u8>()
                        .map_err(|_| format!("invalid {name} width `{s}`"))
                })
                .transpose()?;
            match name.as_str() {
                "int" => Type::Int(bits),
                "float" => {
                    let bits = bits.unwrap_or(32);
                    let variant = match format.as_deref() {
                        None => None,
                        Some("e4m3") => Some(Float8::E4M3),
                        Some("e5m2") => Some(Float8::E5M2),
                        Some(other) => return Err(format!("unsupported float format `{other}`")),
                    };
                    Type::Float { bits, variant }
                }
                "bfloat" => Type::BFloat16,
                "quant" => Type::Quant(
                    format
                        .clone()
                        .unwrap_or_else(|| bits.map(|b| b.to_string()).unwrap_or_default()),
                ),
                other => return Err(format!("unknown sized type `{other}`")),
            }
        }
        TypeAst::UnaryContainer { name, element } => {
            let elem = element
                .as_deref()
                .map(type_from_ast)
                .transpose()?
                .map(Box::new);
            match name.as_str() {
                "list" => Type::List(elem),
                "sheet" => Type::Sheet(elem),
                _ => return Err(format!("unknown container `{name}`")),
            }
        }
        TypeAst::Matrix {
            rank,
            dimensions,
            element,
            ..
        } => {
            let rank = rank
                .as_deref()
                .map(|s| {
                    s.parse::<usize>()
                        .map_err(|_| format!("invalid matrix rank `{s}`"))
                })
                .transpose()?;
            let dims = dimensions
                .iter()
                .map(|s| {
                    s.parse::<usize>()
                        .map_err(|_| format!("matrix dimension `{s}` is not a constant integer"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Type::Matrix {
                rank,
                dims,
                elem: Box::new(
                    element
                        .as_deref()
                        .map(type_from_ast)
                        .transpose()?
                        .unwrap_or(Type::Auto),
                ),
            }
        }
        TypeAst::Structured(fields) => Type::Structured(
            fields
                .iter()
                .map(|(ty, name)| {
                    Ok(Field {
                        name: name.clone(),
                        ty: type_from_ast(ty)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        ),
    };
    ty.valid()?;
    Ok(ty)
}

/// Deterministic static checking over the structured parser AST.
pub fn check_ast(plan: &AstPlan) -> Vec<TypeDiagnostic> {
    let mut checker = Checker {
        diagnostics: Vec::new(),
        globals: BTreeMap::new(),
    };
    // Predeclare top-level names to permit forward references.
    for node in &plan.declarations {
        if let Some(name) = declaration_name(node) {
            checker.globals.insert(
                name,
                node.type_ast
                    .as_ref()
                    .and_then(|t| type_from_ast(t).ok())
                    .unwrap_or(Type::Auto),
            );
        }
    }
    for node in &plan.declarations {
        checker.check_node(node, &mut checker.globals.clone());
    }
    checker.diagnostics
}

struct Checker {
    diagnostics: Vec<TypeDiagnostic>,
    globals: BTreeMap<String, Type>,
}
impl Checker {
    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(TypeDiagnostic {
            code,
            message: message.into(),
            span,
        });
    }
    fn check_node(&mut self, node: &SyntaxNode, env: &mut BTreeMap<String, Type>) {
        if let Some(ast) = &node.type_ast {
            if let Err(e) = type_from_ast(ast) {
                self.error("UFP3201", e, node.span);
            }
        }
        if node.kind == SyntaxKind::Procedure {
            let mut local = self.globals.clone();
            for pin in node.children.iter().filter(|n| n.kind == SyntaxKind::Pin) {
                let name = pin
                    .children
                    .iter()
                    .find(|c| c.kind == SyntaxKind::Identifier)
                    .and_then(|c| c.token.as_ref())
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                let ty = pin
                    .type_ast
                    .as_ref()
                    .and_then(|t| type_from_ast(t).ok())
                    .unwrap_or(Type::Auto);
                if !name.is_empty() {
                    local.insert(name, ty.clone());
                }
                for constraint in &pin.constraints {
                    let got = self.infer(constraint, &local);
                    if !matches!(got, Type::Bool | Type::Condition | Type::Auto) {
                        self.error("UFP3302", "pin constraint must be boolean", constraint.span);
                    }
                }
            }
            for child in &node.children {
                self.check_node(child, &mut local);
            }
            return;
        }
        if let Some(expr) = &node.expression {
            let actual = self.infer(expr, env);
            if let Some(expected) = node.type_ast.as_ref().and_then(|t| type_from_ast(t).ok()) {
                if !compatible(&expected, &actual) {
                    self.error(
                        "UFP3301",
                        format!("expected `{expected}`, found `{actual}`"),
                        expr.span,
                    );
                }
            }
            if node.kind == SyntaxKind::Let {
                if let Some(name) = node
                    .children
                    .first()
                    .and_then(|c| c.token.as_ref())
                    .map(|t| t.text.clone())
                {
                    env.insert(name, actual);
                }
            }
        }
        for child in &node.children {
            self.check_node(child, env);
        }
    }
    fn infer(&mut self, e: &Expr, env: &BTreeMap<String, Type>) -> Type {
        match &e.kind {
            ExprKind::Literal(v) if v == "true" || v == "false" => Type::Bool,
            ExprKind::Literal(v) if v == "null" => Type::Auto,
            ExprKind::Literal(v) if v.starts_with('"') => Type::String,
            ExprKind::Literal(v) if v.contains('.') || v.contains('e') || v.contains('E') => {
                Type::Float {
                    bits: 32,
                    variant: None,
                }
            }
            ExprKind::Literal(_) => Type::Int(None),
            ExprKind::Identifier(n) => env.get(n).cloned().unwrap_or_else(|| {
                self.error("UFP3101", format!("unknown name `{n}`"), e.span);
                Type::Auto
            }),
            ExprKind::Scope { depth, name, .. } => if *depth == 0 {
                self.globals.get(name).cloned()
            } else {
                None
            }
            .unwrap_or_else(|| {
                self.error(
                    "UFP3101",
                    format!("unresolved scoped name `{name}`"),
                    e.span,
                );
                Type::Auto
            }),
            ExprKind::Current => Type::Auto,
            ExprKind::Unary { op, value } => {
                let t = self.infer(value, env);
                match op.as_str() {
                    "not" if matches!(t, Type::Bool | Type::Condition | Type::Auto) => Type::Bool,
                    "-" | "~" if is_numeric(&t) => t,
                    _ => {
                        self.error(
                            "UFP3303",
                            format!("operator `{op}` is not defined for `{t}`"),
                            e.span,
                        );
                        Type::Auto
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let a = self.infer(left, env);
                let b = self.infer(right, env);
                match op.as_str() {
                    "and" | "or" if boolish(&a) && boolish(&b) => Type::Bool,
                    "==" | "!=" | "=" | "<" | "<=" | ">" | ">=" if compatible(&a, &b) => Type::Bool,
                    "+" | "-" | "*" | ".*" | "/" | "%" if is_numeric(&a) && compatible(&a, &b) => a,
                    _ => {
                        self.error(
                            "UFP3303",
                            format!("operator `{op}` is not defined for `{a}` and `{b}`"),
                            e.span,
                        );
                        Type::Auto
                    }
                }
            }
            ExprKind::List(items) | ExprKind::Matrix(items) => {
                let mut ty = None;
                for item in items {
                    let next = self.infer(item, env);
                    if ty.as_ref().is_some_and(|t| !compatible(t, &next)) {
                        self.error(
                            "UFP3304",
                            "container elements have inconsistent types",
                            item.span,
                        );
                    } else if ty.is_none() {
                        ty = Some(next);
                    }
                }
                if matches!(e.kind, ExprKind::Matrix(_)) {
                    Type::Matrix {
                        rank: None,
                        dims: vec![],
                        elem: Box::new(ty.unwrap_or(Type::Auto)),
                    }
                } else {
                    Type::List(ty.map(Box::new))
                }
            }
            ExprKind::Structured(fields) => Type::Structured(
                fields
                    .iter()
                    .map(|f| {
                        let ty =
                            f.ty.as_ref()
                                .and_then(|t| type_from_ast(t).ok())
                                .or_else(|| f.value.as_ref().map(|v| self.infer(v, env)))
                                .unwrap_or(Type::Auto);
                        Field {
                            name: f.name.clone(),
                            ty,
                        }
                    })
                    .collect(),
            ),
            ExprKind::Field { base, name } => {
                let t = self.infer(base, env);
                if let Type::Structured(fs) = t {
                    fs.into_iter()
                        .find(|f| f.name == *name)
                        .map(|f| f.ty)
                        .unwrap_or_else(|| {
                            self.error("UFP3305", format!("unknown field `{name}`"), e.span);
                            Type::Auto
                        })
                } else {
                    self.error(
                        "UFP3305",
                        format!("cannot select field `{name}` from `{t}`"),
                        e.span,
                    );
                    Type::Auto
                }
            }
            ExprKind::Index { base, indices } => {
                let t = self.infer(base, env);
                for i in indices {
                    let it = self.infer(i, env);
                    if !matches!(it, Type::Int(_) | Type::Auto) {
                        self.error("UFP3306", "index must be an integer", i.span);
                    }
                }
                match t {
                    Type::List(Some(t)) => *t,
                    Type::List(None) => Type::Auto,
                    Type::Matrix { elem, .. } => *elem,
                    other => {
                        self.error("UFP3306", format!("cannot index `{other}`"), e.span);
                        Type::Auto
                    }
                }
            }
            ExprKind::Condition(v) => {
                self.infer(v, env);
                Type::Condition
            }
            ExprKind::NamedArg { value, .. } => self.infer(value, env),
            ExprKind::Call { callee, args } | ExprKind::PortMap { callee, args } => {
                self.infer(callee, env);
                for a in args {
                    self.infer(a, env);
                }
                Type::Auto
            }
            ExprKind::PortChain { links } => {
                for x in links {
                    self.infer(x, env);
                }
                Type::Auto
            }
            ExprKind::RunUntil { value, condition } => {
                let t = self.infer(value, env);
                let c = self.infer(condition, env);
                if !boolish(&c) {
                    self.error(
                        "UFP3302",
                        "run-until condition must be boolean",
                        condition.span,
                    );
                }
                t
            }
        }
    }
}
fn declaration_name(n: &SyntaxNode) -> Option<String> {
    n.children
        .iter()
        .find(|c| c.kind == SyntaxKind::Identifier)
        .and_then(|c| c.token.as_ref())
        .map(|t| t.text.clone())
}
fn is_numeric(t: &Type) -> bool {
    matches!(
        t,
        Type::Int(_) | Type::Float { .. } | Type::BFloat16 | Type::Quant(_) | Type::Auto
    )
}
fn boolish(t: &Type) -> bool {
    matches!(t, Type::Bool | Type::Condition | Type::Auto)
}
fn compatible(a: &Type, b: &Type) -> bool {
    a == b || matches!(a, Type::Auto) || matches!(b, Type::Auto) || (is_numeric(a) && is_numeric(b))
}

/// First checker pass: validates the D-012 terminal graph before runtime/codegen.
pub fn check_text_terminals(plan: &Plan) -> Vec<TypeDiagnostic> {
    let mut result = Vec::new();
    let mut declared = BTreeMap::<String, Span>::new();
    let mut outputs = BTreeSet::new();
    let mut incoming = BTreeMap::<String, usize>::new();
    for item in &plan.items {
        match item {
            Item::Input { name, span, .. } | Item::Output { name, span } => {
                if declared.insert(name.clone(), *span).is_some() {
                    result.push(TypeDiagnostic {
                        code: "UFP3403",
                        message: format!("duplicate terminal `{name}`"),
                        span: *span,
                    });
                }
            }
            _ => {}
        }
    }
    for item in &plan.items {
        if let Item::Output { name, .. } = item {
            outputs.insert(name.clone());
        }
    }
    for item in &plan.items {
        if let Item::Pipe { from, to, span } = item {
            if (declared.contains_key(&from.node) || declared.contains_key(&to.node))
                && (from.port != "text" || to.port != "text")
            {
                result.push(TypeDiagnostic {
                    code: "UFP3405",
                    message: "terminal pipes must use `.text`".into(),
                    span: *span,
                });
            }
            if outputs.contains(&to.node) {
                *incoming.entry(to.node.clone()).or_default() += 1;
            }
        }
    }
    for output in outputs {
        if incoming.get(&output).copied().unwrap_or(0) != 1 {
            let span = declared[&output];
            result.push(TypeDiagnostic {
                code: "UFP3404",
                message: format!("output `{output}` must have exactly one incoming `.text` pipe"),
                span,
            });
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use ufp_syntax::parse_plan;
    #[test]
    fn matrix_validates_rank() {
        assert!(Type::matrix(
            vec![3, 4],
            Type::Float {
                bits: 32,
                variant: None
            }
        )
        .valid()
        .is_ok());
    }
    #[test]
    fn terminal_errors() {
        let p = parse_plan("input x=\"\";output y;").unwrap();
        assert_eq!(check_text_terminals(&p)[0].code, "UFP3404");
    }

    #[test]
    fn converts_structured_and_sized_types() {
        let ast = ufp_syntax::parse_type("structured { int<16> count, float value }").unwrap();
        assert_eq!(
            type_from_ast(&ast).unwrap(),
            Type::Structured(vec![
                Field {
                    name: "count".into(),
                    ty: Type::Int(Some(16))
                },
                Field {
                    name: "value".into(),
                    ty: Type::Float {
                        bits: 32,
                        variant: None
                    }
                },
            ])
        );
    }

    #[test]
    fn checks_expression_names_and_types() {
        let plan = ufp_syntax::parse_ast(
            "procedure p(input int x) { let y: int = x + 1; let z: bool = y; }",
        )
        .unwrap();
        let diagnostics = check_ast(&plan);
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["UFP3301"]
        );
    }

    #[test]
    fn unknown_expression_names_have_stable_code() {
        let plan = ufp_syntax::parse_ast("procedure p() { missing + 1; }").unwrap();
        assert_eq!(check_ast(&plan)[0].code, "UFP3101");
    }
}
