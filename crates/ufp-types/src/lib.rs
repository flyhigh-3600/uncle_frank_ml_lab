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
    SymbolicMatrix {
        rank: Option<Dimension>,
        dims: Vec<Dimension>,
        elem: Box<Type>,
    },
    Structured(Vec<Field>),
    Condition,
    Auto,
    Quant(String),
    Named(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dimension {
    Known(usize),
    Variable(String),
}
fn dimension_text(d: &Dimension) -> String {
    match d {
        Dimension::Known(n) => n.to_string(),
        Dimension::Variable(name) => name.clone(),
    }
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
            Self::SymbolicMatrix { rank, dims, elem } => {
                if let (Some(Dimension::Known(rank)), false) = (rank, dims.is_empty()) {
                    if *rank != dims.len() {
                        return Err(format!(
                            "matrix rank {rank} does not match {} dimensions",
                            dims.len()
                        ));
                    }
                }
                if dims.iter().any(|d| matches!(d, Dimension::Known(0))) {
                    return Err("matrix dimensions must be positive".into());
                }
                elem.valid()
            }
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
            Self::SymbolicMatrix { rank, dims, elem } => {
                write!(f, "matrix")?;
                if let Some(rank) = rank {
                    write!(f, "<{}>", dimension_text(rank))?;
                }
                write!(
                    f,
                    "[{}] of {elem}",
                    dims.iter()
                        .map(dimension_text)
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
            let rank = rank.as_deref().map(parse_dimension);
            let dims = dimensions
                .iter()
                .map(|s| parse_dimension(s))
                .collect::<Vec<_>>();
            let elem = Box::new(
                element
                    .as_deref()
                    .map(type_from_ast)
                    .transpose()?
                    .unwrap_or(Type::Auto),
            );
            if rank
                .as_ref()
                .is_some_and(|d| matches!(d, Dimension::Variable(_)))
                || dims.iter().any(|d| matches!(d, Dimension::Variable(_)))
            {
                Type::SymbolicMatrix { rank, dims, elem }
            } else {
                let rank = rank.map(|d| match d {
                    Dimension::Known(n) => n,
                    _ => unreachable!(),
                });
                let dims = dims
                    .into_iter()
                    .map(|d| match d {
                        Dimension::Known(n) => n,
                        _ => unreachable!(),
                    })
                    .collect();
                Type::Matrix { rank, dims, elem }
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
fn parse_dimension(text: &str) -> Dimension {
    text.parse::<usize>()
        .map(Dimension::Known)
        .unwrap_or_else(|_| Dimension::Variable(text.trim().to_owned()))
}

/// Deterministic static checking over the structured parser AST.
pub fn check_ast(plan: &AstPlan) -> Vec<TypeDiagnostic> {
    let mut checker = Checker {
        diagnostics: Vec::new(),
        globals: BTreeMap::new(),
        signatures: BTreeMap::new(),
        dimensions: BTreeMap::new(),
    };
    // Predeclare top-level names to permit forward references.
    for node in &plan.declarations {
        if node.kind == SyntaxKind::Procedure {
            if let Some(name) = declaration_name(node) {
                let pins = node
                    .children
                    .iter()
                    .filter(|n| n.kind == SyntaxKind::Pin)
                    .filter_map(|pin| {
                        let direction = pin
                            .children
                            .iter()
                            .find_map(|c| c.token.as_ref())
                            .map(|t| t.text.as_str())
                            .unwrap_or("input");
                        let ty = pin
                            .type_ast
                            .as_ref()
                            .and_then(|t| type_from_ast(t).ok())
                            .map(|ty| qualify_dimensions(ty, &format!("procedure::{name}")))
                            .unwrap_or(Type::Auto);
                        let pin_name = pin
                            .children
                            .iter()
                            .find(|c| c.kind == SyntaxKind::Identifier)
                            .and_then(|c| c.token.as_ref())
                            .map(|t| t.text.clone())?;
                        Some((direction.to_owned(), pin_name, ty))
                    })
                    .collect();
                checker.signatures.insert(name, pins);
            }
        }
        if let Some(name) = declaration_name(node) {
            checker.globals.insert(
                name.clone(),
                node.type_ast
                    .as_ref()
                    .and_then(|t| type_from_ast(t).ok())
                    .map(|ty| qualify_dimensions(ty, &format!("global::{name}")))
                    .unwrap_or(Type::Auto),
            );
        }
    }
    // Imports and portals are names in the surrounding module namespace. Their
    // members are intentionally opaque until a module loader supplies exports.
    for node in &plan.declarations {
        if let Some(import) = &node.import_ast {
            let name = import.alias.clone().or_else(|| import.path.last().cloned());
            if let Some(name) = name {
                checker.globals.insert(name, Type::Named("module".into()));
            }
        } else if node.kind == SyntaxKind::Portal {
            if let Some(name) = declaration_name(node) {
                checker.globals.insert(name, Type::Named("portal".into()));
                if let Some(alias) = node
                    .children
                    .iter()
                    .filter_map(|c| c.token.as_ref())
                    .find(|t| t.text == "as")
                    .and_then(|_| node.children.last())
                    .and_then(|c| c.token.as_ref())
                    .map(|t| t.text.clone())
                {
                    checker.globals.insert(alias, Type::Named("portal".into()));
                }
            }
        }
    }
    for node in &plan.declarations {
        checker.check_node(node, &mut vec![checker.globals.clone()]);
    }
    checker.diagnostics
}
fn qualify_dimensions(ty: Type, namespace: &str) -> Type {
    let qualify = |d: Dimension| match d {
        Dimension::Known(n) => Dimension::Known(n),
        Dimension::Variable(name) => Dimension::Variable(format!("{namespace}::{name}")),
    };
    match ty {
        Type::Matrix { rank, dims, elem } => Type::Matrix {
            rank,
            dims,
            elem: Box::new(qualify_dimensions(*elem, namespace)),
        },
        Type::SymbolicMatrix { rank, dims, elem } => Type::SymbolicMatrix {
            rank: rank.map(&qualify),
            dims: dims.into_iter().map(qualify).collect(),
            elem: Box::new(qualify_dimensions(*elem, namespace)),
        },
        Type::List(elem) => Type::List(elem.map(|t| Box::new(qualify_dimensions(*t, namespace)))),
        Type::Sheet(elem) => Type::Sheet(elem.map(|t| Box::new(qualify_dimensions(*t, namespace)))),
        Type::Structured(fields) => Type::Structured(
            fields
                .into_iter()
                .map(|f| Field {
                    name: f.name,
                    ty: qualify_dimensions(f.ty, namespace),
                })
                .collect(),
        ),
        other => other,
    }
}

struct Checker {
    diagnostics: Vec<TypeDiagnostic>,
    globals: BTreeMap<String, Type>,
    signatures: BTreeMap<String, Vec<(String, String, Type)>>,
    dimensions: BTreeMap<String, Dimension>,
}
impl Checker {
    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(TypeDiagnostic {
            code,
            message: message.into(),
            span,
        });
    }
    fn check_node(&mut self, node: &SyntaxNode, env: &mut Vec<BTreeMap<String, Type>>) {
        if let Some(ast) = &node.type_ast {
            if let Err(e) = type_from_ast(ast) {
                self.error("UFP3201", e, node.span);
            }
        }
        if node.kind == SyntaxKind::Procedure {
            let mut local = vec![self.globals.clone(), BTreeMap::new()];
            let pins: Vec<_> = node
                .children
                .iter()
                .filter(|n| n.kind == SyntaxKind::Pin)
                .collect();
            for pin in &pins {
                let name = pin
                    .children
                    .iter()
                    .find(|c| c.kind == SyntaxKind::Identifier)
                    .and_then(|c| c.token.as_ref())
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                let namespace = declaration_name(node)
                    .map(|name| format!("procedure::{name}"))
                    .unwrap_or_else(|| "procedure::<anonymous>".into());
                let ty = pin
                    .type_ast
                    .as_ref()
                    .and_then(|t| type_from_ast(t).ok())
                    .map(|ty| qualify_dimensions(ty, &namespace))
                    .unwrap_or(Type::Auto);
                if !name.is_empty() {
                    local.last_mut().unwrap().insert(name, ty.clone());
                }
            }
            for pin in pins {
                for constraint in &pin.constraints {
                    if self.check_shape_constraint(constraint, &local) {
                        continue;
                    }
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
            if let ExprKind::Binary { op, left, .. } = &expr.kind {
                if op == "=" {
                    let target = match &left.kind {
                        ExprKind::Identifier(name) => {
                            env.iter_mut().rev().find_map(|scope| scope.get_mut(name))
                        }
                        ExprKind::Scope { depth, name, .. } => {
                            if *depth > 1 {
                                self.error(
                                    "UFP3102",
                                    format!("cannot write to outer-scope binding `{name}`"),
                                    left.span,
                                );
                                None
                            } else {
                                env.last_mut().and_then(|scope| scope.get_mut(name))
                            }
                        }
                        _ => None,
                    };
                    if let Some(target) = target {
                        if matches!(target, Type::Auto) && !matches!(actual, Type::Auto) {
                            *target = actual.clone();
                        }
                    }
                }
            }
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
                    env.last_mut().unwrap().insert(name, actual);
                }
            }
        }
        // A block is a lexical scope; definitions flow between statements in
        // that block but never escape to its parent.
        if node.kind == SyntaxKind::Block {
            env.push(BTreeMap::new());
            for child in &node.children {
                self.check_node(child, env);
            }
            env.pop();
            return;
        }
        if node.expression.is_some() {
            if let Some(lhs) = node.children.first().and_then(|n| n.expression.as_ref()) {
                if let ExprKind::Scope { depth, name, .. } = &lhs.kind {
                    if *depth > 1 {
                        self.error(
                            "UFP3102",
                            format!("cannot write to outer-scope binding `{name}`"),
                            lhs.span,
                        );
                    }
                }
            }
        }
        if let Some(Expr {
            kind: ExprKind::Binary { op, left, .. },
            ..
        }) = &node.expression
        {
            if op == "=" {
                if let ExprKind::Scope { depth, name, .. } = &left.kind {
                    if *depth > 1 {
                        self.error(
                            "UFP3102",
                            format!("cannot write to outer-scope binding `{name}`"),
                            left.span,
                        );
                    }
                }
            }
        }
        for child in &node.children {
            self.check_node(child, env);
        }
    }
    fn infer(&mut self, e: &Expr, env: &[BTreeMap<String, Type>]) -> Type {
        match &e.kind {
            ExprKind::Literal(v) if v == "true" || v == "false" => Type::Bool,
            ExprKind::Literal(v) if v == "null" => Type::Auto,
            ExprKind::Literal(v) if v.parse::<i64>().is_ok() => Type::Int(None),
            ExprKind::Literal(v) if v.parse::<f64>().is_ok() => Type::Float {
                bits: 32,
                variant: None,
            },
            ExprKind::Literal(_) => Type::String,
            ExprKind::Identifier(n) => env
                .iter()
                .rev()
                .find_map(|scope| scope.get(n))
                .cloned()
                .unwrap_or_else(|| {
                    self.error("UFP3101", format!("unknown name `{n}`"), e.span);
                    Type::Auto
                }),
            ExprKind::Scope { depth, name, .. } => env
                .len()
                .checked_sub(*depth)
                .and_then(|index| env[index].get(name))
                .cloned()
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
                if op == "=" {
                    let value = self.infer(right, env);
                    if let ExprKind::Scope { depth, name, .. } = &left.kind {
                        if *depth > 1 {
                            self.error(
                                "UFP3102",
                                format!("cannot write to outer-scope binding `{name}`"),
                                left.span,
                            );
                        }
                    }
                    return value;
                }
                let a = self.infer(left, env);
                let b = self.infer(right, env);
                match op.as_str() {
                    "and" | "or" if boolish(&a) && boolish(&b) => Type::Bool,
                    "==" | "!=" | "=" | "<" | "<=" | ">" | ">=" if compatible(&a, &b) => match a {
                        Type::Matrix { rank, dims, .. } => Type::Matrix {
                            rank,
                            dims,
                            elem: Box::new(Type::Bool),
                        },
                        Type::SymbolicMatrix { rank, dims, .. } => Type::SymbolicMatrix {
                            rank,
                            dims,
                            elem: Box::new(Type::Bool),
                        },
                        _ => Type::Bool,
                    },
                    "*" if let (
                        Type::Matrix { dims: ad, elem, .. },
                        Type::Matrix { dims: bd, .. },
                    ) = (&a, &b) =>
                    {
                        if ad.len() == 2 && bd.len() == 2 && ad[1] != bd[0] {
                            self.error(
                                "UFP3307",
                                "matrix multiplication inner dimensions do not match",
                                e.span,
                            );
                        }
                        if ad.len() == 2 && bd.len() == 2 {
                            Type::Matrix {
                                rank: Some(2),
                                dims: vec![ad[0], bd[1]],
                                elem: elem.clone(),
                            }
                        } else {
                            Type::Auto
                        }
                    }
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
            ExprKind::Structured(fields) => {
                let mut local = env.to_vec();
                local.push(BTreeMap::new());
                let mut result = Vec::new();
                for f in fields {
                    let annotated = f.ty.as_ref().and_then(|t| type_from_ast(t).ok());
                    let ty = match annotated {
                        Some(Type::Auto) | None => f
                            .value
                            .as_ref()
                            .map(|v| self.infer(v, &local))
                            .unwrap_or(Type::Auto),
                        Some(ty) => ty,
                    };
                    if !f.name.is_empty() {
                        local.last_mut().unwrap().insert(f.name.clone(), ty.clone());
                    }
                    result.push(Field {
                        name: f.name.clone(),
                        ty,
                    });
                }
                Type::Structured(result)
            }
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
                    Type::Matrix { elem, dims, .. } => {
                        if !dims.is_empty() && dims.len() != indices.len() {
                            self.error(
                                "UFP3306",
                                format!(
                                    "matrix rank {} requires {} indices, found {}",
                                    dims.len(),
                                    dims.len(),
                                    indices.len()
                                ),
                                e.span,
                            );
                        }
                        *elem
                    }
                    Type::SymbolicMatrix { elem, dims, .. } => {
                        if dims.len() != indices.len() {
                            self.error(
                                "UFP3306",
                                format!(
                                    "matrix rank {} requires {} indices, found {}",
                                    dims.len(),
                                    dims.len(),
                                    indices.len()
                                ),
                                e.span,
                            );
                        }
                        *elem
                    }
                    Type::Bool => Type::Bool,
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
                if let ExprKind::Field { base, .. } = &callee.kind {
                    if let ExprKind::Identifier(alias) = &base.kind {
                        if matches!(env.iter().rev().find_map(|s| s.get(alias)), Some(Type::Named(kind)) if kind == "module" || kind == "portal")
                        {
                            for arg in args {
                                self.infer(arg, env);
                            }
                            return Type::Auto;
                        }
                    }
                }
                if let ExprKind::Identifier(name) = &callee.kind {
                    if ["transpose", "mean", "zero_like", "probe"].contains(&name.as_str()) {
                        let inputs = args
                            .iter()
                            .map(|arg| self.infer(arg, env))
                            .collect::<Vec<_>>();
                        return match name.as_str() {
                            "transpose" => match inputs.first() {
                                Some(Type::Matrix { rank, dims, elem }) if dims.len() == 2 => {
                                    Type::Matrix {
                                        rank: *rank,
                                        dims: vec![dims[1], dims[0]],
                                        elem: elem.clone(),
                                    }
                                }
                                Some(Type::SymbolicMatrix { rank, dims, elem })
                                    if dims.len() == 2 =>
                                {
                                    Type::SymbolicMatrix {
                                        rank: rank.clone(),
                                        dims: vec![dims[1].clone(), dims[0].clone()],
                                        elem: elem.clone(),
                                    }
                                }
                                Some(t) => t.clone(),
                                None => Type::Auto,
                            },
                            "zero_like" => inputs.first().cloned().unwrap_or(Type::Auto),
                            "mean" => Type::Float {
                                bits: 32,
                                variant: None,
                            },
                            _ => Type::Auto,
                        };
                    }
                    if let Some(signature) = self.signatures.get(name).cloned() {
                        let inputs: Vec<_> = signature
                            .iter()
                            .filter(|(d, _, _)| d == "input" || d == "inout")
                            .cloned()
                            .collect();
                        if inputs.len() != args.len() {
                            self.error(
                                "UFP3203",
                                format!(
                                    "`{name}` expects {} input arguments, found {}",
                                    inputs.len(),
                                    args.len()
                                ),
                                e.span,
                            );
                        }
                        for (arg, (_, pin, expected)) in args.iter().zip(inputs.iter()) {
                            let actual = self.infer(arg, env);
                            if !compatible(expected, &actual) {
                                self.error("UFP3301", format!("argument for `{pin}` expects `{expected}`, found `{actual}`"), arg.span);
                            }
                        }
                        let outputs: Vec<_> = signature
                            .iter()
                            .filter(|(d, _, _)| d == "output" || d == "inout")
                            .collect();
                        return match outputs.len() {
                            0 => Type::Auto,
                            1 => outputs[0].2.clone(),
                            _ => Type::Structured(
                                outputs
                                    .iter()
                                    .map(|(_, n, t)| Field {
                                        name: n.clone(),
                                        ty: t.clone(),
                                    })
                                    .collect(),
                            ),
                        };
                    }
                    if name == "import_gguf" {
                        for a in args {
                            self.infer(a, env);
                        }
                        return Type::Structured(vec![]);
                    }
                }
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
    fn check_shape_constraint(
        &mut self,
        constraint: &Expr,
        env: &[BTreeMap<String, Type>],
    ) -> bool {
        if let ExprKind::Binary { op, left, right } = &constraint.kind {
            if op == "and" {
                let left_ok = self.check_shape_constraint(left, env);
                let right_ok = self.check_shape_constraint(right, env);
                return left_ok && right_ok;
            }
        }
        let ExprKind::Binary { op, left, right } = &constraint.kind else {
            return false;
        };
        if op != "=" && op != "==" {
            return false;
        }
        let (
            ExprKind::Field {
                base: left_base,
                name: left_field,
            },
            ExprKind::Field {
                base: right_base,
                name: right_field,
            },
        ) = (&left.kind, &right.kind)
        else {
            return false;
        };
        if left_field != right_field || !matches!(left_field.as_str(), "shape" | "rank") {
            return false;
        }
        let (ExprKind::Identifier(left_name), ExprKind::Identifier(right_name)) =
            (&left_base.kind, &right_base.kind)
        else {
            return false;
        };
        let lookup = |name: &str| env.iter().rev().find_map(|s| s.get(name));
        let (Some(a), Some(b)) = (lookup(left_name), lookup(right_name)) else {
            return true;
        };
        let Some((ar, ad)) = matrix_dimensions(a) else {
            return true;
        };
        let Some((br, bd)) = matrix_dimensions(b) else {
            return true;
        };
        let mut conflict = false;
        if left_field == "rank" {
            conflict |= ad.len() != bd.len();
            if let (Some(a), Some(b)) = (ar, br) {
                conflict |= !unify_dimensions(&mut self.dimensions, a, b);
            }
        } else {
            conflict |= ad.len() != bd.len();
            if ad.len() == bd.len() {
                for (a, b) in ad.into_iter().zip(bd) {
                    conflict |= !unify_dimensions(&mut self.dimensions, a, b);
                }
            }
        }
        if conflict {
            self.error("UFP3307", format!("shape constraint conflict: `{left_name}.{left_field}` cannot equal `{right_name}.{right_field}`"), constraint.span);
        }
        true
    }
}
fn matrix_dimensions(ty: &Type) -> Option<(Option<Dimension>, Vec<Dimension>)> {
    match ty {
        Type::Matrix { rank, dims, .. } => Some((
            rank.map(Dimension::Known),
            dims.iter().copied().map(Dimension::Known).collect(),
        )),
        Type::SymbolicMatrix { rank, dims, .. } => Some((rank.clone(), dims.clone())),
        _ => None,
    }
}
fn unify_dimensions(
    bindings: &mut BTreeMap<String, Dimension>,
    left: Dimension,
    right: Dimension,
) -> bool {
    fn resolve(bindings: &BTreeMap<String, Dimension>, dim: Dimension) -> Dimension {
        match dim {
            Dimension::Variable(name) => match bindings.get(&name) {
                Some(next) if next != &Dimension::Variable(name.clone()) => {
                    resolve(bindings, next.clone())
                }
                _ => Dimension::Variable(name),
            },
            known => known,
        }
    }
    match (resolve(bindings, left), resolve(bindings, right)) {
        (Dimension::Known(a), Dimension::Known(b)) => a == b,
        (Dimension::Variable(a), Dimension::Known(n))
        | (Dimension::Known(n), Dimension::Variable(a)) => {
            bindings.insert(a, Dimension::Known(n));
            true
        }
        (Dimension::Variable(a), Dimension::Variable(b)) if a == b => true,
        (Dimension::Variable(a), Dimension::Variable(b)) => {
            bindings.insert(a, Dimension::Variable(b));
            true
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
    a == b
        || matches!(a, Type::Auto)
        || matches!(b, Type::Auto)
        || (is_numeric(a) && is_numeric(b))
        || matches!((a, b), (Type::Matrix { rank: ar, dims: ad, elem: ae }, Type::Matrix { rank: br, dims: bd, elem: be }) if ar == br && ad == bd && compatible(ae, be))
        || matches!((matrix_dimensions(a), matrix_dimensions(b)), (Some((ar, ad)), Some((br, bd))) if ar.as_ref().is_none_or(|r| br.as_ref().is_none_or(|other| r == other)) && ad.len() == bd.len() && ad.iter().zip(&bd).all(|(x,y)| x == y || matches!(x, Dimension::Variable(_)) || matches!(y, Dimension::Variable(_))))
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

    #[test]
    fn dot_scope_walks_outward_and_rejects_outer_write() {
        let plan = ufp_syntax::parse_ast(
            "procedure p(input int x) { let y: int = x; if (true) { let z: int = ..y; ..y = 3; } }",
        )
        .unwrap();
        let diagnostics = check_ast(&plan);
        assert!(
            diagnostics.iter().any(|d| d.code == "UFP3102"),
            "{diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|d| d.code == "UFP3101"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn procedure_calls_check_arguments_and_return_output() {
        let plan = ufp_syntax::parse_ast(
            "procedure inc(input int x, output int y) { y = x + 1; } procedure p() { let result: int = inc(1); }",
        ).unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }

    #[test]
    fn shape_constraints_compare_known_dimensions() {
        let plan = ufp_syntax::parse_ast(
            "procedure p(input matrix<2>[2,3] a with a.shape = b.shape, input matrix<2>[2,4] b) {}",
        )
        .unwrap();
        assert!(check_ast(&plan).iter().any(|d| d.code == "UFP3307"));
    }

    #[test]
    fn existing_structured_example_parses_and_checks() {
        let plan = ufp_syntax::parse_ast(include_str!("../../../examples/structured.ufp")).unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }

    #[test]
    fn existing_predict_example_checks_shape_constraints_and_builtins() {
        let plan = ufp_syntax::parse_ast(include_str!("../../../examples/predict.ufp")).unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }

    #[test]
    fn symbolic_dimensions_unify_transitively_and_report_one_conflict() {
        let source = "procedure p(input matrix<2>[n,4] a with a.shape = b.shape and a.shape = c.shape, input matrix<2>[8,4] b, input matrix<2>[9,4] c) {}";
        let plan = ufp_syntax::parse_ast(source).unwrap();
        let errors = check_ast(&plan);
        assert_eq!(
            errors.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["UFP3307"]
        );
        assert!(errors[0].message.contains("shape constraint conflict"));
        assert_eq!(
            errors[0].span.start,
            source.find("a.shape = c.shape").unwrap()
        );
    }

    #[test]
    fn symbolic_names_are_isolated_between_procedures() {
        let plan = ufp_syntax::parse_ast(
            "procedure first(input matrix<2>[n,4] a with a.shape = b.shape, input matrix<2>[8,4] b) {} procedure second(input matrix<2>[n,4] a with a.shape = b.shape, input matrix<2>[9,4] b) {}",
        )
        .unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }

    #[test]
    fn auto_let_infers_from_initializer() {
        let plan =
            ufp_syntax::parse_ast("procedure p() { let value: auto = 7; let copy: int = value; }")
                .unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }

    #[test]
    fn symbolic_type_conversion_keeps_rank_and_dimension_variables() {
        let ast = ufp_syntax::parse_type("matrix<r>[rows,4] of float").unwrap();
        assert_eq!(
            type_from_ast(&ast).unwrap(),
            Type::SymbolicMatrix {
                rank: Some(Dimension::Variable("r".into())),
                dims: vec![Dimension::Variable("rows".into()), Dimension::Known(4)],
                elem: Box::new(Type::Float {
                    bits: 32,
                    variant: None
                }),
            }
        );
    }

    #[test]
    fn auto_structured_field_infers_its_initializer() {
        let plan = ufp_syntax::parse_ast(
            "procedure p() { let value = structured { auto count = 4 }; let total: int = value.count; }",
        ).unwrap();
        assert!(check_ast(&plan).is_empty(), "{:?}", check_ast(&plan));
    }
}
