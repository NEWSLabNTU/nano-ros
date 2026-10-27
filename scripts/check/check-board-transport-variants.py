#!/usr/bin/env python3
"""Every transport variant of a dual-transport board crate builds (issue 2116).

A board crate that offers both `ethernet` and `serial` (mutually exclusive,
`compile_error!` on both) is built by its examples and fixtures with its
DEFAULT feature set only — which carries `ethernet`. So the `serial` variant
was compiled by nothing: `nros-board-esp32-qemu` had not built with `serial`
since a network-identity seed read the ethernet-only `mac_addr` / `ip` fields
unconditionally, and a clippy run of the ethernet variant found a lint no lane
had ever reported.

This gate DERIVES its subjects, so a new dual-transport board is covered
without a list to forget:

  * a board crate is any `packages/boards/*/Cargo.toml` whose `[features]`
    define both `ethernet` and `serial`;
  * its triple is the board descriptor's (`nros-board.toml`) first
    `target = "<triple>"`, the one place a board states it (phase-445 W2);
  * each variant is the crate's default features with the OTHER transport
    swapped out, so `rmw-zenoh` and friends stay on, as an image builds it.

Each variant runs `cargo clippy --no-default-features --features <set>
--target <triple> -- -D warnings` in a scoped target dir (never the crate's
own `target/`, phase-340 P2; one per board, since each board crate is its
own workspace root and a target dir serves one root, issue 0616). Build lane: it compiles each board twice.

`--self-test` runs the derivation controls only (no cargo); `--list` prints
the variants it would build.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
from pathlib import Path

TRANSPORTS = ("ethernet", "serial")
ROOT = Path(__file__).resolve().parents[2]


def board_triple(descriptor_text: str) -> str | None:
    """The first `target = "<triple>"` line of a board descriptor."""
    m = re.search(r'^\s*target\s*=\s*"([^"]+)"', descriptor_text, re.M)
    return m.group(1) if m else None


def variants(features: dict[str, list[str]]) -> list[list[str]]:
    """One feature set per transport: the defaults, with that transport in
    place of every other one. Empty when the crate does not offer both."""
    if not all(t in features for t in TRANSPORTS):
        return []
    defaults = [f for f in features.get("default", []) if f not in TRANSPORTS]
    return [sorted(set(defaults) | {t}) for t in TRANSPORTS]


def subjects() -> list[tuple[Path, str, list[str]]]:
    out = []
    tracked = subprocess.run(
        ["git", "ls-files", "packages/boards/*/Cargo.toml"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout.split()
    for rel in sorted(tracked):
        manifest = ROOT / rel
        feats = tomllib.loads(manifest.read_text()).get("features", {})
        vs = variants(feats)
        if not vs:
            continue
        desc = manifest.parent / "nros-board.toml"
        triple = board_triple(desc.read_text()) if desc.is_file() else None
        if triple is None:
            sys.exit(
                f"check-board-transport-variants: {rel} offers both transports "
                f"but {desc.relative_to(ROOT)} states no `target = \"<triple>\"`"
            )
        out.extend((manifest, triple, v) for v in vs)
    return out


def self_test() -> int:
    ok = True

    def check(cond: bool, what: str) -> None:
        nonlocal ok
        print(f"  {'ok  ' if cond else 'FAIL'} {what}")
        ok &= cond

    v = variants({"default": ["ethernet", "rmw-zenoh"], "ethernet": [], "serial": []})
    check(v == [["ethernet", "rmw-zenoh"], ["rmw-zenoh", "serial"]],
          "each variant keeps the defaults and swaps the transport")
    check(variants({"default": ["ethernet"], "ethernet": []}) == [],
          "a single-transport crate is not a subject")
    check(board_triple('[board]\ncargo_config = """\n[build]\ntarget = "thumbv7m-none-eabi"\n"""\n')
          == "thumbv7m-none-eabi", "the triple is read from the descriptor")
    check(len(subjects()) >= 2, "the tree has at least one dual-transport board")
    return 0 if ok else 1


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "run"
    if self_test() != 0:
        print("check-board-transport-variants: SELF-TEST FAILED", file=sys.stderr)
        return 1
    if mode == "--self-test":
        return 0
    todo = subjects()
    if mode == "--list":
        for manifest, triple, feats in todo:
            print(f"  {manifest.parent.relative_to(ROOT)}  {triple}  {','.join(feats)}")
        return 0
    failed = []
    for manifest, triple, feats in todo:
        # One target dir per board: each board crate is its own workspace root,
        # and a target dir serves exactly one root (issue 0616).
        target_dir = subprocess.run(
            ["bash", "-c", "source scripts/build/cargo.sh && "
             f"nros_scoped_target_dir board-transport-variants-{manifest.parent.name}"],
            cwd=ROOT, check=True, capture_output=True, text=True,
        ).stdout.strip()
        label = f"{manifest.parent.name} [{','.join(feats)}] {triple}"
        print(f"== {label}", flush=True)
        rc = subprocess.run(
            ["cargo", "clippy", "--quiet", "--manifest-path", str(manifest),
             "--no-default-features", "--features", ",".join(feats),
             "--target", triple, "--", "-D", "warnings"],
            cwd=ROOT, env={**os.environ, "CARGO_TARGET_DIR": target_dir},
        ).returncode
        if rc != 0:
            failed.append(label)
    if failed:
        print("check-board-transport-variants: FAILED —", *failed, sep="\n  ", file=sys.stderr)
        return 1
    print(f"check-board-transport-variants: {len(todo)} variant(s) build clippy-clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
