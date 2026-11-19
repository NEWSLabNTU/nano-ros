#!/usr/bin/env python3
"""The image crate's bundling decision agrees with the CLI's -- issue 2726.

Whether a generated image crate bundles an RMW backend into the image's own
staticlib is decided twice:

* in Rust, `RmwDescriptor::image_crate_bundles()`, which the NuttX make road's
  refusal asks (`nros locate nuttx-cargo-config`, issue 2705), and
* in cmake, `nros_image_backend()` in `cmake/NanoRosImageCrate.cmake`, which
  writes the image crate.

If they drift, the make road either refuses a backend the crate would bundle,
or lets through one the crate leaves out: an image with no backend in it and
nothing said. Issue 2726 tracks making the cmake side read a Rust-computed row
instead; until then this gate holds the two equal.

For every RMW the scan announces, in BOTH directions:

    bundled by nros_image_backend()  <=>  `--image-bundles` says ON

The Rust side is `nros dev rmw dispatch --known --image-bundles`; the cmake side
is `scripts/check/image-crate-bundling-harness.cmake`, which runs the real
`nros_image_backend()` on the real dispatch rows. Both scan the same search
path (the nano-ros tree, an empty workspace).

Negative controls run on every invocation, against the live tree: the harness
edits one RMW's dispatch rows after the query (drops a bundled backend's
register crate, flips its link strategy, forces an unbundled one to bundle)
and the comparison must go red each time. A gate that cannot see those
cannot see a real drift either.

Needs cmake and the in-tree CLI (`just setup-cli`); without the CLI it reports
NOT VERIFIED through the `nros_check_skip` ledger, never a quiet pass.

Usage::

    check-image-crate-bundling-agreement.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402  issue 1043's middle outcome, NOT VERIFIED

NAME = "image-crate-bundling-agreement"
# The in-tree CLI, named explicitly to both sides: left to resolve itself, the
# cmake dispatch finds whatever `nros` the PATH holds.
CLI = ROOT / "packages" / "cli" / "target" / "release" / "nros"
HARNESS = ROOT / "scripts" / "check" / "image-crate-bundling-harness.cmake"


def parse(text: str) -> dict[str, bool]:
    """`name<TAB>ON|OFF` lines -> {name: bundled}."""
    out = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        name, _, verdict = line.partition("\t")
        if verdict not in ("ON", "OFF"):
            raise ValueError(f"expected <name><TAB>ON|OFF, got {line!r}")
        out[name] = verdict == "ON"
    return out


def compare(rust: dict[str, bool], cmake: dict[str, bool]) -> list[str]:
    """Every disagreement, both directions, and every name one side lacks."""
    errors = []
    for name in sorted(set(rust) | set(cmake)):
        if name not in cmake:
            errors.append(f"{name}: the CLI lists it, the cmake harness gave no answer")
        elif name not in rust:
            errors.append(f"{name}: the cmake harness answered, the CLI does not list it")
        elif rust[name] and not cmake[name]:
            errors.append(
                f"{name}: image_crate_bundles() says bundled, nros_image_backend() "
                "leaves it out -- the NuttX make road would build an image with no backend")
        elif cmake[name] and not rust[name]:
            errors.append(
                f"{name}: nros_image_backend() bundles it, image_crate_bundles() says "
                "not -- the NuttX make road would refuse a backend it can build")
    return errors


def rust_side(ws: Path) -> dict[str, bool]:
    r = subprocess.run(
        [str(CLI), "dev", "rmw", "dispatch", "--known", "--image-bundles",
         "--workspace", str(ws), "--nano-ros-root", str(ROOT)],
        capture_output=True, text=True, check=False, cwd=ws,
    )
    if r.returncode != 0:
        raise RuntimeError(f"nros dev rmw dispatch --known --image-bundles failed:\n{r.stderr}")
    return parse(r.stdout)


def cmake_side(ws: Path, rmws: list[str], mutate: tuple[str, str] | None = None) -> dict[str, bool]:
    cmd = ["cmake", f"-D_NANO_ROS_CODEGEN_TOOL={CLI}", f"-DRMWS={';'.join(rmws)}"]
    if mutate:
        cmd += [f"-DMUTATE_RMW={mutate[0]}", f"-DMUTATE={mutate[1]}"]
    cmd += ["-P", str(HARNESS)]
    # cwd is the workspace: in script mode CMAKE_SOURCE_DIR is the cwd, which
    # the dispatch query passes as `--workspace`.
    r = subprocess.run(cmd, capture_output=True, text=True, check=False, cwd=ws)
    if r.returncode != 0:
        raise RuntimeError(f"the cmake harness failed (rc={r.returncode}):\n{r.stderr[-2000:]}")
    return parse(r.stdout)


def negative_controls(ws: Path, rust: dict[str, bool]) -> list[str]:
    """Each mutation must make `compare` go red. Returns the ones that did NOT."""
    on = sorted(n for n, b in rust.items() if b)
    off = sorted(n for n, b in rust.items() if not b)
    if not on or not off:
        return [f"the scan has no {'bundled' if not on else 'unbundled'} RMW, so a "
                "negative control cannot run in that direction; refusing to pass"]
    misses = []
    for rmw, mutation in ((on[0], "drop-register"), (on[0], "flip-strategy"),
                          (off[0], "force-bundle")):
        got = cmake_side(ws, [rmw], (rmw, mutation))
        if not compare({rmw: rust[rmw]}, got):
            misses.append(f"negative control {mutation} on {rmw} stayed green ({got})")
    return misses


def self_test() -> None:
    """The comparison, on synthetic answers."""
    assert parse("a\tON\nb\tOFF\n") == {"a": True, "b": False}
    try:
        parse("a\tyes\n")
        raise AssertionError("parse accepted a verdict that is not ON/OFF")
    except ValueError:
        pass
    assert compare({"a": True, "b": False}, {"a": True, "b": False}) == []
    e = compare({"a": True}, {"a": False})
    assert len(e) == 1 and "leaves it out" in e[0], e
    e = compare({"b": False}, {"b": True})
    assert len(e) == 1 and "would refuse" in e[0], e
    assert any("no answer" in x for x in compare({"a": True}, {}))
    assert any("does not list" in x for x in compare({}, {"a": True}))


def main() -> int:
    self_test()
    if shutil.which("cmake") is None:
        print(f"check-{NAME}: cmake not on PATH", file=sys.stderr)
        return 1
    if not CLI.is_file():
        return check_skip.unverified(
            NAME,
            f"no in-tree CLI at {CLI.relative_to(ROOT)} (both sides ask it for the "
            "backends' facts) -- run `just setup-cli`")
    with tempfile.TemporaryDirectory() as d:
        ws = Path(d)
        try:
            rust = rust_side(ws)
            if not rust:
                print(f"check-{NAME}: the CLI lists no RMW; refusing to pass", file=sys.stderr)
                return 1
            errors = compare(rust, cmake_side(ws, sorted(rust)))
            errors += negative_controls(ws, rust)
        except (RuntimeError, ValueError) as exc:
            errors = [str(exc)]
    if errors:
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print(f"check-{NAME}: FAILED (issue 2726: the two spellings of one rule)",
              file=sys.stderr)
        return 1
    bundled = ", ".join(n for n in sorted(rust) if rust[n])
    print(f"check-{NAME}: OK ({len(rust)} RMW(s) agree, bundled: {bundled}; "
          "3 negative controls red; self-test ok)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
