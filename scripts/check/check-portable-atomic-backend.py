#!/usr/bin/env python3
"""Issue 2962 -- no library crate chooses portable-atomic's backend.

On a target with no atomic CAS (thumbv6m: rpi-pico-w-zephyr; riscv32imc),
portable-atomic needs a backend: the `critical-section` feature (an impl
supplied by someone) or `unsafe-assume-single-core` (interrupts off). It
refuses both at once. Which one is right is a fact about the SYSTEM, and
portable-atomic's own documentation asks libraries to leave it to the end
user. Four library crates chose `unsafe-assume-single-core` by target cfg,
and every Rust image on rpi-pico-w-zephyr failed to compile, because the
`zephyr` crate (upstream) chooses `critical-section` there.

The image root chooses instead:
  - a Zephyr Rust image: upstream's `zephyr` crate (`critical-section`);
  - a C/C++ image: `nros-c`'s `critical-section` capability, which forwards
    `portable-atomic/critical-section` (nros-c is that image's cargo root);
  - a bare-metal board: its own board crate (packages/boards/), or the
    target's rustflags.

This gate refuses a tracked Cargo.toml outside those owners that enables one
of the backend features on portable-atomic, as a dependency feature (in any
dependency table, target-scoped ones included) or as a `portable-atomic/<f>`
forward in `[features]`.

  check-portable-atomic-backend.py            the tree
  check-portable-atomic-backend.py --self-test
"""
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
BACKENDS = {"critical-section", "unsafe-assume-single-core", "unsafe-assume-privileged"}
# The owners: a crate that is an image's cargo root, or a board.
OWNER_FILES = {"packages/api/nros-c/Cargo.toml"}
OWNER_PREFIXES = ("packages/boards/", "examples/")


def is_owner(rel):
    return rel in OWNER_FILES or rel.startswith(OWNER_PREFIXES)


def _dep_tables(doc):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        if isinstance(doc.get(key), dict):
            yield key, doc[key]
    for cfg, tbl in (doc.get("target") or {}).items():
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            if isinstance(tbl.get(key), dict):
                yield "target.%s.%s" % (cfg, key), tbl[key]


def findings(text):
    """[(where, feature)] of the backend choices one manifest makes."""
    doc = tomllib.loads(text)
    out = []
    for where, deps in _dep_tables(doc):
        for name, spec in deps.items():
            pkg = spec.get("package", name) if isinstance(spec, dict) else name
            if pkg != "portable-atomic" or not isinstance(spec, dict):
                continue
            for f in spec.get("features", []):
                if f in BACKENDS:
                    out.append((where, f))
    for feat, items in (doc.get("features") or {}).items():
        for item in items:
            dep, _, f = item.partition("/")
            if dep.rstrip("?") == "portable-atomic" and f in BACKENDS:
                out.append(("features.%s" % feat, f))
    return out


def scan(root):
    files = subprocess.run(["git", "-C", str(root), "ls-files", "*Cargo.toml"],
                           capture_output=True, text=True, check=True).stdout.split()
    bad = []
    for rel in files:
        if is_owner(rel) or "/third-party/" in "/" + rel:
            continue
        try:
            text = (root / rel).read_text(encoding="utf8")
        except OSError:
            continue
        for where, f in findings(text):
            bad.append((rel, where, f))
    return bad


def self_test():
    failures = 0
    cases = [
        ("a library's target-scoped single-core row",
         '[target.\'cfg(all(target_os = "none", not(target_has_atomic = "ptr")))\'.dependencies]\n'
         'portable-atomic = { version = "1", default-features = false, features = ["unsafe-assume-single-core"] }\n',
         1),
        ("a feature forward", '[features]\nx = ["portable-atomic/critical-section"]\n'
         '[dependencies]\nportable-atomic = "1"\n', 1),
        ("an optional forward", '[features]\nx = ["portable-atomic?/unsafe-assume-privileged"]\n', 1),
        ("a renamed dependency",
         '[dependencies]\npa = { package = "portable-atomic", version = "1", features = ["critical-section"] }\n', 1),
        ("the plain dependency", '[dependencies]\nportable-atomic = { version = "1", default-features = false }\n', 0),
        ("an unrelated critical-section", '[features]\nx = ["dep:critical-section"]\n'
         '[dependencies]\ncritical-section = { version = "1", optional = true }\n', 0),
        ("fallback, which is not a backend", '[features]\nx = ["portable-atomic/fallback"]\n', 0),
    ]
    for name, text, want in cases:
        got = len(findings(text))
        if got != want:
            print("self-test: %s: %d finding(s), want %d" % (name, got, want))
            failures += 1
    for rel, want in (("packages/api/nros-c/Cargo.toml", True), ("packages/boards/x/Cargo.toml", True),
                      ("packages/core/nros-log/Cargo.toml", False)):
        if is_owner(rel) != want:
            print("self-test: owner(%s) is %s, want %s" % (rel, is_owner(rel), want))
            failures += 1
    print("check-portable-atomic-backend self-test: %s" % ("FAILED (%d)" % failures if failures else "ok"))
    return 1 if failures else 0


def main(argv):
    rc = self_test()
    if argv == ["--self-test"] or rc:
        return rc
    bad = scan(ROOT)
    if not bad:
        print("check-portable-atomic-backend: OK (no library crate chooses portable-atomic's backend)")
        return 0
    print("check-portable-atomic-backend: FAILED (issue 2962) -- a library crate chooses "
          "portable-atomic's backend:", file=sys.stderr)
    for rel, where, f in bad:
        print("    %s  [%s]  %s" % (rel, where, f), file=sys.stderr)
    print("\n  portable-atomic refuses `critical-section` and `unsafe-assume-*` together, and which\n"
          "  one a CAS-less target needs is the image's choice (the `zephyr` crate, nros-c's\n"
          "  `critical-section`, or a board crate). Depend on portable-atomic with\n"
          "  `default-features = false` and no backend feature; state the backend in the\n"
          "  board or the image root.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
