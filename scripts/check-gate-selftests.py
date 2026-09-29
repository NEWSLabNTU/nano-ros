#!/usr/bin/env python3
"""A gate must exercise its own failure path every time it runs — phase-395.

WHY THIS IS THE ONE OBLIGATION CI CAN ACTUALLY CARRY

Of the three things we ask of agent-submitted work, two are not mechanically
checkable and should not pretend to be: "separate what you measured from what
you reasoned" and "name the command behind a coverage claim" are conventions,
because CI can check that a section EXISTS and never that it is HONEST. A gate
that only checks the form is present reports compliance it never established,
which is the failure this whole family of checks exists to prevent.

This one is different. "The gate can fail" is a property of the gate, not a
claim about a person, so it can be enforced.

WHY 'RUNS ON THE NORMAL PATH' AND NOT MERELY 'EXISTS'

`check-board-tiers.py` says it best, in its own comment: a negative control
nobody runs decays into a comment. A selftest behind `--selftest` is run once,
by its author, on the day it is written; afterwards it is prose. Running it on
every invocation converts "someone once demonstrated a red" from a claim in a
commit message into something re-verified on every push — which is exactly the
difference between a report and a measurement.

That is also why the fix for a violation is never "add `--selftest` to the
justfile line". Two invocations of the same script is twice the cost and still
leaves the direct callers unprotected.

WHY A BASELINE RATCHET, NOT A DIFF SCOPE

Diff-scoping looks natural (only check what changed) and is quietly vacuous:
run it on `main` with nothing in the diff and it examines zero files while
printing OK. A baseline checks all of them, every time, and tightens by itself —
a script that GAINS a selftest must leave the baseline, so the debt can only
shrink.

Usage::

    check-gate-selftests.py                  # the gate
    check-gate-selftests.py --audit          # full picture, never fails
    check-gate-selftests.py --write-baseline # after fixing scripts
"""

import ast
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "gate-selftest-baseline.txt")
# Module-level so `check-baseline-shape` can hold the file to it.
BASELINE_HEADER = (
    "# Gate scripts that do NOT yet run their own selftest on the normal\n"
    "# path. A RATCHET, not an allowlist: this file may only shrink.\n"
    "#\n"
    "# `check-gate-selftests` fails when a script here gains a selftest\n"
    "# (remove its line) or disappears (delete its line) — so the debt\n"
    "# cannot silently grow and cannot silently go stale, which is the\n"
    "# issue-0743 class.\n"
    "#\n"
    "# Regenerate: python3 scripts/check-gate-selftests.py --write-baseline\n"
)

# WHAT COUNTS AS "RUNS ON THE NORMAL PATH" — a REACHABILITY question.
#
# The first classifier matched LINES: a `self_test(` call anywhere, excused if
# a shallower line within four lines back contained the literal `--self`. That
# was both too generous and too blind (phase-472 W9, audit 2026-09-28):
#
#   * it did not recognise `if args.self_test:` — argparse's spelling of the
#     SAME guard carries no `--self` on the guarding line — so 17 gates that run
#     their control ONLY behind a flag were counted compliant, among them
#     `check-no-vacuous-tests`;
#   * it counted prose: a docstring saying "`self_test()` below" or a test
#     string inside this very gate's own cases classified a script `auto`;
#   * it counted a DEFINITION that happened not to use `{` (`selftest() (` —
#     a subshell body) as a call.
#
# So the question is now asked the way it is meant: starting from what runs
# when the script is executed with no arguments, is a selftest routine CALLED
# on a path no flag guards? Python is answered on the AST (module statements
# plus every module function they call, transitively); shell with a block-aware
# line scanner (if/elif/else/fi, case arms, functions, heredocs skipped) plus
# the same function reachability. A GUARD is a condition that names the flag —
# `--self-test` / `--selftest`, `args.self_test`, a `$SELFTEST`-style variable —
# and NOT one that merely calls the routine: `if self_test() != 0:` and
# `if ! selftest; then` are the inline shape, and they run it.
SELFTEST_NAME = re.compile(r"self_?test", re.I)
# A condition mentioning the selftest FLAG. Python: any name, attribute or
# string in the test outside a call to the routine itself.
PY_GUARD_TEXT = re.compile(r"self.?test", re.I)
# Shell: the literal flag, or a variable reference whose name says selftest.
SH_GUARD = re.compile(r"--self|\$\{?[A-Za-z0-9_]*self_?test", re.I)
HAS_SELFTEST = re.compile(r"self.?test", re.I)


# `import "just/x.just"` MERGES that file's recipes into the root namespace —
# unlike `mod`, which namespaces them. A gate that enumerates `check-*` recipes
# therefore has to read the imported files too, or every moved recipe reads as
# "backs no `check-*` recipe any more" while it is running on every push.
#
# Same latent hole `check-just-recipe-refs` carried: invisible while every gate
# happened to live in one file, and surfacing as a flood — 117 at once — the
# moment phase-399 moved 200 of them into `just/check.just`.
IMPORT_DEF = re.compile(r'^import\s+[\'"]([^\'"]+)[\'"]', re.M)


def _justfile_sources():
    """The root justfile plus every file it `import`s (not `mod`s)."""
    root = os.path.join(ROOT, "justfile")
    with open(root, encoding="utf8") as fh:
        text = fh.read()
    files = [root]
    for rel in IMPORT_DEF.findall(text):
        f = os.path.join(ROOT, rel)
        if os.path.exists(f):
            files.append(f)
    return files


def gate_scripts():
    """Scripts invoked by a gate — i.e. by a recipe in the `check` module.

    A gate used to be spelled `check-foo:` at the root, so "is this a gate?"
    was a question about the NAME. The `check` module now holds them as bare
    names (`foo:`), so it becomes a question about the FILE — which is the more
    honest one anyway, since the prefix was only ever a namespace worn as a
    name.

    Deliberately the `just/check.just` import closure and not the root: widening
    it to every recipe in every justfile made this report 135 problems, most of
    them root verbs like `bootstrap.sh` that assert nothing and were never
    gates.
    """
    # `just/check.just` AND the topic files it imports. The gates moved into
    # `just/check/*.just`; reading only the index finds the seven lane recipes
    # and no gate scripts at all, which would report every one of the 121
    # baseline entries as "backs no `check-*` recipe any more" -- loud, but
    # loud about the wrong thing.
    gate_file = os.path.join(ROOT, "just", "check.just")
    with open(gate_file, encoding="utf8") as fh:
        text = fh.read()
    for rel in re.findall(r"^import\s+'([^']+)'", text, re.MULTILINE):
        imported = os.path.join(os.path.dirname(gate_file), rel)
        if os.path.isfile(imported):
            with open(imported, encoding="utf8") as fh:
                text += "\n" + fh.read()
    lines = text.split("\n")
    found, in_recipe = set(), False
    for line in lines:
        if re.match(r"^[a-z][a-z0-9-]*[ :]", line):
            in_recipe = True
            continue
        if line and not line[0].isspace():
            in_recipe = False
        if not in_recipe:
            continue
        for m in re.finditer(r"scripts/[A-Za-z0-9._/-]+\.(?:sh|py)", line):
            p = m.group(0)
            # `scripts/build/**` PRODUCES artifacts; it does not assert, so it
            # is not a gate and cannot have a failure path to exercise. A gate
            # legitimately invokes one as a prerequisite —
            # `check-source-gates` builds its own compile-check stamps — and
            # counting that as a gate demanded a selftest of a build step.
            if p.startswith("scripts/build/"):
                continue
            if os.path.exists(os.path.join(ROOT, p)):
                found.add(p)
    return sorted(found)


def _py_callee(call):
    f = call.func
    if isinstance(f, ast.Name):
        return f.id
    if isinstance(f, ast.Attribute):
        return f.attr
    return None


def _py_guard(test, on_by_default=frozenset()):
    """'pos' if `test` names the selftest flag, 'neg' for `not <that>`, else None.

    `on_by_default` holds the enclosing function's parameters that DEFAULT to
    True — `def main(argv=None, _self_test=True)`, the recursion stopper a
    selftest passes as False when it drives `main` itself. Testing one of those
    is the normal path, not a flag.
    """
    if isinstance(test, ast.UnaryOp) and isinstance(test.op, ast.Not):
        inner = _py_guard(test.operand, on_by_default)
        return {"pos": "neg", "neg": "pos"}.get(inner)
    if isinstance(test, ast.Name) and test.id in on_by_default:
        return None

    def names_flag(node):
        if isinstance(node, ast.Call):
            callee = _py_callee(node)
            if callee and SELFTEST_NAME.search(callee):
                return False  # CALLING the routine is not guarding on it
        if isinstance(node, ast.Attribute) and PY_GUARD_TEXT.search(node.attr):
            return True
        if isinstance(node, ast.Name) and PY_GUARD_TEXT.search(node.id):
            return True
        if (isinstance(node, ast.Constant) and isinstance(node.value, str)
                and PY_GUARD_TEXT.search(node.value)):
            return True
        return any(names_flag(c) for c in ast.iter_child_nodes(node))

    return "pos" if names_flag(test) else None


def _classify_py(text):
    """(state, lineno) — see the block comment above."""
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return None
    funcs = {n.name: n for n in tree.body
             if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
    auto, guarded_seen, seen, on_stack = [], [False], set(), [frozenset()]

    def visit_fn(name, guarded):
        if (name, guarded) in seen:
            return
        seen.add((name, guarded))
        fn = funcs[name]
        a = fn.args
        pos = a.posonlyargs + a.args
        defaults = list(zip(pos[len(pos) - len(a.defaults):], a.defaults))
        defaults += [(k, d) for k, d in zip(a.kwonlyargs, a.kw_defaults) if d is not None]
        on = frozenset(p.arg for p, d in defaults
                       if isinstance(d, ast.Constant) and d.value is True)
        on_stack.append(on)
        stmts(fn.body, guarded)
        on_stack.pop()

    def guard(test):
        return _py_guard(test, on_stack[-1])

    def expr(node, guarded):
        if isinstance(node, ast.stmt):
            stmts([node], guarded)
            return
        if isinstance(node, ast.Lambda):
            return
        if isinstance(node, ast.IfExp):
            g = guard(node.test)
            expr(node.test, guarded)
            expr(node.body, guarded or g == "pos")
            expr(node.orelse, guarded or g == "neg")
            return
        if isinstance(node, ast.BoolOp) and isinstance(node.op, ast.And):
            g = guarded
            for v in node.values:
                expr(v, g)
                g = g or guard(v) == "pos"
            return
        if isinstance(node, ast.Call):
            callee = _py_callee(node)
            if callee and SELFTEST_NAME.search(callee):
                if guarded:
                    guarded_seen[0] = True
                else:
                    auto.append(node.lineno)
            elif isinstance(node.func, ast.Name) and node.func.id in funcs:
                visit_fn(node.func.id, guarded)
        for child in ast.iter_child_nodes(node):
            expr(child, guarded)

    def stmts(body, guarded):
        for s in body:
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                continue
            if isinstance(s, ast.If):
                g = guard(s.test)
                expr(s.test, guarded)
                stmts(s.body, guarded or g == "pos")
                stmts(s.orelse, guarded or g == "neg")
                continue
            for child in ast.iter_child_nodes(s):
                expr(child, guarded)

    stmts(tree.body, False)
    if auto:
        return "auto", min(auto)
    if guarded_seen[0]:
        return "flag-only", None
    return None


SH_FUNC_DEF = re.compile(
    r"^\s*(?:function\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:\(\))?|([A-Za-z_][A-Za-z0-9_]*)\s*\(\))"
    r"\s*([{(])?\s*(.*)$")
SH_PY_HEREDOC = re.compile(r"python3?\b[^\n]*<<-?\s*['\"]?([A-Za-z_][A-Za-z0-9_]*)['\"]?[^\n]*")
SH_CMD_POS = r"(?:^|[;&|!(){`]|\$\(|\b(?:then|do|else|if|elif|while|until)\b)\s*"


def _sh_guard(cond):
    """'pos' when the branch runs only WITH the flag, 'neg' when only without.

    `[ -z "${X_SELFTEST:-}" ]` is the negative spelling: it runs the control
    when the recursion-stopper variable is UNSET, i.e. on the normal path.
    """
    if not SH_GUARD.search(cond):
        return None
    neg = "!=" in cond or re.match(r"\s*!\s", cond) or re.search(r"\s-z\s", cond)
    return "neg" if neg else "pos"


def _sh_mask(text):
    """Blank what is not shell CODE, keeping every newline so line numbers hold.

    Comments, heredoc bodies and MULTI-LINE quoted strings go (an embedded awk
    or python program is full of `if`/`{` that would unbalance the block
    scanner — measured: `check-cpp-freestanding-includes` read its whole
    selftest as nested in an awk `if`). A one-line quoted string stays, since a
    `case` pattern is often spelled `'--self-test')` and must remain visible.
    """
    out, i, n = [], 0, len(text)
    heredocs = []  # terminators pending for the next newline
    at_word_start = True
    while i < n:
        c = text[i]
        if c == "\n" and heredocs:
            out.append(c)
            i += 1
            while heredocs and i < n:
                end = text.find("\n", i)
                end = n if end < 0 else end
                line = text[i:end]
                out.append(" " * len(line) + ("\n" if end < n else ""))
                i = end + 1
                if line.strip() == heredocs[0]:
                    heredocs.pop(0)
            at_word_start = True
            continue
        if c == "\\" and i + 1 < n:
            out.append(text[i:i + 2])
            i += 2
            at_word_start = False
            continue
        if c == "#" and at_word_start:
            end = text.find("\n", i)
            end = n if end < 0 else end
            out.append(" " * (end - i))
            i = end
            continue
        if c == "<" and text.startswith("<<", i) and not text.startswith("<<<", i):
            m = re.match(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", text[i:])
            if m:
                heredocs.append(m.group(2))
                out.append(m.group(0))
                i += len(m.group(0))
                at_word_start = False
                continue
        if c in "'\"":
            j = i + 1
            while j < n and text[j] != c:
                if c == '"' and text[j] == "\\":
                    j += 1
                j += 1
            body = text[i + 1:j]
            if "\n" in body:
                body = re.sub(r"[^\n]", " ", body)
            out.append(c + body + (c if j < n else ""))
            i = j + 1
            at_word_start = False
            continue
        out.append(c)
        at_word_start = c in " \t\n;&|()`"
        i += 1
    return "".join(out)


def _classify_sh(text):
    """Shell, by EXTENT rather than by grammar.

    Only two kinds of block matter to the question: function bodies (a call
    inside one runs only if the function does) and FLAG-GUARDED branches. Every
    other `if`/`case`/loop is irrelevant, so it is not parsed — a full shell
    grammar here was measured to lose its place inside ordinary gates
    (`…; }` group closers, awk programs, `run_case` nested in a selftest) and
    classify their inline controls `flag-only`. A block ends at the first line
    back at its opener's indentation, which is how every gate in the tree is
    laid out; `_sh_mask` has already blanked comments, heredocs and multi-line
    strings, so their contents cannot end a block early.
    """
    lines = _sh_mask(text).split("\n")

    def indent(l):
        return len(l) - len(l.lstrip())

    fns = []  # (name, first, last) — body lines, inclusive
    for i, line in enumerate(lines):
        m = SH_FUNC_DEF.match(line)
        if not m:
            continue
        name, opener, rest = m.group(1) or m.group(2), m.group(3), m.group(4)
        closer = ")" if opener == "(" else "}"
        if rest.strip() and rest.rstrip().endswith(closer):
            fns.append((name, i, i))
            continue
        k, end = indent(line), len(lines) - 1
        for j in range(i + 1, len(lines)):
            s = lines[j].strip()
            if s and indent(lines[j]) <= k and s[0] in "})":
                end = j
                break
        fns.append((name, i, end))
    defined = {f[0] for f in fns}

    guarded = [False] * len(lines)
    for i, line in enumerate(lines):
        s, k = line.strip(), indent(line)
        m = re.match(r"(if|elif)\b(.*?)(;\s*then\b|$)", s)
        if m and re.search(r"\bfi\s*;?\s*$", s) is None:
            g = _sh_guard(m.group(2))
            if g is None:
                continue
            # The guarded half: the then-branch for `= --self-test`, the
            # else-branch for `!= --self-test`.
            j = i + 1
            branch_then = True
            while j < len(lines):
                t = lines[j].strip()
                if t and indent(lines[j]) <= k and re.match(r"(elif|else|fi)\b", t):
                    if t.startswith("fi"):
                        break
                    branch_then = False
                    if t.startswith("elif"):
                        break  # a later elif is its own condition
                    j += 1
                    continue
                if (g == "pos") == branch_then:
                    guarded[j] = True
                j += 1
            continue
        am = re.match(r"\(?\s*([^()]*)\)(.*)$", s)
        if am and SH_GUARD.search(am.group(1)) and not s.startswith(("if", "[", "elif")):
            if am.group(2).rstrip().endswith(";;"):
                continue  # one-line arm: the line rule below covers it
            for j in range(i + 1, len(lines)):
                t = lines[j].strip()
                if t and indent(lines[j]) <= k:
                    break
                guarded[j] = True
                if t.endswith(";;"):
                    break

    call_rx = re.compile(
        SH_CMD_POS + r"([A-Za-z_][A-Za-z0-9_]*self_?test[A-Za-z0-9_]*"
        + "".join("|" + re.escape(n) for n in sorted(defined, key=len, reverse=True))
        + r")(?![A-Za-z0-9_=])(?!\s*\(\))", re.I)
    sites = []  # (enclosing fn | None, guarded, callee, lineno)
    for i, line in enumerate(lines):
        segment = line
        m = SH_FUNC_DEF.match(line)
        if m:
            segment = m.group(4)  # the def itself is not a call; its body may be
        enclosing = None
        best = None
        for name, first, last in fns:
            if first <= i <= last and (best is None or first >= best):
                if first == i and not m:
                    continue
                enclosing, best = name, first
        if m and fns and enclosing is None:
            continue
        g = guarded[i] or bool(SH_GUARD.search(line))
        for c in call_rx.finditer(segment):
            sites.append((enclosing, g, c.group(1), i + 1))

    reachable, changed = {None}, True
    while changed:
        changed = False
        for fn, g, callee, _ in sites:
            if not g and fn in reachable and callee in defined and callee not in reachable:
                reachable.add(callee)
                changed = True
    auto = [ln for fn, g, callee, ln in sites
            if not g and fn in reachable and SELFTEST_NAME.search(callee)]
    if auto:
        return "auto", min(auto)
    if any(SELFTEST_NAME.search(c) for _, _, c, _ in sites):
        return "flag-only", None
    return None


def classify(rel):
    """('auto' | 'flag-only' | 'unreached' | 'none', evidence)."""
    with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
        text = fh.read()
    lines = text.split("\n")
    if not any(HAS_SELFTEST.search(l) for l in lines if not l.strip().startswith("#")):
        return "none", ""
    if rel.endswith(".py"):
        verdict = _classify_py(text)
    else:
        verdict = _classify_sh(text)
        if verdict is None or verdict[0] != "auto":
            # A shell wrapper whose whole body is `python3 - <<'PY' … PY`: the
            # selftest lives in the embedded program, which `_sh_mask` blanks.
            for m in SH_PY_HEREDOC.finditer(text):
                start = text.count("\n", 0, m.end()) + 1
                body = text[m.end():].split("\n" + m.group(1) + "\n", 1)[0]
                inner = _classify_py(body.split("\n", 1)[-1])
                if inner and inner[0] == "auto":
                    verdict = ("auto", start + inner[1])
                    break
                verdict = verdict or inner
    if verdict is None:
        # Named somewhere — a definition, prose, a string — and called on no
        # path at all. Not a control that runs; not one behind a flag either.
        return "unreached", ""
    state, lineno = verdict
    if state == "auto":
        return "auto", f"{rel}:{lineno}: {lines[lineno - 1].strip()[:70]}"
    return state, ""


def load_baseline():
    if not os.path.exists(BASELINE):
        raise SystemExit(
            f"check-gate-selftests: baseline missing at {BASELINE}.\n"
            "  It is tracked, so its absence is a PATH bug, not an empty ratchet.\n"
            "  Regenerate deliberately with --write-baseline."
        )
    with open(BASELINE, encoding="utf8") as fh:
        return {l.strip() for l in fh if l.strip() and not l.startswith("#")}


def write_baseline(states):
    debt = sorted(r for r, (s, _) in states.items() if s != "auto")
    with open(BASELINE, "w", encoding="utf8") as fh:
        fh.write(BASELINE_HEADER)
        for r in debt:
            fh.write(r + "\n")
    print(f"wrote {BASELINE} — {len(debt)} script(s) of {len(states)} still owe a selftest")


def main():
    audit = "--audit" in sys.argv
    states = {r: classify(r) for r in gate_scripts()}

    if "--write-baseline" in sys.argv:
        write_baseline(states)
        return 0

    if audit:
        by = {}
        for r, (s, _) in states.items():
            by.setdefault(s, []).append(r)
        print(f"gate scripts backing a `check-*` recipe: {len(states)}")
        for k, label in (("auto", "runs its selftest on the normal path"),
                         ("flag-only", "has a selftest, only behind a flag"),
                         ("unreached", "names a selftest that no path calls"),
                         ("none", "no selftest at all")):
            print(f"  {len(by.get(k, [])):3d}  {label}")
        for k in ("flag-only", "unreached"):
            for r in sorted(by.get(k, [])):
                print(f"    {k}: {r}")
        return 0

    baseline = load_baseline()
    errs = []

    for rel, (state, _ev) in sorted(states.items()):
        if state != "auto" and rel not in baseline:
            errs.append(
                f"{rel}: a gate must run its own selftest on the NORMAL path.\n"
                f"      state: {state}\n"
                f"      A negative control nobody runs decays into a comment. Call\n"
                f"      it from main (see scripts/check-board-tiers.py), do NOT add\n"
                f"      a second `--selftest` invocation to the justfile."
            )

    for rel in sorted(baseline):
        if rel not in states:
            errs.append(
                f"{rel}: in the baseline but backs no `check-*` recipe any more.\n"
                f"      Delete the line. A stale entry is inert while reading as\n"
                f"      tracked debt — the issue-0743 class."
            )
        elif states[rel][0] == "auto":
            errs.append(
                f"{rel}: now runs its selftest — remove it from the baseline.\n"
                f"      The ratchet only tightens; leaving it here lets the gate\n"
                f"      be silently loosened again later."
            )

    if errs:
        print(f"check-gate-selftests: {len(errs)} problem(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print(f"\n  Baseline: {os.path.relpath(BASELINE, ROOT)} "
              f"(--write-baseline after fixing, --audit for the full picture)",
              file=sys.stderr)
        return 1

    auto = sum(1 for s, _ in states.values() if s == "auto")
    print(f"check-gate-selftests OK — {auto}/{len(states)} gate script(s) run their own "
          f"selftest; {len(baseline)} still owe one and may only decrease.")
    return 0


def self_test(quiet=True):
    """Prove the classifier can fail. Runs on EVERY invocation — this gate must
    hold itself to the rule it enforces, or it is advice rather than a gate."""
    import tempfile
    # (expected, extension, body). EVERY flag-guard spelling the tree uses
    # has a row that must read `flag-only`, and every inline shape a row that
    # must read `auto` — a classifier that drifts either way fails here, on
    # every run, before it can mis-ratchet a single real gate.
    DEF = "import sys\ndef self_test():\n    return 0\n\n"
    cases = [
        # --- Python, inline: these RUN the control on the normal path.
        ("auto", "py", DEF + "def main():\n    self_test()\n    return 0\n\n"
                       "if __name__ == '__main__':\n    sys.exit(main())\n"),
        ("auto", "py", DEF + "def main():\n    if self_test() != 0:\n        return 1\n"
                       "    return 0\n\nsys.exit(main())\n"),
        # This gate's own shape: a flag for the loud form, AND the quiet call.
        ("auto", "py", DEF + "if __name__ == '__main__':\n"
                       "    if '--self-test' in sys.argv:\n        sys.exit(self_test())\n"
                       "    self_test()\n"),
        ("auto", "py", DEF + "def main(args):\n    if not args.self_test:\n"
                       "        self_test()\n\nmain(None)\n"),
        ("auto", "py", DEF + "import lib\nsys.exit(lib.selftest() or 0)\n"),
        # --- Python, flag-only: every guard spelling.
        ("flag-only", "py", DEF + "if '--selftest' in sys.argv:\n    self_test()\n"),
        ("flag-only", "py", DEF + "def main():\n    args = parse()\n"
                            "    if args.self_test:\n        return self_test()\n"
                            "    return 0\n\nsys.exit(main())\n"),
        ("flag-only", "py", DEF + "def main():\n    args = parse()\n"
                            "    if args.selftest: return self_test()\n"
                            "    return 0\n\nsys.exit(main())\n"),
        ("flag-only", "py", DEF + "sys.exit(self_test() if opts.self_test else 0)\n"),
        ("flag-only", "py", DEF + "if sys.argv[1:] == ['--self-test']:\n    self_test()\n"
                            "else:\n    pass\n"),
        ("flag-only", "py", DEF + "if len(sys.argv) > 1 and sys.argv[1] == 'selftest':\n"
                            "    sys.exit(self_test())\n"),
        ("flag-only", "py", DEF + "args.self_test and self_test()\n"),
        # A recursion stopper that DEFAULTS on is the normal path, not a flag.
        ("auto", "py", DEF + "def main(argv=None, _self_test=True):\n"
                       "    if _self_test:\n        self_test()\n\nmain()\n"),
        ("flag-only", "py", DEF + "def main(argv=None, _self_test=False):\n"
                            "    if _self_test:\n        self_test()\n\nmain()\n"),
        # --- Python, NOT a call on any path: prose, a comment, a string, a
        # function nobody calls.
        ("unreached", "py", DEF + "#    self_test()\n"),
        ("unreached", "py", DEF + '"""Runs `self_test()` below."""\n'),
        ("unreached", "py", DEF + "CASES = ['self_test()']\n"),
        ("unreached", "py", DEF + "def main():\n    self_test()\n"),
        ("none", "py", "def main():\n    return 0\n"),
        # --- Shell, inline.
        ("auto", "sh", "self_test() {\n    return 0\n}\n\nself_test || exit 1\n"),
        ("auto", "sh", "_cc_selftest() {\n    return 0\n}\n\n_cc_selftest\n"),
        ("auto", "sh", "selftest() {\n    return 0\n}\nif ! selftest; then\n"
                       "    exit 1\nfi\n"),
        ("auto", "sh", "selftest() { return 0; }\nmain() {\n    selftest || exit 1\n}\n"
                       'main "$@"\n'),
        ("auto", "sh", "selftest() {\n  cat <<EOF\nif fi case\nEOF\n}\n"
                       'case "$1" in\n  --self-test) selftest; exit ;;\nesac\nselftest\n'),
        # --- Shell, flag-only: every guard spelling.
        ("flag-only", "sh", 'self_test() {\n    return 0\n}\n\nif [ "$1" = "--self-test" ]; then\n'
                            "    self_test\nfi\n"),
        ("flag-only", "sh", 'self_test() { return 0; }\n[ "${1:-}" = --self-test ] && self_test\n'),
        ("flag-only", "sh", 'selftest() { return 0; }\ncase "${1:-}" in\n'
                            "  --self-test|--selftest) selftest; exit $? ;;\n  *) ;;\nesac\n"),
        ("flag-only", "sh", 'selftest() { return 0; }\ncase "$1" in\n  --selftest)\n'
                            "    selftest\n    ;;\nesac\n"),
        ("flag-only", "sh", 'selftest() { return 0; }\nif [ -n "${NROS_SELFTEST:-}" ]; then\n'
                            "    selftest\nfi\n"),
        ("flag-only", "sh", 'selftest() { return 0; }\nrun() {\n    selftest\n}\n'
                            'if [ "$1" = --selftest ]; then run; fi\n'),
        # A recursion-stopper variable tested with `-z` runs when UNSET.
        ("auto", "sh", 'selftest() { return 0; }\nif [ -z "${X_SELFTEST:-}" ]; then\n'
                       "    selftest || exit 1\nfi\n"),
        # A wrapper around an embedded Python program: the program decides.
        ("auto", "sh", "python3 - \"$@\" <<'PY'\n" + DEF + "self_test()\nPY\n"),
        ("flag-only", "sh", "python3 - \"$@\" <<'PY'\n" + DEF
                            + "if '--self-test' in sys.argv:\n    self_test()\nPY\n"),
        # --- Shell, a subshell-bodied definition is not a call.
        ("unreached", "sh", "selftest() (\n    true\n)\n\necho done\n"),
    ]
    fails = []
    with tempfile.TemporaryDirectory() as d:
        for i, (want, ext, body) in enumerate(cases):
            rel = f"t{i}.{ext}"
            with open(os.path.join(d, rel), "w", encoding="utf8") as fh:
                fh.write(body)
            g, real = globals(), globals()["ROOT"]
            g["ROOT"] = d
            got = classify(rel)[0]
            g["ROOT"] = real
            if got != want:
                fails.append(f"case {i}: expected {want}, got {got}")
    if fails:
        for f in fails:
            print(f"check-gate-selftests self-test: FAIL {f}", file=sys.stderr)
        raise SystemExit(1)
    if not quiet:
        print("check-gate-selftests self-test: OK")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv or "--selftest" in sys.argv:
        sys.exit(self_test(quiet=False))
    # Always, not only behind the flag: this gate enforces exactly this rule.
    self_test()
    sys.exit(main())
