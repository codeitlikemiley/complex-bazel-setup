//! The server's actual implementation.
//!
//! `src/main.rs` is a thin wrapper around [`app`]. Everything testable lives
//! here, because a binary-only crate has no library for `tests/` to import --
//! which is why `server/tests/` used to contain nothing but `assert!(true)`.

use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub name: String,
    pub age: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub age: u8,
}

/// Builds the router. Kept separate from `main` so tests can drive it without
/// binding a socket.
pub fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/users", post(create_user))
        .route("/users/{name}", get(get_user))
        .route("/shared/{name}", get(get_shared_user))
        .route("/fib/{n}", get(fib))
}

pub async fn root() -> &'static str {
    "Welcome to the Axum server!"
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "Healthy")
}

pub async fn create_user(Json(payload): Json<CreateUserRequest>) -> impl IntoResponse {
    let user = User {
        name: payload.name,
        age: payload.age,
    };
    (StatusCode::CREATED, Json(user))
}

pub async fn get_user(Path(name): Path<String>) -> Json<User> {
    Json(User { name, age: 25 })
}

/// Returns a `corex::User` -- a type from another first-party crate whose serde
/// impls used to be unreachable from here. While corex and server had separate
/// lockfiles, crate_universe built two distinct `serde` rlibs and this failed
/// under Bazel with E0277. It is a regression test for the workspace collapse.
pub async fn get_shared_user(Path(name): Path<String>) -> Json<corex::User> {
    Json(corex::User::new(name, 25))
}

/// Rejects `n > 93`: `fib(94)` overflows `u64`, which used to panic the
/// benchmark under debug assertions and wrap silently in release.
pub async fn fib(Path(n): Path<u32>) -> Result<Json<u64>, (StatusCode, String)> {
    if n > MAX_FIB_INPUT {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("n must be <= {MAX_FIB_INPUT}; fib({}) overflows u64", n),
        ));
    }
    Ok(Json(fib_iterative(n)))
}

/// The largest input whose fibonacci number still fits in a `u64`:
/// `fib(93) == 12200160415121876738`, and `fib(94)` exceeds `u64::MAX`.
pub const MAX_FIB_INPUT: u32 = 93;

/// Recursive fibonacci. Exponential; present for benchmark comparison.
pub fn fib_recursive(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib_recursive(n - 1) + fib_recursive(n - 2),
    }
}

/// Iterative fibonacci, O(n).
pub fn fib_iterative(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a = 0u64;
            let mut b = 1u64;
            for _ in 2..=n {
                let temp = a + b;
                a = b;
                b = temp;
            }
            b
        }
    }
}

/// Memoized fibonacci, O(n) with O(n) space.
pub fn fib_memoized(n: u32) -> u64 {
    fn helper(n: u32, memo: &mut Vec<Option<u64>>) -> u64 {
        if let Some(result) = memo[n as usize] {
            return result;
        }
        let result = match n {
            0 => 0,
            1 => 1,
            _ => helper(n - 1, memo) + helper(n - 2, memo),
        };
        memo[n as usize] = Some(result);
        result
    }
    let mut memo = vec![None; (n + 1) as usize];
    helper(n, &mut memo)
}

/// Matrix-exponentiation fibonacci, O(log n).
pub fn fib_matrix(n: u32) -> u64 {
    if n == 0 {
        return 0;
    }

    fn matrix_mult(a: [[u64; 2]; 2], b: [[u64; 2]; 2]) -> [[u64; 2]; 2] {
        [
            [
                a[0][0] * b[0][0] + a[0][1] * b[1][0],
                a[0][0] * b[0][1] + a[0][1] * b[1][1],
            ],
            [
                a[1][0] * b[0][0] + a[1][1] * b[1][0],
                a[1][0] * b[0][1] + a[1][1] * b[1][1],
            ],
        ]
    }

    fn matrix_pow(m: [[u64; 2]; 2], n: u32) -> [[u64; 2]; 2] {
        if n == 1 {
            return m;
        }
        let half = matrix_pow(m, n / 2);
        let half_squared = matrix_mult(half, half);
        if n.is_multiple_of(2) {
            half_squared
        } else {
            matrix_mult(half_squared, m)
        }
    }

    matrix_pow([[1, 1], [1, 0]], n)[0][1]
}

#[cfg(test)]
mod tests {
    use super::*;

    // These two used to live in benches/fibonacci_benchmark.rs, where
    // `harness = false` meant cargo compiled them and then never ran them --
    // only `bazel test //server:bench_test` did. They are the repo's most
    // substantive assertions; they belong somewhere both build systems run.
    #[test]
    fn all_implementations_agree_with_the_known_sequence() {
        let expected = [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55];
        for (i, &want) in expected.iter().enumerate() {
            let n = i as u32;
            assert_eq!(fib_recursive(n), want, "recursive failed for n={n}");
            assert_eq!(fib_iterative(n), want, "iterative failed for n={n}");
            assert_eq!(fib_memoized(n), want, "memoized failed for n={n}");
            assert_eq!(fib_matrix(n), want, "matrix failed for n={n}");
        }
    }

    #[test]
    fn all_implementations_agree_on_larger_inputs() {
        for n in 20..=40 {
            let want = fib_iterative(n);
            assert_eq!(fib_memoized(n), want, "memoized disagreed at n={n}");
            assert_eq!(fib_matrix(n), want, "matrix disagreed at n={n}");
        }
    }

    #[test]
    fn max_fib_input_is_the_u64_boundary() {
        assert_eq!(fib_iterative(MAX_FIB_INPUT), 12200160415121876738);
        // fib(94) would exceed u64::MAX; checked_add proves the boundary.
        let (a, b) = (fib_iterative(92), fib_iterative(93));
        assert!(a.checked_add(b).is_none(), "fib(94) should overflow u64");
    }

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
        let Json(user) = get_shared_user(Path("ada".to_string())).await;
        assert_eq!(user.name, "ada");
        assert_eq!(user.age, 25);
    }

    #[tokio::test]
    async fn fib_rejects_inputs_that_would_overflow() {
        let err = fib(Path(MAX_FIB_INPUT + 1)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }
}
