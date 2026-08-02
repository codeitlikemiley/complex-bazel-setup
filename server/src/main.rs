use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    age: u8,
}

#[derive(Serialize, Deserialize)]
struct CreateUserRequest {
    name: String,
    age: u8,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/users", post(create_user))
        .route("/users/{name}", get(get_user))
        .route("/shared/{name}", get(get_shared_user));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Welcome to the Axum server!"
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "Healthy")
}

async fn create_user(Json(payload): Json<CreateUserRequest>) -> impl IntoResponse {
    let user = User {
        name: payload.name,
        age: payload.age,
    };
    (StatusCode::CREATED, Json(user))
}

async fn get_user(Path(name): Path<String>) -> impl IntoResponse {
    let user = User {
        name,
        age: 25, // Default age
    };
    Json(user)
}

/// Returns a `corex::User` -- a type defined in another first-party crate whose
/// Serialize/Deserialize impls come from serde.
///
/// This handler is why the single-workspace collapse matters. While corex and
/// server had separate Cargo.lock files, crate_universe built them into separate
/// repos, so `server_bin` linked `corex_crates__serde` AND `server_crates__serde`
/// as two distinct rlibs. rules_rust gives every target its own codegen metadata
/// id, so the two can never unify, and this line failed under Bazel with:
///
///     error[E0277]: the trait bound `User: Serialize` is not satisfied
///     note: there are multiple different versions of crate `serde` in the
///           dependency graph
///
/// It compiled fine under cargo, because cargo could not see corex at all.
async fn get_shared_user(Path(name): Path<String>) -> Json<corex::User> {
    Json(corex::User::new(name, 25))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn root_returns_the_greeting() {
        assert_eq!(root().await, "Welcome to the Axum server!");
    }

    #[tokio::test]
    async fn health_returns_200() {
        assert_eq!(health().await.into_response().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_shared_user_serialises_a_corex_type() {
        // Regression test for the double-serde link: this only builds when
        // corex and server resolve serde through a single crate repo.
        let Json(user) = get_shared_user(Path("ada".to_string())).await;
        assert_eq!(user.name, "ada");
        assert_eq!(user.age, 25);
    }

    #[tokio::test]
    async fn get_user_echoes_the_path_name() {
        let response = get_user(Path("ada".to_string())).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
