#!/usr/bin/env bash
# Regenerate rust-project.json for rust-analyzer.
#
# The generated file is machine-specific -- it embeds absolute output-base paths
# and the host target triple -- so it is gitignored. Run this after changing any
# BUILD.bazel, or skip it entirely: rust-analyzer also discovers corex/, server/
# and combos/ through plain Cargo.
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BAZEL="${BAZEL:-$(command -v bazelisk || command -v bazel || true)}"
if [[ -z "$BAZEL" ]]; then
    echo "error: neither bazelisk nor bazel found on PATH" >&2
    exit 1
fi

exec "$BAZEL" run @rules_rust//tools/rust_analyzer:gen_rust_project -- "${@:-//...}"
