//! Thin entry point. Everything testable lives in the library (src/lib.rs).

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, server::app()).await.unwrap();
}
