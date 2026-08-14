#!/usr/bin/env python3
"""phase-359 W0 — the `std` census, and a ratchet that only turns one way.

## Why a census at all

The campaign to drop `std` from the core crates is a migration, not a patch:
181 `cfg(feature = "std")` sites and 425 `std::` paths, spread over nine
crates — measured, after two crates were excluded for cause (below); the "~190"
figure quoted while planning was a hand-grep and was wrong in both directions. Without a committed baseline "did that work item land?" is
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

CFG_RE = re.compile(r'cfg(?:_attr)?\s*\(\s*(?:not\s*\(\s*)?feature\s*=\s*"std"')
PATH_RE = re.compile(r'\bstd::')

# phase-359 W0 baseline, measured 2026-08-15 on 249277946.
# Lower these as work items land; the gate rejects any increase.
BASELINE = {
    "nros": {"cfg": 61, "path": 20},
    "nros-c": {"cfg": 11, "path": 9},
    "nros-core": {"cfg": 6, "path": 6},
    "nros-cpp": {"cfg": 4, "path": 26},
    "nros-log": {"cfg": 1, "path": 0},
    "nros-node": {"cfg": 85, "path": 346},
    "nros-params": {"cfg": 11, "path": 18},
    "nros-rmw": {"cfg": 1, "path": 0},
    "nros-serdes": {"cfg": 1, "path": 0},
}


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
                for line in rs.read_text(errors="replace").splitlines():
                    code = strip_comments(line)
                    if not code:
                        continue
                    cfg += len(CFG_RE.findall(code))
                    path += len(PATH_RE.findall(code))
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
