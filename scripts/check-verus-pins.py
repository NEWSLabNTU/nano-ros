#!/usr/bin/env python3
"""The Verus crate's two tracked pins agree with the provisioned Verus binary.

Issue 2499: `[tool.verus]` in `nros-sdk-index.toml` pins one Verus release.
That release is built against one rust toolchain and bundles one `vstd`. The
crate states both again, and they drifted apart: the toolchain file said
1.93.0 for a binary built on 1.96.0, and a caret `vstd` requirement let cargo
resolve a crates.io `vstd` newer than the binary, so rustc panicked inside
`vstd` with no mention of either pin.

The binary's bundle is the authority (`version.json` names the toolchain,
`vstd/Cargo.toml` the `vstd` version). This compares, against it:

  * `packages/verification/nros-verification/rust-toolchain.toml` channel
  * the `vstd` requirement in that crate's `Cargo.toml`, which must be `=`
  * the `vstd` version its `Cargo.lock` resolved

Usage:
    check-verus-pins.py <verus bin dir>    # check the tree
    check-verus-pins.py --self-test        # negative controls
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # python < 3.11
    import tomli as tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
CRATE = ROOT / "packages/verification/nros-verification"


def bundle_facts(verus_dir: pathlib.Path) -> tuple[str, str]:
    """(toolchain channel, vstd version) the binary was released with."""
    info = json.loads((verus_dir / "version.json").read_text())
    # `1.96.0-x86_64-unknown-linux-gnu` -> `1.96.0`; rustup accepts either,
    # but the toolchain file states the bare channel.
    tc = re.sub(r"-[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+(-[a-z0-9_]+)?$", "",
                info["verus"]["toolchain"])
    with open(verus_dir / "vstd/Cargo.toml", "rb") as fh:
        vstd = tomllib.load(fh)["package"]["version"]
    return tc, vstd


def crate_facts(crate: pathlib.Path) -> tuple[str, str, str | None]:
    """(toolchain channel, vstd requirement, locked vstd version)."""
    with open(crate / "rust-toolchain.toml", "rb") as fh:
        tc = tomllib.load(fh)["toolchain"]["channel"]
    with open(crate / "Cargo.toml", "rb") as fh:
        dep = tomllib.load(fh)["dependencies"]["vstd"]
    req = dep if isinstance(dep, str) else dep.get("version", "")
    locked = None
    lock = crate / "Cargo.lock"
    if lock.exists():
        with open(lock, "rb") as fh:
            for pkg in tomllib.load(fh).get("package", []):
                if pkg["name"] == "vstd":
                    locked = pkg["version"]
    return tc, req, locked


def disagreements(bundle, crate) -> list[str]:
    b_tc, b_vstd = bundle
    c_tc, c_req, c_locked = crate
    out = []
    if c_tc != b_tc:
        out.append(f"rust-toolchain.toml says `{c_tc}`, but the Verus binary was "
                   f"built with `{b_tc}`. Set `channel = \"{b_tc}\"`.")
    if c_req != f"={b_vstd}":
        out.append(f"Cargo.toml requires vstd `{c_req}`, but the Verus binary "
                   f"bundles `{b_vstd}`. Set `vstd = \"={b_vstd}\"` (exact: a "
                   "newer vstd is not compiled by this binary).")
    if c_locked is not None and c_locked != b_vstd:
        out.append(f"Cargo.lock resolved vstd `{c_locked}`, not `{b_vstd}`. Run "
                   f"`cargo update -p vstd --precise {b_vstd}` in the crate.")
    return out


def self_test() -> int:
    good = ("1.96.0", "0.0.0-2026-06-14-0213")
    cases = [
        ("agreeing pins", ("1.96.0", "=0.0.0-2026-06-14-0213", "0.0.0-2026-06-14-0213"), 0),
        # The shape that shipped: old toolchain and a caret requirement that
        # resolved a newer crates.io release.
        ("the shipped drift", ("1.93.0", "0.0.0-2026-02-08-0120", "0.0.0-2026-07-27-0206"), 3),
        ("caret req, right lock", ("1.96.0", "0.0.0-2026-06-14-0213", "0.0.0-2026-06-14-0213"), 1),
        ("no lock is not a disagreement", ("1.96.0", "=0.0.0-2026-06-14-0213", None), 0),
        ("toolchain only", ("1.95.0", "=0.0.0-2026-06-14-0213", "0.0.0-2026-06-14-0213"), 1),
    ]
    for name, crate, want in cases:
        got = disagreements(good, crate)
        assert len(got) == want, (name, got)
    print(f"verus-pins self-test: {len(cases)} case(s) OK")
    return 0


def main(argv) -> int:
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) != 2:
        print("usage: check-verus-pins.py <verus bin dir> | --self-test", file=sys.stderr)
        return 2
    verus_dir = pathlib.Path(argv[1])
    bad = disagreements(bundle_facts(verus_dir), crate_facts(CRATE))
    for line in bad:
        print(f"FAIL {line}", file=sys.stderr)
    if bad:
        print("  The Verus binary, the crate's rust toolchain and vstd are one "
              "release; move all three together with `[tool.verus]` in "
              "nros-sdk-index.toml.", file=sys.stderr)
        return 1
    print("verus-pins: toolchain and vstd agree with the Verus binary")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
