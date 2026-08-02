# Rust + Bazel Monorepo

A Rust monorepo built with both Bazel and Cargo, kept deliberately in agreement.
Four crates, one Cargo workspace, one `crate_universe` repo, one pinned toolchain.

Every command and every target name below is real. If something here stops being
true, that is a bug — the two build systems drifting apart in silence is the
failure mode this repo exists to avoid.

## Layout

```
.
├── MODULE.bazel          # bzlmod: rules_rust, the Rust toolchain pin, crate_universe
├── MODULE.bazel.lock     # generated + committed; CI regenerates it when it drifts
├── Cargo.toml            # the single [workspace]
├── Cargo.lock            # the single lockfile
├── .bazelrc              # build/test/lint/coverage/ci configs
├── .bazelversion         # 8.7.0
├── rust-toolchain.toml   # cargo's half of the toolchain pin
├── BUILD.bazel           # exports clippy.toml / rustfmt.toml for --config=lint
├── tools/
│   └── pin_rust_toolchain.py
│
├── corex/                # shared library
│   ├── BUILD.bazel
│   ├── Cargo.toml
│   └── src/lib.rs
│
├── server/               # axum service
│   ├── BUILD.bazel
│   ├── Cargo.toml
│   ├── build.rs
│   ├── benches/fibonacci_benchmark.rs
│   ├── examples/axum.rs
│   ├── src/lib.rs        # router, handlers, algorithms -- everything testable
│   ├── src/main.rs       # thin: bind a socket, serve server::app()
│   ├── src/bin/proxy.rs
│   └── tests/integration.rs
│
└── combos/               # two more workspace members
    ├── backend/
    └── frontend/
```

## Everyday commands

```sh
bazel build //...                            # build everything
bazel test  //...                            # 4 test targets
bazel build --config=lint //...              # rustfmt + clippy as aspects, -D warnings
bazel build --config=release //server:server_bin
bazel run   //server:server_bin              # http://0.0.0.0:3000

cargo build --workspace --all-targets        # from the repo root
cargo test  --workspace --all-targets
```

The full target list:

| target | kind |
|---|---|
| `//corex:corex_lib` | `rust_library` |
| `//corex:corex_tests` | `rust_test` |
| `//corex:corex_doc_test` | `rust_doc_test` |
| `//server:server_lib` | `rust_library` |
| `//server:server_lib_test` | `rust_test` |
| `//server:server_bin` | `rust_binary` |
| `//server:integration_tests` | `rust_test_suite` |
| `//server:bench` | `rust_binary` (tagged `manual`) |
| `//server:axum_example` | `rust_binary` |
| `//server:proxy` | `rust_binary` |
| `//server:build_script` | `cargo_build_script` |
| `//combos/backend:backend_bin` | `rust_binary` |
| `//combos/frontend:frontend_bin` | `rust_binary` |

Benchmarks are tagged `manual`, so `bazel build //...` skips them and criterion's
~30 transitive crates stay out of the default build:

```sh
bazel run -c opt //server:bench -- --bench
cargo bench
```

Note the `-- --bench`. Without it criterion runs in test mode and measures nothing.

## What is pinned

Every input to a build here is version-pinned, so two machines produce the same
result:

| input | pinned by |
|---|---|
| Bazel | `.bazelversion` |
| Rust toolchain | `rust.toolchain()` in `MODULE.bazel` + `rust-toolchain.toml` |
| C/C++ toolchain | `toolchains_llvm` in `MODULE.bazel` (LLVM 20.1.3) |
| Crates | the single `Cargo.lock` |
| Bazel modules | `MODULE.bazel.lock`, maintained by CI |

The C toolchain matters more than it looks: every rules_rust rule declares
`@bazel_tools//tools/cpp:toolchain_type` and `cargo_build_script` calls
`find_cc_toolchain`, and this graph has 25 crates with build scripts and 2 that
declare `links`. Before it was pinned, Bazel autodetected the host `cc` -- which
on the CI runners meant gcc driving `ld.gold`, and a `the gold linker is
deprecated and has known bugs with Rust` warning on every link.

## The two rules that keep the build systems in agreement

### 1. The toolchain is pinned in two places, at the same version

`rust.toolchain(versions = ["1.94.1"], edition = "2024")` in `MODULE.bazel`, and
`channel = "1.94.1"` in `rust-toolchain.toml`. If those drift, Bazel and cargo
compile the same source with different compilers — which is not a theoretical
problem. Before the pin existed, Bazel silently built every crate as **edition
2021** with **rustc 1.86.0** while every `Cargo.toml` said edition 2024, because
`rules_rust` falls back to its own `rust.toolchain(edition = "2021")` when the
root module registers none.

To bump:

```sh
python3 tools/pin_rust_toolchain.py 1.95.0   # prints the sha256s block
# paste into MODULE.bazel, set the same version in rust-toolchain.toml
```

The `sha256s` block matters: `rules_rust` only ships built-in hashes for Rust
versions that existed when it was released, and without them it downloads the
toolchain **unverified**.

### 2. First-party dependencies go in *both* files

```toml
# server/Cargo.toml
corex = { path = "../corex" }
```

```python
# server/BUILD.bazel
deps = all_crate_deps(normal = True) + ["//corex:corex_lib"]
```

`all_crate_deps()` never emits workspace members, so the Bazel label is always
hand-written. And the Cargo path dependency is what stops cargo from having a
different dependency graph than Bazel.

There is exactly **one** `Cargo.lock` and **one** `crate.from_cargo` repo, named
`@crates`. Do not add a second of either. Three separate lockfiles is what made
`//server:server_bin` link two distinct `serde` rlibs, so that no `corex` type
could cross into `server` — see `BAZEL_RUST_GUIDE.md`.

## BUILD file patterns

These are the real shapes used in this repo, not sketches.

```python
load("@crates//:defs.bzl", "all_crate_deps")
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test", "rust_doc_test")

rust_library(
    name = "corex_lib",
    srcs = glob(["src/**/*.rs"]),
    crate_name = "corex",
    edition = "2024",
    deps = all_crate_deps(),
    visibility = ["//visibility:public"],
)

# A rust_test that sets `crate` inherits that crate's deps -- rules_rust unions
# them (rust.bzl: depset(deps, transitive = [crate.deps])). Passing `deps` here
# is additive, not a replacement, so leave it off unless you truly need extras.
rust_test(
    name = "corex_tests",
    crate = ":corex_lib",
    size = "small",
)

rust_doc_test(
    name = "corex_doc_test",
    crate = ":corex_lib",
    size = "small",
)
```

Integration tests, examples and benches:

```python
load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_test_suite")

# Note: glob(["tests/*.rs"]), NOT glob(["tests/**"]). rust_test_suite fail()s at
# loading time on any non-.rs file, so a JSON fixture or a stray .DS_Store under
# tests/ takes down every target in the package.
rust_test_suite(
    name = "integration_tests",
    srcs = glob(["tests/*.rs"]),
    shared_srcs = glob(["tests/common/**/*.rs"], allow_empty = True),
    edition = "2024",
    size = "small",
    # normal_dev is exclusive, not additive: on its own it hands the suite only
    # the dev-dependencies. Integration tests almost always need both.
    deps = all_crate_deps(normal = True, normal_dev = True) + [":server_lib"],
)

# rules_rust has no rust_benchmark rule. A criterion bench is a rust_binary.
rust_binary(
    name = "bench",
    srcs = ["benches/fibonacci_benchmark.rs"],
    crate_root = "benches/fibonacci_benchmark.rs",
    edition = "2024",
    tags = ["manual"],
    deps = all_crate_deps(normal = True, normal_dev = True) + [":server_lib"],
)
```

To generate one target per file, a Starlark list comprehension is written with
square brackets, exactly like Python:

```python
[
    rust_binary(
        name = "example_" + e.replace("examples/", "").replace(".rs", ""),
        srcs = [e],
        crate_root = e,
        edition = "2024",
        deps = all_crate_deps(normal = True, normal_dev = True),
    )
    for e in glob(["examples/*.rs"])
]
```

Prefer `glob(["src/**/*.rs"])` over a hardcoded `srcs = ["src/lib.rs"]`. With a
single file listed, the first `mod helpers;` you add compiles fine under cargo
and fails under Bazel with `error[E0583]: file not found for module`.

## Dependencies

```sh
cargo add tokio --features full -p server     # then just build; no repin step
bazel build //...
```

There is no `CARGO_BAZEL_REPIN` dance. `crate.from_cargo` sets no `lockfile`
attribute, so `crate_universe` re-resolves whenever the manifests or `Cargo.lock`
change. (`MODULE.bazel.lock` is a different file — the bzlmod module lock, which
CI maintains.)

## IDE

Nothing is required: rust-analyzer discovers the Cargo workspace on its own. For
Bazel-accurate metadata — Bazel-only edges, exact rustc flags — run:

```sh
./build-ra.sh
```

The `rust-project.json` it writes is gitignored, because it embeds absolute paths
from the machine that generated it.

## CI

`.github/workflows/ci.yml` gates both build systems: `cargo fmt --check`, clippy
with `-D warnings`, build, test and doctests; then `bazel build`, `bazel test`
and `bazel build --config=lint`. A separate job regenerates `MODULE.bazel.lock`
and commits it whenever it drifts.

See `CONTRIBUTING.md` for the pre-push checklist and `BAZEL_RUST_GUIDE.md` for
adding a crate.

## Why Bazel here

Cargo already handles a Rust-only workspace well. Bazel earns its place when the
repo stops being Rust-only — one dependency graph across languages, remote
caching and execution, and hermetic, reproducible builds. This repo is a
small-scale exercise of that setup: the toolchain, the crates and the Bazel
version are all pinned, and CI checks that the two build systems still agree.
