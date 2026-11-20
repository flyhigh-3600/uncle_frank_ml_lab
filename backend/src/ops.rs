//! The catalogue of things Frank can bolt onto the table.

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
pub struct PortSpec {
    pub name: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    pub label: &'static str,
    /// "number" | "int" | "select" | "text"
    pub kind: &'static str,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Vec<&'static str>,
    pub hint: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpSpec {
    pub kind: &'static str,
    pub label: &'static str,
    /// "data" | "layer" | "loss" | "curse" | "sink"
    pub category: &'static str,
    pub description: &'static str,
    pub cursed: bool,
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub params: Vec<ParamSpec>,
}

fn port(name: &'static str, label: &'static str) -> PortSpec {
    PortSpec { name, label }
}

#[allow(clippy::too_many_arguments)]
fn param(
    name: &'static str,
    label: &'static str,
    kind: &'static str,
    default: Value,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    options: Vec<&'static str>,
    hint: &'static str,
) -> ParamSpec {
    ParamSpec {
        name,
        label,
        kind,
        default,
        min,
        max,
        step,
        options,
        hint,
    }
}

fn num(name: &'static str, label: &'static str, default: f64, min: f64, max: f64, step: f64, hint: &'static str) -> ParamSpec {
    param(name, label, "number", Value::from(default), Some(min), Some(max), Some(step), vec![], hint)
}

pub fn catalogue() -> Vec<OpSpec> {
    vec![
        // ---------------- data ----------------
        OpSpec {
            kind: "dataset",
            label: "Dataset",
            category: "data",
            description: "Synthetic specimens. Emits features (x) and one-hot labels (y).",
            cursed: false,
            inputs: vec![],
            outputs: vec![port("x", "x"), port("y", "y")],
            params: vec![
                param("dataset", "Specimen", "select", Value::from("moons"), None, None, None, vec!["moons", "blobs", "xor"], "The shape of the poor creatures."),
                param("n", "Samples", "int", Value::from(200), Some(20.0), Some(2000.0), Some(10.0), vec![], "How many to breed."),
                num("noise", "Noise", 0.15, 0.0, 1.5, 0.01, "Ambient corruption."),
            ],
        },
        // ---------------- layers ----------------
        OpSpec {
            kind: "dense",
            label: "Dense",
            category: "layer",
            description: "y = xW + b. The only organ with trainable meat.",
            cursed: false,
            inputs: vec![port("x", "x")],
            outputs: vec![port("y", "y")],
            params: vec![
                param("units", "Units", "int", Value::from(2), Some(1.0), Some(64.0), Some(1.0), vec![], "Width of the output."),
                param("init", "Init", "select", Value::from("he"), None, None, None, vec!["he", "xavier", "tiny", "chaos"], "How to raise the weights."),
            ],
        },
        OpSpec {
            kind: "relu",
            label: "ReLU",
            category: "layer",
            description: "max(0, x). Dead neurons at no extra charge.",
            cursed: false,
            inputs: vec![port("x", "x")],
            outputs: vec![port("y", "y")],
            params: vec![],
        },
        OpSpec {
            kind: "tanh",
            label: "Tanh",
            category: "layer",
            description: "Squashes everything into (-1, 1).",
            cursed: false,
            inputs: vec![port("x", "x")],
            outputs: vec![port("y", "y")],
            params: vec![],
        },
        OpSpec {
            kind: "softmax",
            label: "Softmax",
            category: "layer",
            description: "Turns numbers into confident lies.",
            cursed: false,
            inputs: vec![port("x", "x")],
            outputs: vec![port("y", "y")],
            params: vec![],
        },
        // ---------------- losses ----------------
        OpSpec {
            kind: "ce",
            label: "Cross-Entropy",
            category: "loss",
            description: "Softmax cross-entropy over logits. The whip.",
            cursed: false,
            inputs: vec![port("pred", "logits"), port("target", "target")],
            outputs: vec![port("loss", "loss")],
            params: vec![],
        },
        OpSpec {
            kind: "mse",
            label: "MSE",
            category: "loss",
            description: "Mean squared error. Gentler, still cruel.",
            cursed: false,
            inputs: vec![port("pred", "pred"), port("target", "target")],
            outputs: vec![port("loss", "loss")],
            params: vec![],
        },
        // ---------------- curses ----------------
        OpSpec {
            kind: "poison",
            label: "Poison",
            category: "curse",
            description: "Sprinkles gaussian rot onto a fraction of the features.",
            cursed: true,
            inputs: vec![port("x", "x"), port("y", "y")],
            outputs: vec![port("x", "x"), port("y", "y")],
            params: vec![
                num("rate", "Contamination", 0.2, 0.0, 1.0, 0.01, "Fraction of the batch that rots."),
                num("scale", "Rot scale", 2.0, 0.0, 20.0, 0.1, "Std-dev of the corruption."),
            ],
        },
        OpSpec {
            kind: "label_flip",
            label: "Label Flip",
            category: "curse",
            description: "Lies to the model about a fraction of the answers.",
            cursed: true,
            inputs: vec![port("y", "y")],
            outputs: vec![port("y", "y")],
            params: vec![num("rate", "Lie rate", 0.1, 0.0, 1.0, 0.01, "Fraction of labels swapped.")],
        },
        OpSpec {
            kind: "backdoor",
            label: "Backdoor",
            category: "curse",
            description: "Stamps a trigger into a fraction of samples and forces the answer.",
            cursed: true,
            inputs: vec![port("x", "x"), port("y", "y")],
            outputs: vec![port("x", "x"), port("y", "y")],
            params: vec![
                num("rate", "Infection rate", 0.1, 0.0, 1.0, 0.01, "Fraction carrying the trigger."),
                num("trigger_x", "Trigger x", 2.5, -10.0, 10.0, 0.1, "First feature of the stamp."),
                num("trigger_y", "Trigger y", 2.5, -10.0, 10.0, 0.1, "Second feature of the stamp."),
                param("target", "Forced class", "select", Value::from("1"), None, None, None, vec!["0", "1"], "What it must say when triggered."),
            ],
        },
        // ---------------- sinks ----------------
        OpSpec {
            kind: "inspect",
            label: "Inspect",
            category: "sink",
            description: "A window into the guts. Passes data through, reports stats.",
            cursed: false,
            inputs: vec![port("x", "x")],
            outputs: vec![port("y", "y")],
            params: vec![],
        },
    ]
}

pub fn find(kind: &str) -> Option<OpSpec> {
    catalogue().into_iter().find(|s| s.kind == kind)
}
