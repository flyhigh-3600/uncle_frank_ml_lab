//! Graph model, topological ordering, and shape inference.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ops;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub params: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub from_port: String,
    pub to: String,
    pub to_port: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct Graph {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Validation {
    pub ok: bool,
    pub order: Vec<String>,
    /// (node id, output port) -> shape
    pub shapes: HashMap<String, Vec<usize>>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub losses: Vec<String>,
    pub cursed: Vec<String>,
    pub params_used: HashMap<String, HashMap<String, Value>>,
}

impl Validation {
    pub fn shape_of(&self, node: &str, port: &str) -> Option<&Vec<usize>> {
        self.shapes.get(&format!("{node}:{port}"))
    }
}

// -------- param helpers --------

pub fn p_f64(params: &Map<String, Value>, key: &str, default: f64) -> f64 {
    params.get(key).and_then(Value::as_f64).unwrap_or(default)
}

pub fn p_f32(params: &Map<String, Value>, key: &str, default: f32) -> f32 {
    p_f64(params, key, default as f64) as f32
}

pub fn p_usize(params: &Map<String, Value>, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
}

pub fn p_str<'a>(params: &'a Map<String, Value>, key: &str, default: &'a str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or(default)
}

pub fn clamp(mut v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() {
        return lo;
    }
    v = v.max(lo);
    v.min(hi)
}

// -------- graph analysis --------

pub fn validate(graph: &Graph) -> Validation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut shapes: HashMap<String, Vec<usize>> = HashMap::new();

    let mut by_id: HashMap<&str, &Node> = HashMap::new();
    for n in &graph.nodes {
        if by_id.insert(n.id.as_str(), n).is_some() {
            errors.push(format!("duplicate node id '{}'", n.id));
        }
        if ops::find(&n.kind).is_none() {
            errors.push(format!("node '{}' has unknown kind '{}'", n.id, n.kind));
        }
    }

    // incoming edge map: (node, port) -> edge
    let mut incoming: HashMap<(String, String), &Edge> = HashMap::new();
    for e in &graph.edges {
        let Some(from) = by_id.get(e.from.as_str()) else {
            errors.push(format!("edge '{}' starts at missing node '{}'", e.id, e.from));
            continue;
        };
        let Some(to) = by_id.get(e.to.as_str()) else {
            errors.push(format!("edge '{}' ends at missing node '{}'", e.id, e.to));
            continue;
        };
        let Some(spec) = ops::find(&from.kind) else { continue };
        if !spec.outputs.iter().any(|p| p.name == e.from_port) {
            errors.push(format!(
                "edge '{}': '{}' has no output port '{}'",
                e.id, from.kind, e.from_port
            ));
        }
        let Some(to_spec) = ops::find(&to.kind) else { continue };
        if !to_spec.inputs.iter().any(|p| p.name == e.to_port) {
            errors.push(format!(
                "edge '{}': '{}' has no input port '{}'",
                e.id, to.kind, e.to_port
            ));
        }
        if incoming
            .insert((e.to.clone(), e.to_port.clone()), e)
            .is_some()
        {
            errors.push(format!(
                "input '{}.{}' has more than one incoming edge",
                e.to, e.to_port
            ));
        }
    }

    // Kahn topological sort.
    let mut indeg: HashMap<&str, usize> = graph.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &graph.edges {
        if by_id.contains_key(e.from.as_str()) && by_id.contains_key(e.to.as_str()) {
            *indeg.entry(e.to.as_str()).or_insert(0) += 1;
            adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
        }
    }
    let mut ready: Vec<&str> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort_unstable();
    let mut order: Vec<String> = Vec::new();
    while let Some(id) = ready.pop() {
        order.push(id.to_string());
        if let Some(nexts) = adj.get(id) {
            for next in nexts {
                if let Some(d) = indeg.get_mut(next) {
                    *d -= 1;
                    if *d == 0 {
                        ready.push(next);
                    }
                }
            }
        }
    }
    if order.len() != graph.nodes.len() {
        errors.push("cycle detected — the graph eats its own tail".into());
        return Validation {
            ok: false,
            order,
            shapes,
            errors,
            warnings,
            losses: vec![],
            cursed: vec![],
            params_used: HashMap::new(),
        };
    }

    // Shape inference along the order.
    for id in &order {
        let node = match by_id.get(id.as_str()) {
            Some(n) => *n,
            None => continue,
        };
        let Some(spec) = ops::find(&node.kind) else { continue };
        let effective = effective_params(&node.kind, &node.params);

        let mut input_shapes: HashMap<&str, Vec<usize>> = HashMap::new();
        for p in &spec.inputs {
            let Some(edge) = incoming.get(&(node.id.clone(), p.name.to_string())) else {
                errors.push(format!("node '{}' ({}) is missing input '{}'", node.id, node.kind, p.label));
                continue;
            };
            let key = format!("{}:{}", edge.from, edge.from_port);
            if let Some(shape) = shapes.get(&key) {
                input_shapes.insert(p.name, shape.clone());
            }
        }
        if input_shapes.len() != spec.inputs.len() {
            continue;
        }
        match infer(&node.kind, &effective, &input_shapes) {
            Ok(outs) => {
                for (port_name, shape) in outs {
                    shapes.insert(format!("{}:{}", node.id, port_name), shape);
                }
            }
            Err(e) => errors.push(format!("node '{}' ({}): {}", node.id, node.kind, e)),
        }
    }

    let losses: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| ops::find(&n.kind).map(|s| s.category) == Some("loss"))
        .map(|n| n.id.clone())
        .collect();
    let cursed: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| ops::find(&n.kind).map(|s| s.cursed) == Some(true))
        .map(|n| n.id.clone())
        .collect();

    if losses.is_empty() {
        warnings.push("no loss node — the model has nothing to suffer for".into());
    }
    if losses.len() > 1 {
        warnings.push(format!(
            "{} loss nodes found; run uses only '{}'",
            losses.len(),
            losses[0]
        ));
    }
    let has_dataset = graph.nodes.iter().any(|n| n.kind == "dataset");
    if !has_dataset {
        warnings.push("no dataset node — nothing to train on".into());
    }
    let mut seen_data: HashSet<&str> = HashSet::new();
    for n in &graph.nodes {
        if n.kind == "dataset" && !seen_data.insert(n.id.as_str()) {
            warnings.push("multiple datasets; shapes will disagree".into());
        }
    }

    Validation {
        ok: errors.is_empty(),
        order,
        shapes,
        errors,
        warnings,
        losses,
        cursed,
        params_used: graph
            .nodes
            .iter()
            .map(|n| (n.id.clone(), effective_params(&n.kind, &n.params)))
            .collect(),
    }
}

/// Clamp/normalise params so the runtime and the UI agree on values.
pub fn effective_params(kind: &str, params: &Map<String, Value>) -> HashMap<String, Value> {
    let mut out: HashMap<String, Value> = HashMap::new();
    let Some(spec) = ops::find(kind) else { return out };
    for p in &spec.params {
        let mut v = params.get(p.name).cloned().unwrap_or_else(|| p.default.clone());
        match p.kind {
            "int" => {
                let n = v.as_f64().unwrap_or(0.0);
                let lo = p.min.unwrap_or(f64::MIN);
                let hi = p.max.unwrap_or(f64::MAX);
                v = Value::from(clamp(n.round(), lo, hi) as u64);
            }
            "number" => {
                let n = v.as_f64().unwrap_or(0.0);
                let lo = p.min.unwrap_or(f64::MIN);
                let hi = p.max.unwrap_or(f64::MAX);
                v = Value::from(clamp(n, lo, hi));
            }
            "select" => {
                let s = v.as_str().unwrap_or("").to_string();
                if !p.options.contains(&s.as_str()) {
                    v = p.default.clone();
                }
            }
            _ => {}
        }
        out.insert(p.name.to_string(), v);
    }
    out
}

fn infer(
    kind: &str,
    params: &HashMap<String, Value>,
    inputs: &HashMap<&str, Vec<usize>>,
) -> Result<Vec<(&'static str, Vec<usize>)>, String> {
    let get = |name: &str| -> Result<&Vec<usize>, String> {
        inputs
            .get(name)
            .ok_or_else(|| format!("missing input '{name}'"))
    };
    let usz = |key: &str, default: usize| params.get(key).and_then(Value::as_u64).map(|v| v as usize).unwrap_or(default);

    let require_2d = |t: &Vec<usize>, what: &str| -> Result<(), String> {
        if t.len() != 2 {
            return Err(format!("{what} expects a 2-D tensor, got {:?}", t));
        }
        Ok(())
    };

    Ok(match kind {
        "dataset" => {
            let n = usz("n", 200).clamp(20, 2000);
            vec![("x", vec![n, 2]), ("y", vec![n, 2])]
        }
        "dense" => {
            let x = get("x")?;
            require_2d(x, "dense")?;
            let units = usz("units", 2).clamp(1, 64);
            vec![("y", vec![x[0], units])]
        }
        "relu" | "tanh" | "softmax" | "inspect" => {
            let x = get("x")?;
            require_2d(x, kind)?;
            vec![("y", x.clone())]
        }
        "poison" | "backdoor" => {
            let x = get("x")?;
            let y = get("y")?;
            require_2d(x, kind)?;
            require_2d(y, kind)?;
            if x[0] != y[0] {
                return Err(format!(
                    "{} received {} feature rows but {} label rows",
                    kind, x[0], y[0]
                ));
            }
            vec![("x", x.clone()), ("y", y.clone())]
        }
        "label_flip" => {
            let y = get("y")?;
            require_2d(y, "label_flip")?;
            if y[1] < 2 {
                return Err("label_flip wants at least 2 classes".into());
            }
            vec![("y", y.clone())]
        }
        "ce" => {
            let p = get("pred")?;
            let t = get("target")?;
            if p != t {
                return Err(format!("ce shapes disagree: {:?} vs {:?}", p, t));
            }
            if p.len() != 2 {
                return Err("ce expects 2-D logits".into());
            }
            vec![("loss", vec![1])]
        }
        "mse" => {
            let p = get("pred")?;
            let t = get("target")?;
            if p != t {
                return Err(format!("mse shapes disagree: {:?} vs {:?}", p, t));
            }
            vec![("loss", vec![1])]
        }
        other => return Err(format!("no shape rule for '{other}'")),
    })
}
