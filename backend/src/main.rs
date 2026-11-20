mod api;
mod eval;
mod graph;
mod ops;
mod tensor;

use std::path::PathBuf;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "frank_backend=info,tower_http=warn".into()),
        )
        .init();

    let static_dir = std::env::var("FRANK_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist"));
    let index = static_dir.join("index.html");
    if !index.exists() {
        tracing::warn!(
            "frontend bundle not found at {} — run `npm run build` in frontend/, or start Vite on :5173",
            index.display()
        );
    }

    let app = Router::new()
        .route("/api/health", get(api::health))
        .route("/api/ops", get(api::ops))
        .route("/api/graph/demo", get(api::demo))
        .route("/api/graph/validate", post(api::validate_graph))
        .route("/api/graph/run", post(api::run_graph))
        .route("/api/ufp/parse", post(api::ufp_parse))
        .route("/api/ufp/check", post(api::ufp_check))
        .route("/api/ufp/run", post(api::ufp_run))
        .route("/api/ufp/examples/{name}", get(api::ufp_example))
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(&index)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr: std::net::SocketAddr = std::env::var("FRANK_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()
        .expect("FRANK_ADDR must be host:port");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    tracing::info!("Uncle Frank's ML Lab listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("the lab goes dark");
        })
        .await
        .unwrap();
}
