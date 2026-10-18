#!/usr/bin/env python3
"""issue 1903 — every bespoke prebuilt artifact a gated sweep claims has a lane producer.

THE RULE
========

`nros_tests::fixtures::require_prebuilt_artifact(path, remedy)` is the absence
funnel for a build-stage artifact that NO manifest row names. Because no row
names it, the lane resolver cannot attribute the path to a coordinate, so it is
never out of lane: every gated run that reaches the call claims the artifact.
The only thing that can make that claim true is a fixture lane that BUILDS it,
and the only statement of the producer the call site makes is its `remedy`.

So, for every such call a gated sweep (`just test-all`, which `just test <scope>`
and every `ci` tier run) can reach, the remedy must be a `just` recipe in the
closure of `build-test-fixtures` — the lane build. A remedy outside it names a
command the lane never runs: issue 1903's `cyclone_slirp_pair` resolved its
guest with `just cyclonedds build-rmw`, which only `check rmw-cyclonedds`
reached, so `just test linux` claimed the test and failed it as a broken
fixture promise. The fix for such a site is a `[[compile_check_fixture]]` row
(issue 1656's `borrowed_e2e`, 1903's `cyclone_slirp_guest`) resolved with
`require_compile_check*`, which every lane builds — not a wider remedy.

WHAT IS DERIVED, AND FROM WHERE
===============================

Nothing here is a list someone keeps:

* The CALL SITES are harvested from the tracked `nros-tests` sources. A helper
  that FORWARDS its own `remedy` parameter into a funnel becomes a funnel
  itself (`ros_env::require_edition_example`), and its callers are harvested in
  turn — a second spelling of the funnel is the #282/#326 shape this follows.
* WHO REACHES a site: a call in `tests/<name>.rs` belongs to test binary
  `<name>`; a call in `src/` belongs to every binary that reaches its enclosing
  function, found by name through the same sources. A library function nothing
  names is claimed by EVERY binary — unknown fails closed.
* What a gated sweep DOES NOT CLAIM: the `not binary(..)` exclusions in the
  `test-all` recipe (the opt-in ros_editions docker harness, the licensed FVP,
  dormant esp32). A site all of whose binaries are excluded there is owned by
  its opt-in lane, which builds before it runs.
* The LANE BUILD's closure: `just --dump --dump-format json` from
  `build-test-fixtures`, through every dependency and every literal `just`
  call in a recipe body. A body call with a VARIABLE module
  (`just "$platform" build-fixtures`, the per-module fan-out) reaches that
  recipe in every module that has it.

THE ROW HALF
============

The fix for an unproduced site is a `[[compile_check_fixture]]` row, so the row
resolvers (`require_compile_check`, `require_compile_check_bin`,
`require_compile_verdict`) get the same question: every id a test names must be
a row `compile-check-fixtures.sh` builds (a manifest row, or one of that
script's PX4 ids), and that script's recipe, `build-compile-check-fixtures`,
must be in the lane build. Deleting `cyclone_slirp_guest`'s row while the test
still names it is refused here, not at run time.

Usage: check-prebuilt-artifact-producers.py [--self-test] [--list]
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import just_closure  # noqa: E402  issue 1959 — the one recipe-closure walk

ROOT = Path(__file__).resolve().parents[1]
TESTS = "packages/testing/nros-tests"
SEED_FUNNEL = ("require_prebuilt_artifact", 1)  # (name, remedy arg index)
LANE_ROOT = "build-test-fixtures"
ROW_FUNNELS = ("require_compile_check", "require_compile_check_bin", "require_compile_verdict")
ROW_BUILDER_RECIPE = "build-compile-check-fixtures"
MANIFEST = "examples/fixtures.toml"
ROW_SCRIPT = "scripts/build/compile-check-fixtures.sh"
SWEEP_RECIPE = "test-all"

IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
FN_DECL = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>{;]*>)?\s*\(")
STRING = re.compile(r'"((?:[^"\\]|\\.)*)"')


# --------------------------------------------------------------------------
# Rust source scanning (deliberately small: calls, args, enclosing fn).
# --------------------------------------------------------------------------


def _skip_string(text: str, i: int) -> int:
    """`text[i]` is `"`; return the index just past the closing quote."""
    i += 1
    while i < len(text):
        if text[i] == "\\":
            i += 2
            continue
        if text[i] == '"':
            return i + 1
        i += 1
    return i


def call_args(text: str, open_paren: int) -> list[str] | None:
    """Top-level comma-split arguments of the call whose `(` is at `open_paren`."""
    depth = 0
    args: list[str] = []
    cur_start = open_paren + 1
    i = open_paren
    while i < len(text):
        ch = text[i]
        if ch == '"':
            i = _skip_string(text, i)
            continue
        if ch == "/" and text.startswith("//", i):
            nl = text.find("\n", i)
            i = len(text) if nl < 0 else nl
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                tail = text[cur_start:i].strip()
                if tail:
                    args.append(tail)
                return args
        elif ch == "," and depth == 1:
            args.append(text[cur_start:i].strip())
            cur_start = i + 1
        i += 1
    return None


def _line_is_comment(text: str, pos: int) -> bool:
    ls = text.rfind("\n", 0, pos) + 1
    return text[ls:pos].lstrip().startswith("//")


def enclosing_fn(text: str, pos: int) -> tuple[str, list[str]] | None:
    """The nearest `fn NAME(params)` declared before `pos`, with param names."""
    last = None
    for m in FN_DECL.finditer(text, 0, pos):
        if not _line_is_comment(text, m.start()):
            last = m
    if last is None:
        return None
    params = call_args(text, last.end() - 1) or []
    names = []
    for p in params:
        p = re.sub(r"^\s*(?:mut\s+)?", "", p)
        names.append(p.split(":", 1)[0].strip())
    return last.group(1), names


def calls_of(text: str, name: str):
    """Yield (pos, args) for every non-comment, non-declaration call of `name`."""
    pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"\s*\(")
    for m in pat.finditer(text):
        if _line_is_comment(text, m.start()):
            continue
        before = text[max(0, m.start() - 8) : m.start()]
        if re.search(r"\bfn\s+$", before):
            continue
        args = call_args(text, m.end() - 1)
        if args is not None:
            yield m.start(), args


def binary_of(rel: str) -> str | None:
    """Test binary a `tests/` file belongs to; None for library code."""
    parts = rel.split("/")
    try:
        k = parts.index("tests")
    except ValueError:
        return None
    if "/".join(parts[:k]) != TESTS:
        return None
    rest = parts[k + 1 :]
    if len(rest) == 1 and rest[0].endswith(".rs"):
        return rest[0][:-3]
    if len(rest) >= 2:
        return rest[0]
    return None


# --------------------------------------------------------------------------
# The analysis.
# --------------------------------------------------------------------------


class Site:
    def __init__(self, rel, line, funnel, remedy_expr, remedy, fn_name):
        self.rel, self.line, self.funnel = rel, line, funnel
        self.remedy_expr, self.remedy, self.fn_name = remedy_expr, remedy, fn_name
        self.binaries: set[str] | None = None  # None = every binary

    def where(self):
        return f"{self.rel}:{self.line}"


def harvest(sources: dict[str, str]) -> list[Site]:
    """Every terminal funnel call site, following forwarding helpers."""
    funnels = [SEED_FUNNEL]
    seen = {SEED_FUNNEL[0]}
    sites: list[Site] = []
    while funnels:
        name, idx = funnels.pop()
        for rel, text in sources.items():
            for pos, args in calls_of(text, name):
                line = text[:pos].count("\n") + 1
                enc = enclosing_fn(text, pos)
                if len(args) <= idx:
                    sites.append(Site(rel, line, name, "", None, enc and enc[0]))
                    continue
                expr = args[idx].lstrip("&").strip()
                if enc and IDENT.match(expr) and expr in enc[1]:
                    # A forwarding helper: its OWN callers are the sites.
                    if enc[0] not in seen:
                        seen.add(enc[0])
                        funnels.append((enc[0], enc[1].index(expr)))
                    continue
                lit = STRING.search(expr)
                sites.append(
                    Site(rel, line, name, expr, lit.group(1) if lit else None,
                         enc and enc[0])
                )
    return sites


def reaching_binaries(sources, rel, fn_name, _seen=None) -> set[str] | None:
    """Binaries that reach `fn_name` in `rel`; None = cannot bound (every one)."""
    b = binary_of(rel)
    if b is not None:
        return {b}
    if not fn_name:
        return None
    _seen = _seen if _seen is not None else set()
    if fn_name in _seen:
        return set()
    _seen.add(fn_name)
    out: set[str] = set()
    found = False
    for orel, text in sources.items():
        for pos, _args in calls_of(text, fn_name):
            found = True
            enc = enclosing_fn(text, pos)
            sub = reaching_binaries(sources, orel, enc and enc[0], _seen)
            if sub is None:
                return None
            out |= sub
    return out if found else None


def sweep_exclusions(justfile_text: str) -> tuple[set[str], set[str]]:
    """(exact, substring) binary names `test-all` declares out of its scope."""
    m = re.search(rf"^{re.escape(SWEEP_RECIPE)}\b[^\n]*\n((?:[ \t]+[^\n]*\n|\n)*)",
                  justfile_text, re.M)
    if not m:
        raise SystemExit(f"check-prebuilt-artifact-producers: no `{SWEEP_RECIPE}` recipe in justfile")
    exact, sub = set(), set()
    for line in m.group(1).splitlines():
        s = line.strip()
        if s.startswith("#") or "env_exclude+=" not in s:
            continue
        for neg in re.finditer(r"not binary\((~?)([A-Za-z0-9_\-]+)\)", s):
            (sub if neg.group(1) else exact).add(neg.group(2))
    return exact, sub


def excluded(binary: str, exact: set[str], sub: set[str]) -> bool:
    return binary in exact or any(s in binary for s in sub)


# --- the lane build's closure, from just's own dump -------------------------

def lane_closure(dump) -> set[str]:
    return just_closure.closure(dump, LANE_ROOT)


def remedy_recipe(remedy: str, modules: set[str]) -> str | None:
    toks = remedy.split()
    if len(toks) < 2 or toks[0] != "just":
        return None
    a = toks[1]
    if "::" in a:
        return a
    if a in modules and len(toks) >= 3:
        return f"{a}::{toks[2]}"
    return a


def row_sites(sources) -> list[tuple[str, int, str | None, str]]:
    """(rel, line, id-or-None, funnel) for every row-resolver call."""
    out = []
    for name in ROW_FUNNELS:
        for rel, text in sources.items():
            for pos, args in calls_of(text, name):
                line = text[:pos].count("\n") + 1
                expr = args[0].lstrip("&").strip() if args else ""
                lit = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', expr)
                rid = lit.group(1) if lit else None
                if rid is None and IDENT.match(expr):
                    c = re.search(rf'\bconst\s+{re.escape(expr)}\s*:\s*&(?:\'static\s+)?str\s*=\s*"([^"]+)"', text)
                    rid = c.group(1) if c else None
                out.append((rel, line, rid, name))
    return out


def row_ids(script_text: str) -> set[str]:
    """Ids `compile-check-fixtures.sh` can build: its manifest rows, read through
    the SAME `fixtures-manifest.py list-compile-checks` the builder iterates, plus
    its PX4 ids (not manifest rows)."""
    out = subprocess.run(
        [sys.executable, "scripts/build/fixtures-manifest.py", "list-compile-checks"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"check-prebuilt-artifact-producers: list-compile-checks failed: {out.stderr[:300]}")
    ids = {ln.split("\x1f", 1)[0] for ln in out.stdout.splitlines() if ln}
    if not ids:
        raise SystemExit("check-prebuilt-artifact-producers: list-compile-checks listed no rows")
    m = re.search(r'^_cc_px4_ids="([^"]*)"', script_text, re.M)
    if m:
        ids |= set(m.group(1).split())
    return ids


# --------------------------------------------------------------------------


def tracked_sources() -> dict[str, str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", f"{TESTS}/**/*.rs"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0 or not out.stdout:
        raise SystemExit(f"check-prebuilt-artifact-producers: `git ls-files` failed: {out.stderr[:300]}")
    return {p: (ROOT / p).read_text(errors="replace") for p in out.stdout.split("\0") if p}


def just_dump() -> dict:
    out = subprocess.run(
        ["just", "--dump", "--dump-format", "json"], cwd=ROOT, capture_output=True, text=True
    )
    if out.returncode != 0:
        raise SystemExit(f"check-prebuilt-artifact-producers: `just --dump` failed: {out.stderr[:300]}")
    return json.loads(out.stdout)


def judge(sources, justfile_text, dump, rows: set[str]) -> tuple[list[str], list[str]]:
    sites = harvest(sources)
    exact, sub = sweep_exclusions(justfile_text)
    closure = lane_closure(dump)
    modules = set(dump.get("modules", {}))
    if LANE_ROOT not in closure:
        raise SystemExit(f"check-prebuilt-artifact-producers: no `{LANE_ROOT}` recipe")
    bad, report = [], []
    for s in sites:
        bins = reaching_binaries(sources, s.rel, s.fn_name)
        claimed = sorted(bins) if bins is not None else ["<every test binary>"]
        if bins is not None and bins and all(excluded(b, exact, sub) for b in bins):
            report.append(f"opt-in   {s.where()}  [{', '.join(claimed)}]  remedy: {s.remedy}")
            continue
        if s.remedy is None:
            bad.append(
                f"{s.where()}: `{s.funnel}` remedy `{s.remedy_expr}` is not a string literal, so "
                "nothing can say what produces the artifact. Pass the producing `just` recipe "
                "as a literal (or `format!(\"just …\")`)."
            )
            continue
        rec = remedy_recipe(s.remedy, modules)
        if rec is None:
            bad.append(
                f"{s.where()}: remedy `{s.remedy}` names no `just` recipe — a gated sweep "
                f"claims this artifact ({', '.join(claimed)}), so its producer must be a "
                f"recipe the lane build (`{LANE_ROOT}`) runs."
            )
            continue
        if rec not in closure:
            bad.append(
                f"{s.where()}: `{s.funnel}(.., \"{s.remedy}\")` — `{rec}` is not reached by "
                f"`just {LANE_ROOT}`, yet a gated sweep claims it ({', '.join(claimed)}: not "
                f"excluded by `{SWEEP_RECIPE}`). No fixture lane builds it, and the path is in "
                "no manifest row, so the run cannot lane-skip either (issue 1903). Give it a "
                "`[[compile_check_fixture]]` row and resolve it with `require_compile_check*` "
                "(issue 1656 / 1903), or call its recipe from the lane build."
            )
            continue
        report.append(f"lane     {s.where()}  [{', '.join(claimed)}]  `{rec}`")

    # The row half: a row's producer is `build-compile-check-fixtures`.
    if ROW_BUILDER_RECIPE not in closure:
        bad.append(
            f"`{ROW_BUILDER_RECIPE}` is not reached by `just {LANE_ROOT}`, so no fixture lane "
            "builds ANY `[[compile_check_fixture]]` row the tests resolve."
        )
    unresolved = 0
    for rel, line, rid, fn in row_sites(sources):
        if rid is None:
            unresolved += 1
            continue
        if rid not in rows:
            bad.append(
                f"{rel}:{line}: `{fn}(\"{rid}\")` names no `[[compile_check_fixture]]` row in "
                f"{MANIFEST} (nor a PX4 id of {ROW_SCRIPT}), so no lane builds what it resolves."
            )
            continue
        report.append(f"row      {rel}:{line}  `{rid}`")
    report.append(f"(row-resolver calls with a non-literal id, not checked: {unresolved})")
    return bad, report


def self_test() -> list[str]:
    errs = []
    src = {
        f"{TESTS}/tests/good.rs": 'fn t() { require_prebuilt_artifact(&p, "just m1 build-x").require("x"); }\n',
        f"{TESTS}/tests/bad.rs": 'fn t() { require_prebuilt_artifact(&p, "just m1 build-y"); }\n',
        f"{TESTS}/tests/optin_e2e.rs": 'fn t() { helper::fwd(p, &format!("just m1 nope {x}")); }\n',
        f"{TESTS}/src/helper.rs": (
            "/// require_prebuilt_artifact(&p, \"just never\") in prose is not a call\n"
            "pub fn fwd(bin: PathBuf, remedy: &str) -> PathBuf {\n"
            "    crate::fixtures::require_prebuilt_artifact(&bin, remedy).require(\"b\")\n}\n"
        ),
        f"{TESTS}/tests/lit.rs": "fn t() { require_prebuilt_artifact(&p, r); }\n",
        f"{TESTS}/tests/rows.rs": (
            'const ROW: &str = "known";\n'
            'fn t() { require_compile_check(ROW); require_compile_check_bin("gone", "b"); }\n'
        ),
    }
    jf = 'test-all verbose="":\n    env_exclude+=("not binary(~optin)")\n\nnext:\n'
    dump = {
        "recipes": {
            "build-test-fixtures": {"dependencies": [], "body": [
                ['just "$platform" build-fixtures'], ["just build-compile-check-fixtures"]]},
            "build-compile-check-fixtures": {"dependencies": [], "body": []},
        },
        "modules": {
            "m1": {"recipes": {
                "build-fixtures": {"dependencies": [{"recipe": "build-x"}], "body": []},
                "build-x": {"dependencies": [], "body": []},
                "build-y": {"dependencies": [], "body": []},
            }, "modules": {}},
        },
    }
    bad, report = judge(src, jf, dump, {"known"})
    got = sorted(b.split(":", 1)[0].rsplit("/", 1)[-1] for b in bad)
    if got != ["bad.rs", "lit.rs", "rows.rs"]:
        errs.append(f"expected bad.rs + lit.rs + rows.rs refused, got {got}: {bad}")
    if not any("`known`" in r and r.startswith("row") for r in report):
        errs.append(f"const-named row id not resolved: {report}")
    if not any("good.rs" in r and r.startswith("lane") for r in report):
        errs.append(f"good.rs not accepted as lane-built: {report}")
    if not any("optin_e2e.rs" in r and r.startswith("opt-in") for r in report):
        errs.append(f"forwarded opt-in site not attributed through the helper: {report}")
    return errs


def main(argv) -> int:
    errs = self_test()
    if errs:
        print("check-prebuilt-artifact-producers: SELF-TEST FAILED", file=sys.stderr)
        for e in errs:
            print(f"  {e}", file=sys.stderr)
        return 2
    if "--self-test" in argv:
        print("check-prebuilt-artifact-producers self-test: OK")
        return 0
    rows = row_ids((ROOT / ROW_SCRIPT).read_text())
    bad, report = judge(tracked_sources(), (ROOT / "justfile").read_text(), just_dump(), rows)
    if "--list" in argv:
        print("\n".join(report))
    if bad:
        print(f"check-prebuilt-artifact-producers: FAIL — {len(bad)} prebuilt artifact(s) a "
              "gated sweep claims and no fixture lane builds:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    n_art = sum(1 for r in report if r.startswith(("lane", "opt-in")))
    n_row = sum(1 for r in report if r.startswith("row"))
    print(f"check-prebuilt-artifact-producers: OK — {n_art} `require_prebuilt_artifact` "
          "site(s), each lane-built or owned by an opt-in lane `test-all` excludes; "
          f"{n_row} row-resolver id(s), each a row the lane builds")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
