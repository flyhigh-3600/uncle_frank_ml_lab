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
                visit(root, Path::new(path), active, done, modules)?;
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
        let mut graph = ufp_ir::lower_plan(&plan).map_err(|e| {
            RunError(format!(
                "lower error in `{}`: {}",
                relative.display(),
                e[0].message
            ))
        })?;
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
            metadata: graph.metadata,
        });
        done.insert(relative);
        Ok(())
    }
    let mut modules = Vec::new();
    visit(
        project_root,
        entry,
        &mut Vec::new(),
        &mut BTreeSet::new(),
        &mut modules,
    )?;
    Ok(modules)
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
}
