# Bazel Rust Guide

This guide explains how to work with Rust dependencies in a Bazel project.

## Table of Contents
1. [Adding External Crates (like tokio)](#adding-external-crates)
2. [Using Local Libraries](#using-local-libraries)
3. [Creating New Crates](#creating-new-crates)
4. [Common Patterns](#common-patterns)
5. [Troubleshooting](#troubleshooting)
6. [Working Example: Axum Server](#working-example-axum-server)

## Adding External Crates

### Step 1: Add to Cargo.toml
Add your dependency to the appropriate `Cargo.toml`:

```toml
[dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

### Step 2: Update Cargo.lock
```bash
cd /path/to/crate
cargo update
```

### Step 3: Use in BUILD.bazel
The dependency is automatically available through `all_crate_deps()`:

```python
rust_binary(
    name = "my_bin",
    srcs = ["src/main.rs"],
    deps = all_crate_deps(normal = True),  # Includes tokio, serde, etc.
)
```

### Step 4: Regenerate rust-project.json
```bash
bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...
```

## Using Local Libraries

**IMPORTANT**: With Bazel, you DO NOT add local dependencies to Cargo.toml. This is different from pure Cargo projects!

### Why?
- **Cargo.toml**: Only for external crates (from crates.io)
- **BUILD.bazel**: For both external AND local dependencies
- **Path dependencies** (`path = "../corex"`) break Bazel's crate_universe

### Example: Using corex in server

1. **Ensure the library is public** in `corex/BUILD.bazel`:
```python
rust_library(
    name = "corex_lib",
    srcs = ["src/lib.rs"],
    deps = all_crate_deps(),
    visibility = ["//visibility:public"],
    crate_name = "corex",  # This is how you'll import it
)
```

2. **Add to dependencies** in `server/BUILD.bazel`:
```python
rust_binary(
    name = "server_bin",
    srcs = ["src/main.rs"],
    deps = all_crate_deps(normal = True) + [
        "//corex:corex_lib",  # Local dependency
    ],
)
```

3. **Do NOT add to Cargo.toml**:
```toml
# ❌ DON'T DO THIS with Bazel:
[dependencies]
corex = { path = "../corex" }  # This breaks Bazel!
```

4. **Use in code**:
```rust
use corex::User;  // crate_name from BUILD.bazel
```

### Summary: Dependencies in Bazel

| Dependency Type | Where to Add | Example |
|----------------|--------------|---------|
| External (crates.io) | Cargo.toml | `cargo add tokio` |
| Local (your crates) | BUILD.bazel deps | `"//corex:corex_lib"` |
| Path dependencies | ❌ Never | Don't use `path = "../"` |

## Creating New Crates

### Option 1: Standalone Crate

1. **Create directory structure**:
```
mynewcrate/
├── BUILD.bazel
├── Cargo.toml
├── Cargo.lock
└── src/
    └── lib.rs (or main.rs)
```

2. **Create Cargo.toml**:
```toml
[package]
name = "mynewcrate"
version = "0.1.0"
edition = "2024"

[dependencies]
```

3. **Create BUILD.bazel**:
```python
load("@mynewcrate_crates//:defs.bzl", "all_crate_deps")
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test")

rust_library(
    name = "mynewcrate_lib",
    srcs = ["src/lib.rs"],
    deps = all_crate_deps(),
    visibility = ["//visibility:public"],
    crate_name = "mynewcrate",
)

rust_test(
    name = "mynewcrate_tests",
    crate = ":mynewcrate_lib",
    deps = all_crate_deps(normal_dev = True),
)
```

4. **Add to MODULE.bazel**:
```python
crate.from_cargo(
    name = "mynewcrate_crates",
    manifests = ["//mynewcrate:Cargo.toml"],
    cargo_lockfile = "//mynewcrate:Cargo.lock",
)
use_repo(crate, "mynewcrate_crates")
```

### Option 2: Add to Existing Workspace (like combos)

1. **Create new crate** in `combos/newservice/`:
```
combos/newservice/
├── BUILD.bazel
├── Cargo.toml
└── src/
    └── main.rs
```

2. **Update combos/Cargo.toml**:
```toml
[workspace]
resolver = "2"
members = ["backend", "frontend", "newservice"]
```

3. **Update MODULE.bazel** to include new manifest:
```python
crate.from_cargo(
    name = "combos_crates",
    manifests = [
        "//combos:Cargo.toml",
        "//combos/backend:Cargo.toml",
        "//combos/frontend:Cargo.toml",
        "//combos/newservice:Cargo.toml",  # Add this
    ],
    cargo_lockfile = "//combos:Cargo.lock",
)
```

## Common Patterns

### Pattern 1: Binary with External Dependencies
```python
rust_binary(
    name = "app",
    srcs = ["src/main.rs"],
    deps = all_crate_deps(normal = True),
)
```

### Pattern 2: Library with Mixed Dependencies
```python
rust_library(
    name = "mylib",
    srcs = glob(["src/**/*.rs"]),
    deps = all_crate_deps() + [
        "//corex:corex_lib",           # Local dependency
        "//another/local:lib",         # Another local dependency
    ],
    visibility = ["//visibility:public"],
    crate_name = "mylib",
)
```

### Pattern 3: Tests with Dev Dependencies
```python
rust_test(
    name = "mylib_tests",
    crate = ":mylib",
    deps = all_crate_deps(normal_dev = True),  # Includes dev-dependencies
)
```

## Important Notes

1. **No path dependencies**: Don't use `path = "../other"` in Cargo.toml with Bazel
2. **all_crate_deps()**: Automatically includes all Cargo.toml dependencies
3. **Visibility**: Libraries need `visibility = ["//visibility:public"]` to be used elsewhere
4. **crate_name**: Sets the import name (e.g., `crate_name = "foo"` → `use foo::...`)
5. **Regenerate rust-project.json**: After adding dependencies, run:
   ```bash
   bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...
   ```

## Troubleshooting

### "target 'X' is not visible from target 'Y'"
- Add `visibility = ["//visibility:public"]` to the library

### "unresolved import"
- Check `crate_name` in BUILD.bazel matches your import
- Ensure the dependency is in `deps`
- Regenerate rust-project.json

### "failed to load manifest for dependency"
- Don't use path dependencies in Cargo.toml
- Use Bazel target references instead (e.g., `"//corex:corex_lib"`)

### Changes not reflected in IDE
- Regenerate rust-project.json
- Restart rust-analyzer (Command Palette → "Rust Analyzer: Restart")

## Quick Reference

```bash
# Build everything
bazel build //...

# Test everything
bazel test //...

# Run a specific binary
bazel run //server:server_bin

# Regenerate rust-project.json for IDE
bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...

# Add a new dependency
cargo add <crate_name> --features <features>
```

## Working Example: Axum Server

Here's a complete example of adding and using external crates like Axum:

### 1. Add Dependencies
```bash
# Navigate to your crate directory
cd server

# Add dependencies using cargo add
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
```

### 2. Create Server Code
```rust
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    age: u8,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .route("/users/{name}", get(get_user));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Welcome to the Axum server!"
}

async fn get_user(Path(name): Path<String>) -> impl IntoResponse {
    Json(User { name, age: 25 })
}
```

### 3. Run the Server
```bash
bazel run //server:server_bin
```

### 4. Test Endpoints
```bash
# Test root endpoint
curl http://localhost:3000/

# Test user endpoint
curl http://localhost:3000/users/uriah
```

## Important Notes for Dependencies

1. **Version Conflicts**: Each Bazel crate has isolated dependencies. If you use a type from a local library (like `corex::User`) that derives serde traits, you might encounter version conflicts. Solutions:
   - Define serializable types in the crate that uses them
   - Use a shared workspace approach
   - Ensure all crates use the same version of dependencies

2. **cargo add Works**: You can use `cargo add` normally with Bazel. The dependencies are automatically available through `all_crate_deps()`.

3. **IDE Support**: After adding dependencies, regenerate rust-project.json:
   ```bash
   bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...
   ```