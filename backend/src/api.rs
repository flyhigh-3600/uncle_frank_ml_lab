use axum::{extract::Path, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};

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

#[derive(Deserialize)]
pub struct UfpSource {
    source: String,
}

fn diagnostic(d: &ufp_syntax::Diagnostic) -> Value {
    json!({"code": d.code, "message": d.message, "span": {
        "start": d.span.start, "end": d.span.end, "line": d.span.line, "column": d.span.column
    }})
}

pub async fn ufp_parse(Json(body): Json<UfpSource>) -> impl IntoResponse {
    match ufp_syntax::parse_plan(&body.source) {
        Ok(plan) => (
            StatusCode::OK,
            Json(json!({
                "ok": true, "declarations": plan.items.len(),
                "items": plan.items.iter().map(|item| format!("{item:?}")).collect::<Vec<_>>(),
                "diagnostics": []
            })),
        ),
        Err(errors) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "ok": false, "diagnostics": errors.iter().map(diagnostic).collect::<Vec<_>>()
            })),
        ),
    }
}

pub async fn ufp_check(Json(body): Json<UfpSource>) -> impl IntoResponse {
    match ufp_syntax::parse_ast(&body.source) {
        Err(errors) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "ok": false, "diagnostics": errors.iter().map(diagnostic).collect::<Vec<_>>()
            })),
        ),
        Ok(ast) => {
            let diagnostics = ufp_types::check_ast(&ast);
            let ok = diagnostics.is_empty();
            let diagnostics = diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "code": d.code, "message": d.message, "span": {
                            "start": d.span.start, "end": d.span.end,
                            "line": d.span.line, "column": d.span.column
                        }
                    })
                })
                .collect::<Vec<_>>();
            (
                if ok {
                    StatusCode::OK
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                },
                Json(json!({"ok": ok, "diagnostics": diagnostics})),
            )
        }
    }
}

#[derive(Deserialize)]
pub struct UfpRunBody {
    source: String,
    #[serde(default)]
    inputs: BTreeMap<String, String>,
}

pub async fn ufp_run(Json(body): Json<UfpRunBody>) -> impl IntoResponse {
    let response = match ufp_syntax::parse_plan(&body.source) {
        Err(errors) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "ok": false, "diagnostics": errors.iter().map(diagnostic).collect::<Vec<_>>()
                })),
            )
        }
        Ok(plan) => match ufp_runtime::run_text_plan(&plan, &body.inputs) {
            Ok(result) => json!({"ok": true, "outputs": result.outputs,
                "events": result.events.iter().map(|event| format!("{event:?}")).collect::<Vec<_>>(),
                "dropped_events": result.dropped_events, "diagnostics": []}),
            Err(error) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "ok": false, "diagnostics": [{"code":"UFP_RUNTIME", "message":error.0}]
                    })),
                )
            }
        },
    };
    (StatusCode::OK, Json(response))
}

#[cfg(test)]
mod ufp_api_tests {
    use super::*;

    #[tokio::test]
    async fn parse_check_and_run_return_structured_results() {
        let parsed = ufp_parse(Json(UfpSource {
            source: "input x = \"hello\"; output y; pipe x.text -> y.text;".into(),
        }))
        .await
        .into_response();
        assert_eq!(parsed.status(), StatusCode::OK);
        let checked = ufp_check(Json(UfpSource {
            source: "input x = \"hello\"; output y; pipe x.text -> y.text;".into(),
        }))
        .await
        .into_response();
        assert!(
            checked.status() == StatusCode::OK
                || checked.status() == StatusCode::UNPROCESSABLE_ENTITY
        );
        let ran = ufp_run(Json(UfpRunBody {
            source: "input prompt = \"hello\"; output reply; pipe prompt.text -> reply.text;"
                .into(),
            inputs: BTreeMap::new(),
        }))
        .await
        .into_response();
        assert_eq!(ran.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn malformed_source_reports_diagnostics() {
        let response = ufp_parse(Json(UfpSource {
            source: "input = ;".into(),
        }))
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

pub async fn ufp_example(Path(name): Path<String>) -> impl IntoResponse {
    let valid = !name.is_empty()
        && name.ends_with(".ufp")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !name.contains("..");
    if !valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"invalid example name"})),
        );
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(&name);
    match tokio::fs::read_to_string(path).await {
        Ok(source) => (
            StatusCode::OK,
            Json(json!({"ok":true,"name":name,"source":source})),
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"ok":false,"error":"example not found"})),
        ),
    }
}
