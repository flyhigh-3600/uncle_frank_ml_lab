//! Small deterministic reference runtime for the executable M0 text-terminal slice.
//!
//! The full typed graph interpreter lands behind the same API in UF-309. Today
//! this runtime deliberately executes only direct text-terminal pipes; it never
//! pretends that procedures or tensor graphs have been evaluated.

use std::collections::{BTreeMap, BTreeSet};

use std::path::{Path, PathBuf};
use ufp_syntax::{Endpoint, Item, Plan};

pub mod loaders;

/// A project-local module and its directly declared terminal signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModule {
    pub path: PathBuf,
    pub content_hash: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub metadata: BTreeMap<String, String>,
    pub graph: ufp_ir::Graph,
    pub exports: BTreeMap<String, ufp_ir::Graph>,
    pub portals: Vec<ufp_ir::ModulePortal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModuleGraph {
    pub entry: PathBuf,
    pub modules: Vec<ResolvedModule>,
    pub ir: ufp_ir::ModuleGraph,
}

type PinSignature = (
    Vec<String>,
    Vec<String>,
    Vec<ufp_types::Type>,
    Vec<ufp_types::Type>,
);

fn procedure_signatures(
    source: &str,
    module: &Path,
) -> Result<BTreeMap<String, PinSignature>, RunError> {
    let ast = ufp_syntax::parse_ast(source)
        .map_err(|e| RunError(format!("parse error in `{}`: {}", module.display(), e[0])))?;
    let mut signatures = BTreeMap::new();
    for declaration in ast
        .declarations
        .iter()
        .filter(|n| n.kind == ufp_syntax::SyntaxKind::Procedure)
    {
        let name = declaration
            .children
            .iter()
            .find(|n| n.kind == ufp_syntax::SyntaxKind::Identifier)
            .and_then(|n| n.token.as_ref())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        let (mut inputs, mut outputs, mut input_types, mut output_types) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for pin in declaration
            .children
            .iter()
            .filter(|n| n.kind == ufp_syntax::SyntaxKind::Pin)
        {
            let direction = pin
                .children
                .iter()
                .find_map(|n| n.token.as_ref())
                .map(|t| t.text.as_str())
                .unwrap_or("input");
            let name = pin
                .children
                .iter()
                .find(|n| n.kind == ufp_syntax::SyntaxKind::Identifier)
                .and_then(|n| n.token.as_ref())
                .map(|t| t.text.clone())
                .unwrap_or_default();
            let ty = pin
                .type_ast
                .as_ref()
                .ok_or_else(|| RunError(format!("pin `{name}` has no type")))?;
            let ty = ufp_types::type_from_ast(ty)
                .map_err(|e| RunError(format!("invalid type for pin `{name}`: {e}")))?;
            if direction == "input" || direction == "inout" {
                inputs.push(name.clone());
                input_types.push(ty.clone());
            }
            if direction == "output" || direction == "inout" {
                outputs.push(name);
                output_types.push(ty);
            }
        }
        signatures.insert(name, (inputs, outputs, input_types, output_types));
    }
    Ok(signatures)
}

fn lower_procedure_body(
    source: &str,
    ast: &ufp_syntax::AstPlan,
    portal_links: &[ufp_ir::ModulePortal],
    procedure_name: &str,
) -> Result<Option<ufp_ir::Graph>, RunError> {
    use ufp_syntax::{ExprKind, SyntaxKind};
    fn contains_fold(expr: &ufp_syntax::Expr) -> bool {
        match &expr.kind {
            ExprKind::RunUntil { .. } => true,
            ExprKind::Unary { value, .. } | ExprKind::Condition(value) => contains_fold(value),
            ExprKind::Binary { left, right, .. } => contains_fold(left) || contains_fold(right),
            ExprKind::Field { base, .. } => contains_fold(base),
            ExprKind::Index { base, indices } => {
                contains_fold(base) || indices.iter().any(contains_fold)
            }
            ExprKind::Call { callee, args } | ExprKind::PortMap { callee, args } => {
                contains_fold(callee) || args.iter().any(contains_fold)
            }
            ExprKind::PortChain { links } | ExprKind::List(links) | ExprKind::Matrix(links) => {
                links.iter().any(contains_fold)
            }
            ExprKind::Structured(fields) => fields
                .iter()
                .filter_map(|f| f.value.as_ref())
                .any(contains_fold),
            ExprKind::NamedArg { value, .. } => contains_fold(value),
            ExprKind::Literal(_)
            | ExprKind::Identifier(_)
            | ExprKind::Current
            | ExprKind::Scope { .. } => false,
        }
    }
    let Some(proc_node) = ast.declarations.iter().find(|n| {
        n.kind == SyntaxKind::Procedure
            && n.children
                .iter()
                .find(|c| c.kind == SyntaxKind::Identifier)
                .and_then(|c| c.token.as_ref())
                .is_some_and(|t| t.text == procedure_name)
    }) else {
        return Ok(None);
    };
    let body = proc_node
        .children
        .iter()
        .find(|n| n.kind == SyntaxKind::Block)
        .ok_or_else(|| RunError("main procedure has no block body".into()))?;
    let mut graph = ufp_ir::Graph::default();
    for pin in proc_node
        .children
        .iter()
        .filter(|n| n.kind == SyntaxKind::Pin)
    {
        let direction = pin
            .children
            .iter()
            .find_map(|c| c.token.as_ref())
            .map(|t| t.text.as_str())
            .unwrap_or("input");
        let name = pin
            .children
            .iter()
            .find(|c| c.kind == SyntaxKind::Identifier)
            .and_then(|c| c.token.as_ref())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        let ty = ufp_types::type_from_ast(
            pin.type_ast
                .as_ref()
                .ok_or_else(|| RunError(format!("pin `{name}` has no type")))?,
        )
        .map_err(|e| RunError(format!("invalid pin `{name}` type: {e}")))?;
        if direction == "inout" {
            return Err(RunError(format!(
                "unsupported procedure pin direction `inout` for `{name}`"
            )));
        }
        let port = ufp_ir::Port {
            name: if direction == "input" { "out" } else { "in" }.into(),
            ty,
            required: direction == "output",
            variadic: false,
        };
        graph.nodes.push(ufp_ir::Node {
            id: name.clone(),
            name,
            kind: if direction == "input" {
                ufp_ir::NodeKind::Input
            } else {
                ufp_ir::NodeKind::Output
            },
            inputs: if direction == "output" {
                vec![port.clone()]
            } else {
                vec![]
            },
            outputs: if direction == "input" {
                vec![port]
            } else {
                vec![]
            },
            zone: None,
            span: pin.span,
            region: None,
            fold_feedback: false,
        });
    }
    let mut pipes = Vec::new();
    for statement in &body.children {
        match statement.kind {
            SyntaxKind::Node => {
                let identifiers = statement.children.iter().filter(|c| c.kind == SyntaxKind::Identifier)
                    .filter_map(|c| c.token.as_ref().map(|t| t.text.clone())).collect::<Vec<_>>();
                let instance = identifiers.first().ok_or_else(|| RunError("node declaration is missing a name".into()))?;
                let raw = source.get(statement.span.start..statement.span.end).unwrap_or_default();
                let target = raw.split_once(':').map(|(_, rhs)| rhs.trim())
                    .map(|rhs| rhs.split_once('(').map(|(target, _)| target.trim()).unwrap_or_else(|| rhs.trim_end_matches(';').trim()))
                    .ok_or_else(|| RunError(format!("node `{instance}` requires a callable target and argument list")))?;
                if let Some(link) = portal_links.iter().find(|p| p.node == target) {
                    let inputs = link.inputs.iter().zip(&link.input_types).map(|(n,t)| ufp_ir::Port { name:n.clone(), ty:t.clone(), required:true, variadic:false }).collect();
                    let outputs = link.outputs.iter().zip(&link.output_types).map(|(n,t)| ufp_ir::Port { name:n.clone(), ty:t.clone(), required:false, variadic:false }).collect();
                    graph.nodes.push(ufp_ir::Node { id:instance.clone(), name:target.into(), kind:ufp_ir::NodeKind::Portal,
                        inputs, outputs, zone:None, span:statement.span, region:None, fold_feedback:false });
                } else {
                    let expression_count = statement.children.iter().filter(|c| c.kind == SyntaxKind::Expression && c.expression.is_some()).count();
                    let argument_names = identifiers.iter().rev().take(expression_count).cloned().collect::<Vec<_>>();
                    let mut argument_names = argument_names;
                    argument_names.reverse();
                    let inputs = argument_names.iter().map(|name| ufp_ir::Port { name:name.clone(), ty:ufp_types::Type::Auto, required:false, variadic:false }).collect();
                    let outputs = vec![ufp_ir::Port { name:"out".into(), ty:ufp_types::Type::Auto, required:false, variadic:false }];
                    for (index, expr) in statement.children.iter().filter(|c| c.kind == SyntaxKind::Expression && c.expression.is_some()).enumerate() {
                        if let Some(expression) = &expr.expression {
                            if contains_fold(expression) {
                                return Err(RunError(format!(
                                    "unsupported fold/run-until expression in node `{instance}`; fold lowering is not implemented"
                                )));
                            }
                            graph.metadata.insert(format!("node.{instance}.arg.{}", argument_names.get(index).map(String::as_str).unwrap_or("?")), format!("{:?}", expression.kind));
                        }
                    }
                    graph.metadata.insert(format!("node.{instance}.callee"), target.into());
                    graph.nodes.push(ufp_ir::Node { id:instance.clone(), name:target.into(), kind:ufp_ir::NodeKind::Operation,
                        inputs, outputs, zone:None, span:statement.span, region:None, fold_feedback:false });
                }
            }
            SyntaxKind::Pipe => {
                let expr = statement.children.iter().find_map(|n| n.expression.as_ref())
                    .ok_or_else(|| RunError("pipe has no parsed port chain".into()))?;
                let ExprKind::PortChain { links } = &expr.kind else { return Err(RunError("only simple port-chain pipes are supported in main".into())); };
                if links.len() < 2 { return Err(RunError("pipe must connect at least two endpoints".into())); }
                for pair in links.windows(2) {
                    fn endpoint(e: &ufp_syntax::Expr) -> Option<(String,String)> {
                        if let ExprKind::Field { base, name } = &e.kind {
                            if let ExprKind::Identifier(node) = &base.kind { return Some((node.clone(), name.clone())); }
                        }
                        None
                    }
                    let (from, from_port) = endpoint(&pair[0]).ok_or_else(|| RunError("pipe endpoint must be `node.port`".into()))?;
                    let (to, to_port) = endpoint(&pair[1]).ok_or_else(|| RunError("pipe endpoint must be `node.port`".into()))?;
                    pipes.push(ufp_ir::Edge { id:format!("{from}:{from_port}->{to}:{to_port}"), from, from_port, to, to_port,
                        kind:ufp_ir::EdgeKind::Data, span:statement.span });
                }
            }
            SyntaxKind::If => lower_if_region(source, statement, portal_links, procedure_name, &mut graph)?,
            SyntaxKind::Let | SyntaxKind::Assign => {
                if let Some(ufp_syntax::Expr { kind: ExprKind::RunUntil { value, condition }, span: fold_span }) = statement.expression.as_ref() {
                    let target = assignment_target(statement).ok_or_else(|| RunError("run-until must assign to a simple named variable".into()))?;
                    lower_fold_region(source, value, condition, *fold_span, &target, &mut graph)?;
                } else if statement.expression.as_ref().is_some_and(contains_fold) {
                    return Err(RunError(format!("unsupported nested fold expression in procedure `{procedure_name}`")));
                } else {
                    let target = assignment_target(statement).ok_or_else(|| RunError("assignment must target a simple named variable".into()))?;
                    let expression = statement.expression.as_ref().ok_or_else(|| RunError("assignment has no expression".into()))?;
                    let id = format!("assign_{}", statement.span.start);
                    graph.nodes.push(control_node(&id, "assignment", ufp_ir::NodeKind::Operation, vec![], vec![], statement.span));
                    graph.metadata.insert(format!("{id}.target"), target);
                    graph.metadata.insert(format!("{id}.expression"), source.get(expression.span.start..expression.span.end).unwrap_or_default().trim().into());
                }
            }
            SyntaxKind::Return | SyntaxKind::ExpressionStatement => {
                return Err(RunError(format!(
                    "unsupported {:?} expression in procedure `{procedure_name}`",
                    statement.kind
                )));
            }
            _ => return Err(RunError(format!(
                "unsupported {:?} statement in procedure `{procedure_name}`; only node declarations and pipes are currently lowerable",
                statement.kind
            ))),
        }
    }
    graph.edges.extend(pipes);
    let errors = graph.validate();
    if !errors.is_empty() {
        return Err(RunError(format!(
            "procedure graph validation: {}",
            errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    let _ = source;
    Ok(Some(graph))
}

fn lower_if_region(
    source: &str,
    statement: &ufp_syntax::SyntaxNode,
    portal_links: &[ufp_ir::ModulePortal],
    procedure_name: &str,
    graph: &mut ufp_ir::Graph,
) -> Result<(), RunError> {
    use ufp_syntax::SyntaxKind;
    let id = format!("if_{}", statement.span.start);
    let mut region = ufp_ir::Graph::default();
    let mut mode = "if";
    let mut predicate: Option<ufp_syntax::Expr> = None;
    let mut branches = Vec::new();
    let mut else_block = None;
    let mut index = 0usize;
    for child in &statement.children {
        match child.kind {
            SyntaxKind::Expression => predicate = child.expression.clone(),
            SyntaxKind::Operator => {
                if let Some(token) = &child.token {
                    if token.text == "elif" {
                        mode = "elif";
                    }
                    if token.text == "else" {
                        mode = "else";
                    }
                }
            }
            SyntaxKind::Block if mode == "else" => else_block = Some(child),
            SyntaxKind::Block => {
                let expr = predicate.take().ok_or_else(|| {
                    RunError("conditional is missing a predicate expression".into())
                })?;
                let pred_id = format!("{id}_predicate_{index}");
                let branch_id = format!("{id}_branch_{index}");
                let mut pred = control_node(
                    &pred_id,
                    "predicate",
                    ufp_ir::NodeKind::Operation,
                    vec![],
                    vec![ufp_ir::Port {
                        name: "condition".into(),
                        ty: ufp_types::Type::Condition,
                        required: false,
                        variadic: false,
                    }],
                    expr.span,
                );
                region.metadata.insert(
                    format!("{pred_id}.expression"),
                    source
                        .get(expr.span.start..expr.span.end)
                        .unwrap_or_default()
                        .trim()
                        .into(),
                );
                region
                    .metadata
                    .insert(format!("{branch_id}.mode"), mode.into());
                region.metadata.insert(
                    format!("{branch_id}.gate"),
                    if mode == "if" {
                        "current_predicate_true".into()
                    } else {
                        "all_previous_predicates_false_and_current_true".into()
                    },
                );
                pred.name = mode.into();
                region.nodes.push(pred);
                let branch_graph =
                    lower_nested_region(source, &child.children, portal_links, procedure_name)?;
                region.nodes.push(
                    control_node(
                        &branch_id,
                        "branch",
                        ufp_ir::NodeKind::Region,
                        vec![ufp_ir::Port {
                            name: "condition".into(),
                            ty: ufp_types::Type::Condition,
                            required: true,
                            variadic: false,
                        }],
                        vec![],
                        child.span,
                    )
                    .with_region(branch_graph),
                );
                region.edges.push(ufp_ir::Edge {
                    id: format!("{pred_id}->{branch_id}"),
                    from: pred_id,
                    from_port: "condition".into(),
                    to: branch_id.clone(),
                    to_port: "condition".into(),
                    kind: ufp_ir::EdgeKind::Data,
                    span: child.span,
                });
                branches.push(branch_id);
                index += 1;
                mode = "elif";
            }
            _ => {}
        }
    }
    if let Some(block) = else_block {
        let else_id = format!("{id}_else");
        let nested = lower_nested_region(source, &block.children, portal_links, procedure_name)?;
        region.nodes.push(
            control_node(
                &else_id,
                "else",
                ufp_ir::NodeKind::Region,
                vec![],
                vec![],
                block.span,
            )
            .with_region(nested),
        );
        region
            .metadata
            .insert(format!("{else_id}.mode"), "else".into());
        region.metadata.insert(
            format!("{else_id}.gate"),
            "all_previous_predicates_false".into(),
        );
        branches.push(else_id);
    }
    let merge_id = format!("{id}_merge");
    region.nodes.push(
        control_node(
            &merge_id,
            "merge",
            ufp_ir::NodeKind::Region,
            vec![],
            vec![],
            statement.span,
        )
        .with_region(ufp_ir::Graph::default()),
    );
    region
        .metadata
        .insert(format!("{id}.branches"), branches.join(","));
    region
        .metadata
        .insert(format!("{merge_id}.semantics"), "join branch exits".into());
    if let Some(error) = region.validate().first() {
        return Err(RunError(format!(
            "conditional region validation: {}",
            error.message
        )));
    }
    graph.nodes.push(
        control_node(
            &id,
            "if-region",
            ufp_ir::NodeKind::Region,
            vec![],
            vec![],
            statement.span,
        )
        .with_region(region),
    );
    Ok(())
}

fn lower_nested_region(
    source: &str,
    statements: &[ufp_syntax::SyntaxNode],
    portal_links: &[ufp_ir::ModulePortal],
    procedure_name: &str,
) -> Result<ufp_ir::Graph, RunError> {
    let mut graph = ufp_ir::Graph::default();
    for statement in statements {
        match statement.kind {
            ufp_syntax::SyntaxKind::Node => {
                lower_node_for_region(source, statement, portal_links, &mut graph)?
            }
            ufp_syntax::SyntaxKind::Pipe => {
                let expr = statement
                    .children
                    .iter()
                    .find_map(|n| n.expression.as_ref())
                    .ok_or_else(|| RunError("pipe has no parsed port chain".into()))?;
                let ufp_syntax::ExprKind::PortChain { links } = &expr.kind else {
                    return Err(RunError(format!(
                        "unsupported pipe in conditional `{procedure_name}`"
                    )));
                };
                for pair in links.windows(2) {
                    let (from, from_port) = pipe_endpoint(&pair[0])?;
                    let (to, to_port) = pipe_endpoint(&pair[1])?;
                    graph.edges.push(ufp_ir::Edge {
                        id: format!("{from}:{from_port}->{to}:{to_port}"),
                        from,
                        from_port,
                        to,
                        to_port,
                        kind: ufp_ir::EdgeKind::Data,
                        span: statement.span,
                    });
                }
            }
            ufp_syntax::SyntaxKind::If => {
                lower_if_region(source, statement, portal_links, procedure_name, &mut graph)?
            }
            ufp_syntax::SyntaxKind::Let | ufp_syntax::SyntaxKind::Assign => {
                if let Some(ufp_syntax::Expr {
                    kind: ufp_syntax::ExprKind::RunUntil { value, condition },
                    span: fold_span,
                }) = statement.expression.as_ref()
                {
                    let target = assignment_target(statement).ok_or_else(|| {
                        RunError("run-until must assign to a simple named variable".into())
                    })?;
                    lower_fold_region(source, value, condition, *fold_span, &target, &mut graph)?;
                } else {
                    let target = assignment_target(statement).ok_or_else(|| {
                        RunError("assignment must target a simple named variable".into())
                    })?;
                    let expression = statement
                        .expression
                        .as_ref()
                        .ok_or_else(|| RunError("assignment has no expression".into()))?;
                    let id = format!("assign_{}", statement.span.start);
                    graph.nodes.push(control_node(
                        &id,
                        "assignment",
                        ufp_ir::NodeKind::Operation,
                        vec![],
                        vec![],
                        statement.span,
                    ));
                    graph.metadata.insert(format!("{id}.target"), target);
                    graph.metadata.insert(
                        format!("{id}.expression"),
                        source
                            .get(expression.span.start..expression.span.end)
                            .unwrap_or_default()
                            .trim()
                            .into(),
                    );
                }
            }
            _ => {
                return Err(RunError(format!(
                    "unsupported {:?} inside conditional region",
                    statement.kind
                )))
            }
        }
    }
    Ok(graph)
}

fn pipe_endpoint(expr: &ufp_syntax::Expr) -> Result<(String, String), RunError> {
    if let ufp_syntax::ExprKind::Field { base, name } = &expr.kind {
        if let ufp_syntax::ExprKind::Identifier(node) = &base.kind {
            return Ok((node.clone(), name.clone()));
        }
    }
    Err(RunError("pipe endpoint must be `node.port`".into()))
}

fn assignment_target(statement: &ufp_syntax::SyntaxNode) -> Option<String> {
    if let Some(name) = statement
        .children
        .iter()
        .find(|n| n.kind == ufp_syntax::SyntaxKind::Identifier)
        .and_then(|n| n.token.as_ref())
        .map(|t| t.text.clone())
    {
        return Some(name);
    }
    let lhs = statement
        .children
        .iter()
        .find_map(|n| n.expression.as_ref())?;
    match &lhs.kind {
        ufp_syntax::ExprKind::Identifier(name) => Some(name.clone()),
        _ => None,
    }
}

fn lower_node_for_region(
    source: &str,
    statement: &ufp_syntax::SyntaxNode,
    portal_links: &[ufp_ir::ModulePortal],
    graph: &mut ufp_ir::Graph,
) -> Result<(), RunError> {
    use ufp_syntax::SyntaxKind;
    let ids = statement
        .children
        .iter()
        .filter(|c| c.kind == SyntaxKind::Identifier)
        .filter_map(|c| c.token.as_ref().map(|t| t.text.clone()))
        .collect::<Vec<_>>();
    let id = ids
        .first()
        .ok_or_else(|| RunError("node declaration is missing a name".into()))?;
    let raw = source
        .get(statement.span.start..statement.span.end)
        .unwrap_or_default();
    let target = raw
        .split_once(':')
        .map(|(_, r)| r.trim())
        .map(|r| {
            r.split_once('(')
                .map(|(t, _)| t.trim())
                .unwrap_or_else(|| r.trim_end_matches(';').trim())
        })
        .ok_or_else(|| RunError(format!("node `{id}` has no target")))?;
    if let Some(link) = portal_links.iter().find(|p| p.node == target) {
        graph.nodes.push(control_node(
            id,
            target,
            ufp_ir::NodeKind::Portal,
            link.inputs
                .iter()
                .zip(&link.input_types)
                .map(|(name, ty)| ufp_ir::Port {
                    name: name.clone(),
                    ty: ty.clone(),
                    required: true,
                    variadic: false,
                })
                .collect(),
            link.outputs
                .iter()
                .zip(&link.output_types)
                .map(|(name, ty)| ufp_ir::Port {
                    name: name.clone(),
                    ty: ty.clone(),
                    required: false,
                    variadic: false,
                })
                .collect(),
            statement.span,
        ));
    } else {
        graph
            .metadata
            .insert(format!("node.{id}.callee"), target.into());
        graph.nodes.push(control_node(
            id,
            target,
            ufp_ir::NodeKind::Operation,
            vec![],
            vec![ufp_ir::Port {
                name: "out".into(),
                ty: ufp_types::Type::Auto,
                required: false,
                variadic: false,
            }],
            statement.span,
        ));
    }
    Ok(())
}

fn lower_fold_region(
    source: &str,
    value: &ufp_syntax::Expr,
    condition: &ufp_syntax::Expr,
    span: ufp_syntax::Span,
    target: &str,
    graph: &mut ufp_ir::Graph,
) -> Result<(), RunError> {
    let id = format!("fold_{}", span.start);
    let complete = source.get(span.start..span.end).unwrap_or_default();
    let initial = complete
        .split_once("run")
        .map(|(initial, _)| initial.trim())
        .filter(|initial| !initial.is_empty())
        .ok_or_else(|| RunError("run-until must have an initial state before `run`".into()))?;
    let body = control_node(
        &format!("{id}_body"),
        "fold-body",
        ufp_ir::NodeKind::Operation,
        vec![ufp_ir::Port {
            name: "carry".into(),
            ty: ufp_types::Type::Auto,
            required: false,
            variadic: false,
        }],
        vec![ufp_ir::Port {
            name: "next".into(),
            ty: ufp_types::Type::Auto,
            required: false,
            variadic: false,
        }],
        value.span,
    );
    let predicate = control_node(
        &format!("{id}_predicate"),
        "fold-condition",
        ufp_ir::NodeKind::Operation,
        vec![],
        vec![ufp_ir::Port {
            name: "condition".into(),
            ty: ufp_types::Type::Condition,
            required: false,
            variadic: false,
        }],
        condition.span,
    );
    let mut region = ufp_ir::Graph {
        nodes: vec![body, predicate],
        ..Default::default()
    };
    region.metadata.insert(
        "fold.value_expression".into(),
        source
            .get(value.span.start..value.span.end)
            .unwrap_or_default()
            .trim()
            .into(),
    );
    region.metadata.insert(
        "fold.condition_expression".into(),
        source
            .get(condition.span.start..condition.span.end)
            .unwrap_or_default()
            .trim()
            .into(),
    );
    graph.nodes.push(ufp_ir::Node {
        id: id.clone(),
        name: "run-until".into(),
        kind: ufp_ir::NodeKind::Fold,
        inputs: vec![ufp_ir::Port {
            name: "carry".into(),
            ty: ufp_types::Type::Auto,
            required: false,
            variadic: false,
        }],
        outputs: vec![ufp_ir::Port {
            name: "next".into(),
            ty: ufp_types::Type::Auto,
            required: false,
            variadic: false,
        }],
        zone: None,
        span,
        region: Some(Box::new(region)),
        fold_feedback: true,
    });
    graph.edges.push(ufp_ir::Edge {
        id: format!("{id}_feedback"),
        from: id.clone(),
        from_port: "next".into(),
        to: id.clone(),
        to_port: "carry".into(),
        kind: ufp_ir::EdgeKind::FoldFeedback,
        span,
    });
    graph
        .metadata
        .insert(format!("{id}.feedback_gate"), "condition_unmet".into());
    graph.metadata.insert(format!("{id}.fuel"), "10000".into());
    graph.metadata.insert(
        format!("{id}.execution_order"),
        "body_then_condition".into(),
    );
    graph.metadata.insert(format!("{id}.target"), target.into());
    graph
        .metadata
        .insert(format!("{id}.initial"), initial.into());
    Ok(())
}

fn control_node(
    id: &str,
    name: &str,
    kind: ufp_ir::NodeKind,
    inputs: Vec<ufp_ir::Port>,
    outputs: Vec<ufp_ir::Port>,
    span: ufp_syntax::Span,
) -> ufp_ir::Node {
    ufp_ir::Node {
        id: id.into(),
        name: name.into(),
        kind,
        inputs,
        outputs,
        zone: None,
        span,
        region: None,
        fold_feedback: false,
    }
}

trait WithRegion {
    fn with_region(self, region: ufp_ir::Graph) -> Self;
}

impl WithRegion for ufp_ir::Node {
    fn with_region(mut self, region: ufp_ir::Graph) -> Self {
        self.region = Some(Box::new(region));
        self
    }
}

/// Resolve portal files relative to the project root, recursively checking
/// that the module graph is acyclic. Portal paths are project-relative (not
/// relative to the importing file).
pub fn resolve_portals(project_root: &Path, entry: &Path) -> Result<Vec<ResolvedModule>, RunError> {
    fn visit(
        root: &Path,
        path: &Path,
        active: &mut Vec<PathBuf>,
        done: &mut BTreeSet<PathBuf>,
        modules: &mut Vec<ResolvedModule>,
        selected: &mut BTreeMap<PathBuf, BTreeMap<String, PinSignature>>,
    ) -> Result<(), RunError> {
        let full = root.join(path);
        let canonical = full
            .canonicalize()
            .map_err(|e| RunError(format!("cannot resolve module `{}`: {e}", path.display())))?;
        let relative = canonical
            .strip_prefix(
                root.canonicalize()
                    .map_err(|e| RunError(format!("invalid project root: {e}")))?,
            )
            .map_err(|_| RunError(format!("module `{}` escapes project root", path.display())))?
            .to_path_buf();
        if active.contains(&relative) {
            let mut chain = active
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>();
            chain.push(relative.display().to_string());
            return Err(RunError(format!("portal cycle: {}", chain.join(" -> "))));
        }
        if done.contains(&relative) {
            return Ok(());
        }
        let source = std::fs::read_to_string(&canonical)
            .map_err(|e| RunError(format!("cannot read module `{}`: {e}", relative.display())))?;
        let plan = ufp_syntax::parse_plan(&source)
            .map_err(|e| RunError(format!("parse error in `{}`: {}", relative.display(), e[0])))?;
        active.push(relative.clone());
        for item in &plan.items {
            if let Item::Portal { path, .. } = item {
                visit(root, Path::new(path), active, done, modules, selected)?;
            }
        }
        active.pop();
        let mut hash = 0xcbf29ce484222325u64;
        for byte in source.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let (mut inputs, mut outputs) = (Vec::new(), Vec::new());
        for item in &plan.items {
            match item {
                Item::Input { name, .. } => inputs.push(name.clone()),
                Item::Output { name, .. } => outputs.push(name.clone()),
                _ => {}
            }
        }
        let signatures = procedure_signatures(&source, &relative)?;
        let mut portal_links = Vec::new();
        for item in &plan.items {
            if let Item::Portal {
                name,
                path: target,
                alias,
                ..
            } = item
            {
                let target_path = root
                    .join(target)
                    .canonicalize()
                    .map_err(|e| RunError(format!("cannot resolve module `{target}`: {e}")))?
                    .strip_prefix(
                        root.canonicalize()
                            .map_err(|e| RunError(format!("invalid project root: {e}")))?,
                    )
                    .map_err(|_| RunError(format!("module `{target}` escapes project root")))?
                    .to_path_buf();
                let target_export = alias.clone().unwrap_or_else(|| "main".into());
                let exports = selected
                    .get(&target_path)
                    .ok_or_else(|| RunError(format!("portal target `{target}` was not loaded")))?;
                let (pin_inputs, pin_outputs, input_types, output_types) =
                    exports.get(&target_export).cloned().ok_or_else(|| {
                        RunError(format!(
                            "module `{target}` does not export procedure `{target_export}`"
                        ))
                    })?;
                portal_links.push(ufp_ir::ModulePortal {
                    node: name.clone(),
                    target: target_path.display().to_string(),
                    export: target_export,
                    inputs: pin_inputs,
                    outputs: pin_outputs,
                    input_types,
                    output_types,
                });
            }
        }
        let ast = ufp_syntax::parse_ast(&source)
            .map_err(|e| RunError(format!("parse error in `{}`: {}", relative.display(), e[0])))?;
        let mut export_graphs = BTreeMap::new();
        for export_name in signatures.keys() {
            if let Some(graph) = lower_procedure_body(&source, &ast, &portal_links, export_name)? {
                export_graphs.insert(export_name.clone(), graph);
            }
        }
        let graph = if !export_graphs.is_empty() {
            export_graphs
                .get("main")
                .or_else(|| export_graphs.values().next())
                .cloned()
                .unwrap()
        } else {
            ufp_ir::lower_plan(&plan).map_err(|e| {
                RunError(format!(
                    "lower error in `{}`: {}",
                    relative.display(),
                    e[0].message
                ))
            })?
        };
        let mut exports = signatures;
        if exports.is_empty() {
            exports.insert(
                "main".into(),
                (
                    inputs.clone(),
                    outputs.clone(),
                    vec![ufp_types::Type::String; inputs.len()],
                    vec![ufp_types::Type::String; outputs.len()],
                ),
            );
        }
        selected.insert(relative.clone(), exports);
        let mut graph = graph;
        for link in &portal_links {
            if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == link.node) {
                node.inputs = link
                    .inputs
                    .iter()
                    .zip(&link.input_types)
                    .map(|(name, ty)| ufp_ir::Port {
                        name: name.clone(),
                        ty: ty.clone(),
                        required: true,
                        variadic: false,
                    })
                    .collect();
                node.outputs = link
                    .outputs
                    .iter()
                    .zip(&link.output_types)
                    .map(|(name, ty)| ufp_ir::Port {
                        name: name.clone(),
                        ty: ty.clone(),
                        required: false,
                        variadic: false,
                    })
                    .collect();
            }
        }
        graph
            .metadata
            .insert("module.path".into(), relative.display().to_string());
        graph
            .metadata
            .insert("module.hash".into(), format!("{hash:016x}"));
        graph
            .metadata
            .insert("module.inputs".into(), inputs.join(","));
        graph
            .metadata
            .insert("module.outputs".into(), outputs.join(","));
        modules.push(ResolvedModule {
            path: relative.clone(),
            content_hash: format!("{hash:016x}"),
            inputs,
            outputs,
            metadata: graph.metadata.clone(),
            graph,
            exports: export_graphs,
            portals: portal_links,
        });
        done.insert(relative);
        Ok(())
    }
    let mut modules = Vec::new();
    let mut selected = BTreeMap::new();
    visit(
        project_root,
        entry,
        &mut Vec::new(),
        &mut BTreeSet::new(),
        &mut modules,
        &mut selected,
    )?;
    Ok(modules)
}

/// Resolve the entry module into an explicit IR module graph, preserving each
/// portal's selected export and the selected procedure's pin names.
pub fn resolve_module_graph(
    project_root: &Path,
    entry: &Path,
) -> Result<ResolvedModuleGraph, RunError> {
    let modules = resolve_portals(project_root, entry)?;
    let root = project_root
        .canonicalize()
        .map_err(|e| RunError(format!("invalid project root: {e}")))?;
    let entry = root
        .join(entry)
        .canonicalize()
        .map_err(|e| RunError(format!("cannot resolve entry `{}`: {e}", entry.display())))?
        .strip_prefix(&root)
        .map_err(|_| RunError("entry escapes project root".into()))?
        .display()
        .to_string();
    let ir = ufp_ir::ModuleGraph {
        entry: entry.clone(),
        modules: modules
            .iter()
            .map(|module| ufp_ir::LinkedModule {
                id: module.path.display().to_string(),
                graph: module.graph.clone(),
                exports: module.exports.clone(),
                portals: module.portals.clone(),
            })
            .collect(),
    };
    Ok(ResolvedModuleGraph {
        entry: PathBuf::from(entry),
        modules,
        ir,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeEvent {
    RunStarted,
    NodeStarted(String),
    NodeFinished(String),
    OutputUpdated { name: String, text: String },
    RunFinished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub outputs: BTreeMap<String, String>,
    pub events: Vec<RuntimeEvent>,
    /// Number of omitted intermediate events. Always zero in the M0 slice.
    pub dropped_events: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError(pub String);

/// Values supported by the deliberately small control-flow IR interpreter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScalarValue {
    Bool(bool),
    Int(i64),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRunResult {
    pub values: BTreeMap<String, ScalarValue>,
    pub events: Vec<String>,
}

/// Execute the supported scalar subset of lowered procedure control flow:
/// integer/bool/string literals, named scalar inputs, `.` in folds, unary `-`
/// and `not`, integer arithmetic/comparisons, boolean `and`/`or`, and equality.
/// `fuel` overrides each fold's metadata budget; `None` uses its recorded
/// default. Calls, floats, structured values, and operation/portal execution
/// produce an explicit `RunError`.
pub fn execute_control_graph(
    graph: &ufp_ir::Graph,
    inputs: &BTreeMap<String, ScalarValue>,
    fuel: Option<usize>,
) -> Result<ControlRunResult, RunError> {
    let mut values = inputs.clone();
    let mut events = Vec::new();
    execute_graph(graph, &mut values, &mut events, fuel)?;
    Ok(ControlRunResult { values, events })
}

fn execute_graph(
    graph: &ufp_ir::Graph,
    values: &mut BTreeMap<String, ScalarValue>,
    events: &mut Vec<String>,
    fuel_override: Option<usize>,
) -> Result<(), RunError> {
    for node in &graph.nodes {
        if node.name == "if-region" {
            let region = node
                .region
                .as_ref()
                .ok_or_else(|| RunError("if region is missing nested graph".into()))?;
            let branches = region
                .metadata
                .get(&format!("{}.branches", node.id))
                .cloned()
                .unwrap_or_default();
            let mut selected = false;
            for branch_id in branches.split(',').filter(|s| !s.is_empty()) {
                let branch = region
                    .nodes
                    .iter()
                    .find(|n| n.id == branch_id)
                    .ok_or_else(|| RunError(format!("branch region `{branch_id}` is missing")))?;
                if branch.name == "else" {
                    if let Some(body) = &branch.region {
                        execute_graph(body, values, events, fuel_override)?;
                    }
                    events.push(format!("branch:{branch_id}"));
                    selected = true;
                    break;
                }
                let predicate_id = branch_id.replace("_branch_", "_predicate_");
                let expression = region
                    .metadata
                    .get(&format!("{predicate_id}.expression"))
                    .ok_or_else(|| {
                        RunError(format!("predicate `{predicate_id}` has no expression"))
                    })?;
                match eval_source(expression, values, None)? {
                    ScalarValue::Bool(true) => {
                        if let Some(body) = &branch.region {
                            execute_graph(body, values, events, fuel_override)?;
                        }
                        events.push(format!("branch:{branch_id}"));
                        selected = true;
                        break;
                    }
                    ScalarValue::Bool(false) => {}
                    _ => {
                        return Err(RunError(format!(
                            "predicate `{predicate_id}` did not evaluate to bool"
                        )))
                    }
                }
            }
            if !selected {
                events.push(format!("branch:{}:none", node.id));
            }
        } else if node.kind == ufp_ir::NodeKind::Fold {
            execute_fold(graph, node, values, events, fuel_override)?;
        } else if let Some(target) = graph.metadata.get(&format!("{}.target", node.id)) {
            if let Some(expression) = graph.metadata.get(&format!("{}.expression", node.id)) {
                let value = eval_source(expression, values, None)?;
                values.insert(target.clone(), value);
            }
        } else if node.kind == ufp_ir::NodeKind::Operation || node.kind == ufp_ir::NodeKind::Portal
        {
            return Err(RunError(format!(
                "scalar control interpreter cannot execute operation/portal node `{}`",
                node.id
            )));
        }
    }
    Ok(())
}

fn execute_fold(
    graph: &ufp_ir::Graph,
    node: &ufp_ir::Node,
    values: &mut BTreeMap<String, ScalarValue>,
    events: &mut Vec<String>,
    fuel_override: Option<usize>,
) -> Result<(), RunError> {
    let metadata = node
        .region
        .as_ref()
        .ok_or_else(|| RunError(format!("fold `{}` has no region", node.id)))?;
    let initial = graph
        .metadata
        .get(&format!("{}.initial", node.id))
        .ok_or_else(|| RunError(format!("fold `{}` has no initial state", node.id)))?;
    let target = graph
        .metadata
        .get(&format!("{}.target", node.id))
        .ok_or_else(|| RunError(format!("fold `{}` has no assignment target", node.id)))?;
    let step = metadata
        .metadata
        .get("fold.value_expression")
        .ok_or_else(|| RunError(format!("fold `{}` has no step expression", node.id)))?;
    let condition = metadata
        .metadata
        .get("fold.condition_expression")
        .ok_or_else(|| RunError(format!("fold `{}` has no condition expression", node.id)))?;
    let default_fuel = graph
        .metadata
        .get(&format!("{}.fuel", node.id))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10_000);
    let limit = fuel_override.unwrap_or(default_fuel);
    let mut state = eval_source(initial, values, None)?;
    for iteration in 1..=limit {
        state = eval_source(step, values, Some(&state))?;
        match eval_source(condition, values, Some(&state))? {
            ScalarValue::Bool(true) => {
                values.insert(target.clone(), state);
                events.push(format!("fold:{}:{iteration}", node.id));
                return Ok(());
            }
            ScalarValue::Bool(false) => {}
            _ => {
                return Err(RunError(format!(
                    "fold `{}` condition did not evaluate to bool",
                    node.id
                )))
            }
        }
    }
    Err(RunError(format!(
        "fold `{}` exhausted fuel {limit}",
        node.id
    )))
}

fn eval_source(
    source: &str,
    values: &BTreeMap<String, ScalarValue>,
    current: Option<&ScalarValue>,
) -> Result<ScalarValue, RunError> {
    let expression = ufp_syntax::parse_expression(source).map_err(|e| {
        RunError(format!(
            "unsupported expression `{source}`: {}",
            e[0].message
        ))
    })?;
    eval_expr(&expression, values, current)
}

fn eval_expr(
    expr: &ufp_syntax::Expr,
    values: &BTreeMap<String, ScalarValue>,
    current: Option<&ScalarValue>,
) -> Result<ScalarValue, RunError> {
    use ufp_syntax::ExprKind;
    match &expr.kind {
        ExprKind::Literal(value) if value == "true" => Ok(ScalarValue::Bool(true)),
        ExprKind::Literal(value) if value == "false" => Ok(ScalarValue::Bool(false)),
        ExprKind::Literal(value) => {
            if let Ok(v) = value.parse::<i64>() {
                return Ok(ScalarValue::Int(v));
            }
            if value == "null" {
                return Err(RunError(
                    "null is outside the scalar interpreter subset".into(),
                ));
            }
            Ok(ScalarValue::String(value.clone()))
        }
        ExprKind::Identifier(name) => values
            .get(name)
            .cloned()
            .ok_or_else(|| RunError(format!("unknown scalar value `{name}`"))),
        ExprKind::Current => current
            .cloned()
            .ok_or_else(|| RunError("`.` is only supported inside a fold expression".into())),
        ExprKind::Condition(value) => eval_expr(value, values, current),
        ExprKind::Unary { op, value } => {
            let value = eval_expr(value, values, current)?;
            match (op.as_str(), value) {
                ("not", ScalarValue::Bool(v)) => Ok(ScalarValue::Bool(!v)),
                ("-", ScalarValue::Int(v)) => v
                    .checked_neg()
                    .map(ScalarValue::Int)
                    .ok_or_else(|| RunError("integer overflow".into())),
                _ => Err(RunError(format!(
                    "unsupported unary operation `{op}` for this scalar value"
                ))),
            }
        }
        ExprKind::Binary { op, left, right } => {
            let left = eval_expr(left, values, current)?;
            let right = eval_expr(right, values, current)?;
            match (op.as_str(), left, right) {
                ("+", ScalarValue::Int(a), ScalarValue::Int(b)) => a
                    .checked_add(b)
                    .map(ScalarValue::Int)
                    .ok_or_else(|| RunError("integer overflow".into())),
                ("-", ScalarValue::Int(a), ScalarValue::Int(b)) => a
                    .checked_sub(b)
                    .map(ScalarValue::Int)
                    .ok_or_else(|| RunError("integer overflow".into())),
                ("*", ScalarValue::Int(a), ScalarValue::Int(b)) => a
                    .checked_mul(b)
                    .map(ScalarValue::Int)
                    .ok_or_else(|| RunError("integer overflow".into())),
                ("/", ScalarValue::Int(_), ScalarValue::Int(0))
                | ("%", ScalarValue::Int(_), ScalarValue::Int(0)) => {
                    Err(RunError("division by zero".into()))
                }
                ("/", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Int(a / b)),
                ("%", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Int(a % b)),
                ("==", a, b) => Ok(ScalarValue::Bool(a == b)),
                ("!=", a, b) => Ok(ScalarValue::Bool(a != b)),
                ("<", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Bool(a < b)),
                ("<=", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Bool(a <= b)),
                (">", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Bool(a > b)),
                (">=", ScalarValue::Int(a), ScalarValue::Int(b)) => Ok(ScalarValue::Bool(a >= b)),
                ("and", ScalarValue::Bool(a), ScalarValue::Bool(b)) => {
                    Ok(ScalarValue::Bool(a && b))
                }
                ("or", ScalarValue::Bool(a), ScalarValue::Bool(b)) => Ok(ScalarValue::Bool(a || b)),
                _ => Err(RunError(format!("unsupported scalar operands for `{op}`"))),
            }
        }
        _ => Err(RunError(
            "expression is outside the scalar interpreter subset".into(),
        )),
    }
}

/// Run the direct `input.text -> output.text` graph.
///
/// `overrides` are transient session values: callers may supply them without
/// changing the source literal. Any unsupported node/procedure on the path is a
/// deliberate error rather than a silently ignored computation.
pub fn run_text_plan(
    plan: &Plan,
    overrides: &BTreeMap<String, String>,
) -> Result<RunResult, RunError> {
    // Lower and validate first so runtime behavior follows the same graph
    // invariants as other IR consumers, while retaining terminal literal values
    // from the shallow source plan for this deliberately small executor.
    ufp_ir::lower_plan(plan).map_err(|errors| {
        let error = &errors[0];
        RunError(format!("{}: {}", error.code, error.message))
    })?;
    if let Some(error) = ufp_types::check_text_terminals(plan).into_iter().next() {
        return Err(RunError(format!("{}: {}", error.code, error.message)));
    }
    let mut inputs = BTreeMap::new();
    let mut outputs = BTreeSet::new();
    let mut pipes: Vec<(&Endpoint, &Endpoint)> = Vec::new();
    let mut unsupported = Vec::new();

    for item in &plan.items {
        match item {
            Item::Input { name, value, .. } => {
                if inputs.insert(name.clone(), value.clone()).is_some() {
                    return Err(RunError(format!("duplicate input terminal `{name}`")));
                }
            }
            Item::Output { name, .. } => {
                if !outputs.insert(name.clone()) {
                    return Err(RunError(format!("duplicate output terminal `{name}`")));
                }
            }
            Item::Pipe { from, to, .. } => pipes.push((from, to)),
            Item::Other { keyword, .. } => unsupported.push(keyword.clone()),
            Item::Text { .. } | Item::Portal { .. } => {}
        }
    }
    if !unsupported.is_empty() {
        return Err(RunError(format!(
            "M0 runtime cannot execute {} yet; parse/validate it first",
            unsupported.join(", ")
        )));
    }
    for (name, value) in overrides {
        if !inputs.contains_key(name) {
            return Err(RunError(format!("unknown input terminal `{name}`")));
        }
        inputs.insert(name.clone(), value.clone());
    }

    let mut events = vec![RuntimeEvent::RunStarted];
    let mut result = BTreeMap::new();
    for output in outputs {
        let incoming: Vec<_> = pipes
            .iter()
            .filter(|(_, to)| to.node == output && to.port == "text")
            .collect();
        if incoming.len() != 1 {
            return Err(RunError(format!(
                "output `{output}` must have exactly one incoming `.text` pipe"
            )));
        }
        let (from, _) = incoming[0];
        if from.port != "text" {
            return Err(RunError(format!(
                "terminal pipe from `{}` must use `.text`",
                from.node
            )));
        }
        let Some(value) = inputs.get(&from.node) else {
            return Err(RunError(format!(
                "M0 runtime only supports input.text sources; `{}` is not an input",
                from.node
            )));
        };
        events.push(RuntimeEvent::NodeStarted(from.node.clone()));
        events.push(RuntimeEvent::NodeFinished(from.node.clone()));
        events.push(RuntimeEvent::NodeStarted(output.clone()));
        events.push(RuntimeEvent::OutputUpdated {
            name: output.clone(),
            text: value.clone(),
        });
        events.push(RuntimeEvent::NodeFinished(output.clone()));
        result.insert(output, value.clone());
    }
    events.push(RuntimeEvent::RunFinished);
    Ok(RunResult {
        outputs: result,
        events,
        dropped_events: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ufp_syntax::parse_plan;

    #[test]
    fn runs_text_pipe_with_transient_override() {
        let plan =
            parse_plan("input prompt = \"source\"; output reply; pipe prompt.text -> reply.text;")
                .unwrap();
        let mut overrides = BTreeMap::new();
        overrides.insert("prompt".into(), "session".into());
        let result = run_text_plan(&plan, &overrides).unwrap();
        assert_eq!(result.outputs["reply"], "session");
        assert!(matches!(
            result.events.first(),
            Some(RuntimeEvent::RunStarted)
        ));
    }

    #[test]
    fn resolves_portal_signature_and_compact_metadata() {
        let root = std::env::temp_dir().join(format!("ufp-portal-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.ufp"), "portal child = \"child.ufp\";").unwrap();
        std::fs::write(
            root.join("child.ufp"),
            "input prompt = \"x\"; output reply; pipe prompt.text -> reply.text;",
        )
        .unwrap();
        let modules = resolve_portals(&root, Path::new("main.ufp")).unwrap();
        let child = modules
            .iter()
            .find(|m| m.path == Path::new("child.ufp"))
            .unwrap();
        assert_eq!(child.inputs, ["prompt"]);
        assert_eq!(child.outputs, ["reply"]);
        assert_eq!(child.content_hash.len(), 16);
        assert_eq!(child.metadata["module.hash"], child.content_hash);
        assert_eq!(child.metadata["module.inputs"], "prompt");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_portal_cycles() {
        let root = std::env::temp_dir().join(format!("ufp-portal-cycle-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.ufp"), "portal b = \"b.ufp\";").unwrap();
        std::fs::write(root.join("b.ufp"), "portal a = \"a.ufp\";").unwrap();
        let error = resolve_portals(&root, Path::new("a.ufp")).unwrap_err();
        assert!(error.0.contains("portal cycle"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_selected_procedure_pin_signature_into_linked_ir() {
        let root = std::env::temp_dir().join(format!("ufp-portal-export-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            "portal child = \"child.ufp\" as chosen;",
        )
        .unwrap();
        std::fs::write(
            root.join("child.ufp"),
            "procedure main(input string ignored) { } procedure chosen(input string prompt, output string answer) { pipe prompt.out -> answer.in; }",
        ).unwrap();
        let graph = resolve_module_graph(&root, Path::new("root.ufp")).unwrap();
        let root_module = graph
            .ir
            .modules
            .iter()
            .find(|m| m.id == "root.ufp")
            .unwrap();
        let link = &root_module.portals[0];
        assert_eq!(link.export, "chosen");
        assert_eq!(link.inputs, ["prompt"]);
        assert_eq!(link.outputs, ["answer"]);
        assert_eq!(link.input_types, [ufp_types::Type::String]);
        assert_eq!(link.output_types, [ufp_types::Type::String]);
        let child = graph
            .ir
            .modules
            .iter()
            .find(|m| m.id == "child.ufp")
            .unwrap();
        assert_eq!(child.exports["chosen"].edges.len(), 1);
        assert_eq!(graph.ir.entry, "root.ufp");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lowers_procedure_nodes_and_pipes_with_typed_portal_boundary() {
        let root = std::env::temp_dir().join(format!("ufp-portal-lower-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            r#"
            portal child = "child.ufp";
            procedure main(input string prompt, output string answer) {
                node call : child(prompt = prompt);
                pipe prompt.out -> call.prompt;
                pipe call.answer -> answer.in;
            }
        "#,
        )
        .unwrap();
        std::fs::write(
            root.join("child.ufp"),
            "procedure main(input string prompt, output string answer) { pipe prompt.out -> answer.in; }",
        )
        .unwrap();
        let result = resolve_module_graph(&root, Path::new("root.ufp")).unwrap();
        let module = result
            .ir
            .modules
            .iter()
            .find(|m| m.id == "root.ufp")
            .unwrap();
        assert_eq!(module.graph.edges.len(), 2);
        let call = module.graph.nodes.iter().find(|n| n.id == "call").unwrap();
        assert_eq!(call.kind, ufp_ir::NodeKind::Portal);
        assert_eq!(call.inputs[0].ty, ufp_types::Type::String);
        assert_eq!(call.outputs[0].ty, ufp_types::Type::String);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsupported_procedure_body_instead_of_empty_graph() {
        let root =
            std::env::temp_dir().join(format!("ufp-portal-unsupported-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            "procedure main(input string prompt) { return prompt; }",
        )
        .unwrap();
        let error = resolve_module_graph(&root, Path::new("root.ufp")).unwrap_err();
        assert!(error.0.contains("unsupported"), "{error:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lowers_general_operation_nodes_and_expression_metadata() {
        let root = std::env::temp_dir().join(format!("ufp-portal-op-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            r#"
            procedure main(input string prompt, output string answer) {
                node scale : math.scale(value = prompt, factor = 2);
                pipe prompt.out -> scale.value;
                pipe scale.out -> answer.in;
            }
        "#,
        )
        .unwrap();
        let graph = resolve_module_graph(&root, Path::new("root.ufp")).unwrap();
        let module = graph
            .ir
            .modules
            .iter()
            .find(|m| m.id == "root.ufp")
            .unwrap();
        let scale = module.graph.nodes.iter().find(|n| n.id == "scale").unwrap();
        assert_eq!(scale.kind, ufp_ir::NodeKind::Operation);
        assert_eq!(
            scale
                .inputs
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["value", "factor"]
        );
        assert!(module.graph.metadata["node.scale.callee"].contains("math.scale"));
        assert!(module.graph.metadata.contains_key("node.scale.arg.factor"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lowers_if_elif_else_into_predicate_branch_and_merge_regions() {
        let root = std::env::temp_dir().join(format!("ufp-portal-if-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            "procedure main(input bool flag) { if (flag) { node branch_op : math.identity(value = 1); } elif (not flag) { } else { } }",
        )
        .unwrap();
        let result = resolve_module_graph(&root, Path::new("root.ufp")).unwrap();
        let module = result
            .ir
            .modules
            .iter()
            .find(|m| m.id == "root.ufp")
            .unwrap();
        let control = module
            .graph
            .nodes
            .iter()
            .find(|n| n.name == "if-region")
            .unwrap()
            .region
            .as_ref()
            .unwrap();
        assert!(control.nodes.iter().any(|n| n.name == "if"));
        assert!(control.nodes.iter().any(|n| n.name == "elif"));
        assert!(control.nodes.iter().any(|n| n.name == "merge"));
        assert!(control.nodes.iter().any(|n| n.name == "else"));
        assert_eq!(control.edges.len(), 2);
        let branch = control.nodes.iter().find(|n| n.name == "branch").unwrap();
        assert!(branch
            .region
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|n| n.id == "branch_op"));
        assert!(control
            .metadata
            .values()
            .any(|v| { v == "all_previous_predicates_false_and_current_true" }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lowers_run_until_to_fold_region_with_feedback_metadata() {
        let root = std::env::temp_dir().join(format!("ufp-portal-fold-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("root.ufp"),
            r#"
            procedure main(input string prompt) {
                let result = prompt run(prompt until (condition(true)));
            }
        "#,
        )
        .unwrap();
        let result = resolve_module_graph(&root, Path::new("root.ufp")).unwrap();
        let module = result
            .ir
            .modules
            .iter()
            .find(|m| m.id == "root.ufp")
            .unwrap();
        let fold = module
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == ufp_ir::NodeKind::Fold)
            .unwrap();
        assert!(fold.fold_feedback);
        assert!(fold.region.is_some());
        assert!(module
            .graph
            .edges
            .iter()
            .any(|e| e.kind == ufp_ir::EdgeKind::FoldFeedback));
        assert_eq!(
            module.graph.metadata[&format!("{}.feedback_gate", fold.id)],
            "condition_unmet"
        );
        assert_eq!(module.graph.metadata[&format!("{}.fuel", fold.id)], "10000");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scalar_interpreter_selects_true_and_false_branches_and_merges_values() {
        let root =
            std::env::temp_dir().join(format!("ufp-control-branches-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("main.ufp"),
            r#"
            procedure main(input bool flag) {
                if (flag) { let answer = 1; } else { let answer = 2; }
            }
        "#,
        )
        .unwrap();
        let graph = resolve_module_graph(&root, Path::new("main.ufp")).unwrap();
        let mut inputs = BTreeMap::new();
        inputs.insert("flag".into(), ScalarValue::Bool(true));
        let yes = execute_control_graph(&graph.ir.modules[0].graph, &inputs, None).unwrap();
        assert_eq!(yes.values["answer"], ScalarValue::Int(1));
        inputs.insert("flag".into(), ScalarValue::Bool(false));
        let no = execute_control_graph(&graph.ir.modules[0].graph, &inputs, None).unwrap();
        assert_eq!(no.values["answer"], ScalarValue::Int(2));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scalar_interpreter_runs_until_condition_or_reports_fuel_exhaustion() {
        let root = std::env::temp_dir().join(format!("ufp-control-fold-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("main.ufp"),
            r#"
            procedure main() { let count = 0 run(. + 1 until (condition(. >= 3))); }
        "#,
        )
        .unwrap();
        let graph = resolve_module_graph(&root, Path::new("main.ufp")).unwrap();
        let module_graph = &graph.ir.modules[0].graph;
        let result = execute_control_graph(module_graph, &BTreeMap::new(), None).unwrap();
        assert_eq!(result.values["count"], ScalarValue::Int(3));
        assert!(
            execute_control_graph(module_graph, &BTreeMap::new(), Some(2))
                .unwrap_err()
                .0
                .contains("exhausted fuel 2")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
