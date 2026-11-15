#!/usr/bin/env python3
"""The example lint lane's units, and the proof that they cover every example.

`check::linux-check` (`scripts/build/lint-examples.sh`) lints the example Rust
packages a host can lint. There are two shapes of them:

  leaf     an `examples/<plat>/<lang>/<name>/` (or deeper) directory holding
           its own `Cargo.toml` and `system.toml`; linted by
           `lint-example-leaf.sh`.
  witness  a one-node workspace (`.colcon_workspace` marker at
           `examples/<plat>/<lang>/<name>/`, phase-531 W6), whose node
           packages sit under `src/<pkg>/`; linted by
           `lint-example-witness.sh`, through the workspace's own configure.

Issue 2595: the leaf list dropped every witness package ("the fixture rows
build it"), and no lane linted one, so as examples migrated the lane went from
32 leaves to 1 and kept passing. So this file states the units AND checks them:
every tracked example `Cargo.toml` is linted by exactly one unit, or is named
by an exclusion below with its reason. A filter that drops a package without a
unit to take it fails here, on the listing path the gate itself runs.

Usage:
  example_lint_units.py list         print `<kind>\t<unit>\t<dir>` per unit
                                     (after the coverage check)
  example_lint_units.py packages <dir>
                                     print the Cargo.toml packages a witness
                                     unit lints, one dir per line
  example_lint_units.py --self-test
"""

import re
import subprocess
import sys
from pathlib import PurePosixPath

MARKER = ".colcon_workspace"

# Example trees this host lane cannot lint, each with the reason. One spelling:
# `lint-examples.sh` held these as a `grep -vE` until issue 2595.
EXCLUDED = [
    ("/zephyr/", "built by west in the Zephyr lanes; its Rust needs the west workspace"),
    ("/mps2-an385-freertos/", "cross RTOS image, linted by its platform lane"),
    ("/qemu-armv7a-nuttx/", "cross RTOS image, linted by its platform lane"),
    ("/threadx-linux/", "RTOS image, built by its platform lane"),
    ("/rv-virt-threadx/", "cross RTOS image, linted by its platform lane"),
    ("/px4/", "PX4 module, built inside the PX4 tree"),
    ("/workspaces/", "multi-target workspaces whose entries are cross-compiled"),
    ("/multi-package-workspace/", "message packages resolve only via the workspace's patch rows"),
    ("/templates/local-msg-package/", "message packages resolve only via the workspace's patch rows"),
    ("/templates/multi-node-workspace/", "message packages resolve only via the workspace's patch rows"),
    ("/templates/workspace-shadowing/", "message packages resolve only via the workspace's patch rows"),
]
EXCLUDE_RE = re.compile("|".join(re.escape(p) for p, _ in EXCLUDED))


def tracked_manifests():
    out = subprocess.run(
        ["git", "ls-files", "examples/**/Cargo.toml"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    # `NF>=5`: a package below `examples/<plat>/<lang>/<name>/`, never a
    # workspace-root manifest.
    return sorted(p for p in out if len(PurePosixPath(p).parts) >= 5)


def witness_of(rel, is_marker):
    parts = PurePosixPath(rel).parts
    if len(parts) < 5 or parts[1] in ("workspaces", "templates"):
        return None
    root = str(PurePosixPath(*parts[:4]))
    return root if is_marker(root) else None


def plan(manifests, is_marker):
    """(units, covered, excluded): units are (kind, unit, dir)."""
    units, covered, excluded, witnesses = [], {}, {}, {}
    for m in manifests:
        if EXCLUDE_RE.search(m):
            excluded[m] = next(r for p, r in EXCLUDED if p in m)
            continue
        w = witness_of(m, is_marker)
        if w:
            witnesses.setdefault(w, []).append(m)
        else:
            d = str(PurePosixPath(m).parent)
            units.append(("leaf", unit_name(d), d))
            covered[m] = unit_name(d)
    for w, ms in sorted(witnesses.items()):
        units.append(("witness", unit_name(w), w))
        for m in ms:
            covered[m] = unit_name(w)
    return units, covered, excluded


def unit_name(d):
    return d.removeprefix("examples/").replace("/", "-")


def uncovered(manifests, covered, excluded):
    return [m for m in manifests if m not in covered and m not in excluded]


def is_marker_on_disk(root):
    return (PurePosixPath(root) / MARKER).as_posix() in _markers()


_MARKER_CACHE = None


def _markers():
    global _MARKER_CACHE
    if _MARKER_CACHE is None:
        _MARKER_CACHE = set(subprocess.run(
            ["git", "ls-files", f"examples/*/*/*/{MARKER}"],
            capture_output=True, text=True, check=True,
        ).stdout.split())
    return _MARKER_CACHE


def self_test():
    marks = {"examples/linux/rust/talker"}
    ms = [
        "examples/linux/rust/talker/src/native_talker/Cargo.toml",
        "examples/mps2-an385-baremetal/rust/listener/Cargo.toml",
        "examples/zephyr/rust/talker/Cargo.toml",
        "examples/workspaces/rust/src/a/Cargo.toml",
    ]
    units, covered, excluded = plan(ms, marks.__contains__)
    bad = 0

    def case(name, ok):
        nonlocal bad
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
        bad += not ok

    case("a witness package is linted by its witness's unit",
         covered.get(ms[0]) == "linux-rust-talker"
         and ("witness", "linux-rust-talker", "examples/linux/rust/talker") in units)
    case("a leaf is its own unit",
         ("leaf", "mps2-an385-baremetal-rust-listener",
          "examples/mps2-an385-baremetal/rust/listener") in units)
    case("an excluded tree names its reason",
         ms[2] in excluded and ms[3] in excluded)
    case("every package is covered or excluded",
         uncovered(ms, covered, excluded) == [])
    # Negative control: issue 2595's shape. A filter drops the witness package
    # and nothing takes it: the check must name it.
    dropped = {k: v for k, v in covered.items() if k != ms[0]}
    case("negative control: a dropped witness package is reported",
         uncovered(ms, dropped, excluded) == [ms[0]])
    if bad:
        print("example_lint_units self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if argv[:1] == ["packages"] and len(argv) == 2:
        w = argv[1].rstrip("/")
        for m in tracked_manifests():
            if m.startswith(w + "/"):
                print(PurePosixPath(m).parent)
        return 0
    if argv != ["list"]:
        print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
        return 2
    # The negative control runs on the normal path, quietly unless it fails.
    import contextlib, io
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = self_test()
    if rc:
        sys.stderr.write(buf.getvalue())
        return 1
    ms = tracked_manifests()
    units, covered, excluded = plan(ms, is_marker_on_disk)
    missing = uncovered(ms, covered, excluded)
    if missing or not units:
        print("example lint: these tracked example packages are linted by NO unit "
              "and named by no exclusion (issue 2595):", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        if not units:
            print("  (no unit at all: the enumeration is broken, which is not a pass)",
                  file=sys.stderr)
        return 1
    for kind, unit, d in units:
        print(f"{kind}\t{unit}\t{d}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
