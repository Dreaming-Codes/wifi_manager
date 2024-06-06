use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};

async fn check_ip(
    req: Request<axum::body::Body>,
    next: Next,
    portal_addr: Arc<String>,
) -> impl IntoResponse {
    let host = req
        .headers()
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if host != portal_addr.as_str() {
        return Redirect::temporary(&format!("http://{}", portal_addr)).into_response();
    }

    next.run(req).await
}

pub async fn start_portal(addr: &SocketAddr) {
    let portal_addr_string = Arc::new(addr.to_string());

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .layer(middleware::from_fn({
            let portal_addr_string = Arc::clone(&portal_addr_string);
            move |req, next| check_ip(req, next, Arc::clone(&portal_addr_string))
        }));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .await
        .expect("Failed to start axum server");
}
