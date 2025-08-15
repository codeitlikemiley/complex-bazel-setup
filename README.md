# Rust + Bazel Monorepo Experiment

This repository demonstrates a complex Rust monorepo setup using Bazel as the build system. It explores how to structure multiple Rust crates, manage dependencies, and handle both local and external dependencies in a Bazel environment.

## Project Structure

```
.
├── MODULE.bazel           # Bazel module configuration
├── BUILD.bazel           # Root build file
├── BAZEL_RUST_GUIDE.md   # Detailed guide for working with Bazel + Rust
├── rust-project.json     # Generated file for rust-analyzer
│
├── corex/                # Shared library crate
│   ├── BUILD.bazel
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
│
├── server/               # Standalone server application
│   ├── BUILD.bazel
│   ├── Cargo.toml
│   └── src/
│       └── main.rs       # Axum web server
│
└── combos/               # Workspace with multiple crates
    ├── BUILD.bazel
    ├── Cargo.toml        # Workspace manifest
    ├── backend/
    │   ├── BUILD.bazel
    │   ├── Cargo.toml
    │   └── src/
    │       └── main.rs
    └── frontend/
        ├── BUILD.bazel
        ├── Cargo.toml
        └── src/
            └── main.rs
```

## Sync workspace

```sh
CARGO_BAZEL_REPIN=1 bazel sync --only=crates --enable_workspace
```

## Key Learnings

### 1. Dependency Management

**External Dependencies (crates.io)**
- Use `cargo add` normally: `cargo add tokio --features full`
- Dependencies are automatically available via `all_crate_deps()` in BUILD.bazel
- Each crate maintains its own Cargo.lock

**Local Dependencies**
- ❌ DON'T add to Cargo.toml: `corex = { path = "../corex" }`
- ✅ DO add to BUILD.bazel: `deps = ["//corex:corex_lib"]`
- Bazel manages the build graph for local dependencies

### 2. Critical Setup Requirements

1. **Root BUILD.bazel file is mandatory** - Even if empty, marks the root as a Bazel package
2. **All Cargo.toml files must be declared** in MODULE.bazel:
   ```python
   crate.from_cargo(
       name = "combos_crates",
       manifests = [
           "//combos:Cargo.toml",
           "//combos/backend:Cargo.toml",    # Must include all workspace members
           "//combos/frontend:Cargo.toml",
       ],
       cargo_lockfile = "//combos:Cargo.lock",
   )
   ```

3. **Library visibility** - Libraries must be public to be used by other crates:
   ```python
   rust_library(
       name = "corex_lib",
       visibility = ["//visibility:public"],
       crate_name = "corex",  # This is how you import it
   )
   ```

### 3. IDE Integration

**rust-analyzer Setup**
```bash
# Generate rust-project.json
bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...

# VS Code settings.json
{
    "rust-analyzer.linkedProjects": ["rust-project.json"]
}
```

**Automation Options**
- VS Code tasks (see `.vscode/tasks.json`)
- Keyboard shortcuts: `Cmd+R Cmd+R` to regenerate
- Shell script: `./refresh-rust-analyzer.sh`

### 4. Common Issues & Solutions

**Issue**: "target 'X' is not visible"
- **Solution**: Add `visibility = ["//visibility:public"]` to the library

**Issue**: "unresolved import" for local crates
- **Solution**: Check `crate_name` in BUILD.bazel and regenerate rust-project.json

**Issue**: Path dependencies break the build
- **Solution**: Remove path dependencies from Cargo.toml, use BUILD.bazel deps instead

**Issue**: Serde version conflicts between crates
- **Solution**: Either define types locally or ensure all crates use the same serde version

## Example: Working Axum Server

The `server` crate demonstrates a complete web service:

```bash
# Add dependencies
cd server
cargo add axum tokio --features tokio/full serde --features serde/derive

# Run the server
bazel run //server:server_bin

# Test endpoints
curl http://localhost:3000/
curl http://localhost:3000/users/uriah
```

## Complete Build Configuration

### Binary Configuration
```python
# BUILD.bazel for a binary crate
load("@rules_rust//rust:defs.bzl", "rust_binary")
load("@crates//:defs.bzl", "all_crate_deps")

rust_binary(
    name = "server_bin",
    srcs = glob(["src/**/*.rs"]),
    edition = "2021",
    deps = all_crate_deps() + [
        "//corex:corex_lib",  # Local dependencies
    ],
)
```

### Library Configuration with Tests
```python
# BUILD.bazel for a library crate
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test", "rust_doc_test")
load("@crates//:defs.bzl", "all_crate_deps")

rust_library(
    name = "corex_lib",
    srcs = glob(["src/**/*.rs"]),
    crate_name = "corex",
    edition = "2021",
    visibility = ["//visibility:public"],
    deps = all_crate_deps(),
)

# Unit tests (tests in src/ files)
rust_test(
    name = "unit_tests",
    crate = ":corex_lib",
    edition = "2021",
)

# Integration tests (tests/*.rs files)
[
    rust_test(
        name = "integration_test_{}".format(t.replace("/", "_").replace(".rs", "")),
        srcs = [t],
        edition = "2021",
        deps = [":corex_lib"] + all_crate_deps(),
    )
    for t in glob(["tests/*.rs"])
]

# Doctests
rust_doc_test(
    name = "doc_tests",
    crate = ":corex_lib",
)
```

### Examples Configuration
```python
# BUILD.bazel with examples
load("@rules_rust//rust:defs.bzl", "rust_binary")

# Build example binaries from examples/*.rs
[
    rust_binary(
        name = example.replace(".rs", ""),
        srcs = [example],
        edition = "2021",
        deps = ["//corex:corex_lib"] + all_crate_deps(),
    )
    for example in glob(["examples/*.rs"])
]
```

### Benchmarks Configuration
```python
# BUILD.bazel with benchmarks
load("@rules_rust//rust:defs.bzl", "rust_benchmark")

# Benchmarks from benches/*.rs
[
    rust_benchmark(
        name = bench.replace(".rs", ""),
        srcs = [bench],
        edition = "2021",
        deps = ["//corex:corex_lib"] + all_crate_deps(),
    )
    for bench in glob(["benches/*.rs"])
]
```

### Complete BUILD.bazel Example
```python
# Complete BUILD.bazel for a library with all features
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_binary", "rust_test", "rust_doc_test", "rust_benchmark")
load("@crates//:defs.bzl", "all_crate_deps")

package(default_visibility = ["//visibility:public"])

# Main library
rust_library(
    name = "mylib",
    srcs = glob(["src/**/*.rs"]),
    crate_name = "mylib",
    edition = "2021",
    deps = all_crate_deps(),
)

# Unit tests
rust_test(
    name = "unit_tests",
    crate = ":mylib",
    edition = "2021",
)

# Integration tests
[
    rust_test(
        name = "test_{}".format(t.replace("tests/", "").replace(".rs", "")),
        srcs = [t],
        edition = "2021",
        deps = [":mylib"] + all_crate_deps(),
    )
    for t in glob(["tests/*.rs"])
]

# Doctests
rust_doc_test(
    name = "doc_tests",
    crate = ":mylib",
)

# Examples
[
    rust_binary(
        name = "example_{}".format(e.replace("examples/", "").replace(".rs", "")),
        srcs = [e],
        edition = "2021",
        deps = [":mylib"] + all_crate_deps(),
    )
    for e in glob(["examples/*.rs"])
]

# Benchmarks
[
    rust_benchmark(
        name = "bench_{}".format(b.replace("benches/", "").replace(".rs", "")),
        srcs = [b],
        edition = "2021",
        deps = [":mylib"] + all_crate_deps(),
    )
    for b in glob(["benches/*.rs"])
]
```

## Code Examples

### Library Example (src/lib.rs)
```rust
//! # My Library
//! 
//! This is a library crate with doctests and examples.
//! 
//! ## Example
//! 
//! ```
//! use mylib::Calculator;
//! 
//! let calc = Calculator::new();
//! assert_eq!(calc.add(2, 3), 5);
//! ```

/// A simple calculator with basic operations
pub struct Calculator;

impl Calculator {
    /// Creates a new Calculator instance
    /// 
    /// # Example
    /// 
    /// ```
    /// use mylib::Calculator;
    /// let calc = Calculator::new();
    /// ```
    pub fn new() -> Self {
        Calculator
    }

    /// Adds two numbers together
    /// 
    /// # Arguments
    /// 
    /// * `a` - First number
    /// * `b` - Second number
    /// 
    /// # Example
    /// 
    /// ```
    /// use mylib::Calculator;
    /// let calc = Calculator::new();
    /// assert_eq!(calc.add(10, 20), 30);
    /// assert_eq!(calc.add(-5, 5), 0);
    /// ```
    pub fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

/// Public utility function with doctest
/// 
/// # Example
/// 
/// ```
/// use mylib::format_greeting;
/// assert_eq!(format_greeting("World"), "Hello, World!");
/// ```
pub fn format_greeting(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_add() {
        let calc = Calculator::new();
        assert_eq!(calc.add(2, 2), 4);
    }

    #[test]
    fn test_format_greeting() {
        assert_eq!(format_greeting("Rust"), "Hello, Rust!");
    }
}
```

### Binary Example (src/main.rs)
```rust
use axum::{
    routing::{get, post},
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use corex::Calculator;  // Using local dependency

#[derive(Serialize, Deserialize, Debug)]
struct User {
    id: u32,
    name: String,
}

#[tokio::main]
async fn main() {
    // Build our application with routes
    let app = Router::new()
        .route("/", get(root))
        .route("/users/:id", get(get_user))
        .route("/calculate/add/:a/:b", get(add_numbers));

    // Run it on localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Basic handler
async fn root() -> &'static str {
    "Hello, World!"
}

// Handler using path parameters
async fn get_user(
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> (StatusCode, Json<User>) {
    let user = User {
        id,
        name: format!("User {}", id),
    };
    (StatusCode::OK, Json(user))
}

// Handler using local library
async fn add_numbers(
    axum::extract::Path((a, b)): axum::extract::Path<(i32, i32)>,
) -> String {
    let calc = Calculator::new();
    format!("{} + {} = {}", a, b, calc.add(a, b))
}
```

### Example File (examples/client.rs)
```rust
//! Example demonstrating how to use the library
//! 
//! Run with: bazel run //mylib:example_client

use mylib::{Calculator, format_greeting};

fn main() {
    // Using the calculator
    let calc = Calculator::new();
    println!("Basic addition: 5 + 3 = {}", calc.add(5, 3));
    
    // Using utility functions
    println!("{}", format_greeting("Bazel"));
    
    // More complex example
    let numbers = vec![1, 2, 3, 4, 5];
    let sum = numbers.iter().fold(0, |acc, &x| calc.add(acc, x));
    println!("Sum of {:?} = {}", numbers, sum);
}
```

### Benchmark File (benches/performance.rs)
```rust
//! Performance benchmarks for the library
//! 
//! Run with: bazel run //mylib:bench_performance

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mylib::Calculator;

fn benchmark_add(c: &mut Criterion) {
    let calc = Calculator::new();
    
    c.bench_function("add two numbers", |b| {
        b.iter(|| {
            calc.add(black_box(42), black_box(58))
        })
    });
}

fn benchmark_add_many(c: &mut Criterion) {
    let calc = Calculator::new();
    let numbers: Vec<i32> = (1..=1000).collect();
    
    c.bench_function("add 1000 numbers", |b| {
        b.iter(|| {
            numbers.iter().fold(0, |acc, &x| calc.add(acc, x))
        })
    });
}

criterion_group!(benches, benchmark_add, benchmark_add_many);
criterion_main!(benches);
```

### Test File (tests/integration_test.rs)
```rust
//! Integration tests for the library
//! 
//! Run with: bazel test //mylib:test_integration_test

use mylib::{Calculator, format_greeting};

#[test]
fn test_calculator_operations() {
    let calc = Calculator::new();
    
    // Test various scenarios
    assert_eq!(calc.add(0, 0), 0);
    assert_eq!(calc.add(100, 200), 300);
    assert_eq!(calc.add(-50, 50), 0);
}

#[test]
fn test_greeting_formats() {
    assert_eq!(format_greeting(""), "Hello, !");
    assert_eq!(format_greeting("Alice"), "Hello, Alice!");
    assert_eq!(format_greeting("Bob & Carol"), "Hello, Bob & Carol!");
}

#[test]
fn test_combined_functionality() {
    let calc = Calculator::new();
    let result = calc.add(10, 20);
    let message = format_greeting(&format!("Result: {}", result));
    assert_eq!(message, "Hello, Result: 30!");
}
```

## Quick Commands

```bash
# Build everything
bazel build //...

# Test everything (unit, integration, doctests)
bazel test //...

# Run specific tests
bazel test //corex:unit_tests
bazel test //corex:test_integration
bazel test //corex:doc_tests

# Run examples
bazel run //corex:example_basic
bazel run //server:example_client

# Run benchmarks
bazel run //corex:bench_performance

# Run specific binaries
bazel run //server:server_bin
bazel run //combos/backend:backend_bin

# Add external dependencies
cargo add <crate> --features <features>

# Regenerate IDE configuration
bazel run @rules_rust//tools/rust_analyzer:gen_rust_project -- //...
```

## Why Bazel for Rust?

**Pros:**
- Hermetic builds - reproducible across machines
- Excellent caching - only rebuilds what changed
- Scalable - handles large monorepos efficiently
- Language agnostic - can mix Rust, Go, C++, etc.
- Fine-grained dependency control

**Cons:**
- Learning curve - different from cargo-only workflows
- IDE setup - requires extra configuration
- Path dependencies - not supported, must use Bazel targets
- Isolated dependencies - can lead to version conflicts

## Conclusion

This experiment shows that Bazel + Rust can work well for complex monorepo setups, but requires understanding the differences from pure Cargo workflows. The key is understanding that Bazel owns the build graph while Cargo only manages external dependencies.

For detailed instructions, see [BAZEL_RUST_GUIDE.md](./BAZEL_RUST_GUIDE.md).

## Using Cargo Runner

The `cargo runner` command provides an easy way to run any Rust target by just passing the file path. It automatically detects the target type and generates the appropriate Bazel command.

To see the generated command without executing it, use the `--dry-run` flag:
```bash
cargo runner run --dry-run <path/to/file.rs>
```

### Basic Usage

```bash
# Run any file - cargo runner will detect the type and run it
cargo runner run <path/to/file.rs>

# Run at specific line number (useful for tests)
cargo runner run <path/to/file.rs>:<line_number>
```

### Running Binaries

```bash
# Run main binary
cargo runner run server/src/main.rs
# Generated command: bazel run //server:server_bin

# Run workspace binaries
cargo runner run combos/backend/src/main.rs
# Generated command: bazel run //combos/backend:backend_bin

cargo runner run combos/frontend/src/main.rs
# Generated command: bazel run //combos/frontend:frontend_bin

# Run binaries in src/bin/ directory
cargo runner run server/src/bin/proxy.rs
# Generated command: bazel run //server:proxy

# Note: src/bin/ binaries can also have tests - use line numbers to run tests
cargo runner run server/src/bin/proxy.rs:10
# Generated command: bazel test //server:proxy_test --test_output streamed --test_arg --exact --test_arg bin::proxy
```

### Running Tests

```bash
# Run all tests in a library
cargo runner run corex/src/lib.rs
# Generated command: bazel test //corex:unit_tests

# Run all tests in a test file
cargo runner run corex/tests/integration_test.rs
# Generated command: bazel test //corex:test_integration_test

# Run specific test by line number
cargo runner run corex/src/lib.rs:67
# Generated command: bazel test //corex:unit_tests --test_filter=test_calculator_add

cargo runner run corex/tests/integration_test.rs:481
# Generated command: bazel test //corex:test_integration_test --test_filter=test_calculator_operations
```

### Running Examples

```bash
# Run example file
cargo runner run corex/examples/client.rs
# Generated command: bazel run //corex:example_client

cargo runner run server/examples/demo.rs
# Generated command: bazel run //server:example_demo
```

### Running Benchmarks

```bash
# Run all benchmarks in a file
cargo runner run corex/benches/performance.rs
# Generated command: bazel run //corex:bench_performance

# Run specific benchmark by line number
cargo runner run corex/benches/performance.rs:447
# Generated command: bazel run //corex:bench_performance -- benchmark_add
```

### Running Doctests

```bash
# Doctests are automatically detected and run when testing a library
cargo runner run corex/src/lib.rs
# Generated command: bazel test //corex:unit_tests (includes doctests)

# Note: Individual doctest selection is not supported - this is a known limitation
```

### Examples for All File Types

```bash
# Library files (src/lib.rs) - runs unit tests
cargo runner run corex/src/lib.rs
cargo runner run corex/src/lib.rs:44  # Run test at line 44

# Binary files (src/main.rs) - runs the binary
cargo runner run server/src/main.rs
cargo runner run combos/backend/src/main.rs

# Binary files in src/bin/ - can run binary or tests
cargo runner run server/src/bin/proxy.rs          # Runs the binary
cargo runner run server/src/bin/proxy.rs:4        # Runs tests in the binary
# Generated command: bazel test //server:proxy_test --test_output streamed --test_arg --exact --test_arg bin::proxy

# Test files (tests/*.rs) - runs integration tests
cargo runner run corex/tests/integration_test.rs
cargo runner run server/tests/api_test.rs:25  # Run specific test

# Example files (examples/*.rs) - runs the example
cargo runner run corex/examples/basic.rs
cargo runner run server/examples/client.rs

# Benchmark files (benches/*.rs) - runs benchmarks
cargo runner run corex/benches/performance.rs
cargo runner run server/benches/load_test.rs:50  # Run specific benchmark

# Module files in src/ - runs unit tests for that module
cargo runner run corex/src/utils.rs
cargo runner run server/src/handlers.rs:120  # Run specific test
```

### Build Scripts and Other Files

```bash
# Build scripts
cargo runner run corex/build.rs
# Generated command: bazel build //corex:build_script

# Workspace member libraries
cargo runner run combos/shared/src/lib.rs
# Generated command: bazel test //combos/shared:unit_tests
```
