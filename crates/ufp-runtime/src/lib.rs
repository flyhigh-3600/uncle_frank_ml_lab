//! Small deterministic reference runtime for the executable M0 text-terminal slice.
//!
//! The full typed graph interpreter lands behind the same API in UF-309. Today
//! this runtime deliberately executes only direct text-terminal pipes; it never
//! pretends that procedures or tensor graphs have been evaluated.

use std::collections::{BTreeMap, BTreeSet};

use ufp_syntax::{Endpoint, Item, Plan};

pub mod loaders;

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
}
