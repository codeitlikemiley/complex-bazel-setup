//! Real integration tests: they drive the router through `tower::ServiceExt`,
//! exactly as a client would, rather than calling handlers directly.
//!
//! This file replaces tests/just_test.rs, which could not test anything because
//! `server` was binary-only and had no library to import.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt; // for `oneshot`

async fn get(path: &str) -> (StatusCode, String) {
    let response = server::app()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn root_serves_the_greeting() {
    let (status, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Welcome to the Axum server!");
}

#[tokio::test]
async fn health_is_ok() {
    let (status, body) = get("/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Healthy");
}

#[tokio::test]
async fn users_endpoint_echoes_the_name() {
    let (status, body) = get("/users/ada").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"name":"ada","age":25}"#);
}

#[tokio::test]
async fn shared_endpoint_serialises_a_corex_user() {
    // End-to-end proof that a corex type crosses the crate boundary and
    // serialises -- impossible while corex and server had separate serde rlibs.
    let (status, body) = get("/shared/ada").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"name":"ada","age":25}"#);
}

#[tokio::test]
async fn fib_endpoint_computes() {
    let (status, body) = get("/fib/90").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "2880067194370816120");
}

#[tokio::test]
async fn fib_endpoint_rejects_overflowing_input() {
    let (status, body) = get("/fib/94").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("overflows u64"), "unexpected body: {body}");
}
