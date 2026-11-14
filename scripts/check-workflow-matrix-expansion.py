#!/usr/bin/env python3
"""Every job's `strategy.matrix` expands to the jobs it says, for every input.

GitHub expands a matrix in a fixed order: the product of the BASE axes (every
key except `include`/`exclude`), then `exclude` removes the combinations an
entry matches, then `include` extends a combination whose original values it
does not overwrite, or appends itself as a new one. Two outcomes stop the job
before any runner is asked, and neither is visible to a local gate that only
reads the YAML's shape:

  - an `exclude` entry naming a key that is not a base axis. On an
    include-only matrix there are NO base axes, so every `exclude` key is one
    of these: the run fails with no job at all;
  - a matrix that expands to zero combinations ("Matrix vector does not
    contain any values");
  - and one that runs MORE than it says: an `include` entry that matches no
    combination left after `exclude` is appended as a job of its own, so a
    per-value `include` (`{board: zephyr, image: ...}`) brings back the very
    combination an `exclude` removed. The first fix for #3865 had this shape.

That is what `merge-runtime.yml` shipped in #3865: an include-only matrix with
an expression `exclude`, green on every local gate and on the queue (the
workflow is dispatch-only, so nothing ran it), and on main no hosted shard
expanded for either lane (run 43643673734).

So this gate expands every matrix itself, once per combination of the
workflow's statically known inputs: each `workflow_dispatch`/`workflow_call`
input with `type: choice` (its options) or `type: boolean`, crossed with each
event in `on:`. `${{ }}` expressions are evaluated by a small evaluator over
literals, `inputs.*`, `github.event_name`, `==`/`!=`/`!`/`&&`/`||` and
parentheses. An expression reading anything else (`vars.*`, `fromJSON`, a
`needs` output) cannot be decided here: that matrix is reported NOT VERIFIED
by name, never passed silently.

Scope is DERIVED: every workflow under `.github/workflows/`. The self-test runs
first on every call; its include-only/exclude case is #3865's exact shape.

    check-workflow-matrix-expansion.py [--selftest]
"""

from __future__ import annotations

import glob
import itertools
import os
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXPR = re.compile(r"\$\{\{(.*?)\}\}", re.S)


class Undecidable(Exception):
    """An expression reads a context this gate cannot know."""


# ---------------------------------------------------------------- expressions

TOKEN = re.compile(
    r"\s*(?:(?P<str>'(?:[^']|'')*')|(?P<num>-?\d+(?:\.\d+)?)"
    r"|(?P<op>==|!=|&&|\|\||!|\(|\))|(?P<ident>[A-Za-z_][A-Za-z0-9_.-]*))"
)


def tokens(src: str):
    pos, out = 0, []
    src = src.strip()
    while pos < len(src):
        m = TOKEN.match(src, pos)
        if not m or m.end() == pos:
            raise Undecidable(f"cannot parse {src[pos:]!r}")
        pos = m.end()
        kind = m.lastgroup or ""
        out.append((kind, m.group(kind) or ""))
    return out


def truthy(v) -> bool:
    return v not in (None, False, 0, "")


def loose_eq(a, b) -> bool:
    # GitHub compares strings case-insensitively; mixed types coerce to number.
    if isinstance(a, str) and isinstance(b, str):
        return a.lower() == b.lower()
    if isinstance(a, bool) or isinstance(b, bool) or isinstance(a, (int, float)) or isinstance(b, (int, float)):
        def num(x):
            if x is None:
                return 0
            if isinstance(x, bool):
                return int(x)
            if isinstance(x, (int, float)):
                return x
            try:
                return float(x) if x.strip() else 0
            except ValueError:
                return float("nan")
        return num(a) == num(b)
    return a == b


def evaluate(src: str, ctx: dict):
    toks = tokens(src)
    i = 0

    def peek() -> tuple[str, str]:
        return toks[i] if i < len(toks) else ("", "")

    def take():
        nonlocal i
        t = toks[i]
        i += 1
        return t

    def primary():
        kind, val = peek()
        if not kind:
            raise Undecidable(f"truncated expression {src!r}")
        take()
        if kind == "str":
            return val[1:-1].replace("''", "'")
        if kind == "num":
            return float(val) if "." in val else int(val)
        if kind == "op" and val == "(":
            v = orexp()
            if take() != ("op", ")"):
                raise Undecidable(f"unbalanced parentheses in {src!r}")
            return v
        if kind == "op" and val == "!":
            return not truthy(primary())
        if kind == "ident":
            if peek() == ("op", "("):
                raise Undecidable(f"function call {val}() in {src!r}")
            low = val.lower()
            if low in ("true", "false"):
                return low == "true"
            if low == "null":
                return None
            if val in ctx:
                return ctx[val]
            if val.startswith("inputs."):
                return ""  # an input the event does not carry reads empty
            raise Undecidable(f"`{val}` is not known statically")
        raise Undecidable(f"unexpected {val!r} in {src!r}")

    def cmp():
        left = primary()
        while peek() in (("op", "=="), ("op", "!=")):
            op = take()[1]
            right = primary()
            left = loose_eq(left, right) if op == "==" else not loose_eq(left, right)
        return left

    def andexp():
        left = cmp()
        while peek() == ("op", "&&"):
            take()
            right = cmp()
            left = right if truthy(left) else left
        return left

    def orexp():
        left = andexp()
        while peek() == ("op", "||"):
            take()
            right = andexp()
            left = left if truthy(left) else right
        return left

    v = orexp()
    if i != len(toks):
        raise Undecidable(f"trailing tokens in {src!r}")
    return v


def substitute(value, ctx):
    """Evaluate every `${{ }}` inside a matrix value."""
    if isinstance(value, str):
        whole = EXPR.fullmatch(value.strip())
        if whole:
            return evaluate(whole.group(1), ctx)
        return EXPR.sub(lambda m: str(evaluate(m.group(1), ctx)), value)
    if isinstance(value, list):
        return [substitute(v, ctx) for v in value]
    if isinstance(value, dict):
        return {k: substitute(v, ctx) for k, v in value.items()}
    return value


# ------------------------------------------------------------------- matrices

def expand(matrix) -> tuple[list[dict], list[str]]:
    """(combinations, errors) by GitHub's order: base, exclude, include."""
    if not isinstance(matrix, dict):
        raise Undecidable("the matrix is one expression")
    errors = []
    excluded: list[dict] = []
    base = {k: v for k, v in matrix.items() if k not in ("include", "exclude")}
    for k, v in base.items():
        if not isinstance(v, list):
            raise Undecidable(f"axis `{k}` is not a list")
    combos = [dict(zip(base, vals)) for vals in itertools.product(*base.values())] if base else []
    for entry in matrix.get("exclude") or []:
        if not isinstance(entry, dict):
            raise Undecidable("an exclude entry is not a mapping")
        for k in entry:
            if k not in base:
                errors.append(
                    f"exclude key `{k}` is not a base axis (base axes: "
                    f"{sorted(base) or 'none -- the matrix is include-only'}); "
                    "GitHub applies exclude BEFORE include, so this fails the run "
                    "with no job"
                )
        excluded += [c for c in combos if all(loose_eq(c.get(k), v) for k, v in entry.items())]
        combos = [c for c in combos if not all(loose_eq(c.get(k), v) for k, v in entry.items())]
    # Only the base combinations are extended; an entry appended by an earlier
    # include is a job of its own and is never extended by a later one.
    original = [dict(c) for c in combos]
    extended = combos
    appended = []
    for entry in matrix.get("include") or []:
        if not isinstance(entry, dict):
            raise Undecidable("an include entry is not a mapping")
        matched = False
        for c, orig in zip(extended, original):
            if all(not (k in orig and not loose_eq(orig[k], v)) for k, v in entry.items()):
                c.update(entry)
                matched = True
        if not matched:
            keyed = {k: v for k, v in entry.items() if k in base}
            if keyed and any(all(loose_eq(x.get(k), v) for k, v in keyed.items()) for x in excluded):
                errors.append(
                    f"include entry {entry} matches no combination left after exclude, so "
                    "GitHub APPENDS it as a job of its own: the excluded combination "
                    f"{keyed} runs anyway"
                )
            appended.append(dict(entry))
    combos = extended + appended
    if not combos and not errors:
        errors.append("the matrix expands to ZERO jobs")
    return combos, errors


def triggers(doc) -> dict:
    on = doc.get("on", doc.get(True))
    if isinstance(on, str):
        return {on: {}}
    if isinstance(on, list):
        return {e: {} for e in on}
    return {k: (v or {}) for k, v in (on or {}).items()}


def input_space(on: dict) -> dict:
    """input name -> list of values it can take, for choice/boolean inputs."""
    space: dict[str, list] = {}
    for event in ("workflow_dispatch", "workflow_call"):
        for name, spec in ((on.get(event) or {}).get("inputs") or {}).items():
            spec = spec or {}
            if spec.get("type") == "choice" and spec.get("options"):
                space.setdefault(name, list(spec["options"]))
            elif spec.get("type") == "boolean":
                space.setdefault(name, [True, False])
    return space


def contexts(on: dict):
    space = input_space(on)
    names = list(space)
    for event in on or {"push": {}}:
        carries = event in ("workflow_dispatch", "workflow_call")
        for vals in itertools.product(*(space[n] for n in names)) if carries and names else [()]:
            ctx = {"github.event_name": event}
            if carries:
                ctx.update({f"inputs.{n}": v for n, v in zip(names, vals)})
            yield ctx


def label(ctx: dict) -> str:
    return ", ".join(f"{k}={v}" for k, v in ctx.items())


def check_doc(doc, where: str) -> tuple[list[str], list[str], int]:
    """(failures, not_verified, matrices checked) for one workflow."""
    fails, unknown, checked = [], [], 0
    on = triggers(doc)
    for job, body in (doc.get("jobs") or {}).items():
        matrix = ((body or {}).get("strategy") or {}).get("matrix")
        if matrix is None:
            continue
        checked += 1
        for ctx in contexts(on):
            try:
                combos, errors = expand(substitute(matrix, ctx))
            except Undecidable as e:
                unknown.append(f"{where} job `{job}` ({label(ctx)}): {e}")
                break
            for err in errors:
                fails.append(f"{where} job `{job}` ({label(ctx)}): {err}")
    return fails, unknown, checked


# ------------------------------------------------------------------ self-test

SHAPE_3865 = """
on:
  workflow_dispatch:
    inputs:
      lane: {type: choice, options: [merge, tier1]}
jobs:
  j:
    strategy:
      matrix:
        include:
          - {board: threadx_linux, image: a}
          - {board: zephyr, image: b}
        exclude:
          - board: ${{ inputs.lane == 'tier1' && 'none' || 'zephyr' }}
"""

# The first fix for #3865: board became a base axis but kept a per-board
# `include`, which re-adds the zephyr shard the merge lane excludes.
READDED = SHAPE_3865.replace("      matrix:\n", "      matrix:\n        board: [threadx_linux, zephyr]\n")

ALL_EXCLUDED = """
on: {workflow_dispatch: {inputs: {lane: {type: choice, options: [merge, tier1]}}}}
jobs:
  j:
    strategy:
      matrix:
        board: [zephyr]
        exclude:
          - board: ${{ inputs.lane == 'merge' && 'zephyr' || 'none' }}
"""


# The shipped fix: base axis, no include (the image is chosen elsewhere).
SHIPPED = """
on: {workflow_dispatch: {inputs: {lane: {type: choice, options: [merge, tier1]}}}}
jobs:
  j:
    strategy:
      matrix:
        board: [threadx_linux, zephyr]
        exclude:
          - board: ${{ inputs.lane == 'tier1' && 'none' || 'zephyr' }}
"""

INCLUDE_ONLY = """
on: push
jobs:
  j:
    strategy:
      matrix:
        include: [{a: 1}, {a: 2}]
"""

VARS = """
on: push
jobs:
  j:
    strategy:
      matrix:
        os: ${{ fromJSON(vars.OSES) }}
"""


def selftest() -> int:
    cases = [
        ("#3865: include-only matrix + exclude", SHAPE_3865, 1, 0),
        ("base axis + per-board include re-adds the excluded shard", READDED, 1, 0),
        ("the shipped fix: base axis, no include", SHIPPED, 0, 0),
        ("an exclude that empties one lane", ALL_EXCLUDED, 1, 0),
        ("include-only, no exclude, expands", INCLUDE_ONLY, 0, 0),
        ("a vars/fromJSON matrix is NOT VERIFIED, not passed", VARS, 0, 1),
    ]
    bad = 0
    for name, src, want_fail, want_unknown in cases:
        fails, unknown, _ = check_doc(yaml.safe_load(src), "selftest")
        ok = (len(fails) >= 1) == bool(want_fail) and (len(unknown) >= 1) == bool(want_unknown)
        if not ok:
            bad += 1
            print(f"  FAILED: {name}: fails={fails} unknown={unknown}", file=sys.stderr)
    # The expansion itself, against GitHub's documented example.
    combos, errs = expand({
        "fruit": ["apple", "pear"], "animal": ["cat", "dog"],
        "include": [{"color": "green"}, {"color": "pink", "animal": "cat"},
                    {"fruit": "apple", "shape": "circle"}, {"fruit": "banana"},
                    {"fruit": "banana", "animal": "cat"}],
    })
    want = [
        {"fruit": "apple", "animal": "cat", "color": "pink", "shape": "circle"},
        {"fruit": "apple", "animal": "dog", "color": "green", "shape": "circle"},
        {"fruit": "pear", "animal": "cat", "color": "pink"},
        {"fruit": "pear", "animal": "dog", "color": "green"},
        {"fruit": "banana"},
        {"fruit": "banana", "animal": "cat"},
    ]
    if errs or combos != want:
        bad += 1
        print(f"  FAILED: GitHub's include example expanded to {combos}", file=sys.stderr)
    total = len(cases) + 1
    if bad:
        print(f"check-workflow-matrix-expansion: selftest FAILED ({bad} of {total} case(s))",
              file=sys.stderr)
        return 1
    print(f"check-workflow-matrix-expansion: selftest OK ({total} cases)")
    return 0


def main(argv) -> int:
    rc = selftest()
    if rc or "--selftest" in argv:
        return rc
    paths = sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.y*ml")))
    if not paths:
        print("check-workflow-matrix-expansion: no workflows found -- the gate would pass "
              "vacuously", file=sys.stderr)
        return 1
    fails, unknown, checked = [], [], 0
    for p in paths:
        with open(p) as f:
            doc = yaml.safe_load(f) or {}
        f_, u_, c_ = check_doc(doc, os.path.relpath(p, ROOT))
        fails += f_
        unknown += u_
        checked += c_
    if checked == 0:
        print("check-workflow-matrix-expansion: no job with a strategy.matrix found -- the "
              "parse is broken", file=sys.stderr)
        return 1
    for u in unknown:
        print(f"check-workflow-matrix-expansion: NOT VERIFIED: {u}")
    if fails:
        print("check-workflow-matrix-expansion: a job's matrix does not expand as written:", file=sys.stderr)
        for f_ in fails:
            print(f"  {f_}", file=sys.stderr)
        print("  Make every key an `exclude` names a base axis (`board: [a, b]`), and choose "
              "per-value settings with an expression on `matrix.<axis>` rather than an "
              "`include` entry an exclude can strand.", file=sys.stderr)
        return 1
    print(f"check-workflow-matrix-expansion: OK -- {checked} matrix job(s) across "
          f"{len(paths)} workflow(s), {len(unknown)} not statically decidable (named above).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
