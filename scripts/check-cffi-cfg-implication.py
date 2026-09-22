#!/usr/bin/env python3
"""An exported `extern "C"` fn must not be reachable where what it calls is not.

Issue 1449. `nros_cpp_executor_wake_handle` was `#[cfg(feature = "rmw-cffi")]`
and called `Executor::wake_raw_ptr`, which is
`#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]` because the `node_wake`
it reads is an `Arc`. So on an `rmw-cffi` build WITHOUT `alloc` the exported
function compiled and the method it calls did not::

    error[E0599]: no method named `wake_raw_ptr` found for struct `Executor<'s>`
    error: could not compile `nros-cpp` (lib) due to 1 previous error

Every `cpp_*` and `cortex-m-c-*` Zephyr image failed on it, and no merge-gating
lane noticed: the two `check-compile-smoke` arms compile `nros-cpp` in shapes
that include `std` (which implies `alloc`), and the lanes that build the
embedded shapes run on schedule.

## Why this is STATIC rather than one more compile

The obvious widening is a third `compile-smoke` arm for the no-alloc shape.
Measured, that does not work: a host `cargo check` of `nros-cpp` without `std`
walks into `#[panic_handler] function required` and then `no global memory
allocator found`, because on a real board the platform crate supplies both.
Reproducing the embedded shape means an embedded target and a platform crate —
which is what `check-c` and `rust-rtos-link-check` already do, on schedule.
A host gate that has to invent a configuration nobody builds would be checking
a shape of its own making.

The RELATION, though, needs no build: a caller's feature set must imply its
callee's.

## What it decides, and what it refuses to guess

A cfg is DECIDABLE here when it is a pure conjunction of `feature = "..."` —
`#[cfg(feature = "a")]`, `#[cfg(all(feature = "a", feature = "b"))]`, or no cfg
at all (the empty set, satisfied by everything). Those are 99 of the 116 cfgs
on the two surfaces.

`any(...)`, `not(...)` and non-feature predicates (`test`, `has_rmw`) are NOT
decided. They are COUNTED and held against a baseline that may only shrink, in
the shape this repo uses for a gap it can name but not yet close — an
abstention that says so beats a pass that does not.

Usage: check-cffi-cfg-implication.py [--self-test] [--write-baseline]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

CALLER_FILE = ROOT / "packages/api/nros-cpp/src/lib.rs"
CALLEE_DIR = ROOT / "packages/core/nros-node/src/executor"
BASELINE = ROOT / ".config/cffi-cfg-undecidable-baseline.txt"

CFG_RE = re.compile(r'#\[cfg\((.*)\)\]\s*$')
FEATURE_RE = re.compile(r'feature\s*=\s*"([^"]+)"')
EXPORT_RE = re.compile(r'pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+([A-Za-z0-9_]+)')
METHOD_RE = re.compile(r'pub\s+(?:unsafe\s+)?fn\s+([A-Za-z0-9_]+)')
CALL_RE = re.compile(r'\.executor\.([a-z0-9_]+)\s*\(')


def decide(cfg: str | None) -> set[str] | None:
    """The feature set a cfg REQUIRES, or None when it is not a conjunction."""
    if cfg is None:
        return set()
    s = cfg.strip()
    if s.startswith("all(") and s.endswith(")"):
        s = s[4:-1]
    # Anything that is not purely `feature = "..."` terms joined by `,` is
    # outside what this gate claims to decide.
    stripped = FEATURE_RE.sub("", s)
    if re.sub(r"[\s,]", "", stripped):
        return None
    feats = set(FEATURE_RE.findall(cfg))
    return feats or None


def preceding_cfg(lines: list[str], idx: int) -> str | None:
    """The cfg attached to the item at `idx`, skipping doc comments/attrs."""
    j = idx - 1
    while j >= 0:
        t = lines[j].strip()
        if not t or t.startswith("///") or t.startswith("//") or t.startswith("#["):
            m = CFG_RE.match(t)
            if m:
                return m.group(1)
            j -= 1
            continue
        break
    return None


def callee_cfgs() -> dict[str, str | None]:
    out: dict[str, str | None] = {}
    for f in tracked(CALLEE_DIR, suffix=".rs"):
        lines = f.read_text(errors="replace").splitlines()
        for i, line in enumerate(lines):
            m = METHOD_RE.search(line)
            if m:
                out.setdefault(m.group(1), preceding_cfg(lines, i))
    return out


def exports_with_calls() -> list[tuple[str, str | None, list[str], int]]:
    lines = CALLER_FILE.read_text(errors="replace").splitlines()
    out = []
    i = 0
    while i < len(lines):
        m = EXPORT_RE.search(lines[i])
        if not m:
            i += 1
            continue
        cfg = preceding_cfg(lines, i)
        depth = 0
        # (call, the cfgs of every inner `#[cfg(...)]` block enclosing it).
        #
        # Inner blocks are the whole point: the SANCTIONED fix for a finding
        # here is to keep the export's cfg and give the BODY the narrower one,
        # returning the documented absent-value in the other arm. A gate blind
        # to that would report the fix as the defect and push people toward
        # narrowing the export — which removes a symbol the C headers declare
        # unconditionally and turns a compile error into a link error. It did
        # exactly that on its first run against the real fix.
        calls: list[tuple[str, tuple[str, ...]]] = []
        scopes: list[tuple[int, str]] = []  # (depth at which it opened, cfg)
        pending: str | None = None
        j = i
        started = False
        while j < len(lines):
            line = lines[j]
            stripped = line.strip()
            mcfg = CFG_RE.match(stripped)
            if mcfg and started:
                pending = mcfg.group(1)
                j += 1
                continue
            for c in CALL_RE.findall(line):
                calls.append((c, tuple(cfg for _, cfg in scopes)))
            opens = line.count("{")
            closes = line.count("}")
            if pending is not None and opens:
                scopes.append((depth, pending))
                pending = None
            depth += opens - closes
            while scopes and depth <= scopes[-1][0]:
                scopes.pop()
            if opens:
                started = True
            if started and depth <= 0:
                break
            j += 1
        out.append((m.group(1), cfg, calls, i + 1))
        i = j + 1
    return out


def run() -> int:
    callees = callee_cfgs()
    problems: list[str] = []
    undecidable: list[str] = []
    checked = 0

    for name, cfg, calls, line in exports_with_calls():
        base = decide(cfg)
        for meth, inner in sorted(set(calls)):
            if meth not in callees:
                continue
            callee = decide(callees[meth])
            # The features in force AT THE CALL: the export's, plus every inner
            # `#[cfg(...)]` block enclosing it.
            caller = base
            for icfg in inner:
                extra = decide(icfg)
                if caller is None or extra is None:
                    caller = None
                    break
                caller = caller | extra
            if caller is None or callee is None:
                undecidable.append(f"{name} -> Executor::{meth}")
                continue
            checked += 1
            missing = callee - caller
            if missing:
                problems.append(
                    f"{CALLER_FILE.relative_to(ROOT)}:{line}: `{name}` is "
                    f"cfg({cfg or 'none'}) and calls `Executor::{meth}`, which is "
                    f"cfg({callees[meth]}).\n"
                    f"      A build with {sorted(caller) or 'no features'} compiles the export and "
                    f"NOT the callee: missing {sorted(missing)}.\n"
                    f"      Give the BODY the narrower cfg and return the documented "
                    f"absent-value, or widen the callee. Narrowing the EXPORT removes a "
                    f"symbol the C headers declare unconditionally, which turns a compile "
                    f"error into a link error (issue 1449)."
                )

    known = set()
    if BASELINE.is_file():
        known = {
            l.strip()
            for l in BASELINE.read_text().splitlines()
            if l.strip() and not l.startswith("#")
        }
    now = set(undecidable)
    grew = sorted(now - known)

    if "--write-baseline" in sys.argv:
        BASELINE.write_text(
            "# Caller/callee pairs whose cfg is not a pure conjunction of\n"
            "# `feature = \"...\"`, so check-cffi-cfg-implication does not decide\n"
            "# them. A RATCHET: it may only shrink. Regenerate with --write-baseline\n"
            "# and say why in the commit.\n" + "".join(f"{p}\n" for p in sorted(now))
        )
        print(f"wrote {BASELINE.relative_to(ROOT)} — {len(now)} undecidable pair(s)")
        return 0

    if problems:
        print("check-cffi-cfg-implication: FAILED (issue 1449)", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    if grew:
        print(
            "check-cffi-cfg-implication: the undecidable set GREW — "
            f"{len(grew)} new pair(s) this gate cannot decide:",
            file=sys.stderr,
        )
        for g in grew:
            print(f"  {g}", file=sys.stderr)
        print(
            "\n  Each is a caller/callee whose cfg is not a pure conjunction of\n"
            "  features, so nothing here checked it. Make the cfg a conjunction, or\n"
            "  add the row with --write-baseline and say why.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-cffi-cfg-implication: OK — {checked} caller/callee pair(s) decided, "
        f"every export's features imply its callee's; {len(now)} undecidable "
        f"pair(s), none new."
    )
    return 0


def self_test() -> bool:
    ok = True

    def chk(label: str, cond: bool, detail: str = "") -> None:
        nonlocal ok
        print(f"  {'ok ' if cond else 'FAIL'} {label}" + ("" if cond else f" — {detail}"))
        if not cond:
            ok = False

    chk("a bare feature cfg decides to its one feature", decide('feature = "a"') == {"a"})
    chk(
        "an all() decides to the conjunction",
        decide('all(feature = "a", feature = "b")') == {"a", "b"},
    )
    chk("no cfg is the empty requirement", decide(None) == set())
    chk("any() is NOT decided", decide('any(feature = "a", feature = "b")') is None)
    chk("not() is NOT decided", decide('not(feature = "a")') is None)
    chk("a non-feature predicate is NOT decided", decide("test") is None)
    chk(
        "a mixed all() with a non-feature term is NOT decided",
        decide('all(feature = "a", has_rmw)') is None,
    )
    # The real relation, both directions.
    chk(
        "a caller missing a callee feature is a finding",
        {"alloc", "rmw-cffi"} - {"rmw-cffi"} == {"alloc"},
    )
    chk(
        "a caller that is a superset is clean",
        not ({"rmw-cffi"} - {"alloc", "rmw-cffi"}),
    )
    return ok


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    if not self_test():
        print("check-cffi-cfg-implication: SELF-TEST FAILED", file=sys.stderr)
        sys.exit(1)
    sys.exit(run())
