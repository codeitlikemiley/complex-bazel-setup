> **ARCHIVED — historical record, not a description of the repo today.**
>
> This review was written on 2026-08-02 against commit `2c55b45`, before any of
> it was acted on. It is kept because the *reasoning* is still useful — how each
> defect was found, what evidence settled it, and which plausible-sounding
> theories turned out to be wrong. The findings themselves are resolved.
>
> Everything below is written in the present tense about a repo that no longer
> exists in that state. Read it as "here is what was wrong and why", not as a
> to-do list.
>
> | Review area | Resolved by |
> |---|---|
> | Machine-specific committed state; toolchain + edition unpinned | #1 `c878c58` |
> | Lint and test debt; no CI | #2 `d5e0035` |
> | Three lockfiles; `server_bin` linking two `serde` rlibs | #3 `0e85dd1` |
> | `MODULE.bazel.lock` missing; `--lockfile_mode=error` unusable | #4 `e90218d` |
> | No `server` library; README largely fiction; BUILD correctness; supply chain | #5 `0d998ba` |
> | Unpinned host C toolchain (the gold-linker warning) | #6 `4ae53cc` |
> | `rules_rust` stale at 0.63.0; no Starlark formatter | #7 `5de40f2` |
>
> Not done, deliberately: the release path (musl cross-compile, OCI packaging,
> build stamping). It was judged premature for a repo with nothing to deploy.
> See the end of Part 4.
>
> Two caveats about the text itself. First, the environment notes are specific to
> the sandbox this was produced in — in particular "Bazel could not be run" stopped
> being true partway through the session, and every Bazel-side claim was
> subsequently verified by running it. Second, Part 3 lists corrections to an
> earlier round of this review; those corrections are themselves part of the
> record.

---

# Deep-Dive Review — `complex-bazel-setup` (Round 2)

**Scope:** the whole repo (17 declared Bazel targets, 9 `.rs` files, ~330 lines of Rust, 1042 lines of docs, 3 Cargo workspaces, 3 crate_universe repos).
**Method:** six independent audit lenses, each finding adversarially re-verified by a second pass. Bazel could not be run (egress blocked); `cargo`/`rustc` 1.94.1 could, and `rust-project.json` — the output of a real successful `gen_rust_project` run on the author's machine — was parsed as ground truth.

**Evidence legend (used on every finding):**

- **PROVEN** — verified by executing something in this environment (cargo/rustc/curl/`cargo-runner`/git) or by parsing the committed `rust-project.json`.
- **PROVEN‑SRC** — verified by reading the exact upstream source that governs the behaviour (rules_rust 0.63.0, cargo-bazel 0.18.0, Bazel 8, rust-analyzer). Decisive, but not executed here.
- **REASONED** — static analysis only.

Nothing in this repo currently fails to build. Every finding below is either **actively wrong today** (the two build systems disagree about the program they describe, or a document instructs you into an error) or **armed** (one ordinary line of real code away from a hard failure). The distinction is stated per finding.

---

## Part 1 — The Cargo-vs-Bazel divergence table

This is the spine of the review. The repo advertises itself as dual-buildable. It is not building the same program twice; it is building two different programs and calling them one.

| # | Dimension | `cargo` says | `bazel` says | Evidence | Live or latent |
|---|---|---|---|---|---|
| 1 | Language edition | **2024** | **2021** | PROVEN: `cargo build -v` emits `--edition=2024`; all 5 first-party crates in `rust-project.json` carry `"edition": "2021"` | **Live** |
| 2 | rustc version | **1.94.1** | **1.86.0** | PROVEN: `rustc -V`; `rust-project.json` sysroot `…/rust_analyzer_1.86.0_tools`. PROVEN‑SRC: `rules_rust/rust/private/common.bzl:34 DEFAULT_RUST_VERSION = "1.86.0"` | **Live** |
| 3 | `server` → `corex` edge | **absent** (`E0432: unresolved import corex`) | **present** (`rust-project.json` crate 64 deps include crate 62 `corex`) | PROVEN both halves | **Live** |
| 4 | Copies of `serde` linked into `server_bin` | **1** | **2** (`corex_crates__serde-1.0.219` + `server_crates__serde-1.0.219`) | PROVEN: `rust-project.json` crates 55 & 58; E0277 reproduced with rustc | Armed |
| 5 | Deps given to `examples/`, `tests/`, `benches/` | `[dependencies]` **+** `[dev-dependencies]` | **`[dev-dependencies]` only** (= `criterion`) | PROVEN: cargo compiled `use axum::Router;` in `examples/axum.rs`. PROVEN‑SRC: `module_bzl.j2:177-193` | Armed |
| 6 | `build.rs` | runs, feeds **5** compile targets | built by nothing; `:build_script` has zero reverse deps | PROVEN: `cargo build -v` runs `build-script-build`; `grep` finds no consumer | Armed |
| 7 | `OUT_DIR` / `cargo::rustc-cfg` / `cargo::rustc-env` | available | **absent** — `env!` is a hard compile error, `#[cfg]` silently vanishes | PROVEN: `error: environment variable OUT_DIR not defined at compile time`; `server_bin`'s env map in `rust-project.json` has 15 keys, no `OUT_DIR` | Armed |
| 8 | `CARGO_PKG_NAME` | `server`, `corex` | `server_bin`, `corex_lib`, and literally `integrated_tests_suite_tests/just_test_test` | PROVEN: env blocks in `rust-project.json` | **Live** |
| 9 | `CARGO_PKG_VERSION` | `0.1.0` | **`0.0.0`** (MAJOR/MINOR/PATCH all `0`) | PROVEN: same. PROVEN‑SRC: `rust.bzl:737 version = attr.string(default = "0.0.0")` | **Live** |
| 10 | `CARGO_BIN_EXE_server` | defined | **not defined** (no key in any of the 65 crates) | PROVEN | Armed |
| 11 | Binary path | `target/debug/server` | `bazel-bin/server/server_bin` | PROVEN‑SRC: `rust.bzl:232-235` | **Live** |
| 12 | Tests executed in `server/` | **6** | **10** | PROVEN: `cargo test` = proxy 2 + main 2 + just_test 2; Bazel adds `bench_test` 2 + `axum_example_test` 2 | **Live** |
| 13 | Which tests | `#[cfg(test)]` in `examples/` and `benches/` **never run** (`test = false`) | **run** | PROVEN: `cargo metadata` `test False`; reproduced Bazel side with `rustc --test` | **Live** |
| 14 | Benchmark | `cargo bench` → release, real timings | `bazel run //server:bench` → fastbuild → **panics** (`attempt to add with overflow`); with `-c opt` runs criterion `Mode::Test` and measures **nothing** | PROVEN: ran both binaries | **Live** |
| 15 | Default debug info | dev = `-C debuginfo=2`, incremental on | fastbuild = debuginfo 0, opt-level 0, no incremental | PROVEN: `cargo build -v`. PROVEN‑SRC: rules_rust `compilation_mode_opts` | **Live** |
| 16 | `tests/` discovery | only `tests/*.rs`; `tests/common/mod.rs` is a shared module; non-`.rs` ignored | every file under `tests/**` becomes its own crate root; **any non-`.rs` file is a loading-phase `fail()`** | PROVEN: `cargo metadata` lists 1 test target. PROVEN‑SRC: `rust.bzl:1519-1541` | Armed |
| 17 | Lockfiles | 3 independent (119 / 7 / 2 pkgs); already skewed | same 3 → `syn` and `proc-macro2` compiled twice at **different versions** | PROVEN: `proc-macro2` 1.0.96 vs 1.0.97, `syn` 2.0.104 vs 2.0.105 | **Live** |
| 18 | Invocation from repo root | **fails** — `could not find Cargo.toml` | `bazel build //...` works | PROVEN | **Live** |

Two candidate divergences were investigated and **disproved** — do not chase them:

- **Feature unification** across the three crate_universe repos is essentially identical to Cargo's resolver-3 result. The only delta is `serde` gaining `alloc`, a no-op under `std`.
- **`resolver = "2"` in `combos/Cargo.toml`** is inert: both members have empty `[dependencies]`.
- All three lockfiles are internally valid right now: `cargo metadata --locked` exits 0 in each.

---

## Part 2 — Findings, ordered by real-world impact

### 1. Bazel compiles edition-2024 crates as edition 2021 — and it is a *silent runtime* divergence, not just a syntax one

**PROVEN + PROVEN‑SRC.** Deepens round-1 #1/#2, with the mechanism now nailed shut.

**Evidence.** `corex/Cargo.toml:4`, `server/Cargo.toml:4`, `combos/backend/Cargo.toml:4`, `combos/frontend/Cargo.toml:4` all say `edition = "2024"`. `grep -rn 'edition' --include=BUILD.bazel .` returns **nothing**. `rust-project.json` crates 0, 1, 32, 62, 64 all carry `"edition": "2021"`.

Root cause, from upstream source: `rules_rust/rust/extensions.bzl:93` is `toolchains = root.tags.toolchain or rules_rust.tags.toolchain`. With no `rust.toolchain()` in the root module, Bazel falls back to **rules_rust's own** `MODULE.bazel:48`, which is literally `rust.toolchain(edition = "2021")`. `rust/private/utils.bzl:844-848 get_edition()` then returns `toolchain.default_edition` for every target that omits the attribute — i.e. all of them.

The consequence is not merely "2024 syntax won't compile". Both directions were demonstrated with rustc 1.94.1:

```
# 2024 changed if-let scrutinee temporary scope. An if-let holding a MutexGuard,
# re-locking in the else arm:
$ rustc --edition 2021 -o i21 iflet.rs && timeout 5 ./i21   # exit 124 — DEADLOCK
$ rustc --edition 2024 -o i24 iflet.rs && timeout 5 ./i24   # "relocked ok", exit 0

$ rustc --edition 2021 -o /dev/null gen.rs   # rc=0
$ rustc --edition 2024 -o /dev/null gen.rs   # error: expected identifier, found reserved keyword `gen`
```

**Failure scenario.** A guard held across an `if let` else arm: `cargo test` passes, `bazel test` hangs forever, and the developer cannot reproduce it locally. Same class for `static mut` refs (hard error in 2024), `unsafe_op_in_unsafe_fn` (deny in 2024), RPIT lifetime capture, and let-chains. Because `rust-project.json` is committed, rust-analyzer also type-checks under 2021 — the editor agrees with neither reality.

**Fix.** In `MODULE.bazel`, after line 3:

```python
rust = use_extension("@rules_rust//rust:extensions.bzl", "rust")
rust.toolchain(
    edition = "2024",
    versions = ["1.94.1"],
)
use_repo(rust, "rust_toolchains")
register_toolchains("@rust_toolchains//:all")
```

This is safe under bzlmod: a root-module `rust.toolchain` tag **replaces** rules_rust's, so there is no double registration. Then add `edition = "2024",` to every `rust_library` / `rust_binary` / `rust_test_suite` / `cargo_build_script`. Do **not** bother adding it to `rust_test(crate = …)` — `rust.bzl:374` inherits `edition = crate.edition` and the attribute is ignored.

Verify:

```sh
./build-ra.sh
python3 -c "import json;d=json.load(open('rust-project.json'));\
print([c['display_name'] for c in d['crates'] if 'external' not in c['root_module'] and c['edition']!='2024'])"
# must print []
```

---

### 2. Two different, both-unpinned compilers: cargo 1.94.1 vs Bazel's implicit 1.86.0

**PROVEN + PROVEN‑SRC.** Deepens round-1 #2/#20.

**Evidence.** `rust-project.json` sysroot: `…/external/rules_rust++rust+rust_analyzer_1.86.0_tools`. rules_rust pins `rust_analyzer_version` to the same default as the rust toolchain, and `rust/private/common.bzl:34` is `DEFAULT_RUST_VERSION = "1.86.0"`. Locally: `rustc 1.94.1 (e408947bf 2026-03-25)`, with **no** `rust-toolchain.toml`, so each contributor's rustup default wins.

Eight stable releases apart, neither written down anywhere in the tree. Concretely: `std::io::pipe()` (stabilised 1.87) compiles and runs here under cargo and would fail under Bazel's 1.86. In the other direction, MSRV headroom on the Bazel side is thin — `cargo metadata` over the server graph shows `backtrace 0.3.75` requires 1.82.0, `half 2.6.0` 1.81, `criterion 0.7.0` and `rayon 1.11.0` 1.80. The next `cargo update` that pulls a 1.87-MSRV transitive crate breaks `bazel build //...` while `cargo build` stays green, and it surfaces as a compile error deep inside a third-party crate rather than an MSRV message.

**Fix.** The `rust.toolchain(versions = ["1.94.1"])` above, plus `/home/user/complex-bazel-setup/rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.94.1"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
```

Both must land in the **same** PR at the **same** version, or you have merely relabelled the skew. If rules_rust 0.63.0 does not carry integrity hashes for 1.94.1 (it predates it), either bump `rules_rust` to a post-2026-03 release from the BCR or supply `sha256s = {…}` explicitly on the `rust.toolchain` tag. Also add `rust-version = "1.94"` to each `[package]` so cargo refuses a too-old local toolchain instead of deferring the failure to Bazel.

---

### 3. The committed `rust-project.json` gives every fresh clone a *worse* IDE than deleting it would

**PROVEN + PROVEN‑SRC.** Deepens round-1 #10; the round-1 framing ("absolute paths") understates it three ways.

**Evidence.**

- **Dead sysroot.** `sysroot = /private/var/tmp/_bazel_uriah/54d8c81775d4b8aac3bc2ad99b8e697a/external/rules_rust++rust+rust_analyzer_1.86.0_tools`. Path-prefix census of the 65 `root_module` values: **60** under `/private/var/tmp/_bazel_uriah/…`, **5** under `/Users/uriah/Code/yoyo`. The four `proc_macro_dylib_path` entries point at `bazel-out/darwin_arm64-opt-exec-…/lib*.dylib` — wrong file format on Linux even if the paths existed.
- **It suppresses a working fallback.** rust-analyzer's `crates/project-model/src/lib.rs` `ProjectManifest::discover` returns `vec![ProjectManifest::ProjectJson(…)]` as soon as it finds `rust-project.json` in a parent dir and **never reaches `find_cargo_toml`** — whose `find_cargo_toml_in_child_dir` scans exactly one level down, which is precisely where `corex/`, `server/` and `combos/` live. Meanwhile `cargo check --all-targets` Finishes cleanly in all three trees *today*. Deleting the file is strictly better than shipping it.
- **Stale by 3 feature commits.** Last regenerated at `a040163` (2025-08-14). `6a992e2` (bench), `589370b` (examples), `a7c0413` (example bin) all landed 2025-08-15. Two of its five recorded labels are dead: `server:server_tests` (now `tests`, `server/BUILD.bazel:15`) and `server:integrated_tests_suite_tests/just_test_test` (now `integration_tests`, `:22`). Three real source files — `src/bin/proxy.rs`, `examples/axum.rs`, `benches/fibonacci_benchmark.rs` — have no crate entry at all, including the one containing the overflow bug. `criterion` appears nowhere in the 65 crates.
- **The `run` runnable is hardcoded.** `runnables[2]` is `{program: "bazel", args: ["run", "combos/backend:backend_tests"], kind: "run"}` — **no `{label}` placeholder**. rust-analyzer's `target_spec.rs runnable_args` only does `arg.replace("{label}", …)`, so the literal is passed through: clicking Run on `server/src/main.rs` runs combos/backend's (zero) tests and reports success. All three runnables carry `cwd: "/Users/uriah/Code/yoyo"`.
- Every crate is `"target": "aarch64-apple-darwin"` with `CARGO_CFG_TARGET_OS=macos`.

**Fix.** One change kills all of it:

```sh
git rm --cached rust-project.json
printf 'rust-project.json\n' >> .gitignore
```

Harden `build-ra.sh` (it already runs the right generator) and make it the documented first step:

```sh
#!/usr/bin/env bash
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BAZEL="${BAZEL:-$(command -v bazelisk || command -v bazel || true)}"
[[ -n "$BAZEL" ]] || { echo "error: bazel/bazelisk not on PATH" >&2; exit 1; }
"$BAZEL" run @rules_rust//tools/rust_analyzer:gen_rust_project -- "${@:-//...}"
```

Do **not** wrap it in an `sh_binary` — `bazel run //:gen_rust_project` nests a `bazel run` inside a `bazel run` in the same output base and blocks on the command lock, and `${BASH_SOURCE[0]}` resolves into the runfiles tree. Note `@rules_rust//tools/rust_analyzer:defs.bzl` does not exist in 0.63.0 (HTTP 404) — do not try to load from it.

---

### 4. The README's BUILD templates are not loadable Starlark, and a commit deliberately made them that way

**PROVEN.** New.

**Evidence.** `README.md:177` asserts: *"Note: In Bazel, list comprehensions at the top level don't use square brackets"* — this is false. `git show 2c55b45` ("fix: remove invalid square brackets from Bazel list comprehensions", 2025-08-15) converted **four** correct comprehensions into syntax errors and added that comment. `ast.parse()` on the resulting snippet raises `SyntaxError: invalid syntax`; the bracketed form parses. Affected blocks: `README.md:178-183, 198-203, 245-250, 259-272`.

Two more load-time failures stack on top:

- `README.md:209, 223, 267` load and call **`rust_benchmark`**. I enumerated all 29 exports of `rules_rust/rust/defs.bzl` @0.63.0 — there is no benchmark rule. The repo itself works around its absence with `rust_binary` at `server/BUILD.bazel:32`.
- `README.md:141, 157, 224` load **`@crates//:defs.bzl`**. `MODULE.bazel:37-39` exposes only `combos_crates`, `corex_crates`, `server_crates`. Under bzlmod an unmapped apparent repo name fails at load time — this is the *first line* of three of the four "Complete BUILD Configuration" snippets.

**Failure scenario.** Copy any README BUILD example and get, in order: `syntax error at 'for': expected newline`, then `does not contain symbol 'rust_benchmark'`, then `no repository visible as '@crates'`. Three loading-phase failures before a single line of Rust compiles, in the document that is the onboarding path. A package that fails to load takes down *every* target in it, not just the copied one.

**Fix.** Revert the 2c55b45 README hunks, delete the line-177 comment, and use the real names:

```python
load("@rules_rust//rust:defs.bzl", "rust_binary")
load("@server_crates//:defs.bzl", "all_crate_deps")

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

# rules_rust has no rust_benchmark; a criterion bench is a rust_binary.
[
    rust_binary(
        name = "bench_" + b.replace("benches/", "").replace(".rs", ""),
        srcs = [b],
        crate_root = b,
        edition = "2024",
        deps = all_crate_deps(normal = True, normal_dev = True),
        tags = ["manual"],
    )
    for b in glob(["benches/*.rs"])
]
```

(Use `.replace()`, not `removeprefix`/`removesuffix` — those are Starlark methods only on newer Bazel, and there is no `.bazelversion` pin.) Add a buildifier check to CI so the snippets cannot rot again.

---

### 5. `all_crate_deps(normal_dev = True)` gives the integration suite and the axum example **only** dev-dependencies

**PROVEN + PROVEN‑SRC.** New. Round 1 saw the duplication (#8); nobody saw that the flag is exclusive.

**Evidence.** cargo-bazel's generated `all_crate_deps` (template `src/rendering/templates/module_bzl.j2:177-193`) is purely additive:

```
all_dependency_maps = []
if normal:      all_dependency_maps.append(_NORMAL_DEPENDENCIES)
if normal_dev:  all_dependency_maps.append(_NORMAL_DEV_DEPENDENCIES)
...
if not all_dependency_maps: all_dependency_maps.append(_NORMAL_DEPENDENCIES)
```

For package `server`, `_NORMAL_DEV_DEPENDENCIES` is exactly `criterion` (`server/Cargo.toml:11-12`). No axum, no tokio, no serde. Cargo's contract is the opposite, and I proved it: prepending `use axum::Router; use criterion::Criterion;` to both `examples/axum.rs` and `tests/just_test.rs` gives `cargo build --example axum` → `Finished dev profile` and `cargo test --test just_test` → `2 passed`. The rustc line cargo emits for the bench target already carries `--extern axum= --extern criterion= --extern serde= --extern tokio=` — all four.

**Scope correction (this is where three lenses over-claimed).** Only two targets are genuinely deprived:

- `server/BUILD.bazel:24` — `rust_test_suite(name = "integration_tests")`, a standalone target.
- `server/BUILD.bazel:48` — `rust_binary(name = "axum_example")`, a standalone target, *named for a crate it cannot see*.

`server/BUILD.bazel:17, 57, 72`, `corex/BUILD.bazel:15, 21` and `combos/*/BUILD.bazel:13` all set `crate = …`, and `rust.bzl:369` unions the parent's deps (`deps = depset(deps, transitive = [crate.deps])`). Ground truth confirms both halves: crate 64 (`server_bin`'s test) has `[corex, axum, serde, tokio]`; crate 32 (the suite) has `deps: []`. `:bench` getting criterion-only is arguably correct for a criterion bench.

**Failure scenario.** The first line of real integration-test code — `use axum::http::StatusCode;` — passes `cargo test --test just_test` and fails `bazel test //server:integration_tests` with `error[E0432]: unresolved import 'axum'`. The three targets build today only because all three files import nothing.

**Fix.** `server/BUILD.bazel`:

```python
rust_test_suite(
    name = "integration_tests",
    srcs = glob(["tests/*.rs"]),
    shared_srcs = glob(["tests/common/**/*.rs"], allow_empty = True),
    data = glob(["tests/fixtures/**"], allow_empty = True),
    edition = "2024",
    size = "small",
    deps = all_crate_deps(normal = True, normal_dev = True) + ["//corex:corex_lib"],
)

rust_binary(
    name = "axum_example",
    srcs = ["examples/axum.rs"],
    crate_root = "examples/axum.rs",
    edition = "2024",
    deps = all_crate_deps(normal = True, normal_dev = True),
)
```

Do **not** write `":server_lib"` in these lists until finding 10's library split has actually landed — that target does not exist and the BUILD file will not load.

---

### 6. `glob(["tests/**"])` is a loading-phase landmine for the whole `//server` package

**PROVEN + PROVEN‑SRC.** Deepens round-1 #9, which called it "too loose". It is worse than loose.

**Evidence.** `server/BUILD.bazel:23`. `rules_rust/rust/private/rust.bzl:1519-1520`:

```python
for src in srcs:
    if not src.endswith(".rs"):
        fail("srcs should have `.rs` extensions")
```

then `:1529 test_name = name + "_" + src[:-3] + "_test"` and one `rust_test(crate_root = src)` per file. That naming rule reproduces the ground-truth label `server:integrated_tests_suite_tests/just_test_test` character for character — including the literal `/` inside a Bazel target name — which is independent confirmation that the macro really does expand per-file.

Cargo's rules, verified: `cargo metadata --no-deps` lists exactly **one** test target (`just_test` from `tests/just_test.rs`). Cargo treats only files directly in `tests/` as targets, treats `tests/<dir>/mod.rs` as a shared module, and ignores non-`.rs` files entirely.

**Failure scenarios.**

- Add `tests/fixtures/users.json` (or a `.golden`, or — the author is on macOS — a gitignored `.DS_Store`, which Bazel globs *do* see because globs read the filesystem, not git): the macro `fail()`s at **loading** time and `bazel build //server:server_bin` dies with a Starlark error about a JSON file. `cargo test` is unaffected.
- Add the standard `tests/common/mod.rs` helper: Bazel creates `//server:integration_tests_tests/common/mod_test` (a test crate rooted at a helper module — it compiles and reports `0 tests`, a vacuous green) **and** `just_test_test` fails with `error[E0583]: file not found for module 'common'` because `common/mod.rs` is not in its srcs. `rust_test_suite` has `shared_srcs` for exactly this and it is unused.

**Fix.** The `integration_tests` block in finding 5, plus `build --incompatible_disallow_empty_glob` in `.bazelrc`. Note: an *empty* srcs list does **not** produce a silently-passing suite — `rust.bzl:1514-1516` warns that Bazel expands a `test_suite` with `tests = []` to every test rule in the package, so you get noisy over-inclusion instead.

---

### 7. `//server:bench_test` is permanently green while `cargo test --all-targets` panics on the same file

**PROVEN.** New, and the single sharpest "the two systems return opposite verdicts" case.

**Evidence.** `cd server && cargo test --all-targets` exits 101:

```
Testing fibonacci_iterative/100
thread 'main' panicked at benches/fibonacci_benchmark.rs:32:28:
attempt to add with overflow
  22: fibonacci_benchmark::benchmark_iterative at ./benches/fibonacci_benchmark.rs:117:15
```

`benches/fibonacci_benchmark.rs:32` is `let temp = a + b;`; the sweeps at `:116`, `:128`, `:140` all iterate `[10,20,…,90,100]`; `fib(93) = 12200160415121876738` is the last u64-representable value (`u64::MAX = 18446744073709551615`).

Bazel's side, emulated exactly: `cargo rustc --bench fibonacci_benchmark -- --test` produces a binary that prints `test tests::test_fibonacci_correctness ... ok / test tests::test_large_values ... ok`. `rust_test(crate = ":bench")` recompiles the crate root with `--test`, so libtest's generated `main` displaces `criterion_main!` and **no benchmark body ever executes**. Bazel: green forever. Cargo: red.

Three further facts about this target, each verified by running the real binary:

- **`bazel run //server:bench` panics.** Bazel's default `--compilation_mode=fastbuild` maps to opt-level 0 (rules_rust `compilation_mode_opts`), and rustc's `debug-assertions` default follows opt-level — I confirmed `-C opt-level=0` panics on this exact arithmetic while `-C opt-level=1/2/3` silently returns the wrapped `3736710778780434371`. There is no `.bazelrc` to change the default.
- **Even with `-c opt` it measures nothing.** criterion 0.7.0 `src/lib.rs:949-955`: with no `--bench` argument the binary runs in `Mode::Test`. I ran the real `harness = false` release binary with no args in an empty directory: it printed `Testing <id> / Success` for each case, exited 0, and **created zero files**. So the "reports garbage timings at -c opt" claim floating in round-1-adjacent notes is wrong; the target simply never benchmarks.
- **Release mode wraps silently.** `fib_iterative(94) = 1293530146158671551` vs the true `19740274219868223167`; `fib_iterative(100) = 3736710778780434371` vs `354224848179261915075`. All three implementations wrap *identically*, so a cross-implementation consistency check at n=100 still agrees — the bug is invisible to the style of test already in the file.

**Fix.** Cap the sweeps or widen the type, at `benches/fibonacci_benchmark.rs:116`, `:128`, `:140`:

```rust
for n in [10, 20, 30, 40, 50, 60, 70, 80, 90].iter() {
```

(or move `fib_iterative`/`fib_memoized`/`fib_matrix` to `u128`, which covers up to fib(186)). Then keep the bench out of the default build surface and invoke it correctly:

```python
rust_binary(
    name = "bench",
    srcs = ["benches/fibonacci_benchmark.rs"],
    crate_root = "benches/fibonacci_benchmark.rs",
    edition = "2024",
    tags = ["manual"],                       # also keeps criterion's 30 crates out of //...
    deps = all_crate_deps(normal = True, normal_dev = True),
)
```

```sh
CRITERION_HOME=$PWD/target/criterion bazel run -c opt //server:bench -- --bench
```

and delete `bench_test` after moving its two real assertions into a library (finding 10).

---

### 8. `//server:server_bin` links two distinct `serde` rlibs; the lockfiles have already drifted

**PROVEN.** Deepens round-1 #6. Latent — nothing is broken today — but it is the structural defect that makes the repo's own stated architecture unworkable.

**Evidence.** `MODULE.bazel:23-27` and `:30-34` create separate crate_universe repos for corex and server. `rust-project.json` shows the result:

```
crate 55  serde  external/rules_rust++crate+corex_crates__serde-1.0.219/src/lib.rs
crate 58  serde  external/rules_rust++crate+server_crates__serde-1.0.219/src/lib.rs
crate 62  corex        deps = [55]
crate 64  server_bin   deps = [62 corex, 63 axum, 58 serde, 53 tokio]
```

Six duplicated pairs in total: `serde`, `serde_derive` (49/52), `syn` (35/46), `quote` (34/45), `proc-macro2` (33/44), `unicode-ident` (2/31). rules_rust emits a per-target codegen id (`rustc.bzl:984-985 --codegen=metadata=-%s`), so two Bazel targets of the same source can **never** unify.

`corex/src/lib.rs:12` is `#[derive(Debug, serde::Serialize, serde::Deserialize)] pub struct User`. I reproduced the resulting failure with rustc 1.94.1 (two rlibs of one trait crate built with different `-C metadata`):

```
error[E0277]: the trait bound `User: Serialize` is not satisfied
note: there are multiple different versions of crate `sd` in the dependency graph
```

The control build (one copy) linked and ran. `cargo` cannot produce this error, because `cargo build` in `server/` cannot even see corex.

**The skew is already real.** Parsing the two locks: `proc-macro2` corex=1.0.96 / server=1.0.97; `syn` corex=2.0.104 / server=2.0.105. They matched when `rust-project.json` was generated (both 1.0.96 / 2.0.104) — the drift arrived silently when `criterion` was added and cargo bumped the proc-macro stack in one lock only. Nothing in the repo can detect this. (Correction to one lens: the skew *itself* causes only duplicated compilation of the two most expensive crates in the graph, not a correctness bug. Two independent proc-macro repos at different `syn` majors never share types.)

**Failure scenario.** The first `async fn get_user(...) -> Json<corex::User>` — the single most obvious reason `server/BUILD.bazel:9` exists — yields `E0277 … perhaps two different versions of crate 'serde' are being used?` under Bazel, unfixable from `server/BUILD.bazel`, unreproducible under cargo. The repo's own guide already bumped into this and wrote it off: `BAZEL_RUST_GUIDE.md:335` says *"Each Bazel crate has isolated dependencies. If you use a type from a local library (like `corex::User`) that derives serde traits, you might encounter version conflicts."*

**Fix — the structural one (see PR 8).** One workspace, one lock, one crate repo. Validated with cargo 1.94.1:

```toml
# /Cargo.toml  (new)
[workspace]
resolver = "3"
members = ["corex", "server", "combos/backend", "combos/frontend"]
```

```toml
# server/Cargo.toml
[dependencies]
corex = { path = "../corex" }
```

```python
# MODULE.bazel — replaces all three from_cargo calls
crate = use_extension("@rules_rust//crate_universe:extensions.bzl", "crate")
crate.from_cargo(
    name = "crates",
    cargo_lockfile = "//:Cargo.lock",
    manifests = [
        "//:Cargo.toml",
        "//corex:Cargo.toml",
        "//server:Cargo.toml",
        "//combos/backend:Cargo.toml",
        "//combos/frontend:Cargo.toml",
    ],
)
use_repo(crate, "crates")
```

Then retarget the four `load("@*_crates//:defs.bzl", …)` lines to `@crates`. Three things every lens got wrong about this fix, and you must plan for them:

1. **You must strip `[workspace]` from `combos/Cargo.toml`.** I measured it: with the nested table left in place, `cargo metadata` from the repo root reports `workspace_root=<root>` while `cargo metadata` from `combos/backend` reports `workspace_root=<root>/combos`. Worse, `cargo-bazel`'s splicer hard-fails: `splicer.rs:51-54` bails with *"manifests are not allowed to be from different workspaces"* when the listed manifests resolve to more than one workspace root.
2. **`all_crate_deps` will never emit `//corex:corex_lib`.** `partials/module/deps_map.j2:31,41` explicitly `{% continue %}` on workspace members ("Workspace member repositories are not defined"). Keep `+ ["//corex:corex_lib"]` in `deps` by hand, forever.
3. **A fresh root lock is a full dependency bump.** I generated it: 110 packages, with `axum 0.8.4→0.8.9`, `serde 1.0.219→1.0.229`, `matchit 0.8.4→0.8.6`, and *two* `syn` (2.0.119 and 3.0.3 — genuinely different majors, which is fine). If current versions matter, pin them back with `cargo update -p <crate> --precise <ver>`.

**Interim guard**, if the three-lock layout must survive a release cycle — run in CI:

```bash
python3 - <<'EOF'
import re, sys, collections
seen = collections.defaultdict(dict)
for f in ['corex/Cargo.lock','server/Cargo.lock','combos/Cargo.lock']:
    for n,v in re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', open(f).read()):
        seen[n][f] = v
bad = {n:m for n,m in seen.items() if len(set(m.values())) > 1}
if bad: print('cross-lock version skew:', bad); sys.exit(1)
EOF
```

It currently exits 1 on `proc-macro2` and `syn`.

---

### 9. The `//corex:corex_lib` edge exists only in Bazel — and the docs teach that as the rule

**PROVEN.** New framing; round 1 did not have this.

**Evidence.** `server/BUILD.bazel:8-10` (and `:48-50`, `:63-65`) add `//corex:corex_lib`. `server/Cargo.toml` has no corex entry. `grep -rn corex --include=*.rs` over the whole repo returns **nothing** — the edge contributes zero symbols today while pulling 6 extra crates (`corex_crates`' serde/serde_derive/syn/quote/proc-macro2/unicode-ident) into every `bazel build //server:server_bin`.

The Cargo half, proven by inserting `use corex::User as CorexUser;` at `server/src/main.rs:1` (reverted; tree clean):

```
error[E0432]: unresolved import `corex`
  | use corex::User as CorexUser;
  |     ^^^^^ use of unresolved module or unlinked crate `corex`
```

This is not an accident. `README.md:56-59` and `BAZEL_RUST_GUIDE.md:48-53` document it as policy: *"DON'T add to Cargo.toml: `corex = { path = "../corex" }` … Path dependencies (`path = "../corex"`) break Bazel's crate_universe."*

**The docs' claim is half-true, which is why it is dangerous.** A path dep *does* break crate_universe when the referenced manifest is not listed in `manifests` — `splicer.rs:207-211 splice_package` symlinks only `manifest_dir`, so `../corex` escapes the spliced root. But once the manifest **is** listed (or the members share one workspace), path deps are fine and are the only way to make the two graphs agree. The guide states the special case as a universal law and thereby manufactures the divergence.

**Failure scenario.** Two graphs that disagree by construction. `cargo build`/`check`/`clippy`/`doc` and any Cargo-driven rust-analyzer session in `server/` fail on any real use of corex, while Bazel is green. There is no CI to arbitrate. Meanwhile the two comments in the BUILD file — `# include if needed` (`:49`), `# include if proxy depends on corex` (`:64`) — mean nobody knows whether the edges are real, so nobody deletes them, and touching `corex/src/lib.rs` rebuilds `server_bin`, `proxy`, `axum_example` and all their tests for nothing.

**Fix.** Short term (this week): delete the two speculative edges at `server/BUILD.bazel:49` and `:64` — neither `examples/axum.rs` nor `src/bin/proxy.rs` references corex. Long term: finding 8's workspace collapse, and rewrite `BAZEL_RUST_GUIDE.md:48-53` / `:97` / `README.md:56-59` to the accurate rule:

> Path dependencies are supported and required for a single-lockfile setup. They fail only when the referenced `Cargo.toml` is not listed in `crate.from_cargo(manifests = …)`. First-party deps are never emitted by `all_crate_deps()` — always add the Bazel label by hand.

Then delete `BAZEL_RUST_GUIDE.md:335`'s "define serializable types in the crate that uses them" workaround, which becomes obsolete.

---

### 10. `server` has no library target, so nothing can be integration-tested and the only real tests live where cargo never runs them

**PROVEN.** Deepens round-1 #7 with the mechanism and the consequence.

**Evidence.** The two most substantive tests in the repo — `test_fibonacci_correctness` and `test_large_values`, `benches/fibonacci_benchmark.rs:188` and `:211` — are compiled but never executed by cargo. From the actual rustc command line (`cargo build --all-targets -v`):

```
rustc --crate-name fibonacci_benchmark --edition=2024 benches/fibonacci_benchmark.rs … --cfg test …
```

`--cfg test` is present; `--test` is **not**. `harness = false` (`server/Cargo.toml:14-16`) means cargo type-checks the module (proof: `warning: unused import: 'super::*' --> benches/fibonacci_benchmark.rs:186:9`) and then runs criterion's `main` instead of libtest. Plain `cargo test` in `server/` runs exactly three binaries — proxy 2, main 2, just_test 2 — and neither of those names appears.

Only `bazel test //server:bench_test` runs them. The same asymmetry applies to `examples/axum.rs` (`cargo metadata` → `test False`): two more tests Bazel runs and cargo does not.

**Failure scenario.** A developer runs `cargo test`, sees 6 green, and believes the fibonacci implementations are covered. They are not. The first CI a Rust repo gets is a cargo one, and it silently drops the repo's only meaningful assertions. Meanwhile `server/tests/` cannot ever test the server, because there is no library to import and (finding 5) no normal deps.

**Fix.** Split the binary:

```toml
# server/Cargo.toml
[lib]
name = "server"
path = "src/lib.rs"
```

Move `fib_recursive`/`fib_iterative`/`fib_memoized`/`fib_matrix` and the router construction into `server/src/lib.rs` as `pub` items, have `benches/fibonacci_benchmark.rs` do `use server::*;` and delete its `mod tests`, and put the two assertions in `src/lib.rs` under `#[cfg(test)] mod tests`.

```python
rust_library(
    name = "server_lib",
    srcs = glob(["src/**/*.rs"], exclude = ["src/main.rs", "src/bin/**"]),
    crate_name = "server",
    edition = "2024",
    version = "0.1.0",
    deps = all_crate_deps(normal = True) + ["//corex:corex_lib"],
    proc_macro_deps = all_crate_deps(proc_macro = True),
    visibility = ["//visibility:public"],
)
rust_test(name = "server_lib_test", crate = ":server_lib", size = "small")
```

Then `server_bin`, `proxy`, `axum_example`, `bench` and `integration_tests` all take `":server_lib"`, and `bench_test` / `axum_example_test` get deleted. This closes the test-inventory divergence (table row 13) in one move.

---

### 11. A green `bazel test //...` certifies essentially nothing

**PROVEN.** New (round 1 never audited the tests themselves).

**Evidence — full census of all 16 test cases.** 3 are non-vacuous:

| location | test | verdict |
|---|---|---|
| `corex/src/lib.rs:39` | `it_works` — `add(2,2)==4` | real (trivial) — the **only** non-vacuous test cargo executes by default |
| `benches/…:188`, `:211` | `test_fibonacci_correctness`, `test_large_values` | real — but cargo never runs them (finding 10) |
| `corex/src/lib.rs:45`, `server/src/main.rs:62,67`, `tests/just_test.rs:12,24`, `frontend/src/main.rs:9,14` | 7× `assert!(true)` | vacuous — clippy flags every one: `this assertion is always 'true'` at `src/lib.rs:46:9`, `tests/just_test.rs:13:9` and `:25:9`, `src/main.rs:63:6` and `:68:8`, `frontend/src/main.rs:10:9` and `:15:9` |
| `proxy.rs:9`, `examples/axum.rs:9` | 2× `assert_eq!(1,1)` | vacuous |
| `proxy.rs:13-17`, `examples/axum.rs:13-17` | 2× byte-identical `#[should_panic] fn it_will_fail() { assert_eq!(1,2) }` | **inverted tautology** |
| `corex/src/lib.rs:1-3`, `:19-23` | 2 doctests | decorative |
| `combos/backend/src/main.rs` | — | **no tests at all** |

Two proofs beyond the census:

- **Doctests survive mutation.** I changed `add` to `left - right` and `User::new` to store `age: 0`, then ran `cargo test --doc`: `2 passed`. Both doctests are `assert!(true)`; the `User::new` doctest never names `User` or calls `new`. Reverted; tree clean.
- **A test target with zero tests reports PASSED.** `cargo test -p backend` → `running 0 tests / test result: ok`. `//combos/backend:backend_tests` (`combos/backend/BUILD.bazel:10-14`) wraps exactly that binary.

The `should_panic` pair is actively hostile: it asserts that `assert_eq!` still panics (a property of the standard library), the natural cleanup edit (`1,2` → `1,1`) turns a passing test into a failing one, and the bare `#[should_panic]` with no `expected =` will keep reporting `ok` once real code in `proxy.rs` panics for an unrelated reason.

**Failure scenario.** `bazel test //...` returns 9 PASSED. Mutate every handler in `server/src/main.rs` to return a wrong value and all 9 stay green. The build system's most valuable property — a trustworthy `test //...` gate — is being demonstrated on a suite that cannot detect regressions.

**Fix.** Delete the two `it_will_fail` blocks, the seven `assert!(true)` bodies, and `//combos/backend:backend_tests` (until `combos/backend/src/main.rs` has a `#[cfg(test)] mod tests`). Replace the doctests with ones that assert the documented behaviour (`assert_eq!(corex::add(2, 2), 4);` — note `add(0, u64::MAX)` does not overflow, so boundary doctests are safe). If you want a `should_panic` test, make it falsifiable and profile-independent:

```rust
impl User {
    pub fn new(name: String, age: u8) -> Self {
        assert!(!name.is_empty(), "user name must not be empty");
        Self { name, age }
    }
}

#[test]
#[should_panic(expected = "user name must not be empty")]
fn new_rejects_empty_name() { User::new(String::new(), 1); }
```

Do **not** use `#[should_panic(expected = "attempt to add with overflow")]` — I verified that only panics at opt-level 0, so it fails under `cargo test --release` and `bazel test -c opt`. Adding `#![cfg_attr(test, deny(clippy::assertions_on_constants))]` covers only 5 of the 7 sites; `tests/just_test.rs`, `src/bin/proxy.rs` and `examples/axum.rs` are separate crate roots and need their own.

---

### 12. Every Bazel-built artifact reports the wrong package identity and version

**PROVEN + PROVEN‑SRC.** New.

**Evidence.** The env maps rules_rust baked into each crate, straight out of `rust-project.json`:

| target | `CARGO_PKG_NAME` | `CARGO_CRATE_NAME` | `CARGO_PKG_VERSION` |
|---|---|---|---|
| corex | `corex_lib` | `corex` | `0.0.0` |
| server | `server_bin` | `server_bin` | `0.0.0` |
| backend / frontend | `backend_bin` / `frontend_bin` | — | `0.0.0` |
| the test suite | `integrated_tests_suite_tests/just_test_test` (with a literal `/`) | — | `0.0.0` |

Every `Cargo.toml` says `version = "0.1.0"`. Mechanism confirmed in source: `rustc.bzl:138 "CARGO_PKG_NAME": attr.name` — the **target** name — and `rust.bzl:737 "version": attr.string(default = "0.0.0")`. No BUILD target sets `version` or `rustc_env` (`grep` returns nothing). No crate has a `CARGO_BIN_EXE_*` key at all. Output filename comes from the target name (`rust.bzl:232-235`), so Bazel emits `bazel-bin/server/server_bin` where cargo emits `target/debug/server`.

**Failure scenario.** The first `/version` handler, `clap #[command(version)]`, `User-Agent`, Prometheus `build_info` gauge or Sentry release tag reports `0.0.0` from every Bazel-built binary and `0.1.0` from every cargo one. It compiles, it runs, and the wrong string only shows up in production telemetry. Separately, `env!("CARGO_BIN_EXE_server")` — the idiomatic way for `tests/` to spawn the binary under test — compiles under cargo and is a hard error under Bazel.

**Fix.** Set identity on every target that compiles sources, including the `crate = …` tests (they compute their own env from their own attrs, so a test asserting `CARGO_PKG_VERSION == "0.1.0"` fails unless you set it there too):

```python
VERSION = "0.1.0"

rust_binary(
    name = "server_bin",
    srcs = ["src/main.rs"],
    crate_name = "server",
    edition = "2024",
    version = VERSION,
    rustc_env = {"CARGO_PKG_NAME": "server"},
    deps = [":server_lib"] + all_crate_deps(normal = True),
)

rust_test(
    name = "tests",
    crate = ":server_bin",
    version = VERSION,
    rustc_env = {"CARGO_PKG_NAME": "server"},
    size = "small",
)
```

`rustc_env` wins: `rustc.bzl:888` seeds defaults, `:1082-1087` does `env.update(crate_info.rustc_env)` afterwards. For `CARGO_BIN_EXE_*`, supply it explicitly (`rustc_env` values are `$(rootpath)`-expanded against `data`):

```python
rust_test_suite(
    name = "integration_tests",
    data = [":server_bin"],
    rustc_env = {"CARGO_BIN_EXE_server": "$(rootpath :server_bin)"},
    ...
)
```

Consider renaming `server_bin` → `server` so `bazel-bin/server/server` matches `target/debug/server` and Dockerfiles stop being toolchain-specific.

---

### 13. Adding CI is red on day one — and two cells of the obvious matrix are structurally red

**PROVEN.** Deepens round-1 #15/#16.

**Measured, not guessed:**

- `cd corex && cargo clippy --all-targets -- -D warnings` → exit 101, 1 error.
- `cd server && cargo clippy --all-targets -- -D warnings` → 3 targets fail to compile; 6 lint errors (4× assertions-on-constants, 1× `manual implementation of .is_multiple_of()`, 1× `unused import: super::*`).
- `cd combos && cargo clippy --all-targets --workspace -- -D warnings` → 2 errors (`frontend/src/main.rs:10`, `:15`).
- `cargo fmt --all --check` → 8 hunks across `corex/src/lib.rs:5,21`, `server/src/main.rs:1,5,60,67`, `server/benches/fibonacci_benchmark.rs:9`, `server/tests/just_test.rs:1,8,14`. combos is clean.
- `cd server && cargo test --all-targets` → exit 101 (finding 7).
- `cargo test --doc` at the repo root → `error: could not find 'Cargo.toml' in /home/user/complex-bazel-setup or any parent directory`. **There is no root manifest**, so CI must invoke cargo three times with three working directories.
- `cargo test --doc` in `server/` → exit 101, `error: no library targets found in package 'server'`. Same in `combos/`. **Only corex passes.** A naive 3-cell doctest matrix is permanently red in 2 cells until finding 10 lands.

**Failure scenario.** The first PR that adds CI is blocked by ~9 pre-existing lint failures unrelated to the change, so the team ships CI with `continue-on-error` and never re-enables it. This is why lint debt is a *prerequisite* PR, not a follow-up.

**Fix.** Clear the debt first (PR 4), then gate. Two traps in the obvious workflow:

- Make doctests conditional on a lib target existing:

```yaml
- name: doctests (only where a lib target exists)
  working-directory: ${{ matrix.dir }}
  run: |
    if cargo metadata --no-deps --format-version=1 \
       | jq -e '[.packages[].targets[] | select(.kind[] == "lib")] | length > 0' >/dev/null; then
      cargo test --doc --locked
    else
      echo "no lib targets in ${{ matrix.dir }}; skipping"
    fi
```

- A `git diff --exit-code -- MODULE.bazel.lock` drift gate is a **silent no-op** while `.gitignore:4` ignores that file. Delete that line and commit the lock, or delete the gate.

---

### 14. `deps` on `rust_test(crate = …)` is additive, so four test targets drag in criterion and 30 transitive crates they never use

**PROVEN + PROVEN‑SRC.** New.

**Evidence.** `rules_rust/rust/private/rust.bzl:369`: `deps = depset(deps, transitive = [crate.deps])` — the rule's own `deps` are **unioned** with the inherited crate's, not ignored. Consequences:

- `corex_tests` (`corex/BUILD.bazel:15`) and `corex_doc_test` (`:21`) pass `all_crate_deps(normal_dev = True)` but `corex/Cargo.toml` has no `[dev-dependencies]` at all → literally `[]`, a no-op copied from `BAZEL_RUST_GUIDE.md:210-216` "Pattern 3: Tests with Dev Dependencies". (Confirmed harmless, not a `fail()`: `deps_map.j2` emits a key for every workspace member in every map, so the empty case returns `[]`.)
- `//server:tests` (`:17`), `:axum_example_test` (`:57`) and `:proxy_test` (`:72`) attach criterion to crates whose sources never mention it. Measured cost: `cargo tree --target x86_64-unknown-linux-gnu -e normal,build` = **56** packages; `-e normal,build,dev` = **86**. Criterion drags in **30** crates — plotters ×3, rayon ×3, clap ×3, regex ×3, ciborium ×3, walkdir, tinytemplate, itertools, half, wasm-bindgen stack, …
- `bench_test` (`:40-43`), which omits `deps` entirely, is the only member of the family that is right.

**Failure scenario.** `bazel test //server:tests` — two `assert!(true)` unit tests — analyses and builds criterion plus 29 transitive crates first. On a cold CI cache that is minutes per target, ×4 targets. It also destroys the BUILD file's value as documentation: nothing tells you which test actually needs criterion.

**Fix.** Delete the `deps` attribute from every `rust_test` that sets `crate =`:

```python
rust_test(name = "corex_tests",       crate = ":corex_lib", size = "small")
rust_doc_test(name = "corex_doc_test", crate = ":corex_lib")
rust_test(name = "tests",             crate = ":server_bin", size = "small")
rust_test(name = "proxy_test",        crate = ":proxy",      size = "small")
rust_test(name = "frontend_tests",    crate = ":frontend_bin", size = "small")
```

---

### 15. The orphaned `build_script` is a loaded gun, and `build.rs` has no `rerun-if-changed`

**PROVEN.** Deepens round-1 #5 with two things round 1 did not have: a measured cargo cost today, and the exact failure shape tomorrow.

**Evidence.** `server/BUILD.bazel:27-30` is `cargo_build_script(name = "build_script", srcs = ["build.rs"])` with no `deps`, no `edition`, no `visibility`, and **zero reverse dependencies**. Cargo, meanwhile, runs it for all 5 compile targets (`cargo metadata` lists `['custom-build'] build-script-build` alongside bin server, bin proxy, example axum, test just_test, bench fibonacci_benchmark).

The whole script is:

```rust
fn main() { println!("this is rust build.rs"); }
```

That line is not a `cargo:`/`cargo::` directive, so cargo swallows it into `target/debug/build/server-9967fcf3f7be388d/output`. But the **missing** directive costs real time today. Measured: `touch src/bin/proxy.rs && cargo build --all-targets -v` re-runs the build script and recompiles `axum` (the example), `fibonacci_benchmark`, `just_test`, `proxy` ×2 and `server` ×2 — *every* target in the package. With one line added (`println!("cargo::rerun-if-changed=build.rs")`) the same touch runs the script 0 times and recompiles only `proxy` ×2. (Both edits reverted; tree clean.)

**Failure scenario.** Two shapes, both conditional on a future edit, and the day is plausible — commit `dad071f` is "remove generated file" and a stale `server/target/debug/build/server-…/out/generated.rs` still contains `pub const GEN: u32 = 42;`, so this build script previously wrote OUT_DIR content.

- **Silent:** `println!("cargo::rustc-cfg=has_tls")` → cargo compiles the `#[cfg(has_tls)]` branch in, Bazel compiles it out. Two different binaries from one source, no warning.
- **Hard:** `include!(concat!(env!("OUT_DIR"), "/generated.rs"))` → cargo succeeds, Bazel fails with `error: environment variable 'OUT_DIR' not defined at compile time`. Verified both error texts directly.

**Fix — delete it.** Wiring a script whose body is one `println!` into five targets adds a build+run action to each for zero output:

```sh
git rm server/build.rs
# and delete server/BUILD.bazel:27-30
```

If it is ever revived, the correct shape is:

```python
cargo_build_script(
    name = "build_script",
    srcs = ["build.rs"],
    edition = "2024",
    version = "0.1.0",
    deps = all_crate_deps(build = True),
    proc_macro_deps = all_crate_deps(build_proc_macro = True),
    visibility = ["//visibility:private"],
)
```

plus `":build_script"` in the `deps` of **all five** consumers, and `println!("cargo::rerun-if-changed=build.rs");` as the first line of `main`. Never read a clock in `build.rs` — a `SystemTime::now()` stamp makes the Bazel action non-deterministic (cache miss on every run) *and*, once `rerun-if-changed` is set, freezes a stale value under cargo. Use Bazel's `--stamp` / `{STABLE_VERSION}` for build stamps.

---

### 16. No `.bazelrc`, `.bazelversion`, `.bazelignore`, or hermetic C toolchain — with 25 build scripts and 2 `links` crates in the graph

**PROVEN + PROVEN‑SRC.** Deepens round-1 #3/#14 with the actual file.

**Evidence.** `ls -a` confirms none of the three files exist. Yet `.gitignore:2-4` already reserves `.bazelrc.user`, `.bazelversion.user`, `MODULE.bazel.lock` — the author planned a layered config and never wrote the base, so that first gitignore line is dead. `combos/target/`, `server/target/`, `corex/target/` all exist on disk with no `.bazelignore` to keep Bazel's package loader out.

`cargo metadata --locked` on `server/` → 119 packages, of which **25** have a `custom-build` target (crossbeam-utils, crunchy, httparse, io-uring, libc, lock_api, num-traits, object, parking_lot_core, proc-macro2, rayon-core, rustversion, serde, serde_json, server, wasm-bindgen, wasm-bindgen-shared, and 8 `windows_*`), and **2** declare `links` (`rayon-core`, `wasm-bindgen-shared` → `wasm_bindgen`). `cargo/private/cargo_build_script.bzl:369,419` call `find_cc_toolchain`, and every rust rule declares `@bazel_tools//tools/cpp:toolchain_type`. With no `.bazelrc` and no `toolchains_llvm` dep, `local_config_cc` autodetection picks up whatever `/usr/bin/cc` or Xcode CLT the machine has.

Also from the table (row 15): cargo dev = `-C debuginfo=2` + incremental; Bazel fastbuild = debuginfo 0, no incremental. `README.md:128` and `BAZEL_RUST_GUIDE.md:258,321` all say plain `bazel run //server:server_bin`, and neither document mentions `-c dbg` — so backtraces have bare addresses and lldb has nothing to attach to.

**Fix.** Create `.bazelrc`:

```
# ---------- correctness guards ----------
build --incompatible_disallow_empty_glob
build --incompatible_strict_action_env      # fixed PATH -> action keys match laptop and CI

# ---------- match cargo's dev profile ----------
build --compilation_mode=dbg
build:release --compilation_mode=opt
build:release --strip=always

# ---------- rules_rust ----------
build --@rules_rust//rust/settings:pipelined_compilation=True

# ---------- caching ----------
build --disk_cache=~/.cache/bazel-disk
build --repository_cache=~/.cache/bazel-repo

# ---------- tests ----------
test --test_output=errors
test --build_tests_only
test --test_verbose_timeout_warnings

# ---------- lint ----------
build:lint --aspects=@rules_rust//rust:defs.bzl%rustfmt_aspect,@rules_rust//rust:defs.bzl%rust_clippy_aspect
build:lint --output_groups=+rustfmt_checks,+clippy_checks
build:lint --@rules_rust//rust/settings:clippy_flags=-Dwarnings
build:lint --@rules_rust//rust/settings:clippy.toml=//:clippy.toml
build:lint --@rules_rust//rust/settings:rustfmt.toml=//:rustfmt.toml

# ---------- coverage ----------
coverage --combined_report=lcov
coverage --instrumentation_filter=^//corex[:/],^//server[:/],^//combos[:/]

# ---------- CI ----------
common:ci --lockfile_mode=error     # requires un-gitignoring MODULE.bazel.lock
build:ci  --disk_cache=/tmp/bazel-disk --announce_rc --verbose_failures --keep_going
test:ci   --test_output=errors --flaky_test_attempts=1

try-import %workspace%/.bazelrc.user
```

Every rules_rust flag label above was checked against the 0.63.0 tag (`rust/settings/BUILD.bazel` declares `pipelined_compilation`, `clippy_flags`, and label_flags literally named `clippy.toml` / `rustfmt.toml`; the output groups are literally `rustfmt_checks` and `clippy_checks`). Do **not** add `common --enable_bzlmod` (default since Bazel 7) or `--@rules_rust//rust/settings:experimental_use_cc_common_link=False` (already the default).

Plus `.bazelversion` (pin the Bazel actually tested — Bazel 7 vs 8 changed bzlmod semantics), `.bazelignore` containing `corex/target`, `server/target`, `combos/target`, and — for hermeticity — a pinned C toolchain:

```python
bazel_dep(name = "toolchains_llvm", version = "1.4.0")
llvm = use_extension("@toolchains_llvm//toolchain/extensions:llvm.bzl", "llvm")
llvm.toolchain(llvm_version = "19.1.0")
use_repo(llvm, "llvm_toolchain")
register_toolchains("@llvm_toolchain//:all")
```

---

### 17. Zero supply-chain controls, and the obvious automation actively breaks the build

**PROVEN + PROVEN‑SRC.** Deepens round-1 #4/#20.

**Evidence.** `MODULE.bazel:3` is `bazel_dep(name = "rules_rust", version = "0.63.0")  # check for latest release` — the comment is the entire upgrade process. `MODULE.bazel.lock` is gitignored (`.gitignore:4`), so there is no integrity record of what the BCR resolved to; two developers can resolve different transitive module versions and neither can tell. No `deny.toml`, no `dependabot.yml`, no `renovate.json`, no SBOM, no license policy — across 119 + 7 + 2 = **128** locked packages.

The compounding failure is specific and verified in source: `crate_universe/private/generate_utils.bzl:415-426` detects a stale lockfile and calls `fail()` with *"The current `lockfile` is out of date … Please re-run bazel using `CARGO_BAZEL_REPIN=true`"*. Dependabot has **no** bzlmod/`bazel_dep` ecosystem, so a Dependabot cargo PR updates `Cargo.lock`, passes the cargo job, and hard-fails the Bazel job — and because `MODULE.bazel.lock` is gitignored the symptom is "works on my machine after a repin" rather than a clean diff. Renovate does ship a `bazel-module` manager; use it.

**Fix.** Delete `.gitignore:4` and commit `MODULE.bazel.lock`; add `common:ci --lockfile_mode=error`. Add `deny.toml`:

```toml
[graph]
targets = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin"]
all-features = true

[advisories]
yanked = "deny"

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]
confidence-threshold = 0.93

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = [{ name = "openssl-sys", reason = "use rustls; openssl-sys breaks a musl static build" }]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

and `renovate.json` covering both ecosystems, with the repin coupling made explicit in the PR body:

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": ["config:recommended", "helpers:pinGitHubActionDigests"],
  "enabledManagers": ["cargo", "bazel-module", "github-actions"],
  "lockFileMaintenance": { "enabled": true },
  "packageRules": [
    { "matchManagers": ["cargo"], "automerge": false,
      "prBodyNotes": ["After merging you MUST repin crate_universe and commit MODULE.bazel.lock, or `bazel build //...` uses stale vendored crates."] },
    { "matchManagers": ["bazel-module"], "matchPackageNames": ["rules_rust"],
      "automerge": false, "labels": ["toolchain", "needs-manual-verification"] }
  ],
  "vulnerabilityAlerts": { "labels": ["security"], "schedule": ["at any time"] }
}
```

---

### 18. `*.md` in the root `.gitignore` makes `CONTRIBUTING.md` silently unaddable — and there are six `.gitignore` files, not four

**PROVEN.** Corrects and deepens round-1 #12/#13.

**Evidence.** `.gitignore:77-79`:

```
*.md
!BAZEL_RUST_GUIDE.md
!README.md
```

```sh
$ touch CONTRIBUTING.md && git check-ignore -v CONTRIBUTING.md
.gitignore:77:*.md	CONTRIBUTING.md
$ git status --porcelain     # empty
```

It applies at any depth — `corex/NOTES.md`, an ADR, `docs/architecture.md`, all silently swallowed. `git ls-files` also confirms **zero** `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.editorconfig`, `CODEOWNERS`, `CONTRIBUTING.md`, `Makefile`, `justfile`, `.pre-commit-config.yaml`. Consequence already visible: `server/BUILD.bazel:42` carries trailing whitespace (`crate = ":bench",   `), lines 19-20 and 38-39 are stray double blanks, and the file has no final newline.

**Round-1 correction:** there are **six** `.gitignore` files (root + `combos/`, `combos/backend/`, `combos/frontend/`, `corex/`, `server/`), not four. `md5sum` gives `cb20a5d35ce6b525166d98c86b144aa1` for all five leaves and `9f1c661030b4120bcabdfb472c94cfd4` for the root. `diff <(sed -n '1,76p' .gitignore) corex/.gitignore` shows the leaves are exactly the root file minus the trailing `*.md` block — and root patterns apply recursively, so deleting them is safe.

**Fix.**

```diff
 # .gitignore
-*.md
-!BAZEL_RUST_GUIDE.md
-!README.md
+/docs/generated/
```

```sh
git rm corex/.gitignore server/.gitignore combos/.gitignore \
       combos/backend/.gitignore combos/frontend/.gitignore
```

Also change `.vscode/` to `.vscode/*` plus `!.vscode/settings.json`, `!.vscode/extensions.json` — a trailing-slash directory ignore stops git descending, so a later negation cannot un-ignore anything, and `git add .vscode/tasks.json` is refused. (This matters because `README.md:100-102` tells contributors to use `.vscode/tasks.json`, a `Cmd+R Cmd+R` keybinding, and `./refresh-rust-analyzer.sh` — none of which exist and the first of which cannot be committed.) Do **not** ship `"rust-analyzer.linkedProjects": ["rust-project.json"]` in a committed settings.json; that pins every contributor to the poisoned file from finding 3.

Add `rustfmt.toml` (`edition = "2024"`, `max_width = 100`), `clippy.toml` (`msrv = "1.94"`), and an `.editorconfig` with `trim_trailing_whitespace = true` / `insert_final_newline = true`.

---

### 19. Docs name targets, files and commands that do not exist — 6 of 9 "Quick Commands", 12 fabricated paths, and an unrunnable repin

**PROVEN** (including by installing the real tool). New.

**Quick Commands** (`README.md:508-538`) — six of nine bazel commands name nonexistent targets:

| README | reality |
|---|---|
| `//corex:unit_tests` (:518) | `//corex:corex_tests` (`corex/BUILD.bazel:13`) |
| `//corex:test_integration` (:519) | corex has **no** `tests/` directory |
| `//corex:doc_tests` (:520) | `//corex:corex_doc_test` (`:19`) |
| `//corex:example_basic` (:523) | corex has no `examples/` |
| `//server:example_client` (:524) | `//server:axum_example` (`server/BUILD.bazel:46`) |
| `//corex:bench_performance` (:527) | corex has no `benches/`; the bench is `//server:bench` |

**Cargo-runner section** (`README.md:561-698`, 138 lines). I installed the real tool (`cargo install cargo-runner-cli --version 2.1.4 --locked`) and ran `cargo runner run --dry-run` on every real file. The section cites **12 nonexistent source paths** (`corex/tests/integration_test.rs`, `corex/examples/client.rs`, `corex/benches/performance.rs`, `corex/build.rs`, `server/examples/demo.rs`, `server/tests/api_test.rs`, `combos/shared/src/lib.rs`, …) and **9 nonexistent targets**, and gets the command wrong for the two real files it does use:

- `:600` claims `--test_arg --exact --test_arg bin::proxy`; **actual**: `bazel test //server:proxy_test --test_output streamed --test_arg --nocapture --test_arg tests::test_proxy_binary --test_arg --exact`.
- `:608` claims `//corex:unit_tests`; **actual**: `bazel test //corex:corex_tests --test_output streamed --test_arg --nocapture`.
- `:648-652` claims corex unit tests "include doctests"; **actual**: `corex/src/lib.rs:2` yields a *separate* target, `bazel test //corex:corex_doc_test`.
- `:616,:619` claim `--test_filter=<name>`; the tool never emits `--test_filter`.

There are also **no install instructions anywhere** — and the obvious `cargo install cargo-runner` fetches an unrelated crate: crates.io says `cargo-runner` is v0.1.1, last published **2019-09-01**, *"Tool to help with the `.cargo/config` `target.$triple.runner` field"*. The right crate is `cargo-runner-cli` v2.1.4 (`github.com/cargo-runner/cargo-runner`), which installs an executable named `cargo-runner`.

**Repin command** (`README.md:44`): `CARGO_BAZEL_REPIN=1 bazel sync --only=crates --enable_workspace` is a **silent no-op**. There is no repo named `crates`. `SyncCommand.java:245-249 shouldSync()` returns false for anything that is not a workspace-only rule ("We should only sync workspace rules"), so bzlmod extension repos are never candidates; with `--enable_workspace` and no WORKSPACE file the command walks zero rules and exits 0.

**Flagship main.rs example** (`README.md:357-415`): extracted verbatim into a scratch crate with `server/Cargo.toml`'s exact deps and got `error[E0433]: could not find 'Server' in 'axum'` (removed in axum 0.7) and `error[E0432]: unresolved import 'corex'` (`use corex::Calculator;` — corex exports only `add()` and `struct User`). Fixing only the first yields a **runtime panic**: `README.md:379`'s `.route("/users/:id", …)` panics on axum 0.8 with *"Path segments must not start with ':'. For capture groups, use {capture}."* `BAZEL_RUST_GUIDE.md:283-317` shows the correct modern form — the two documents contradict each other, and `README.md:132`'s own `curl http://localhost:3000/users/uriah` targets a route that never registers.

**Directory tree** (`README.md:7-39`) shows `server/` as containing only `BUILD.bazel`, `Cargo.toml`, `src/main.rs` — hiding `benches/`, `examples/`, `tests/`, `src/bin/`, `build.rs`, `Cargo.lock`, i.e. the four directories that produce 7 of the repo's 17 targets, plus `build-ra.sh` and all three `Cargo.lock` files at the root.

**Fix.** Replace the Quick Commands block with labels verified against the current BUILD files:

```bash
bazel build //...
bazel test  //...
bazel test  //corex:corex_tests //corex:corex_doc_test
bazel test  //server:tests //server:integration_tests //server:proxy_test
bazel run   //server:server_bin
bazel run   //server:proxy
bazel run   //server:axum_example
bazel run -c opt //server:bench -- --bench
bazel run   //combos/backend:backend_bin
./build-ra.sh
```

Delete `README.md:43-45` entirely — **no repin step is needed today**: `generate_utils.bzl:389-391` returns `True` from `determine_repin()` whenever no `lockfile` attribute is set, and none of the three `crate.from_cargo` tags set one, so crate_universe repins unconditionally on every fetch. (That is also why `README.md:543`'s "Hermetic builds — reproducible across machines" is false.) If a `lockfile` attr is ever added, the correct scoped form is `CARGO_BAZEL_REPIN=1 CARGO_BAZEL_REPIN_ONLY=server_crates bazel fetch //server/...` — `CARGO_BAZEL_REPIN_ONLY` is the documented bzlmod replacement for `--only=`; setting `CARGO_BAZEL_REPIN=server_crates` does **not** scope anything (any non-falsy value repins everything).

Add the install block at `README.md:562`:

```sh
cargo install cargo-runner-cli --version 2.1.4 --locked
```
> The crate is `cargo-runner-cli`, **not** `cargo-runner` (an unrelated 2019 crate). Source: https://github.com/cargo-runner/cargo-runner

Replace the 138-line cargo-runner section with this table, every row of which I reproduced byte-for-byte from the real tool:

| you run | cargo-runner emits |
|---|---|
| `cargo runner run server/src/main.rs` | `bazel run //server:server_bin` |
| `cargo runner run server/src/main.rs:62` | `bazel test //server:tests --test_output streamed --test_arg --nocapture --test_arg tests::it_works --test_arg --exact` |
| `cargo runner run server/src/bin/proxy.rs` | `bazel run //server:proxy` |
| `cargo runner run server/src/bin/proxy.rs:9` | `bazel test //server:proxy_test … --test_arg tests::test_proxy_binary --test_arg --exact` |
| `cargo runner run corex/src/lib.rs` | `bazel test //corex:corex_tests --test_output streamed --test_arg --nocapture` |
| `cargo runner run corex/src/lib.rs:2` | `bazel test //corex:corex_doc_test --test_output streamed` |
| `cargo runner run server/tests/just_test.rs:12` | `bazel test //server:integration_tests … --test_arg tests::just_test::tests::see_if_it_works --test_arg --exact` |
| `cargo runner run server/benches/fibonacci_benchmark.rs` | `bazel run //server:bench -c opt` |
| `cargo runner run server/examples/axum.rs` | `bazel run //server:axum_example` |
| `cargo runner run combos/frontend/src/main.rs:14` | `bazel test //combos/frontend:frontend_tests … --test_arg tests::it_works_too --test_arg --exact` |

Replace `README.md:357-415` with a link to `server/src/main.rs` so it cannot drift, and regenerate the tree from `git ls-files` with a CI diff gate.

---

### 20. The five per-directory `.cargo-runner.json` files are dead weight that makes behaviour depend on your shell's cwd

**PROVEN** (source read + reproduced empirically). Deepens round-1 #11.

**Evidence.** From `cargo-runner-core` 2.1.4 source:

- **`linked_projects` is inert dead code.** `src/command/builder/bazel/bazel_builder.rs:591-596` does `PathBuf::from(linked_project_str)` and `abs_file_path.starts_with(project_dir)` with **no** `PROJECT_ROOT` join. With `/Users/uriah/Code/yoyo/server/Cargo.toml` stored and `/home/user/…/server/src/main.rs` looked up, that is false for all five entries. Separately, `src/config/cargo_config.rs:9` serialises `linked_projects` (snake_case) while `src/command/builder/cargo/common.rs:39,108` reads the raw key `"linkedProjects"` (camelCase) — the cargo-mode resolver never sees it either. **Empirically confirmed:** I deleted the whole array on a copy and re-ran `--dry-run` on all nine real files; every generated command was byte-identical. The block contributes nothing but a leak of the author's home directory into a public repo.
- **The cwd hazard is real, and I proved it.** `src/config/merge.rs:77-96` finds the "root" config by walking **up from `std::env::current_dir()`** to the first directory containing a `.cargo-runner.json`. Because every package directory has one, running from inside `server/` makes `server/` the PROJECT_ROOT. Test: I added `"bazel": {"extra_run_args": ["--verbose_failures"], "extra_test_args": ["--keep_going"]}` to the root config — running from the repo root emits both flags; running the identical target from inside `server/` **silently drops both**.
- The five files are one no-op (`combos/` — no `command`, no `package`, only empty arrays), three that set only `package`, and one (`combos/frontend/`) with a redundant `"command": "bazel"` and a **doubly dead** override: its `match.file_path` is an absolute `/Users/uriah/…` path, and `types/function_identity.rs:60-76 paths_match` returns `false` for two unequal absolute paths; even if it matched, its only effect is `extra_test_args: ["--nocapture"]`, which is already emitted by default for every test target here.

**Fix.**

```sh
git rm combos/.cargo-runner.json
```

Root `.cargo-runner.json` becomes:

```json
{ "cargo": { "command": "bazel", "extra_args": [], "extra_env": {}, "extra_test_binary_args": [] },
  "overrides": [] }
```

Strip the override and the redundant `command` from `combos/frontend/.cargo-runner.json`. If a per-package override is ever needed, express `match.file_path` **relative** to the repo root (`"combos/frontend/src/main.rs"`) — `function_identity.rs:66-70` supports absolute-vs-relative suffix matching — and document `export PROJECT_ROOT="$(git rev-parse --show-toplevel)"` so discovery stops depending on cwd. Verified: with these edits, output from the repo root and from `combos/frontend/` is identical.

---

### 21. `combos_crates` is a whole crate_universe repo that resolves zero crates

**PROVEN.** Deepens round-1 #6.

**Evidence.** `combos/Cargo.lock` contains exactly two `[[package]]` entries — `backend 0.1.0` and `frontend 0.1.0`, both path members, no third-party at all. `combos/backend/Cargo.toml:6` and `combos/frontend/Cargo.toml:6` are empty `[dependencies]`. `rust-project.json` crates 0 and 1 both have `deps: []`. Grouping all 65 crates by their `rules_rust++crate+<repo>__` prefix: `corex_crates=6`, `server_crates=54`, `combos_crates=0`.

Every `bazel fetch`/lockfile resolution pays for a third module extension repo, an extra `cargo metadata` run, and an entry in `MODULE.bazel.lock` that produces nothing. `MODULE.bazel:12-20` also lists two redundant member manifests; `cargo-bazel`'s `splicer.rs:83` prints *"INFO: Only the workspace's Cargo.toml is required in the `manifests` attribute … the rest can be removed"* on every evaluation.

**Fix.** Delete `MODULE.bazel:11-20` and line 37, and the `load("@combos_crates//:defs.bzl", …)` + `deps = all_crate_deps(…)` lines from both combos BUILD files:

```python
rust_binary(name = "backend_bin", srcs = ["src/main.rs"], edition = "2024")
rust_binary(name = "frontend_bin", srcs = ["src/main.rs"], edition = "2024")
rust_test(name = "frontend_tests", crate = ":frontend_bin", size = "small")
# backend_tests deleted (zero test functions)
```

Or, if the point of `combos/` is to *demonstrate* a cargo workspace under bzlmod, give both members one real shared dependency so the demo demonstrates something.

**One trap worth a comment while you are in there:** `combos/Cargo.toml` is a *virtual* manifest, so `combos` is not a workspace member and has no key in the generated deps map. Anyone who adds `all_crate_deps()` to `combos/BUILD.bazel` gets `fail("Tried to get all_crate_deps for package combos but that package had no Cargo.toml file")` — an error message that is factually false. The escape hatch is `all_crate_deps(normal = True, package_name = "combos/backend")`.

---

### 22. No `proc_macro_deps` anywhere — cheap prophylactic

**PROVEN‑SRC.** New, low severity, ~10 minutes.

`grep -rnE 'proc_macro_deps|proc_macro = True|build = True|aliases\(' --include=*.bazel .` returns nothing. Direct proc-macro deps live in a separate `_PROC_MACRO_DEPENDENCIES` map reachable only via `proc_macro = True`; a bare `all_crate_deps()` silently drops them. The repo escapes this today only because its one proc-macro (`serde_derive`) is transitive via serde's `derive` feature — confirmed in `rust-project.json`, where corex (62) depends on serde (55) and never on serde_derive (49).

The first `async-trait`, `derive_more`, `strum_macros`, `tracing-attributes` or `sqlx` macro fails with `error[E0433]: failed to resolve: use of undeclared crate or module` in a file that compiles fine under cargo, with a BUILD file that looks correct. Add the line now, while the answer is `[]`:

```python
    deps = all_crate_deps(normal = True),
    proc_macro_deps = all_crate_deps(proc_macro = True),
```

to `corex_lib`, `server_lib`, `server_bin`, `proxy`, and both combos binaries.

---

### 23. No release path: no visibility, no platforms, no stamping, no image

**REASONED** (with the two common wrong rationales corrected). New.

`MODULE.bazel` declares one `bazel_dep`. `//server:server_bin` (`server/BUILD.bazel:5`) has **no `visibility` attribute**, so it is private to `//server` and an `oci_image` in another package cannot consume it — while `corex_lib` is `//visibility:public`. There is no `package(default_visibility = …)` anywhere, no `platforms/` package, no rules_oci, no `--workspace_status_command` (no `.bazelrc` to put it in), and no version string in the binary at all (`server/src/main.rs:31` prints a hardcoded banner).

**Two rationales to *not* use, both refuted:**

- *"crate_universe must be told the triples or dep resolution fails."* False — `supported_platform_triples` defaults to rules_rust's full `SUPPORTED_PLATFORM_TRIPLES` (~45 triples). And Bazel only *fetches* what the selected configuration needs, so the 8 `windows_*` crates and the wasm stack never download on macOS or Linux.
- *"Just add `extra_target_triples = ["x86_64-unknown-linux-musl"]` and a musl platform."* rules_rust 0.63 emits **no musl constraint** — `triple_mappings.bzl:329-331` has `# all_abi_constraints.append("//rust/platform/constraints:musl_on")` commented out and that package 404s at the tag. A musl and a gnu toolchain for x86_64-linux therefore carry identical `target_compatible_with`, and `--platforms=//platforms:linux_x86_64_musl` silently selects whichever was registered first — you get a dynamically linked binary in a distroless *static* base.

**Fix.** Declare your own constraint and register the toolchain against it:

```python
# platforms/constraints/BUILD.bazel
constraint_setting(name = "libc", default_constraint_value = ":gnu")
constraint_value(name = "gnu",  constraint_setting = ":libc")
constraint_value(name = "musl", constraint_setting = ":libc")
```

```python
# MODULE.bazel
rust.repository_set(
    name = "rust_linux_x86_64_musl",
    exec_triple = "x86_64-unknown-linux-gnu",
    target_triple = "x86_64-unknown-linux-musl",
    target_compatible_with = ["@platforms//os:linux", "@platforms//cpu:x86_64", "//platforms/constraints:musl"],
    versions = ["1.94.1"],
)
```

Add `visibility = ["//visibility:public"]` to `server_bin`, and stamp it — verified to work: `stamp` is a real `rust_binary` attribute (`rust.bzl:1105`, default `-1`) and the process wrapper substitutes `{VAR}` from the status files (`util/process_wrapper/options.rs:321-327`):

```python
rust_binary(
    name = "server_bin",
    stamp = -1,
    rustc_env = {"SERVER_VERSION": "{STABLE_VERSION}"},
    ...
)
```

read back with `option_env!("SERVER_VERSION")`.

---

### 24. The server itself lies about what it does

**PROVEN by the lens's curl transcript; NOT independently re-verified** (this is the one finding whose verifier returned no verdict — treat the fix as sound and the transcript as one run).

```
$ curl -X POST localhost:3000/users -d '{"name":"alice","age":41}' -H 'content-type: application/json'
{"name":"alice","age":41}
$ curl localhost:3000/users/alice
{"name":"alice","age":25}                       # the 41 is gone
$ curl -w ' [http %{http_code}]' localhost:3000/users/doesnotexist
{"name":"doesnotexist","age":25} [http 200]     # invented a user
```

`get_user` (`server/src/main.rs:51-56`) hardcodes `age: 25, // Default age`; `create_user` (`:43-49`) constructs a `User` and drops it into the response; there is no `Arc`/`Mutex`/`HashMap`/`State` anywhere. A port collision panics at `src/main.rs:30:72` with a 16-frame backtrace and exit 101; there is no graceful shutdown and the `0.0.0.0:3000` bind is not configurable, so the binary cannot be run twice or bound to loopback in a test. The 422 body leaks the internal Rust type shape (`expected u8`) to unauthenticated callers.

This matters for the audit because it is *why* there is nothing to integration-test. Minimum honest version:

```rust
type Db = Arc<RwLock<HashMap<String, User>>>;

async fn create_user(State(db): State<Db>, Json(p): Json<CreateUserRequest>) -> impl IntoResponse {
    let user = User { name: p.name.clone(), age: p.age };
    db.write().await.insert(p.name, user.clone());
    (StatusCode::CREATED, Json(user))
}

async fn get_user(State(db): State<Db>, Path(name): Path<String>) -> Response {
    match db.read().await.get(&name) {
        Some(u) => Json(u.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "no such user").into_response(),
    }
}
```

plus `BIND_ADDR` from the environment, `.with_graceful_shutdown(…)`, and `tracing_subscriber::fmt::init()` (tracing 0.1.41 is already in the Bazel graph transitively — `rust-project.json` crate 48 — so it costs nothing new to build). Then the round-trip becomes testable, which is what finally gives `//server:integration_tests` something to assert.

---

## Part 3 — What round 1 got wrong or overstated

| Round-1 item | Correction |
|---|---|
| **#1** "First-party targets build at edition 2021 though Cargo.toml says 2024" | Correct, and now mechanically proven: rules_rust's *own* `MODULE.bazel:48` is `rust.toolchain(edition = "2021")`, and `extensions.bzl:93` (`root.tags.toolchain or rules_rust.tags.toolchain`) makes it the default whenever the root module registers nothing. Round 1 also implied "add an `edition` attr everywhere" — **adding it to `rust_test(crate = …)` is a no-op**; those inherit `crate.edition` (`rust.bzl:374`). |
| **#3 / #14** "No .bazelrc / no .bazelignore" | True but understated: `.gitignore:2-4` already reserves `.bazelrc.user` and `.bazelversion.user`, i.e. the layered config was planned and never written, so that gitignore line is dead. Also, `cargo` and Bazel disagree on debug info and incremental compilation by default with no documented `-c dbg`. |
| **#4** "MODULE.bazel.lock is gitignored" | True; the consequence round 1 missed is that `--lockfile_mode=error` is impossible, so any Dependabot cargo PR hard-fails the Bazel job with `fail("The current lockfile is out of date")` and there is no committed artefact to diff. |
| **#5** "cargo_build_script :build_script is orphaned" | Correct but the severity was on the wrong axis. The script is a three-line `println!` that emits no `cargo:` directive, so **wiring it up produces nothing observable**. The right action is deletion. The genuinely new cost is the *missing* `rerun-if-changed`: measured, `touch src/bin/proxy.rs` currently recompiles all 6 targets in the package vs 1 with the one-line fix. |
| **#6** "Three separate crate_universe repos / three Cargo.lock files" | Not merely duplication overhead — two of the six shared crates (`proc-macro2`, `syn`) have **already drifted** (1.0.96/1.0.97, 2.0.104/2.0.105), and `server_bin` links two distinct `serde` rlibs simultaneously. Also: `combos_crates` resolves to literally zero crates. |
| **#7** "server is binary-only so tests/ can't test anything" | Correct, but incomplete: even after a lib exists, `integration_tests` still cannot test anything because `all_crate_deps(normal_dev = True)` gives it dev-deps only. And `tests/just_test.rs` *does* run 2 tests today; they are just vacuous. |
| **#8** "Duplicated deps" | Half of the cited duplication is not real: `all_crate_deps()` (corex) and `all_crate_deps(normal = True)` (server) are **semantically identical** in rules_rust 0.63 (`module_bzl.j2:192-193` defaults to `_NORMAL_DEPENDENCIES`). The *real* dep defect is that `deps` on `rust_test(crate = …)` is additive, attaching criterion + 30 crates to four targets that never use it. |
| **#9** "glob(["tests/**"]) too loose" | Understated. It is not looseness — a single non-`.rs` file under `tests/` (a fixture, a `.DS_Store`) hits `fail()` at **loading** time and takes down every target in `//server`. And `rust_test_suite` has a `shared_srcs` attribute, unused, for exactly the `tests/common/mod.rs` case. |
| **#10 / #11** "rust-project.json / .cargo-runner.json have absolute /Users/uriah paths" | Understated three ways: (a) the committed file **suppresses** rust-analyzer's Cargo fallback, which works perfectly today, so shipping it is worse than deleting it; (b) it is stale by three commits with two dead labels and three missing source files; (c) `.cargo-runner.json`'s `linked_projects` block is *inert dead code* — deleting it changes no output — and the real defect is that having a config in every package directory makes root-config discovery depend on your shell's cwd. |
| **#12** "Four identical .gitignore files" | There are **six**; five leaves are byte-identical (`cb20a5d3…`) and the root differs (`9f1c6610…`). The leaves are exactly the root minus the `*.md` block, so deleting them is safe. |
| **#15 / #16 / #17** "No CI / no clippy / no coverage" | Understated: adding CI is **red on day one** (9 clippy errors, 8 fmt hunks, `cargo test --all-targets` exits 101), and a naive doctest matrix is *structurally* red in 2 of 3 cells (`no library targets found`). Also, the common claim that coverage needs a special toolchain is **false** — `rust/repositories.bzl:479-487` ships llvm-tools for any version ≥ 1.45.0, including the implicit 1.86.0, so `bazel coverage //...` works today. |
| **#21** "README repin command is WORKSPACE-era and wrong under bzlmod" | Right conclusion, wrong reasoning, and it misses the punchline. `bazel sync` is **not** removed (`SyncCommand.java` exists in Bazel 8.3.1), and `--enable_workspace` is not a stray flag — `SyncCommand.java:97-104` *hard-errors without it*. The actual defect is `shouldSync()` skipping non-workspace rules, plus a repo named `crates` that does not exist. The punchline: **no repin step is needed at all**, because no `lockfile` attribute is set and `generate_utils.bzl:389-391` therefore repins unconditionally. Also, the author copied this command verbatim from rules_rust's own `crate_universe/extensions.bzl:135`. |
| **#22** "README BUILD examples say edition 2021 while manifests say 2024" | True (11 occurrences), but the README's bigger problem is that four of its BUILD snippets are **not parseable Starlark** — commit `2c55b45`, titled "fix: remove invalid square brackets from Bazel list comprehensions", deliberately broke correct code and added a false rule at `README.md:177`. |

**Do not chase these** (investigated and disproved):

- Missing `module(name = …)` in MODULE.bazel. `_main` is **hardcoded** by Bazel (`WorkspaceNameFunction.java:48-60`), and extension repos are canonically named after the module that *defines* the extension (rules_rust), not the one that uses it. Naming the root module changes neither string. It only matters if you ever publish this as a `bazel_dep`.
- Three `use_repo` calls instead of one. `bazel mod tidy` only rewrites imports when they *disagree* with the extension's exports; here the delta is empty and tidy is a no-op. Pure formatting.
- `combos_crates` listing redundant member manifests "means new members are invisible to Bazel". False — `splice_workspace` (`splicer.rs:162-166`) symlinks the entire workspace root and re-runs `cargo metadata`, so member discovery comes from the on-disk manifest. Adding a member needs no MODULE.bazel edit. Cosmetic INFO line only.
- `crate.annotation(gen_build_script = False)`. Invalid — it is a **string** (`"auto"`/`"on"`/`"off"`, `extensions.bzl:1167`), and both plausible annotations here are pointless anyway (criterion has no build script; rayon-core's is generated by default).
- `rust_test_suite` with an empty `srcs` "silently passes". No — Bazel expands `test_suite(tests = [])` to every test rule in the package (`rust.bzl:1514-1516` says so explicitly). Noisy over-inclusion, not a false green.
- The `#[cfg(test)]` wrapper in `tests/just_test.rs`. Harmless under both systems (`--test` implies `cfg(test)`). Do not "fix" it; fix the missing deps and the vacuous assertions instead.
- Adding `edition` to `rust_test(crate = …)`, or `package_name = "backend"` to `all_crate_deps` in combos (the key is the workspace-member *path*, `"combos/backend"`; the crate name `fail()`s).

---

## Part 4 — Remediation plan

Grouped so that each PR is independently mergeable, reviewable in one sitting, and does not depend on a PR that lands after it. Effort assumes one engineer who knows the repo.

### PR 1 — Stop shipping broken machine state · **30 min** · zero risk

- `git rm --cached rust-project.json`; add it to `.gitignore`.
- Delete the `linked_projects` array from the root `.cargo-runner.json`; `git rm combos/.cargo-runner.json`; strip `combos/frontend`'s dead override and redundant `command`.
- Harden `build-ra.sh` (`set -euo pipefail`, bazelisk lookup); fix `README.md:102` to name it instead of the nonexistent `./refresh-rust-analyzer.sh`; delete the `.vscode/tasks.json` and `Cmd+R Cmd+R` claims at `README.md:100-101`.
- Change `.gitignore:26` `.vscode/` → `.vscode/*` + negations; delete the `*.md` block at `:77-79`; `git rm` the five duplicate leaf `.gitignore` files.

*Rationale:* every one of these makes a fresh clone strictly better, touches no build logic, and unblocks a `CONTRIBUTING.md` in every later PR.

### PR 2 — Pin the toolchains · **2 h** · **highest value in this document**

- `rust.toolchain(edition = "2024", versions = ["1.94.1"])` + `use_repo` + `register_toolchains` in `MODULE.bazel`.
- `rust-toolchain.toml` at the same version; `rust-version = "1.94"` in each `[package]`.
- `.bazelversion`, `.bazelignore`, and the base `.bazelrc` from finding 16.
- `edition = "2024"` on every `rust_library`/`rust_binary`/`rust_test_suite`/`cargo_build_script` (not on `crate = …` tests — ignored).
- Verify with the `rust-project.json` edition check from finding 1.

*Rationale:* closes table rows 1, 2 and 15 — the three divergences that are wrong on **every** build, today, silently. Everything else is either latent or local.

### PR 3 — Docs truth pass · **3 h** · zero build risk

- Restore the four bracketed comprehensions (`README.md:178-183, 198-203, 245-250, 259-272`); delete the false claim at `:177`.
- Replace `rust_benchmark` with `rust_binary`; replace `@crates` with the real repo names; change the 11 `edition = "2021"` to `"2024"`.
- Replace `README.md:357-415` with a link to `server/src/main.rs`.
- Rewrite Quick Commands (finding 19) and the 138-line cargo-runner section (verified table in finding 19); add the `cargo install cargo-runner-cli --version 2.1.4` block.
- Delete `README.md:43-45` (no repin is needed) and the "Hermetic builds" claim at `:543`.
- Rewrite `BAZEL_RUST_GUIDE.md:48-53, 97` and `README.md:56-59` to the accurate path-dep rule; regenerate the directory tree from `git ls-files`.
- Add `CONTRIBUTING.md` (now possible) documenting the per-directory cargo fallback: `cargo` must be run inside `corex/`, `server/`, `combos/` — there is no root manifest.

*Rationale:* cheap, independent, and removes six consecutive walls between `git clone` and a working editor. Doing it before the BUILD refactors means writing the docs twice for a few snippets; doing it after means every new contributor in the meantime hits the walls. Ship it early and re-touch the snippets in PR 5.

### PR 4 — Lint and test debt · **3 h** · **prerequisite for CI**

- Fix the 9 clippy errors and 8 rustfmt hunks; add `rustfmt.toml`, `clippy.toml`, `.editorconfig`.
- Delete the two `#[should_panic] fn it_will_fail` blocks and the seven `assert!(true)` bodies; delete `//combos/backend:backend_tests`.
- Replace the two decorative doctests with real assertions.
- Fix the u64 overflow: cap the three sweeps at 90 (or move to `u128`); add `tags = ["manual"]` to `//server:bench`.

*Rationale:* CI cannot be introduced while `-D warnings` fails in all three workspaces and `cargo test --all-targets` exits 101. Land the debt clearance separately so the CI PR is a clean, reviewable green.

### PR 5 — BUILD file correctness · **3 h**

- `integration_tests`: `glob(["tests/*.rs"])` + `shared_srcs` + `data`, and `all_crate_deps(normal = True, normal_dev = True)`.
- `axum_example`: same dep change; delete the speculative `//corex:corex_lib` edges at `:49` and `:64`.
- Delete `deps` from all `rust_test(crate = …)` targets.
- Add `version = "0.1.0"` + `rustc_env = {"CARGO_PKG_NAME": …}` to every target that compiles sources, including the tests; consider renaming `server_bin` → `server`.
- Add `proc_macro_deps = all_crate_deps(proc_macro = True)` everywhere.
- Delete `server/build.rs` and the `cargo_build_script` target.
- Delete `combos_crates` (`MODULE.bazel:11-20`, `:37`) and the two `@combos_crates` loads.
- Add `visibility` to `server_bin`; run buildifier over the tree.

*Rationale:* closes table rows 5, 6, 7, 8, 9, 11 and 16 in one coherent sweep of five files. All mechanical, all verifiable by re-running `gen_rust_project` and diffing the env/deps blocks.

### PR 6 — Give `server` a library · **4 h**

- `server/src/lib.rs` + `[lib]` + `rust_library(name = "server_lib", crate_name = "server")` + `rust_test(crate = ":server_lib")`.
- Move the fibonacci algorithms and their two real tests out of `benches/` into the library; delete `bench_test` and `axum_example_test` and the `#[cfg(test)]` blocks in `examples/axum.rs`.
- Point `server_bin`, `proxy`, `axum_example`, `bench` and `integration_tests` at `:server_lib`.
- Optionally land finding 24's state/404/graceful-shutdown fix here, so the integration suite has something to assert.

*Rationale:* closes table rows 12 and 13 (the two toolchains finally agree on what the test suite *is*) and is the prerequisite for any integration test that means anything.

### PR 7 — CI and gates · **4 h**

- Matrix over `corex`/`server`/`combos`: `cargo build --locked`, `cargo test --locked`, conditional `cargo test --doc` (finding 13), `cargo clippy -- -D warnings`, `cargo fmt --check`.
- `bazel build //...`, `bazel test //...`, `bazel test --config=lint //...`, `bazel test //:buildifier.check` (as a `buildifier_test` with `workspace = "//:MODULE.bazel"` so it walks subpackages — a root-package `glob` does not).
- Un-gitignore `MODULE.bazel.lock`, commit it, add `common:ci --lockfile_mode=error`.
- Add the cross-lock skew guard (finding 8) until PR 8 lands.
- Add a docs-label gate — but scope the regex to real target patterns; a naive `//[a-z/]*:[a-z_]*` harvests `//visibility:public`, `//localhost:3000` and `//rust:defs.bzl` and fails on legitimate prose.

*Rationale:* by now everything it would gate is green, so it lands as a genuine ratchet rather than a `continue-on-error` placebo.

### PR 8 — One workspace, one lock, one crate repo · **1 day** · **riskiest, and the only fix for the serde bomb**

- Root `Cargo.toml` (`resolver = "3"`, four members); **delete `[workspace]` from `combos/Cargo.toml`**; delete the three leaf locks and `cargo generate-lockfile` at the root.
- `corex = { path = "../corex" }` in `server/Cargo.toml`.
- Single `crate.from_cargo(name = "crates", …)`; retarget the four loads.
- Expect and review the version bump (110 packages; axum → 0.8.9, serde → 1.0.229, matchit → 0.8.6); pin back with `--precise` where it matters.
- Delete the cross-lock guard from PR 7; delete `BAZEL_RUST_GUIDE.md:333-338`'s obsolete workaround.

*Rationale:* last among the structural PRs because it rewrites every lockfile and is the one change most likely to need a revert — CI from PR 7 is what makes it safe. **Move it ahead of PR 6 if anyone is about to write `Json(corex::User)`**, because that line does not compile under Bazel until this lands.

### PR 9 — Supply chain and release path · **1 day**

- `deny.toml`, `renovate.json` (with the repin note), a `cargo-deny` CI job per manifest.
- `platforms/constraints` with an explicit musl `constraint_value`; `rust.repository_set` for the musl target (do **not** rely on `extra_target_triples` — rules_rust 0.63 emits no musl constraint).
- `oci_image`/`oci_load`/`oci_push` for `//server:server_bin`; `--workspace_status_command`; `stamp = -1` + `{STABLE_VERSION}` in `rustc_env`.
- Pin a hermetic cc toolchain (`toolchains_llvm`) so the 25 build scripts and 2 `links` crates stop depending on host `/usr/bin/cc`.
- Bump `rules_rust` off 0.63.0, deliberately, with the toolchain version held fixed so the bump is not a silent compiler change.

*Rationale:* everything here is additive and none of it blocks daily work. It is last because a release path for a server that fabricates its responses (finding 24) is premature.

---

**One-line summary for the author:** nothing here is broken, and that is the problem — Bazel and Cargo compile this repo at different editions with different compilers into different dependency graphs producing differently-named binaries that report the wrong version and run different test suites, and every one of the seventeen targets is green while doing it. PR 2 fixes the half of that which is wrong on every build today; PR 8 fixes the half that detonates the first time someone writes the line the architecture exists to enable.


---

## Part 5 — Completeness pass

A final independent agent re-verified the document above and searched for what it missed. It re-checked 11 specific
claims by running commands or parsing `rust-project.json` and **found no factually wrong claim**; the verification log is
at the end of this part. It then found seven genuine omissions, two of which are defects in the *fixes* proposed above.

**Where these land in the plan:**

| Item | PR |
|---|---|
| 1. Single-file `srcs` — the first `mod foo;` breaks Bazel, not Cargo | **PR 5** (and it corrects the `srcs` lines printed in findings 10 and 12) |
| 2. Root `BUILD.bazel` exports nothing, so two proposed fixes fail analysis | **PR 5** (blocks PR 7's buildifier and finding 16's `--config=lint`) |
| 3. `deny.toml` is red on day one — no crate declares a license | **PR 9** |
| 4. Feature bloat: 13 wasted packages from `tokio = ["full"]` + criterion defaults | **new, before PR 9** — shrinks what PR 9 is sized against |
| 5. `.gitignore:20-21` pre-blocks the `lockfile` attribute, same bug as `MODULE.bazel.lock` | **PR 1** + **PR 2** |
| 6. `BAZEL_RUST_GUIDE.md:100-148` is the policy that manufactures finding 8 | **PR 8** |
| 7. `README.md:104-105` tells the reader to hand-write the IDE config PR 1 deletes | **PR 3** |


## What it missed, most important first

### 1. Every first-party target hardcodes a single-file `srcs`, so the first `mod foo;` breaks Bazel and not Cargo — and the document's own replacement BUILD files keep it

The document's central thesis is "one ordinary line of real code away from a hard failure," and this is the likeliest such line — far likelier than crossing the `corex`/`serde` boundary (finding 8) or dropping a fixture into `tests/` (finding 6). It is absent from the divergence table and from every fix.

Evidence — all nine source-compiling targets:
`server/BUILD.bazel:7` `srcs = ["src/main.rs"]`, `:34` `["benches/fibonacci_benchmark.rs"]`, `:47` `["examples/axum.rs"]`, `:62` `["src/bin/proxy.rs"]`; `corex/BUILD.bazel:6` `srcs = ["src/lib.rs"]`; `combos/backend/BUILD.bazel:6` and `combos/frontend/BUILD.bazel:6` `srcs = ["src/main.rs"]`. Not one glob in the tree. Meanwhile `BAZEL_RUST_GUIDE.md:190` ("Pattern 1") teaches `srcs = ["src/main.rs"]` and only `:195` ("Pattern 2") uses a glob — so the guide teaches both and the repo picked the wrong one everywhere.

Proven: I added `src/util.rs` + `mod util;` to a scratch copy. Cargo: `Finished dev profile`. Staging only `srcs = ["src/main.rs"]` (what the Bazel sandbox does):

```
error[E0583]: file not found for module `util`
 --> src/main.rs:1:1
  |
1 | mod util;
```

Note that the document's finding 12 fix reprints `rust_binary(name = "server_bin", srcs = ["src/main.rs"], …)` verbatim, so applying the document end-to-end leaves the defect in place for `server_bin`, `proxy`, `axum_example` and both combos binaries.

Fix — in all five BUILD files:

```python
rust_binary(
    name = "server_bin",
    srcs = glob(["src/**/*.rs"], exclude = ["src/bin/**"]),
    crate_root = "src/main.rs",
    ...
)
rust_binary(
    name = "proxy",
    srcs = ["src/bin/proxy.rs"] + glob(["src/**/*.rs"], exclude = ["src/main.rs", "src/bin/**"]),
    crate_root = "src/bin/proxy.rs",
    ...
)
```
```python
rust_library(name = "corex_lib", srcs = glob(["src/**/*.rs"]), ...)
rust_binary(name = "backend_bin", srcs = glob(["src/**/*.rs"]), ...)
```

(`crate_root` is already set on the server targets, so a glob is safe — it will not change which file is the crate root.)

### 2. Two fixes in the document reference `//:…` source targets that the root `BUILD.bazel` does not export — they will fail analysis

`/home/user/complex-bazel-setup/BUILD.bazel` and `/home/user/complex-bazel-setup/combos/BUILD.bazel` are the only two repo files no finding in the document touches at all. Each is a single comment line with no trailing newline:

```
# Root BUILD.bazel file - marks this as a Bazel package
```

No `exports_files`, no `package(default_visibility = …)`. Source files in a package are private by default, so:

- Finding 16's `.bazelrc`: `build:lint --@rules_rust//rust/settings:clippy.toml=//:clippy.toml` and `…:rustfmt.toml=//:rustfmt.toml` — the label_flag lives in `@rules_rust//rust/settings`, a different package, so it cannot see `//:clippy.toml`. Fails with `target '//:clippy.toml' is not visible from target '@@rules_rust//rust/settings:clippy.toml'`.
- PR 7's `buildifier_test(workspace = "//:MODULE.bazel")` — same problem for `//:MODULE.bazel`.

Fix, in the root `BUILD.bazel` (which is currently a no-op file and should be doing this work anyway):

```python
package(default_visibility = ["//visibility:public"])

exports_files([
    "MODULE.bazel",
    "clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
])
```

Also add a trailing newline to both stub BUILD files.

### 3. The `deny.toml` in finding 17 is red on day one: no crate in this repo has a `license` field and there is no `LICENSE` file

Verified with `cargo metadata` over the server graph — the license census of all 119 packages is fully covered by the document's allow-list *except* one entry:

```
NO LICENSE FIELD: [('server', '0.1.0', None)]
```

`grep -n "license\|publish" corex/Cargo.toml server/Cargo.toml combos/*/Cargo.toml` → zero hits; `ls LICENSE*` → no such file. `cargo deny check licenses` treats a publishable crate with neither `license` nor `license-file` as **unlicensed** and errors. So the document's PR 9 ships a config that fails on the repo's own four crates.

(The third-party half of the allow-list does hold up, including the awkward ones I checked: `MIT AND BSD-3-Clause` ×1, `(MIT OR Apache-2.0) AND Unicode-3.0` ×1, `Apache-2.0 OR BSL-1.0` ×1, `Unlicense OR MIT` ×3, and four crates using the legacy slash form `MIT/Apache-2.0`.)

Fix — add to each of the four `[package]` tables and drop a `LICENSE` file in the root:

```toml
license = "MIT OR Apache-2.0"
description = "…"
repository = "https://github.com/…"
```

or, if these are never to be published:

```toml
publish = false
```
```toml
# deny.toml
[licenses]
private = { ignore = true }
```

### 4. Dependency-feature hygiene is a whole lens the document never opens — 13 packages are pure waste, and 5 of them are visibly in the Bazel graph

The document counts packages (56/86/119/25 build scripts/2 `links` crates) and treats the number as a given. It never asks whether the feature selections are justified. Two are not:

- `server/Cargo.toml:9` — `tokio = { version = "1.47.1", features = ["full"] }`. The binary uses `TcpListener`, `#[tokio::main]`, and nothing else.
- `server/Cargo.toml:12` — `criterion = { version = "0.7", features = ["html_reports"] }`, i.e. default features (`rayon`, `plotters`) plus HTML.

Measured on a scratch copy, changing only those two lines to `features = ["rt-multi-thread", "net", "macros"]` and `default-features = false`:

```
$ comm -23 base.txt slim.txt          # 86 -> 73 unique packages
crossbeam-deque 0.8.6      plotters 0.3.7            rayon 1.11.0
crossbeam-epoch 0.9.18     plotters-backend 0.3.7    rayon-core 1.13.0
crossbeam-utils 0.8.21     plotters-svg 0.3.7        scopeguard 1.2.0
lock_api 0.4.13            parking_lot 0.12.4        signal-hook-registry 1.4.6
parking_lot_core 0.9.11
$ cargo check --all-targets            # Finished `dev` profile
```

This is not hypothetical Bazel cost: `parking_lot`, `parking_lot_core`, `lock_api`, `scopeguard` and `signal_hook_registry` are all present in `rust-project.json` as real `server_crates__*` targets today. Dropping criterion's defaults also removes `rayon-core` — **one of the two `links` crates** the document's finding 16 uses to argue for a hermetic C toolchain — and one of its 25 build scripts. Worth landing before PR 9, because it shrinks the problem PR 9 is sized against.

### 5. `.gitignore:20-21` pre-blocks the actual fix for crate_universe reproducibility, exactly like `MODULE.bazel.lock` does

Finding 19 correctly proves that no repin step is needed because no `lockfile` attribute is set — and then stops there, deleting `README.md:43-45` and the "Hermetic builds" claim at `:543`. It never proposes the fix that would make that claim *true*. The reason matters:

```
20: cargo-bazel-lock.json
21: .cargo-bazel/
```

These are dead lines for the same reason `.bazelrc.user` is dead — the author reserved the artifact and never produced it. Anyone who adds the `lockfile` attribute (the documented way to get deterministic, network-free crate_universe resolution) will find the resulting file silently unaddable, and will conclude the feature "doesn't work", which is precisely the failure mode the document diagnoses for `MODULE.bazel.lock` in finding 13. Both `git rm`s belong in the same one-line PR.

Fix — delete `.gitignore:20-21`, then on each of the three `crate.from_cargo` tags (`MODULE.bazel:12`, `:23`, `:30`):

```python
crate.from_cargo(
    name = "server_crates",
    manifests = ["//server:Cargo.toml"],
    cargo_lockfile = "//server:Cargo.lock",
    lockfile = "//server:cargo-bazel-lock.json",
)
```

then `CARGO_BAZEL_REPIN=1 bazel mod deps` once and commit the three generated files. This is also what turns finding 17's Renovate `prBodyNotes` from a convention into an enforced gate: with `lockfile` set, a Cargo-only bump makes `generate_utils.bzl` `fail()` in CI instead of silently re-resolving.

### 6. `BAZEL_RUST_GUIDE.md:100-148` is the documented recipe that *manufactures* finding 8, and no PR rewrites it

Finding 9 correctly indicts the path-dependency prohibition at `:48-53` and `:97`. But the guide's growth recipe is a separate, uncited block, and it is the thing that guarantees the problem recurs:

> **### Option 1: Standalone Crate** … 3. Create BUILD.bazel: `load("@mynewcrate_crates//:defs.bzl", …)` … 4. **Add to MODULE.bazel**: `crate.from_cargo(name = "mynewcrate_crates", manifests = ["//mynewcrate:Cargo.toml"], cargo_lockfile = "//mynewcrate:Cargo.lock")`

One `Cargo.lock` and one `crate_universe` repo per crate, forever. The duplicate-`serde` bomb the document spends finding 8 on is not history — it is policy, and it scales quadratically in the number of first-party crates that share a proc-macro dependency. PR 8 lists `BAZEL_RUST_GUIDE.md:333-338` for deletion but leaves `:100-148` standing, so a reader who completes PR 8 and then follows the guide re-creates the exact defect.

Fix — replace Option 1 with the workspace-member recipe PR 8 establishes:

```markdown
### Adding a crate

1. `mkdir mynewcrate/src && cd mynewcrate && cargo init --lib`
2. Add `"mynewcrate"` to the root `Cargo.toml` `[workspace] members`.
3. Add `//mynewcrate:Cargo.toml` to the single `crate.from_cargo(name = "crates", manifests = [...])`.
4. In `mynewcrate/BUILD.bazel`: `load("@crates//:defs.bzl", "all_crate_deps")`.
5. First-party deps are Bazel labels: `deps = all_crate_deps(normal = True) + ["//corex:corex_lib"]`.
   Do NOT create a new `crate.from_cargo` repo or a second `Cargo.lock`.
```

Also flag that the guide's "Option 2" step 3 (`:170-185`, adding each new member manifest to `manifests`) is unnecessary — the document's own "do not chase" list proves `splice_workspace` rediscovers members from the on-disk manifest — so the guide is teaching a step the document has already established is a no-op.

### 7. `README.md:104-105` reintroduces finding 3's poisoned IDE config by hand, after PR 1 removes the file

Finding 18 warns "Do **not** ship `"rust-analyzer.linkedProjects": ["rust-project.json"]` in a committed settings.json." It missed that the README already instructs the reader to type it in:

```
104:    "rust-analyzer.linkedProjects": ["rust-project.json"]
105: }
```

PR 1 does `git rm --cached rust-project.json`; PR 3's docs pass never touches `:104`. Result: a contributor follows the README, sets `linkedProjects` to a path that no longer exists, and rust-analyzer reports a missing project rather than falling back to the three Cargo workspaces — a *worse* outcome than the status quo finding 3 was fixing. Delete `README.md:96-106` along with the `.vscode/tasks.json` / `Cmd+R Cmd+R` / `./refresh-rust-analyzer.sh` claims at `:100-102`, and replace with "no editor config needed — rust-analyzer discovers `corex/`, `server/` and `combos/` via `linkedProjects` auto-discovery, or run `./build-ra.sh` if you want Bazel-accurate metadata."

---

### One residual, cheaply closable gap

The document never states whether `rust_doc_test` in rules_rust 0.63.0 actually accepts a `deps` attribute (`corex/BUILD.bazel:21` passes one, and finding 14's fix silently drops it). I could not verify this offline — the BCR and github are both blocked. It is almost certainly fine, since `//...` analysis succeeded on the author's machine with that line present, but if anyone ever has network access, `bazel query --output=build //corex:corex_doc_test` settles it in one command.

---

### Verification log — claims re-checked independently

**Confirmed exactly as written** — these are not in dispute:

1. `cargo test --all-targets` in `server/` → exit 101, `attempt to add with overflow` at `benches/fibonacci_benchmark.rs:32:28`, frame 22 `benchmark_iterative at ./benches/fibonacci_benchmark.rs:117:15`. Plain `cargo test` → three binaries × 2 tests = 6.
2. `rust-project.json` census: 65 crates, prefix split `server_crates=54 / corex_crates=6 / combos_crates=0 / first-party=5`; all 5 first-party at `"edition": "2021"`, all 65 `"target": "aarch64-apple-darwin"`; **exactly** the six duplicate pairs at the indices claimed (`unicode_ident 2/31`, `proc_macro2 33/44`, `quote 34/45`, `syn 35/46`, `serde_derive 49/52`, `serde 55/58`); 4 `proc_macro_dylib_path` entries, all `darwin_arm64-opt-exec-…/*.dylib`; crate 62 `corex` deps `[55]`; crate 64 `server_bin` deps `[62,63,58,53]`; crate 32 deps `[]`; zero `CARGO_BIN_EXE_*` keys; `criterion`/`plotters`/`rayon` absent; `runnables[2] = {"program":"bazel","args":["run","combos/backend:backend_tests"]}` with no `{label}`; all three runnables `cwd: /Users/uriah/Code/yoyo`. `CARGO_PKG_NAME` values are literally `corex_lib`, `server_bin`, `backend_bin`, `frontend_bin`, `integrated_tests_suite_tests/just_test_test`, all at `CARGO_PKG_VERSION 0.0.0`.
3. Lockfile skew is real: `proc-macro2` 1.0.96 (corex) vs 1.0.97 (server); `syn` 2.0.104 vs 2.0.105. Package counts 7 / 119 / 2. `cargo metadata --locked` exits 0 in all three.
4. `cargo metadata --no-deps` in `server/` lists **one** test target (`just_test`), `example axum test=False`, `bench fibonacci_benchmark test=False`, all at edition 2024. `cargo metadata` at repo root → `could not find Cargo.toml`.
5. Clippy counts: corex 1 error; server 6 lint errors across 3 failing targets (4× assertions-on-constants, 1× `is_multiple_of`, 1× `unused import: super::*`); combos 2. Exactly as claimed.
6. Dep counts: `cargo tree -e normal,build` = **56**, `-e normal,build,dev` = **86**, `cargo metadata` = 119 packages. Matches.
7. Six `.gitignore` files, five leaves byte-identical (`cb20a5d35ce6b525166d98c86b144aa1`), root differs (`9f1c661030b4120bcabdfb472c94cfd4`). `*.md` / `!BAZEL_RUST_GUIDE.md` / `!README.md` at lines 77-79; `.vscode/` at :26; `.bazelrc.user`/`.bazelversion.user`/`MODULE.bazel.lock` at :2-4.
8. Git log matches: `2c55b45`, `a040163`, `6a992e2`, `589370b`, `a7c0413`, `dad071f` all present in the claimed order. The stale artifact really is on disk: `server/target/debug/build/server-9967fcf3f7be388d/out/generated.rs`.
9. README line numbers all correct: `:44` repin command, `:141/:157/:224` `@crates//:defs.bzl`, `:177` the false square-brackets note, `rust_benchmark` at `:209/:212/:223/:267`, **11** occurrences of `edition = "2021"`, `axum::Server::bind` + `/users/:id` + `use corex::Calculator` in the `:357-415` example, and 6 fabricated targets in Quick Commands.
10. Test census: 14 `#[test]` + 2 doctests = 16; exactly 7 `assert!(true)`; two byte-identical `#[should_panic] fn it_will_fail`. `combos/backend/src/main.rs` has no tests.
11. **The one claim I could upgrade from "reasoned" to proven:** finding 14's parenthetical that `all_crate_deps(normal_dev = True)` on a package with no `[dev-dependencies]` returns `[]` rather than `fail()`-ing. `rust-project.json` was produced by a successful `//...` load/analysis, and `corex:corex_tests` and `combos/backend:backend_tests` both appear as `build.label` values — so those calls demonstrably did not fail. Same argument retires the `combos_crates`-resolves-nothing worry as a load-time hazard.

**I found no factually wrong claim in the document.** All corrections below are omissions or fixes that won't apply cleanly.

---
