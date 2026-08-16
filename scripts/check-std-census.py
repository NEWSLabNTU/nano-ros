#!/usr/bin/env python3
"""phase-359 W0 — the `std` census, and a ratchet that only turns one way.

## Why a census at all

The campaign to drop `std` from the core crates is a migration, not a patch:
242 `cfg` mentions of the `std` feature and 421 `std::` paths over nine crates,
after excluding two crates for cause (below). Two earlier figures were wrong and
are superseded: the "~190" from planning was a hand-grep, and W0's own first
"181" came from a regex that could not see `cfg(all(feature = "std", ...))`. Without a committed baseline "did that work item land?" is
unanswerable, and — the failure mode that actually matters — nothing stops a
new `std::` site appearing in a crate someone already finished.

So this counts, and it FAILS when a count goes UP. Going down is the point;
going down means updating the baseline below, which makes progress visible in
the diff rather than asserted in a commit message.

## What is counted, and why two metrics

* `cfg` — lines carrying `cfg(feature = "std")` / `cfg(not(feature = "std"))`
  (including `cfg_attr` and inner `#![cfg(...)]`). This is the SHAPE of the
  split: the branches a reader has to hold in their head.
* `path` — occurrences of a `std::` path in live code. This is the DEPENDENCY
  itself; it must reach zero for the crate to compile without `std`.

They move independently. W2 (collapsing duplicated fields) deletes `cfg`
branches without touching `path` counts; W4/W5 (routing time and threads
through the platform seam) delete `path`s. Tracking one would hide the other.

## `#[cfg(test)]` code is excluded, and that is a CORRECTION not a win

Host unit tests link `std` even in a `no_std` crate, so their `std::` use can
never block a target build. Counting it made the ruler answer the wrong
question: `nros-node` read 309 paths, of which **209 were in its `#[cfg(test)]`
module** — two thirds of the number was code that does not ship. W5's premise
("29 `std::thread` sites to migrate") came from that inflation; 15 of those 29
were `thread::sleep` in tests.

Excluding them LOWERS the counts without anything being fixed. The drop is
recorded as a metric correction in the phase doc, not as progress.

## Comments are excluded, and that is not fussiness

This file's own sibling commits added doc comments that NAME `std::sync::Condvar`
and `std::time::Instant` while REMOVING uses of them. Counting comment text
would have scored that as a regression and taught everyone to ignore the gate.
Line comments and the doc-comment forms are stripped before matching; block
comments are rare in this tree and handled naively (a `/*` line is skipped).

## Scope

`packages/core/*` and `packages/api/*` — the crates the campaign covers. Board
and platform crates are out of scope: several are legitimately std-hosted
(`nros-board-linux`, and NuttX via `nros-board-nuttx`), and deciding their fate
is phase-359 W7, not this gate's business.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SCOPE = ["packages/core", "packages/api"]

# Out of scope, for reasons that are properties of the crate and not judgement
# calls. Both were caught by measuring rather than by the guess this baseline
# started as:
#
#   nros-macros            `proc-macro = true`. It runs on the HOST at compile
#                          time and is always std; its `std::` occurrences are
#                          TOKENS IT EMITS for generated code, not a dependency
#                          of anything embedded. Counting them would make the
#                          ruler lie in both directions.
#   nros-orchestration-ir  says so itself: "host code (serde + thiserror); it
#                          carries no runtime/`no_std`". A schema crate for
#                          `system.toml`, consumed by the CLI.
EXCLUDE = {"nros-macros", "nros-orchestration-ir"}

# Any `feature = "std"` appearing inside a cfg attribute, in ANY nesting.
#
# The first version of this anchored on `cfg(` / `cfg(not(` immediately followed
# by the feature, which silently missed every `cfg(all(feature = "std", ...))` —
# 26 of them in `spin.rs` alone. It was caught by W2: four cfg lines were
# deleted and the gate reported no movement. A ruler that cannot see the most
# common form of the thing it measures is worse than no ruler, because it reads
# as progress.
CFG_FEATURE_RE = re.compile(r'feature\s*=\s*"std"')
PATH_RE = re.compile(r'\bstd::')

# phase-359 baseline. W0 measured it; the corrected cfg metric re-measured
# everything; then `nros-node` came down per work item: W2 139->131 / 346->342,
# W3 131->127 / 342->321, W4 127->112 / 321->309, W6 309->285 (pure re-export
# spellings). Excluding `#[cfg(test)]` code then re-based every number — a
# metric correction, not progress; see the module docstring. W8 then re-gated
# `nros`'s node_metadata from `std` to `alloc`: cfg 64 -> 25; then
# `core::error::Error` made `nros-core`'s Error impls unconditional (cfg 7 -> 5,
# path 5 -> 2), and `nros-params`'s ParameterVariant impls (cfg 13 -> 7,
# path 8 -> 1).
# Lower these as work items land; the gate rejects any increase.
BASELINE = {
    # phase-361 W8.e: +1, a `compile_error!` guard. `metadata-mode` used to
    # ENABLE `std`; it now REQUIRES it and says so. A guard must NAME the
    # feature it checks, so making an implicit enable explicit costs one
    # counted site and removes one implicit enable. Same shape as nros-node.
    # phase-359 W10 follow-up: +1 on top of that campaign's own reduction (13),
    # the `env` `compile_error!` guard. `env` used to ENABLE `std`
    # (`env = ["std"]`), which is the implicit-flavour shape this campaign
    # removes and clause (a) forbids; it now REQUIRES it. The guard must NAME
    # the feature it checks, so the count goes up by one while an implicit
    # enable goes away. Same trade phase-361 W8.e made for `metadata-mode`
    # directly above.
    #
    # Then 14 -> 12, same day, different session: `init` and the `NROS_RMW`
    # read moved off `std` onto `env`, and `ExecutorNodeRuntime::spin`/`halt`
    # lost a gate that described a convention rather than a requirement.
    "nros": {"cfg": 12, "path": 16},
    "nros-c": {"cfg": 13, "path": 8},
    # phase-361 W2.a: 5 -> 3. The heap gate is `cfg(feature = "alloc")` alone,
    # `std` reaching it through `std = ["alloc", …]` in the manifest; the two
    # `any(alloc, std)` spellings are gone. Branches this campaign no longer
    # has to unwind.
    "nros-core": {"cfg": 3, "path": 2},
    "nros-cpp": {"cfg": 10, "path": 21},
    "nros-log": {"cfg": 1, "path": 0},
    # phase-361 W8.e: +1, the `signal-fd-wake` `compile_error!` guard — the
    # feature used to list `"std"` and now requires it by name.
    # Two changes met here and both are counted. `c3a16a529` (#607) raised
    # this to 108/76 by splitting the env cache on
    # `all(feature = "std", test)` / `not(test)`; phase-359 W10 then removed
    # the OS-priority pool's, the signalfd forwarder's and the condvar path's
    # `std` — so the measured figure after both is 91/40, not either side's
    # number. Set from the tree rather than from arithmetic on the two diffs.
    # phase-359 W10 follow-up: 86 -> 87, the `env` `compile_error!` guard.
    # W10 made the process environment a capability but wrote it
    # `env = ["std"]`, which GRANTS the standard library instead of requiring
    # it — the implicit flavour this campaign exists to remove, and a clause
    # (a) violation that sat red on main. Requiring it costs one counted site
    # and removes one silent enable.
    #
    # Then 87 -> 85: `Executor`'s halt/wake API was ONE `std`-gated impl block
    # and only its wall-clock spin loops need `std`, so it split three ways and
    # `halt_flag` joined `wake_flag` on `alloc`.
    "nros-node": {"cfg": 85, "path": 40},
    "nros-params": {"cfg": 7, "path": 1},
    "nros-rmw": {"cfg": 1, "path": 0},
    "nros-serdes": {"cfg": 1, "path": 0},
}


def is_test_gate(stripped: str) -> bool:
    """A `#[cfg(test)]` / `#[cfg(all(test, ...))]` attribute."""
    return stripped.startswith("#[cfg(") and re.search(r"\btest\b", stripped) is not None


def strip_comments(line: str) -> str:
    """Drop comment text so a doc comment naming `std::` is not a std USE."""
    s = line.strip()
    if s.startswith("//") or s.startswith("/*") or s.startswith("*"):
        return ""
    # Trailing `// …` on a code line. Naive on `"http://"`-style literals, which
    # do not occur in these crates; revisit if that changes.
    return line.split("//", 1)[0]


def census():
    out = {}
    for scope in SCOPE:
        root = REPO / scope
        if not root.is_dir():
            continue
        for crate_dir in sorted(root.iterdir()):
            src = crate_dir / "src"
            if not src.is_dir():
                continue
            cfg = path = 0
            for rs in sorted(src.rglob("*.rs")):
                # Generated bindings are not hand-written std use.
                if rs.name == "generated.rs":
                    continue
                # `executor/tests.rs` is declared `#[cfg(all(test, …))] mod tests;`
                # — the whole FILE is host-test code, and the gate lives in the
                # parent module where this per-file scan cannot see it.
                if rs.name == "tests.rs":
                    continue
                # Skip `#[cfg(test)] mod … { … }` bodies by brace depth, and
                # skip whole files that are themselves test modules.
                test_depth = None
                depth = 0
                pending_test = False
                for line in rs.read_text(errors="replace").splitlines():
                    stripped = line.strip()
                    if test_depth is None and is_test_gate(stripped):
                        pending_test = True
                    code = strip_comments(line)
                    opens = code.count("{")
                    closes = code.count("}")
                    if pending_test and opens:
                        test_depth = depth
                        pending_test = False
                    elif pending_test and stripped.endswith(";"):
                        pending_test = False  # `#[cfg(test)] mod tests;`
                    if test_depth is None and code:
                        if "cfg" in code:
                            cfg += len(CFG_FEATURE_RE.findall(code))
                        path += len(PATH_RE.findall(code))
                    depth += opens - closes
                    if test_depth is not None and depth <= test_depth:
                        test_depth = None
            if (cfg or path) and crate_dir.name not in EXCLUDE:
                out[crate_dir.name] = {"cfg": cfg, "path": path}
    return out


def main():
    now = census()
    show = "--show" in sys.argv

    width = max([len(k) for k in now] + [len("crate")])
    print(f"{'crate':<{width}}  {'cfg':>5} {'path':>5}   baseline")
    regressions, improvements, unknown = [], [], []
    for name in sorted(set(now) | set(BASELINE)):
        cur = now.get(name, {"cfg": 0, "path": 0})
        base = BASELINE.get(name)
        if base is None:
            unknown.append(name)
            note = "NEW — not in baseline"
        else:
            note = f"cfg {base['cfg']}, path {base['path']}"
            for metric in ("cfg", "path"):
                if cur[metric] > base[metric]:
                    regressions.append((name, metric, base[metric], cur[metric]))
                elif cur[metric] < base[metric]:
                    improvements.append((name, metric, base[metric], cur[metric]))
        print(f"{name:<{width}}  {cur['cfg']:>5} {cur['path']:>5}   {note}")

    total_cfg = sum(v["cfg"] for v in now.values())
    total_path = sum(v["path"] for v in now.values())
    print(f"\ntotal: {total_cfg} cfg site(s), {total_path} std:: path(s)")

    if show:
        print("\nBASELINE = {")
        for name in sorted(now):
            v = now[name]
            print(f'    "{name}": {{"cfg": {v["cfg"]}, "path": {v["path"]}}},')
        print("}")
        return 0

    rc = 0
    if unknown:
        print(
            f"\n[FAIL] {len(unknown)} crate(s) carry `std` and are not in the baseline: "
            + ", ".join(unknown),
            file=sys.stderr,
        )
        print(
            "  A new crate in scope must be entered deliberately, not absorbed silently.",
            file=sys.stderr,
        )
        rc = 1
    if regressions:
        print(f"\n[FAIL] {len(regressions)} count(s) went UP:", file=sys.stderr)
        for name, metric, was, now_ in regressions:
            print(f"    {name}: {metric} {was} -> {now_}", file=sys.stderr)
        print(
            "\n  phase-359 is removing these, so an increase is a step backwards.\n"
            "  If the new site is genuinely required, say so in the commit AND raise\n"
            "  the baseline in scripts/check-std-census.py — deliberately, in the diff.",
            file=sys.stderr,
        )
        rc = 1
    if improvements:
        print(f"\n{len(improvements)} count(s) went DOWN — update the baseline:")
        for name, metric, was, now_ in improvements:
            print(f"    {name}: {metric} {was} -> {now_}")
        print("  Run `scripts/check-std-census.py --show` for a paste-ready block.")
        rc = 1
    if rc == 0:
        print("\nstd census: OK (no crate moved)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
