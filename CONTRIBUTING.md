# Contributing

## Getting a build

```sh
bazel build //...        # Bazel: pinned Rust 1.94.1, edition 2024
bazel test  //...
cargo build --all-targets   # Cargo: run inside corex/, server/ or combos/
```

Bazel resolves its Rust toolchain from `rust.toolchain()` in `MODULE.bazel`;
cargo resolves the same version from `rust-toolchain.toml`. **Those two must
always name the same version.** If they drift, the two build systems compile the
same source with different compilers, which is the class of bug this repo has
already been bitten by once. To bump:

```sh
python3 tools/pin_rust_toolchain.py 1.95.0    # prints the sha256s block
# paste into MODULE.bazel, and set the same version in rust-toolchain.toml
```

## Editor setup

Nothing is required — rust-analyzer discovers the Cargo projects on its own. For
Bazel-accurate metadata (Bazel-only edges, exact rustc flags), run `./build-ra.sh`.
The `rust-project.json` it writes is gitignored because it embeds absolute paths
from the machine that generated it.

## `MODULE.bazel.lock`

This file is committed, and **CI maintains it.** It records the resolved module
graph plus `registryFileHashes` — the hashes of every file `bcr.bazel.build`
served during resolution. That is what makes a bzlmod build reproducible, and
what `--config=locked` (`--lockfile_mode=error`) verifies.

You normally do not touch it: any local `bazel build` updates it as a side
effect, and if you forget to commit that, the `lockfile` job in CI regenerates
and commits it for you. Two things are worth knowing:

- **Changing `MODULE.bazel` changes the lock.** Adding a `bazel_dep`, bumping
  `rules_rust`, or editing the toolchain pin will all produce a diff. Commit it
  with the change rather than leaving it to CI, so the PR shows the real
  dependency delta.
- **A lock generated against a mirror is not valid.** If you build with
  `--registry=file://...` (an offline or air-gapped setup), Bazel records no
  registry hashes at all, and the resulting lock is treated as out of date the
  moment anyone builds against the real registry. Do not commit one; let CI
  produce it.

Check whether yours is current:

```sh
bazel build --config=locked //...
```

## Before you push

CI gates both build systems. Run the same things locally:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings   # per workspace directory
cargo test --all-targets --locked
bazel test --config=ci //...
bazel build --config=ci --config=lint //...          # rustfmt + clippy aspects
```

`--config=lint` reads `//:clippy.toml` and `//:rustfmt.toml`, which the root
`BUILD.bazel` has to `exports_files` for the rules_rust settings package to see.

## Adding a crate

See `BAZEL_RUST_GUIDE.md`. The short version: add it to the existing
`crate.from_cargo(manifests = [...])` list — do **not** create a second
`Cargo.lock` or a second crate repo.
