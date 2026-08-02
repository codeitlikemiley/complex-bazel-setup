#!/usr/bin/env python3
"""Emit the `sha256s` dict for MODULE.bazel's `rust.toolchain(...)` tag.

rules_rust only ships built-in hashes (rust/known_shas.bzl) for the Rust
versions that existed when that rules_rust release was cut. For anything newer
-- including the version this repo pins -- `load_arbitrary_tool` falls back to
`sha256 = ""`, downloads the toolchain unverified and marks the repository
non-reproducible. This script closes that hole by reading the authoritative
channel manifest from static.rust-lang.org.

Keys must match rules_rust's `produce_tool_path(tool, version, triple)` plus the
archive extension, i.e. "<tool>-<version>-<triple>.tar.xz" (rust-src has no
triple). A key that does not match is silently ignored and the download simply
falls back to unverified, so a mistake here degrades rather than breaks.

Usage:
    python3 tools/pin_rust_toolchain.py 1.94.1
then paste the output into the `sha256s = {...}` attribute in MODULE.bazel.
"""

from __future__ import annotations

import sys
import tomllib
import urllib.request

MANIFEST = "https://static.rust-lang.org/dist/channel-rust-{}.toml"

# Host platforms we build on. Extend when a new exec platform joins the team;
# any triple left out just falls back to an unverified download.
EXEC_TRIPLES = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
]

# rules_rust's DEFAULT_EXTRA_TARGET_TRIPLES: std is fetched for these too.
EXTRA_TARGET_TRIPLES = ["wasm32-unknown-unknown", "wasm32-wasip1"]

# rules_rust tool name -> package name in the channel manifest.
HOST_TOOLS = {
    "rustc": "rustc",
    "cargo": "cargo",
    "clippy": "clippy-preview",
    "rustfmt": "rustfmt-preview",
    "llvm-tools": "llvm-tools-preview",
    "rust-std": "rust-std",
}


def main(version: str) -> int:
    with urllib.request.urlopen(MANIFEST.format(version), timeout=300) as fh:
        manifest = tomllib.loads(fh.read().decode())
    pkg = manifest["pkg"]

    entries: dict[str, str] = {}
    missing: list[str] = []

    def take(tool: str, pkg_name: str, triple: str | None) -> None:
        target = pkg[pkg_name]["target"].get(triple if triple else "*")
        if not target or not target.get("available"):
            missing.append(f"{pkg_name} / {triple}")
            return
        name = "-".join(x for x in (tool, version, triple) if x) + ".tar.xz"
        url_name = target["xz_url"].rsplit("/", 1)[-1]
        if url_name != name:
            missing.append(f"{pkg_name} / {triple}: url is {url_name}, key would be {name}")
            return
        entries[name] = target["xz_hash"]

    for triple in EXEC_TRIPLES:
        for tool, pkg_name in HOST_TOOLS.items():
            take(tool, pkg_name, triple)
    for triple in EXTRA_TARGET_TRIPLES:
        take("rust-std", "rust-std", triple)
    take("rust-src", "rust-src", None)

    for key in sorted(entries):
        print(f'        "{key}": "{entries[key]}",')

    if missing:
        print(f"\n# WARNING: no hash emitted for: {'; '.join(missing)}", file=sys.stderr)
    print(f"# {len(entries)} entries for Rust {version}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
