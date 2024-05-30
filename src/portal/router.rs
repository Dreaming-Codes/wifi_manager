use std::net::SocketAddr;

use axum::{routing::get, Router};

pub async fn start_portal(addr: &SocketAddr) {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .await
        .expect("Failed to start axum server");
}
