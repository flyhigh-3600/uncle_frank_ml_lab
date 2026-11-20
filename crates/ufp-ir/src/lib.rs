//! Hierarchical graph IR for `.ufp`.
//!
//! The shallow source front-end currently lowers terminals and pipes. The IR
//! schema is intentionally useful independently of that parser: tools may
//! construct typed graphs, nested regions, folds, and scheduled nodes directly.

use std::collections::{BTreeMap, BTreeSet};

use ufp_syntax::{Item, Plan, Span};
use ufp_types::Type;

/// The compiler version stamped into artifacts and portal cache keys.
pub const UFP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type NodeId = String;
pub type EdgeId = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Zone(pub Vec<u32>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Input,
    Output,
    Operation,
    Fold,
    Portal,
    Region,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub ty: Type,
    pub required: bool,
    pub variadic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub zone: Option<Zone>,
    pub span: Span,
    pub region: Option<Box<Graph>>,
    /// Fold feedback is the sole permitted cycle and must have a fold node as
    /// its target. Runtime support for fold execution is not implied.
    pub fold_feedback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Data,
    FoldFeedback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub from_port: String,
    pub to: NodeId,
    pub to_port: String,
    pub kind: EdgeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub span: Span,
    /// User/tool metadata that does not affect graph semantics.
    pub metadata: BTreeMap<String, String>,
}

/// A project module linked to other modules by portal declarations. `target`
/// is a normalized project-relative module path; links refer to module ids so
/// recursive source modules remain representable without recursive Rust values.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ModuleGraph {
    pub entry: String,
    pub modules: Vec<LinkedModule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedModule {
    pub id: String,
    pub graph: Graph,
    /// Per-procedure lowered graphs; `graph` remains the conventional `main`
    /// graph (or the first available export when no `main` exists).
    pub exports: BTreeMap<String, Graph>,
    pub portals: Vec<ModulePortal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePortal {
    pub node: String,
    pub target: String,
    pub export: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub input_types: Vec<Type>,
    pub output_types: Vec<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Graph {
    /// Validate port references/types, single-value fan-in, acyclicity outside
    /// fold feedback, nested regions, and non-inverting zone assignments.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if nodes.insert(node.id.as_str(), node).is_some() {
                errors.push(error(
                    "UFP3401",
                    format!("duplicate node id `{}`", node.id),
                    node.span,
                ));
            }
            if let Some(region) = &node.region {
                errors.extend(region.validate());
            }
        }
        let mut fanin: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for edge in &self.edges {
            let (Some(from), Some(to)) =
                (nodes.get(edge.from.as_str()), nodes.get(edge.to.as_str()))
            else {
                errors.push(error(
                    "UFP3402",
                    format!("edge `{}` references an unknown node", edge.id),
                    edge.span,
                ));
                continue;
            };
            let source = from.outputs.iter().find(|p| p.name == edge.from_port);
            let target = to.inputs.iter().find(|p| p.name == edge.to_port);
            let (Some(source), Some(target)) = (source, target) else {
                errors.push(error(
                    "UFP3405",
                    format!("edge `{}` references an unknown port", edge.id),
                    edge.span,
                ));
                continue;
            };
            if source.ty != target.ty && source.ty != Type::Auto && target.ty != Type::Auto {
                errors.push(error(
                    "UFP3406",
                    format!(
                        "edge `{}` type mismatch: {} to {}",
                        edge.id, source.ty, target.ty
                    ),
                    edge.span,
                ));
            }
            if edge.kind == EdgeKind::FoldFeedback
                && (to.kind != NodeKind::Fold || !to.fold_feedback)
            {
                errors.push(error(
                    "UFP3407",
                    "feedback edge must target a fold with feedback enabled".into(),
                    edge.span,
                ));
            }
            if edge.kind == EdgeKind::Data {
                *fanin.entry((&edge.to, &edge.to_port)).or_default() += 1;
                adjacency.entry(&edge.from).or_default().push(&edge.to);
                if let (Some(a), Some(b)) = (&from.zone, &to.zone) {
                    if a > b {
                        errors.push(error(
                            "UFP4202",
                            format!(
                                "edge `{}` points from a later zone to an earlier zone",
                                edge.id
                            ),
                            edge.span,
                        ));
                    }
                }
            }
        }
        for node in &self.nodes {
            for port in &node.inputs {
                let count = fanin
                    .get(&(node.id.as_str(), port.name.as_str()))
                    .copied()
                    .unwrap_or(0);
                if count > 1 && !port.variadic {
                    errors.push(error(
                        "UFP3408",
                        format!(
                            "input `{}.{}` has multiple incoming edges",
                            node.id, port.name
                        ),
                        node.span,
                    ));
                }
                if count == 0 && port.required {
                    errors.push(error(
                        "UFP3409",
                        format!("required input `{}.{}` is unconnected", node.id, port.name),
                        node.span,
                    ));
                }
            }
        }
        fn visit<'a>(
            n: &'a str,
            graph: &BTreeMap<&'a str, Vec<&'a str>>,
            active: &mut BTreeSet<&'a str>,
            done: &mut BTreeSet<&'a str>,
        ) -> bool {
            if done.contains(n) {
                return false;
            }
            if !active.insert(n) {
                return true;
            }
            let cyclic = graph
                .get(n)
                .is_some_and(|next| next.iter().any(|x| visit(x, graph, active, done)));
            active.remove(n);
            done.insert(n);
            cyclic
        }
        let mut active = BTreeSet::new();
        let mut done = BTreeSet::new();
        if nodes
            .keys()
            .any(|n| visit(n, &adjacency, &mut active, &mut done))
        {
            errors.push(error(
                "UFP3410",
                "data edges contain a cycle outside fold feedback".into(),
                self.span,
            ));
        }
        errors
    }
}

fn error(code: &'static str, message: String, span: Span) -> ValidationError {
    ValidationError {
        code,
        message,
        span,
    }
}

/// Lower the supported shallow syntax to a graph. Declarations the front-end
/// does not understand are retained as diagnostics rather than silently lost.
pub fn lower_plan(plan: &Plan) -> Result<Graph, Vec<ValidationError>> {
    let mut graph = Graph::default();
    for item in &plan.items {
        match item {
            Item::Input { name, span, .. } => {
                graph.nodes.push(text_node(name, NodeKind::Input, *span))
            }
            Item::Output { name, span } => {
                graph.nodes.push(text_node(name, NodeKind::Output, *span))
            }
            Item::Pipe { from, to, span } => graph.edges.push(Edge {
                id: format!("{}:{}->{}:{}", from.node, from.port, to.node, to.port),
                from: from.node.clone(),
                from_port: from.port.clone(),
                to: to.node.clone(),
                to_port: to.port.clone(),
                kind: EdgeKind::Data,
                span: *span,
            }),
            Item::Portal { name, span, .. } => {
                graph.nodes.push(text_node(name, NodeKind::Portal, *span))
            }
            Item::Text { name, span, .. } => {
                graph
                    .nodes
                    .push(text_node(name, NodeKind::Operation, *span))
            }
            Item::Other { keyword, span } => {
                return Err(vec![error(
                    "UFP3001",
                    format!("cannot lower unsupported `{keyword}` declaration"),
                    *span,
                )])
            }
        }
    }
    let errors = graph.validate();
    if errors.is_empty() {
        Ok(graph)
    } else {
        Err(errors)
    }
}

fn text_node(name: &str, kind: NodeKind, span: Span) -> Node {
    let text = Port {
        name: "text".into(),
        ty: Type::String,
        required: kind == NodeKind::Output,
        variadic: false,
    };
    Node {
        id: name.into(),
        name: name.into(),
        kind,
        inputs: if kind == NodeKind::Output {
            vec![text.clone()]
        } else {
            vec![]
        },
        outputs: if kind == NodeKind::Input {
            vec![text]
        } else {
            vec![]
        },
        zone: None,
        span,
        region: None,
        fold_feedback: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ufp_syntax::parse_plan;

    fn node(id: &str, kind: NodeKind, input: Vec<Port>, output: Vec<Port>) -> Node {
        Node {
            id: id.into(),
            name: id.into(),
            kind,
            inputs: input,
            outputs: output,
            zone: None,
            span: Span::default(),
            region: None,
            fold_feedback: false,
        }
    }
    fn port(name: &str, ty: Type, required: bool) -> Port {
        Port {
            name: name.into(),
            ty,
            required,
            variadic: false,
        }
    }
    #[test]
    fn lowers_and_validates_shallow_terminals() {
        let plan =
            parse_plan("input prompt = \"hello\"; output answer; pipe prompt.text -> answer.text;")
                .unwrap();
        let graph = lower_plan(&plan).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.validate().is_empty());
    }
    #[test]
    fn catches_type_mismatch_fanin_and_data_cycle() {
        let mut a = node(
            "a",
            NodeKind::Operation,
            vec![],
            vec![port("out", Type::String, false)],
        );
        let mut b = node(
            "b",
            NodeKind::Operation,
            vec![port("in", Type::Int(None), true)],
            vec![port("out", Type::String, false)],
        );
        a.zone = Some(Zone(vec![1]));
        b.zone = Some(Zone(vec![0]));
        let graph = Graph {
            nodes: vec![a, b],
            edges: vec![
                Edge {
                    id: "ab".into(),
                    from: "a".into(),
                    from_port: "out".into(),
                    to: "b".into(),
                    to_port: "in".into(),
                    kind: EdgeKind::Data,
                    span: Span::default(),
                },
                Edge {
                    id: "ba".into(),
                    from: "b".into(),
                    from_port: "out".into(),
                    to: "a".into(),
                    to_port: "missing".into(),
                    kind: EdgeKind::Data,
                    span: Span::default(),
                },
                Edge {
                    id: "ab2".into(),
                    from: "a".into(),
                    from_port: "out".into(),
                    to: "b".into(),
                    to_port: "in".into(),
                    kind: EdgeKind::Data,
                    span: Span::default(),
                },
            ],
            ..Graph::default()
        };
        let codes: Vec<_> = graph.validate().into_iter().map(|e| e.code).collect();
        assert!(codes.contains(&"UFP3406"));
        assert!(codes.contains(&"UFP3408"));
        assert!(codes.contains(&"UFP4202"));
    }

    #[test]
    fn permits_only_explicit_fold_feedback_cycle() {
        let mut fold = node(
            "fold",
            NodeKind::Fold,
            vec![port("carry", Type::String, false)],
            vec![port("next", Type::String, false)],
        );
        fold.fold_feedback = true;
        let graph = Graph {
            nodes: vec![fold],
            edges: vec![Edge {
                id: "feedback".into(),
                from: "fold".into(),
                from_port: "next".into(),
                to: "fold".into(),
                to_port: "carry".into(),
                kind: EdgeKind::FoldFeedback,
                span: Span::default(),
            }],
            ..Graph::default()
        };
        assert!(graph.validate().is_empty());
    }
}
