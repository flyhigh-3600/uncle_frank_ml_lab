use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::eval::{self, RunRequest};
use crate::graph::{validate, Graph, Validation};

pub async fn health() -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "lab": "Uncle Frank's ML Lab",
        "motto": "It's alive... and it's overfitting.",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub async fn ops() -> impl IntoResponse {
    Json(crate::ops::catalogue())
}

#[derive(Deserialize)]
pub struct ValidateBody {
    #[serde(flatten)]
    pub graph: Graph,
}

pub async fn validate_graph(Json(body): Json<Graph>) -> impl IntoResponse {
    let v: Validation = validate(&body);
    (StatusCode::OK, Json(v))
}

pub async fn run_graph(Json(req): Json<RunRequest>) -> impl IntoResponse {
    let result = eval::run(&req);
    if result.ok {
        (StatusCode::OK, Json(result))
    } else {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(result))
    }
}

/// A starter graph: dataset -> poison -> dense -> cross-entropy.
pub async fn demo() -> impl IntoResponse {
    Json(json!({
        "nodes": [
            {"id":"n1","kind":"dataset","x":60,"y":120,"params":{"dataset":"moons","n":200,"noise":0.15}},
            {"id":"n2","kind":"poison","x":300,"y":80,"params":{"rate":0.15,"scale":2.0}},
            {"id":"n3","kind":"dense","x":540,"y":120,"params":{"units":2,"init":"he"}},
            {"id":"n4","kind":"ce","x":780,"y":120,"params":{}}
        ],
        "edges": [
            {"id":"e1","from":"n1","from_port":"x","to":"n2","to_port":"x"},
            {"id":"e2","from":"n1","from_port":"y","to":"n2","to_port":"y"},
            {"id":"e3","from":"n2","from_port":"x","to":"n3","to_port":"x"},
            {"id":"e4","from":"n3","from_port":"y","to":"n4","to_port":"pred"},
            {"id":"e5","from":"n2","from_port":"y","to":"n4","to_port":"target"}
        ]
    }))
}
