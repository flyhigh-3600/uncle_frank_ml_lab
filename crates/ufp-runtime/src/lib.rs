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
) -> Result<Option<ufp_ir::Graph>, RunError> {
    use ufp_syntax::{ExprKind, SyntaxKind};
    let Some(proc_node) = ast.declarations.iter().find(|n| {
        n.kind == SyntaxKind::Procedure
            && n.children
                .iter()
                .find(|c| c.kind == SyntaxKind::Identifier)
                .and_then(|c| c.token.as_ref())
                .is_some_and(|t| t.text == "main")
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
                let callee = identifiers.get(1).ok_or_else(|| RunError(format!("node `{instance}` is missing a portal target")))?;
                let link = portal_links.iter().find(|p| p.node == *callee)
                    .ok_or_else(|| RunError(format!("node `{instance}` calls unsupported or unknown target `{callee}`")))?;
                let inputs = link.inputs.iter().zip(&link.input_types).map(|(n,t)| ufp_ir::Port { name:n.clone(), ty:t.clone(), required:true, variadic:false }).collect();
                let outputs = link.outputs.iter().zip(&link.output_types).map(|(n,t)| ufp_ir::Port { name:n.clone(), ty:t.clone(), required:false, variadic:false }).collect();
                graph.nodes.push(ufp_ir::Node { id:instance.clone(), name:instance.clone(), kind:ufp_ir::NodeKind::Portal,
                    inputs, outputs, zone:None, span:statement.span, region:None, fold_feedback:false });
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
            _ => return Err(RunError(format!(
                "unsupported {:?} statement in main procedure body; supported forms are node and pipe",
                statement.kind
            ))),
        }
    }
    graph.edges = pipes;
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
        let graph = match lower_procedure_body(&source, &ast, &portal_links)? {
            Some(graph) => graph,
            None => ufp_ir::lower_plan(&plan).map_err(|e| {
                RunError(format!(
                    "lower error in `{}`: {}",
                    relative.display(),
                    e[0].message
                ))
            })?,
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
            "procedure main(input string ignored) { } procedure chosen(input string prompt, output string answer, inout int state) { }",
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
        assert_eq!(link.inputs, ["prompt", "state"]);
        assert_eq!(link.outputs, ["answer", "state"]);
        assert_eq!(
            link.input_types,
            [ufp_types::Type::String, ufp_types::Type::Int(None)]
        );
        assert_eq!(
            link.output_types,
            [ufp_types::Type::String, ufp_types::Type::Int(None)]
        );
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
            "procedure main(input string prompt) { let x = prompt; }",
        )
        .unwrap();
        let error = resolve_module_graph(&root, Path::new("root.ufp")).unwrap_err();
        assert!(error.0.contains("unsupported"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
