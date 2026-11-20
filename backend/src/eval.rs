//! Forward pass, hand-written backprop, and the training loop.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::graph::{p_f32, p_f64, p_str, p_usize, Graph, Node};
use crate::tensor::{cross_entropy, hash_seed, mse, Rng, Tensor};

#[derive(Clone, Debug, Deserialize)]
pub struct RunRequest {
    pub graph: Graph,
    #[serde(default = "default_steps")]
    pub steps: usize,
    #[serde(default = "default_lr")]
    pub lr: f64,
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_steps() -> usize {
    300
}
fn default_lr() -> f64 {
    0.3
}
fn default_seed() -> u64 {
    1337
}

#[derive(Clone, Debug, Serialize)]
pub struct StepPoint {
    pub step: usize,
    pub loss: f32,
    pub acc: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Probe {
    pub node: String,
    pub port: String,
    pub shape: Vec<usize>,
    pub mean: f32,
    pub min: f32,
    pub max: f32,
    pub nan: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunResult {
    pub ok: bool,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub history: Vec<StepPoint>,
    pub initial_loss: Option<f32>,
    pub initial_acc: Option<f32>,
    pub final_loss: Option<f32>,
    pub final_acc: Option<f32>,
    pub probes: Vec<Probe>,
    pub curse_notes: Vec<String>,
}

impl RunResult {
    fn fail(msg: String) -> Self {
        Self {
            ok: false,
            error: Some(msg),
            warnings: vec![],
            history: vec![],
            initial_loss: None,
            initial_acc: None,
            final_loss: None,
            final_acc: None,
            probes: vec![],
            curse_notes: vec![],
        }
    }
}

type PortKey = (String, String);
type Table = HashMap<PortKey, Tensor>;

/// Missing values become an empty tensor so a half-wired graph fails softly
/// instead of panicking mid-surgery.
fn get_val(t: &Table, k: &PortKey) -> Tensor {
    t.get(k)
        .cloned()
        .unwrap_or_else(|| Tensor::zeros(vec![0]))
}

struct Dense {
    w: Tensor,
    b: Tensor,
}

fn key(node: &str, port: &str) -> PortKey {
    (node.to_string(), port.to_string())
}

pub fn run(req: &RunRequest) -> RunResult {
    let graph = &req.graph;
    let v = crate::graph::validate(graph);
    if !v.ok {
        let mut r = RunResult::fail(v.errors.join("; "));
        r.warnings = v.warnings;
        return r;
    }
    let Some(loss_node) = graph
        .nodes
        .iter()
        .find(|n| crate::ops::find(&n.kind).map(|s| s.category) == Some("loss"))
    else {
        return RunResult::fail("no loss node — nothing to minimise".into());
    };

    let steps = req.steps.min(5000);
    let lr = req.lr.clamp(0.0, 10.0);
    let seed = req.seed;

    // Resolve edges: (to, to_port) -> (from, from_port)
    let mut incoming: HashMap<PortKey, PortKey> = HashMap::new();
    for e in &graph.edges {
        incoming.insert(
            key(&e.to, &e.to_port),
            key(&e.from, &e.from_port),
        );
    }

    // Build dense organs.
    let mut dense: HashMap<String, Dense> = HashMap::new();
    for node in graph.nodes.iter().filter(|n| n.kind == "dense") {
        let in_shape = incoming
            .get(&key(&node.id, "x"))
            .and_then(|k| v.shapes.get(&format!("{}:{}", k.0, k.1)));
        let Some(in_shape) = in_shape else {
            return RunResult::fail(format!("dense '{}' has no resolved input shape", node.id));
        };
        let in_dim = *in_shape.get(1).unwrap_or(&1);
        let units = p_usize(&node.params, "units", 2).clamp(1, 64);
        let init = p_str(&node.params, "init", "he");
        let scale = match init {
            "xavier" => (1.0 / in_dim as f32).sqrt(),
            "tiny" => 1e-4,
            "chaos" => 10.0,
            _ => (2.0 / in_dim as f32).sqrt(),
        };
        let mut rng = Rng::new(hash_seed(&node.id, seed));
        let mut w = vec![0.0f32; in_dim * units];
        for x in w.iter_mut() {
            *x = rng.gauss() * scale;
        }
        dense.insert(
            node.id.clone(),
            Dense {
                w: Tensor::new(vec![in_dim, units], w).unwrap(),
                b: Tensor::new(vec![units], vec![0.0; units]).unwrap(),
            },
        );
    }

    // Data is generated once; the curses then chew on it deterministically.
    let mut out: Table = Table::new();

    let mut history: Vec<StepPoint> = Vec::new();
    let mut initial: Option<(f32, f32)> = None;
    let mut last_probe_table: Table = Table::new();
    let mut exploded = false;

    for step in 0..=steps {
        if let Err(e) = forward_pass(&v.order, &graph.nodes, &incoming, &dense, &mut out, seed) {
            return RunResult::fail(e);
        }
        let loss = out
            .get(&key(&loss_node.id, "loss"))
            .map(|t| t.data[0])
            .unwrap_or(f32::NAN);
        let acc = accuracy(&out, &incoming, loss_node).unwrap_or(0.0);
        if step == 0 {
            initial = Some((loss, acc));
        }
        history.push(StepPoint { step, loss, acc });
        last_probe_table = out.clone();

        if !loss.is_finite() {
            exploded = true;
            break;
        }
        if step == steps {
            break;
        }

        // ---- backward ----
        let mut grads: Table = Table::new();
        grads.insert(key(&loss_node.id, "loss"), Tensor::scalar(1.0));
        let mut updates: HashMap<String, (Tensor, Tensor)> = HashMap::new();

        for id in v.order.iter().rev() {
            let node = graph.nodes.iter().find(|n| &n.id == id).unwrap();
            let spec = crate::ops::find(&node.kind).unwrap();
            for p in &spec.outputs {
                let Some(g) = grads.get(&key(&node.id, p.name)).cloned() else {
                    continue;
                };
                match node.kind.as_str() {
                    "dense" => {
                        let x = match incoming.get(&key(&node.id, "x")) {
                            Some(k) => get_val(&out, k),
                            None => continue,
                        };
                        if x.data.is_empty() {
                            continue;
                        }
                        let d = dense.get(&node.id).unwrap();
                        let (n, cin) = (x.rows(), x.cols());
                        let u = g.cols();
                        let mut dw = vec![0.0f32; cin * u];
                        let mut db = vec![0.0f32; u];
                        let mut dx = vec![0.0f32; n * cin];
                        for i in 0..n {
                            for a in 0..u {
                                let gv = g.at(i, a);
                                db[a] += gv;
                                for c in 0..cin {
                                    dw[c * u + a] += x.at(i, c) * gv;
                                    dx[i * cin + c] += gv * d.w.at(c, a);
                                }
                            }
                        }
                        updates.insert(
                            node.id.clone(),
                            (
                                Tensor::new(vec![cin, u], dw).unwrap(),
                                Tensor::new(vec![u], db).unwrap(),
                            ),
                        );
                        push_grad(
                            &mut grads,
                            incoming.get(&key(&node.id, "x")),
                            Tensor::new(vec![n, cin], dx).unwrap(),
                        );
                    }
                    "relu" => {
                        let y = get_val(&out, &key(&node.id, "y"));
                        let d: Vec<f32> = g
                            .data
                            .iter()
                            .zip(y.data.iter())
                            .map(|(gv, yv)| if *yv > 0.0 { *gv } else { 0.0 })
                            .collect();
                        push_grad(
                            &mut grads,
                            incoming.get(&key(&node.id, "x")),
                            Tensor::new(g.shape.clone(), d).unwrap(),
                        );
                    }
                    "tanh" => {
                        let y = get_val(&out, &key(&node.id, "y"));
                        let d: Vec<f32> = g
                            .data
                            .iter()
                            .zip(y.data.iter())
                            .map(|(gv, yv)| gv * (1.0 - yv * yv))
                            .collect();
                        push_grad(
                            &mut grads,
                            incoming.get(&key(&node.id, "x")),
                            Tensor::new(g.shape.clone(), d).unwrap(),
                        );
                    }
                    "softmax" => {
                        let y = get_val(&out, &key(&node.id, "y"));
                        let (n, c) = (y.rows(), y.cols());
                        let mut d = vec![0.0f32; n * c];
                        for i in 0..n {
                            let dot: f32 = (0..c).map(|j| g.at(i, j) * y.at(i, j)).sum();
                            for j in 0..c {
                                d[i * c + j] = y.at(i, j) * (g.at(i, j) - dot);
                            }
                        }
                        push_grad(
                            &mut grads,
                            incoming.get(&key(&node.id, "x")),
                            Tensor::new(vec![n, c], d).unwrap(),
                        );
                    }
                    "ce" | "mse" => {
                        let pred = match incoming.get(&key(&node.id, "pred")) {
                            Some(k) => get_val(&out, k),
                            None => continue,
                        };
                        let target = match incoming.get(&key(&node.id, "target")) {
                            Some(k) => get_val(&out, k),
                            None => continue,
                        };
                        let (_, dp, dt) = if node.kind == "ce" {
                            match cross_entropy(&pred, &target) {
                                Ok(v) => v,
                                Err(_) => continue,
                            }
                        } else {
                            match mse(&pred, &target) {
                                Ok(v) => v,
                                Err(_) => continue,
                            }
                        };
                        push_grad(&mut grads, incoming.get(&key(&node.id, "pred")), dp);
                        push_grad(&mut grads, incoming.get(&key(&node.id, "target")), dt);
                    }
                    // Curses are data-transparent: gradients pass straight through.
                    "poison" => {
                        push_grad(&mut grads, incoming.get(&key(&node.id, "x")), g.clone());
                        push_grad(&mut grads, incoming.get(&key(&node.id, "y")), g.clone());
                    }
                    "label_flip" => {
                        push_grad(&mut grads, incoming.get(&key(&node.id, "y")), g.clone());
                    }
                    "backdoor" => {
                        push_grad(&mut grads, incoming.get(&key(&node.id, "x")), g.clone());
                        push_grad(&mut grads, incoming.get(&key(&node.id, "y")), g.clone());
                    }
                    "inspect" => {
                        push_grad(&mut grads, incoming.get(&key(&node.id, "x")), g.clone());
                    }
                    // Sources have no upstream.
                    _ => {}
                }
            }
        }

        // ---- update ----
        for (id, (dw, db)) in updates {
            if let Some(d) = dense.get_mut(&id) {
                for i in 0..d.w.data.len() {
                    d.w.data[i] -= lr as f32 * dw.data[i];
                }
                for i in 0..d.b.data.len() {
                    d.b.data[i] -= lr as f32 * db.data[i];
                }
            }
        }
    }

    let mut curse_notes = Vec::new();
    for node in &graph.nodes {
        if let Some(spec) = crate::ops::find(&node.kind) {
            if spec.cursed {
                curse_notes.push(describe_curse(node));
            }
        }
    }

    let mut probes: Vec<Probe> = Vec::new();
    for ((node_id, port), t) in &last_probe_table {
        probes.push(Probe {
            node: node_id.clone(),
            port: port.clone(),
            shape: t.shape.clone(),
            mean: t.mean(),
            min: t.min(),
            max: t.max(),
            nan: t.has_nan(),
        });
    }
    probes.sort_by(|a, b| a.node.cmp(&b.node).then(a.port.cmp(&b.port)));

    let mut warnings = v.warnings.clone();
    if exploded {
        warnings.push("the loss exploded into NaN/∞ — Frank is pleased, the model is not".into());
    }

    RunResult {
        ok: !exploded,
        error: if exploded {
            Some("training diverged".into())
        } else {
            None
        },
        warnings,
        initial_loss: initial.map(|(l, _)| l),
        initial_acc: initial.map(|(_, a)| a),
        final_loss: history.last().map(|p| p.loss),
        final_acc: history.last().map(|p| p.acc),
        history,
        probes,
        curse_notes,
    }
}

fn forward_pass(
    order: &[String],
    nodes: &[Node],
    incoming: &HashMap<PortKey, PortKey>,
    dense: &HashMap<String, Dense>,
    out: &mut Table,
    seed: u64,
) -> Result<(), String> {
    for id in order {
        let node = nodes.iter().find(|n| &n.id == id).unwrap();
        let input = |port: &str| -> Result<Tensor, String> {
            let src = incoming
                .get(&key(&node.id, port))
                .ok_or_else(|| format!("{}.{} is unwired", node.id, port))?;
            out.get(src)
                .cloned()
                .ok_or_else(|| format!("{}.{} has no value yet", src.0, src.1))
        };
        let outputs: Vec<(&str, Tensor)> = match node.kind.as_str() {
            "dataset" => {
                let (x, y) = generate(node, seed);
                vec![("x", x), ("y", y)]
            }
            "dense" => {
                let x = input("x")?;
                let d = dense.get(&node.id).unwrap();
                vec![("y", x.matmul(&d.w)?.add_bias(&d.b)?)]
            }
            "relu" => vec![("y", input("x")?.relu())],
            "tanh" => vec![("y", input("x")?.tanh())],
            "softmax" => vec![("y", input("x")?.softmax()?)],
            "inspect" => vec![("y", input("x")?)],
            "poison" => {
                let x = input("x")?;
                let y = input("y")?;
                let (x, y) = apply_poison(node, x, y, seed)?;
                vec![("x", x), ("y", y)]
            }
            "label_flip" => {
                let y = input("y")?;
                vec![("y", apply_label_flip(node, y, seed)?)]
            }
            "backdoor" => {
                let x = input("x")?;
                let y = input("y")?;
                let (x, y) = apply_backdoor(node, x, y, seed)?;
                vec![("x", x), ("y", y)]
            }
            "ce" => {
                let pred = input("pred")?;
                let target = input("target")?;
                let (loss, _, _) = cross_entropy(&pred, &target)?;
                vec![("loss", Tensor::scalar(loss))]
            }
            "mse" => {
                let pred = input("pred")?;
                let target = input("target")?;
                let (loss, _, _) = mse(&pred, &target)?;
                vec![("loss", Tensor::scalar(loss))]
            }
            other => return Err(format!("no forward rule for '{other}'")),
        };
        for (port, t) in outputs {
            out.insert(key(&node.id, port), t);
        }
    }
    Ok(())
}

fn push_grad(grads: &mut Table, dest: Option<&PortKey>, g: Tensor) {
    let Some(dest) = dest else { return };
    match grads.get_mut(dest) {
        Some(existing) if existing.data.len() == g.data.len() => {
            for (a, b) in existing.data.iter_mut().zip(g.data.iter()) {
                *a += *b;
            }
        }
        Some(_) => {}
        None => {
            grads.insert(dest.clone(), g);
        }
    }
}

fn accuracy(out: &Table, incoming: &HashMap<PortKey, PortKey>, loss: &Node) -> Option<f32> {
    let pred = out.get(incoming.get(&key(&loss.id, "pred"))?)?;
    let target = out.get(incoming.get(&key(&loss.id, "target"))?)?;
    if pred.shape != target.shape || pred.shape.len() != 2 {
        return None;
    }
    let (a, b) = (pred.argmax(), target.argmax());
    let hits = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    Some(hits as f32 / a.len().max(1) as f32)
}

// ---------------- specimens ----------------

fn generate(node: &Node, seed: u64) -> (Tensor, Tensor) {
    let params = &node.params;
    let n = p_usize(params, "n", 200).clamp(20, 2000);
    let noise = p_f64(params, "noise", 0.15) as f32;
    let kind = p_str(params, "dataset", "moons");
    let mut rng = Rng::new(hash_seed(&node.id, seed ^ 0x5eed));

    let mut x = vec![0.0f32; n * 2];
    let mut y = vec![0.0f32; n * 2];
    for i in 0..n {
        let (fx, fy, class) = match kind {
            "blobs" => {
                let c = if i % 2 == 0 { 0 } else { 1 };
                let (cx, cy) = if c == 0 { (1.4f32, 1.4f32) } else { (-1.4, -1.4) };
                (cx + rng.gauss() * (0.55 + noise), cy + rng.gauss() * (0.55 + noise), c)
            }
            "xor" => {
                let a = rng.range(-1.8, 1.8);
                let b = rng.range(-1.8, 1.8);
                let c = if a * b > 0.0 { 0 } else { 1 };
                (a + rng.gauss() * noise, b + rng.gauss() * noise, c)
            }
            _ => {
                let t = std::f32::consts::PI * rng.unit();
                if i % 2 == 0 {
                    (t.cos(), t.sin(), 0)
                } else {
                    (1.0 - t.cos(), 0.5 - t.sin(), 1)
                }
            }
        };
        let jitter = noise * 0.5;
        x[i * 2] = fx + rng.gauss() * jitter;
        x[i * 2 + 1] = fy + rng.gauss() * jitter;
        y[i * 2 + class] = 1.0;
    }
    (
        Tensor::new(vec![n, 2], x).unwrap(),
        Tensor::new(vec![n, 2], y).unwrap(),
    )
}

fn apply_poison(node: &Node, x: Tensor, y: Tensor, seed: u64) -> Result<(Tensor, Tensor), String> {
    let rate = p_f32(&node.params, "rate", 0.2) as f64;
    let scale = p_f32(&node.params, "scale", 2.0);
    let mut rng = Rng::new(hash_seed(&node.id, seed ^ 0xbad_5eed));
    let mut out = x.clone();
    let n = out.rows();
    for i in 0..n {
        if (rng.unit() as f64) < rate {
            for c in 0..out.cols() {
                out.set(i, c, out.at(i, c) + rng.gauss() * scale);
            }
        }
    }
    Ok((out, y))
}

fn apply_label_flip(node: &Node, y: Tensor, seed: u64) -> Result<Tensor, String> {
    let rate = p_f32(&node.params, "rate", 0.1) as f64;
    let mut rng = Rng::new(hash_seed(&node.id, seed ^ 0xf11b));
    let mut out = y.clone();
    let (n, c) = (out.rows(), out.cols());
    for i in 0..n {
        if (rng.unit() as f64) < rate {
            let from = out.argmax()[i];
            let mut to = (from + 1 + (rng.next_u64() as usize % (c - 1))) % c;
            if to == from {
                to = (from + 1) % c;
            }
            for j in 0..c {
                out.set(i, j, if j == to { 1.0 } else { 0.0 });
            }
        }
    }
    Ok(out)
}

fn apply_backdoor(node: &Node, x: Tensor, y: Tensor, seed: u64) -> Result<(Tensor, Tensor), String> {
    let rate = p_f32(&node.params, "rate", 0.1) as f64;
    let tx = p_f32(&node.params, "trigger_x", 2.5);
    let ty = p_f32(&node.params, "trigger_y", 2.5);
    let target: usize = match p_str(&node.params, "target", "1") {
        "0" => 0,
        _ => 1,
    };
    let mut rng = Rng::new(hash_seed(&node.id, seed ^ 0xd00d));
    let mut xx = x.clone();
    let mut yy = y.clone();
    let (n, c) = (yy.rows(), yy.cols());
    let target = target.min(c.saturating_sub(1));
    for i in 0..n {
        if (rng.unit() as f64) < rate {
            if xx.cols() >= 2 {
                xx.set(i, 0, tx);
                xx.set(i, 1, ty);
            }
            for j in 0..c {
                yy.set(i, j, if j == target { 1.0 } else { 0.0 });
            }
        }
    }
    Ok((xx, yy))
}

fn describe_curse(node: &Node) -> String {
    let p: &Map<String, Value> = &node.params;
    match node.kind.as_str() {
        "poison" => format!(
            "{}: {:.0}% of samples rotted (σ={:.1})",
            node.id,
            p_f64(p, "rate", 0.2) * 100.0,
            p_f64(p, "scale", 2.0)
        ),
        "label_flip" => format!(
            "{}: {:.0}% of labels are lies",
            node.id,
            p_f64(p, "rate", 0.1) * 100.0
        ),
        "backdoor" => format!(
            "{}: {:.0}% carry the trigger, all answering {}",
            node.id,
            p_f64(p, "rate", 0.1) * 100.0,
            p_str(p, "target", "1")
        ),
        _ => format!("{}: {}", node.id, node.kind),
    }
}
