//! Closed UFP type model and first static checks.
//!
//! The parser will attach these to every AST expression in UF-201/UF-203. The
//! terminal checker is already used by the executable text-plan slice.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use ufp_syntax::{Item, Plan, Span};

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
}
