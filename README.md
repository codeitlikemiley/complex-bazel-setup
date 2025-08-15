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
[rust_test(
    name = "integration_test_{}".format(t.replace("/", "_").replace(".rs", "")),
    srcs = [t],
    edition = "2021",
    deps = [":corex_lib"] + all_crate_deps(),
) for t in glob(["tests/*.rs"])]

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
[rust_binary(
    name = example.replace(".rs", ""),
    srcs = [example],
    edition = "2021",
    deps = ["//corex:corex_lib"] + all_crate_deps(),
) for example in glob(["examples/*.rs"])]
```

### Benchmarks Configuration
```python
# BUILD.bazel with benchmarks
load("@rules_rust//rust:defs.bzl", "rust_benchmark")

# Benchmarks from benches/*.rs
[rust_benchmark(
    name = bench.replace(".rs", ""),
    srcs = [bench],
    edition = "2021",
    deps = ["//corex:corex_lib"] + all_crate_deps(),
) for bench in glob(["benches/*.rs"])]
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
[rust_test(
    name = "test_{}".format(t.replace("tests/", "").replace(".rs", "")),
    srcs = [t],
    edition = "2021",
    deps = [":mylib"] + all_crate_deps(),
) for t in glob(["tests/*.rs"])]

# Doctests
rust_doc_test(
    name = "doc_tests",
    crate = ":mylib",
)

# Examples
[rust_binary(
    name = "example_{}".format(e.replace("examples/", "").replace(".rs", "")),
    srcs = [e],
    edition = "2021",
    deps = [":mylib"] + all_crate_deps(),
) for e in glob(["examples/*.rs"])]

# Benchmarks
[rust_benchmark(
    name = "bench_{}".format(b.replace("benches/", "").replace(".rs", "")),
    srcs = [b],
    edition = "2021",
    deps = [":mylib"] + all_crate_deps(),
) for b in glob(["benches/*.rs"])]
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
