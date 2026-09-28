#!/usr/bin/env python3
"""A precondition guard in a TEST FILE must own its verdict — issue 1135.

CLAUDE.md: "Tests must fail on unmet preconditions (`assert!`/`bail!`/
`nros_tests::skip!`). Bare `eprintln!`+`return` reports PASS — never."
`check-no-vacuous-tests` enforces the half of that rule that is a property of a
test BODY (it only prints). This gate enforces the half that is a property of a
test-local HELPER's SIGNATURE.

## The shape

    fn require_node_discoverable(locator: &str) -> bool {  // params.rs, pre-1135
        for attempt in 1..=3 { ... return true; }
        eprintln!("Skipping test: nros node /demo/talker not discoverable ...");
        false
    }

    #[rstest]
    fn test_ros2_param_list(zenohd_unique: ZenohRouter) {
        ...
        if !require_node_discoverable(&locator) {
            return;              // <-- a bare `return` from a #[test] is a PASS
        }
        ...
    }

Four call sites, four identical mistakes, four green tests. And note WHAT was
swallowed: zenohd, ROS 2 and the fixture had all already passed their own
guards, so a `false` here meant "everything is up and our node is not in the
graph" — a delivery failure, the exact thing those four tests exist to detect.
Under `--failure-output never` (what `just native test-ros2-params` passes) the
`eprintln!` was not shown either, so the only evidence was invisible.

## Why the signature and not the call site

The call-site shape is NOT statically separable from its legitimate twin. Both
of these are `if <cond> { return; }` inside a test body:

    if !require_node_discoverable(&locator) { return; }   // defect: PASS on failure
    if server.wait_for_output_pattern(MARKER, T).is_ok() { return; }  // fine: PASS on success

Only the meaning of `<cond>` tells them apart, and a scanner cannot read it. A
rule of "no bare `return;` in a test body" IS decidable, but it is a different
rule from the one we want: measured across the tree it flags 40 sites, most of
them the legitimate success-path form, which buys an authored 40-entry
allowlist — the kind of map CLAUDE.md already records drifting (the rmw parity
map read "gap" for 28 slots that had landed).

The signature is decidable AND load-bearing. A guard named `require_*` /
`ensure_*` / `need_*` / `maybe_*` that hands back a `bool` (or an `Option<()>`,
which is a bool wearing a hat) has delegated the verdict to callers nobody
checks; a guard that returns `()` has kept it, and `skip!` is then the only way
out. Five such helpers existed in this tree and every one of them either was the
bug (`require_node_discoverable`, `require_nuttx_setup`) or carried its dead
branch (`require_native_env`, `maybe_skip`) or reported "check failed" with the
real reason `eprintln!`ed into a stream the runner discards
(`require_freertos`, `require_threadx_riscv64`, `require_esp32_networked`).

## Scope: the whole tree, and that is measured, not asserted

Issue 1160 asked whether this gate should be widened from
`packages/testing/nros-tests/tests/` to `packages/**/tests/`. It should not,
because it was never narrow: `tracked_test_files()` globs `*/tests/*.rs` across
the repository and reads 292 files in 26 directories. 1135 SWEPT one directory;
the gate it left behind reads all of them, and it is green with no allowlist at
the wide scope — which was the property the widening question was really about.

That is a fact about a glob, and a glob is one edit away from being narrower.
`assert_scope_is_the_whole_tree()` below therefore fails the gate if the scan
ever stops reaching test files outside `packages/testing/`. "OK (0 test files)"
and "OK (169 test files, all of them nros-tests)" are both what a gate that has
quietly stopped covering anything would print, and neither is distinguishable
from a real green without a floor.

## What is deliberately NOT covered

* **Library probes in `packages/testing/nros-tests/src/`** — `require_zenohd()`,
  `require_ros2()`, `is_*_available()` and friends legitimately return `bool`:
  they are composed, and each call site writes its own message. The verdict
  belongs to the test-local guard that composes them, which is what this gate
  scopes to.
* **A helper returning a real value** — `fn require_preconditions() ->
  Option<(PathBuf, PathBuf)>` is fine and is not flagged: the caller needs the
  paths, and `let Some(x) = f() else { skip!(..) }` is the correct spelling.
  Only `bool` and the value-free `Option<()>` are refused BY SIGNATURE — and
  since issue 1539 the exemption is no longer taken on trust: rule 2 below
  checks that the caller's `else` arm actually skips or panics.
* **A test that inlines the probe and returns.** The gate keys on helpers
  because that is where the leverage was: one helper, four wrong call sites.

## What to write instead

    fn require_node_discoverable(locator: &str) {
        for attempt in 1..=3 { ... return; }
        nros_tests::skip_class!(resource, "/demo/talker not discoverable ...");
    }

    require_node_discoverable(&locator);   // no verdict for a caller to drop
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# A helper whose NAME announces it is a precondition gate.
GUARD_NAME_RE = re.compile(r"^(require|ensure|need|maybe)_\w+$")

# `fn <name>(<args>) -> <ret> {` — args may span lines, so match the name and
# then find the `->` that belongs to this signature (before the opening brace).
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*\(")

# The two return types that are a VERDICT and nothing else.
VERDICT_RET_RE = re.compile(r"->\s*(bool|Option\s*<\s*\(\s*\)\s*>)\s*\{?\s*$")


def strip_line_comments(line: str) -> str:
    """Drop a trailing `//` comment. Crude but sufficient: no `//` appears
    inside a string literal in any signature this gate reads."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def guard_violations(path: str, src: str) -> list[str]:
    out: list[str] = []
    lines = src.split("\n")
    for i, raw in enumerate(lines):
        line = strip_line_comments(raw)
        m = FN_RE.match(line)
        if not m:
            continue
        name = m.group(1)
        if not GUARD_NAME_RE.match(name):
            continue
        # Accumulate the signature until its opening brace, so a multi-line
        # argument list still exposes the return type.
        sig = line
        j = i
        while "{" not in sig and j + 1 < len(lines) and j - i < 12:
            j += 1
            sig += " " + strip_line_comments(lines[j]).strip()
        # Only the text before the body brace is the signature.
        sig = sig.split("{", 1)[0] + "{"
        if VERDICT_RET_RE.search(sig):
            ret = VERDICT_RET_RE.search(sig).group(1)
            out.append(
                f"{path}:{i + 1}: `{name}` returns `{ret}` — a test-local precondition "
                f"guard must own its verdict (return `()` and `nros_tests::skip!`), "
                f"not hand a caller a value they can drop with a bare `return`"
            )
    return out


# ---------------------------------------------------------------------------
# Rule 2 — the CALLER's `else` arm (issue 1539).
#
# The helper-signature rule above exempts a guard returning a real value, on the
# grounds that "`let Some(x) = f() else { skip!(..) }` is the correct spelling".
# Nothing checked that the caller wrote that spelling. `zenoh_integration.rs`
# had `fn router() -> Option<ZenohRouter>` that `eprintln!`ed and returned
# `None`, and all five callers wrote `let Some(_router) = router() else {
# return };` — five tests that PASSED having run nothing on every host without
# zenohd. Renaming the helper into the rule's population did not help, because
# `Option<ZenohRouter>` is exactly what rule 1 exempts.
#
# Unlike the `if <cond> { return; }` shape rule 1 declines to judge, the `else`
# arm of a `let … else` IS decidable: it runs only when the pattern did NOT
# match, so it is always the failure path. A `return` there, from a TEST body,
# is a pass on failure — whatever the helper is called and whatever it returns.
# The rule therefore keys on the arm, in every test fn in every tracked `.rs`
# (unit-test modules in `src/` included), not on helper names.
#
# Accepted arms diverge by a verdict: `skip!`/`skip_class!`/`panic!`/
# `unreachable!`/`todo!`/`unimplemented!`/`bail!`/an `assert!`, or a
# `return Err(..)` from a `Result`-returning test (a failure, not a pass).
# `continue`/`break` are loop control and not this shape. A `let … else` inside
# a CLOSURE in a test body returns from the closure, not the test, and is
# skipped.
#
# The second half: a HELPER in the same file that prints and declines (returns
# `Option`/`bool` and its body `eprintln!`s or `println!`s) is the shape that
# produced 1539, so every call to one from a test body must sit in a form the
# scanner can see the verdict of — a `let … else` (whose arm the rule above
# checks), `?`, `.expect(`/`.unwrap(`, `assert!(…)`, or an `if !f(..) {…}` /
# `if f(..).is_none() {…}` whose block diverges. `if let Some(..) = f(..) {…}`
# with no `else` that diverges is the same pass-on-failure one level up.
# ---------------------------------------------------------------------------

TEST_ATTR_RE = re.compile(
    r"#\[\s*(?:test|rstest|tokio::test|test_case|serial_test::serial|"
    r"[A-Za-z_:]*::test)\b"
)
DIVERGE_RE = re.compile(
    r"\b(?:skip|skip_class|panic|unreachable|todo|unimplemented|bail|assert|"
    r"assert_eq|assert_ne)!|\breturn\s+Err\s*\(|\bstd::process::exit\b"
)
BARE_RETURN_RE = re.compile(r"\breturn\b\s*(?:;|\}|$|Ok\s*\(\s*\(\s*\)\s*\)|\(\s*\))")
PRINT_RE = re.compile(r"\b(?:eprintln|println|eprint|print)!")
FN_DECL_RE = re.compile(
    r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?"
    r"(?:extern\s+\"[^\"]*\"\s+)?fn\s+(\w+)\s*(?:<[^>{]*>)?\s*\("
)


def mask_rust(src: str) -> str:
    """Blank comments, string and char literals, keeping every offset.

    Brace matching over raw Rust is wrong the moment a string holds `{` — and
    format strings here hold them constantly. Newlines survive so line numbers
    stay exact.
    """
    out = list(src)
    n = len(src)
    i = 0

    def blank(a: int, b: int) -> None:
        for k in range(a, min(b, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(i, j)
            i = j
        elif c == "r" and re.match(r'r#*"', src[i:i + 8]) and (
            i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")
        ):
            hashes = len(re.match(r"r(#*)\"", src[i:]).group(1))
            close = '"' + "#" * hashes
            j = src.find(close, i + 2 + hashes)
            j = n if j < 0 else j + len(close)
            blank(i + 1, j)
            i = j
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            blank(i + 1, j)
            i = j + 1
        elif c == "'":
            # A char literal ('x', '\n', '\u{..}', '{') vs a lifetime ('a).
            m = re.match(r"'(?:\\u\{[0-9a-fA-F]+\}|\\.|[^\\'])'", src[i:i + 12])
            if m:
                blank(i + 1, i + m.end() - 1)
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def match_brace(text: str, open_idx: int) -> int:
    """Index of the `}` closing the `{` at `open_idx` (masked text)."""
    depth = 0
    for k in range(open_idx, len(text)):
        if text[k] == "{":
            depth += 1
        elif text[k] == "}":
            depth -= 1
            if depth == 0:
                return k
    return len(text) - 1


def functions(masked: str):
    """Yield (name, is_test, sig_text, body_start, body_end) per fn with a body."""
    for m in FN_DECL_RE.finditer(masked):
        # Signature runs to the first `{` or `;` at paren/angle depth 0.
        depth, k = 0, m.end() - 1
        while k < len(masked):
            ch = masked[k]
            if ch in "([":
                depth += 1
            elif ch in ")]":
                depth -= 1
            elif depth == 0 and ch in "{;":
                break
            k += 1
        if k >= len(masked) or masked[k] == ";":
            continue  # a trait item / extern decl — no body
        end = match_brace(masked, k)
        # Attributes: the text between the previous item boundary and `fn`.
        head_start = max(
            masked.rfind("}", 0, m.start()),
            masked.rfind(";", 0, m.start()),
            masked.rfind("{", 0, m.start()),
        )
        head = masked[head_start + 1 : m.start()]
        yield m.group(1), bool(TEST_ATTR_RE.search(head)), masked[m.end() : k], k, end


def closure_spans(masked: str, a: int, b: int) -> list[tuple[int, int]]:
    """`|..| {` blocks inside [a, b) — a `return` there leaves the closure."""
    spans = []
    for m in re.finditer(r"(?:\bmove\s*)?\|[^|;{}]*\|\s*(?:->\s*[^{]+)?\{", masked[a:b]):
        o = a + m.end() - 1
        spans.append((o, match_brace(masked, o)))
    return spans


def line_of(text: str, idx: int) -> int:
    return text.count("\n", 0, idx) + 1


LET_ELSE_RE = re.compile(r"\blet\b[^;{}]*?=[^;]*?\belse\s*\{")


def let_else_arms(masked: str, a: int, b: int):
    """Yield (let_idx, arm_open, arm_close) for each `let … else {…}` in [a, b)."""
    for m in LET_ELSE_RE.finditer(masked, a, b):
        stmt = masked[m.start() : m.end()]
        # `let x = if c { .. } else { .. };` is an if-expression, not let-else:
        # the `else` there closes an `if` whose `{` sits in the statement.
        if "{" in stmt[:-1]:
            continue
        o = m.end() - 1
        yield m.start(), o, match_brace(masked, o)


def arm_is_pass_on_failure(arm: str) -> bool:
    return bool(BARE_RETURN_RE.search(arm)) and not DIVERGE_RE.search(arm)


def decliners(masked: str) -> set[str]:
    """Helpers that print and hand back `Option`/`bool` — the 1539 shape.

    Also those that PROPAGATE one with `?` (`open_session()` calling
    `router_locator()?`): the `None` travels, and so does the obligation.
    """
    fns = [f for f in functions(masked) if not f[1]]
    out: set[str] = set()
    changed = True
    while changed:
        changed = False
        for name, _t, sig, bs, be in fns:
            if name in out:
                continue
            if not re.search(r"->\s*(?:Option\s*<|bool\b)", sig):
                continue
            body = masked[bs:be]
            prints = bool(PRINT_RE.search(body))
            propagates = any(re.search(rf"\b{re.escape(d)}\s*\([^;]*?\)\s*\?", body) for d in out)
            if prints or propagates:
                out.add(name)
                changed = True
    return out


def call_context_ok(masked: str, call_start: int, call_end: int, fn_end: int) -> bool:
    """Does the verdict of a decliner call at [call_start, call_end) survive?"""
    after = masked[call_end : call_end + 40]
    if re.match(r"\s*(?:\?|\.\s*(?:expect|unwrap|unwrap_or_else)\s*\()", after):
        return True
    # Statement start: back to the previous `;`, `{` or `}`.
    s = max(masked.rfind(";", 0, call_start), masked.rfind("{", 0, call_start),
            masked.rfind("}", 0, call_start)) + 1
    before = masked[s:call_start]
    if re.search(r"\bassert\w*!\s*\(\s*!?\s*$", before):
        return True
    if re.match(r"\s*let\b", before) and re.match(r"[^;]*?\belse\s*\{", masked[call_end:fn_end]):
        return True  # a let-else; its arm is judged by the arm rule
    m = re.match(r"\s*if\s+(!)?\s*$", before)
    if m:
        rest = masked[call_end:fn_end]
        o = rest.find("{")
        if o < 0:
            return False
        cond_tail = rest[:o]
        negated = m.group(1) is not None or re.match(r"\s*\.\s*is_none\s*\(\s*\)\s*$", cond_tail)
        blk_open = call_end + o
        blk_close = match_brace(masked, blk_open)
        if negated:
            return bool(DIVERGE_RE.search(masked[blk_open:blk_close]))
        # `if f() {…}` / `if f().is_some() {…}`: the failure path is the else.
        tail = masked[blk_close + 1 : blk_close + 20]
        if re.match(r"\s*else\s*\{", tail):
            eo = blk_close + 1 + tail.index("{")
            return bool(DIVERGE_RE.search(masked[eo : match_brace(masked, eo)]))
        return False
    m = re.match(r"\s*if\s+let\s+[^=]*=\s*$", before)
    if m:
        rest = masked[call_end:fn_end]
        o = rest.find("{")
        blk_close = match_brace(masked, call_end + o)
        tail = masked[blk_close + 1 : blk_close + 20]
        if re.match(r"\s*else\s*\{", tail):
            eo = blk_close + 1 + tail.index("{")
            return bool(DIVERGE_RE.search(masked[eo : match_brace(masked, eo)]))
        return False
    return False


def else_arm_violations(path: str, src: str) -> list[str]:
    masked = mask_rust(src)
    out: list[str] = []
    fns = list(functions(masked))
    decl = decliners(masked)
    call_re = re.compile(r"(?<![\w.:])(" + "|".join(map(re.escape, sorted(decl))) + r")\s*\(") if decl else None
    for name, is_test, _sig, bs, be in fns:
        if name in decl:
            continue  # a decliner may propagate; its callers carry the verdict
        closures = closure_spans(masked, bs, be)

        def in_closure(i: int) -> bool:
            return any(o < i < c for o, c in closures)

        if is_test:
            for let_idx, ao, ac in let_else_arms(masked, bs, be):
                if in_closure(let_idx):
                    continue
                if arm_is_pass_on_failure(masked[ao : ac + 1]):
                    out.append(
                        f"{path}:{line_of(masked, let_idx)}: test `{name}` — the `else` arm "
                        f"of a `let … else` RETURNS, and a `return` from a test is a PASS on "
                        f"the path where its precondition failed; write `nros_tests::skip!` "
                        f"(or `panic!`) there"
                    )
        # Only a TEST body turns a dropped `None` into a PASS; a build script
        # or library helper that falls back on `None` is making a real choice.
        if call_re is None or not is_test:
            continue
        for m in call_re.finditer(masked, bs + 1, be):
            if in_closure(m.start()):
                continue
            # Skip the helper's own declaration (`fn router(`).
            if re.search(r"\bfn\s+$", masked[max(0, m.start() - 8) : m.start()]):
                continue
            close = masked.find("(", m.start())
            depth, k = 0, close
            while k < be:
                if masked[k] == "(":
                    depth += 1
                elif masked[k] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            if not call_context_ok(masked, m.start(), k + 1, be):
                out.append(
                    f"{path}:{line_of(masked, m.start())}: `{name}` calls `{m.group(1)}()`, "
                    f"a helper that PRINTS and hands back `None`/`false`, in a form whose "
                    f"failure path this gate cannot see diverge — bind it with "
                    f"`let … else {{ nros_tests::skip!(..) }}`, `?`, or `.expect(..)`, or make "
                    f"the helper skip itself and return the value"
                )
    return out


# Files that carry the arm-rule shape today and are NOT converted by the change
# that introduced the rule, with how many sites each may hold. SHRINK ONLY: a
# file over its count fails, and a file under it is reported so the number
# comes down. Counts, not lines, so converting one site cannot turn it red.
#
# Every entry is in `packages/cli/`, a separate cargo workspace with no
# `nros_tests` dependency and so no `skip!`. Its ROS-input parity tests return
# on a host without ROS BY DESIGN (issue 0693: the `check-cli-tests` lane is
# ROS-less and runs plain `cargo test`, where a skip-panic is a failure), and
# the two `nros-cli-core` unit tests return on a non-git / packaged tree. Those
# ARE passes over nothing; converting them needs a decision about how the CLI
# workspace spells a skip, which is issue 1544's open item, not a mechanical
# edit.
ARM_RULE_BASELINE: dict[str, int] = {
    "packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs": 1,
    "packages/cli/nros-cli-core/src/source_stamp.rs": 1,
    "packages/cli/rosidl-codegen/tests/comparison_test.rs": 3,
    "packages/cli/rosidl-codegen/tests/parity_test.rs": 9,
}


def apply_arm_baseline(violations: list[str], root: Path) -> tuple[list[str], list[str]]:
    """Split rule-2 violations into (failing, notes) against ARM_RULE_BASELINE."""
    per_file: dict[str, list[str]] = {}
    other: list[str] = []
    for v in violations:
        rel = v.split(":", 1)[0]
        try:
            rel = Path(rel).resolve().relative_to(root).as_posix()
        except ValueError:
            pass
        if rel in ARM_RULE_BASELINE:
            per_file.setdefault(rel, []).append(v)
        else:
            other.append(v)
    notes: list[str] = []
    for rel, allowed in sorted(ARM_RULE_BASELINE.items()):
        got = per_file.get(rel, [])
        if len(got) > allowed:
            other.extend(got)
            other.append(
                f"{rel}: {len(got)} arm-rule site(s), baseline {allowed} — the baseline "
                f"only shrinks"
            )
        elif len(got) < allowed:
            notes.append(f"{rel}: {len(got)} site(s) < baseline {allowed} — shrink it")
    return other, notes


def tracked_rs_files() -> list[Path]:
    """Every tracked `.rs` — unit-test modules in `src/` included (issue 1544).

    Vendored trees are git submodules, so `ls-files` of the superproject never
    lists their contents; nothing to exclude.
    """
    root = Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )
    out = subprocess.run(
        ["git", "ls-files", "*.rs"], capture_output=True, text=True, check=True, cwd=root,
    ).stdout.split()
    return [root / f for f in out]


def tracked_test_files() -> list[Path]:
    root = Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )
    out = subprocess.run(
        ["git", "ls-files", "*/tests/*.rs", "tests/*.rs"],
        capture_output=True,
        text=True,
        check=True,
        cwd=root,
    ).stdout.split()
    return [root / f for f in out]


def assert_scope_is_the_whole_tree(files: list[Path]) -> list[str]:
    """Issue 1160 — a coverage FLOOR, because a green says nothing about reach.

    The 1135 sweep covered one directory and the gate covers the tree; a reader
    (and the issue that filed this) could not tell which from the output. Refuse
    to report OK over a scan that has stopped seeing the rest of the repo.
    """
    outside = [f for f in files if "packages/testing/" not in f.as_posix()]
    if not files:
        return [
            "the scan found NO test files at all — the glob in `tracked_test_files` "
            "no longer matches anything, and an empty scan reports OK"
        ]
    if not outside:
        return [
            f"the scan found {len(files)} test files and every one of them is under "
            "`packages/testing/` — this gate covers `*/tests/*.rs` across the whole "
            "repo on purpose (issue 1160: 37 sites of the 1135 shape live in "
            "`packages/cli/` and `packages/rmw/`). A scan that reaches only "
            "nros-tests has narrowed, and a narrowed gate still prints OK"
        ]
    return []


def scan(paths, arm_paths=None) -> list[str]:
    """Rule 1 over `paths`; rule 2 over `arm_paths` (default: the same files)."""
    violations: list[str] = []
    for p in paths:
        src = Path(p).read_text(encoding="utf-8", errors="replace")
        violations.extend(guard_violations(str(p), src))
    for p in paths if arm_paths is None else arm_paths:
        src = Path(p).read_text(encoding="utf-8", errors="replace")
        violations.extend(else_arm_violations(str(p), src))
    return violations


SELF_TESTS = [
    (
        "require_* -> bool is the 1135 shape",
        "fn require_node_discoverable(locator: &str) -> bool {\n    true\n}\n",
        True,
    ),
    (
        "require_* -> Option<()> is a bool wearing a hat",
        "fn require_nuttx_setup() -> Option<()> {\n    Some(())\n}\n",
        True,
    ),
    (
        "maybe_* -> bool too — `if maybe_skip(..) { return; }` was three call sites",
        "fn maybe_skip(p: Platform, l: Lang) -> bool {\n    false\n}\n",
        True,
    ),
    (
        "the fixed spelling -> ()",
        "fn require_node_discoverable(locator: &str) {\n"
        '    nros_tests::skip!("nope");\n}\n',
        False,
    ),
    (
        "a guard returning a real value is correct — the caller needs it",
        "fn require_preconditions() -> Option<(PathBuf, PathBuf)> {\n    None\n}\n",
        False,
    ),
    (
        "Result<T> guard is not this shape",
        "fn require_agent() -> TestResult<XrceAgent> {\n    todo!()\n}\n",
        False,
    ),
    (
        "a non-guard name returning bool is nobody's precondition",
        "fn is_freertos_available() -> bool {\n    true\n}\n",
        False,
    ),
    (
        "multi-line argument list still exposes the return type",
        "fn require_cell_runnable(\n    platform: Platform,\n    lang: Lang,\n"
        ") -> bool {\n    false\n}\n",
        True,
    ),
    (
        "pub visibility does not exempt it",
        "pub fn require_thing() -> bool {\n    true\n}\n",
        True,
    ),
    (
        "a commented-out signature is not code",
        "// fn require_thing() -> bool {\n//     true\n// }\n",
        False,
    ),
    (
        "Option<Something> spanning a line break is not Option<()>",
        "fn require_paths() -> Option<\n    (PathBuf, PathBuf),\n> {\n    None\n}\n",
        False,
    ),
    # ---- rule 2: the caller's `else` arm (issue 1539) ----
    (
        "1539 verbatim: a printing Option helper and `else { return }` in a test",
        "fn router() -> Option<ZenohRouter> {\n"
        "    if let Some(why) = unavailable() {\n"
        '        eprintln!("[SKIP] {why}");\n        return None;\n    }\n'
        "    Some(ZenohRouter::start_unique().expect(\"x\"))\n}\n"
        "#[test]\nfn t() {\n    let Some(_router) = router() else { return };\n"
        "    work();\n}\n",
        True,
    ),
    (
        "the same helper with a `skip!` arm is the correct spelling",
        "fn router() -> Option<ZenohRouter> {\n"
        '    eprintln!("[SKIP] x");\n    None\n}\n'
        "#[test]\nfn t() {\n"
        '    let Some(_router) = router() else { nros_tests::skip!("no zenohd") };\n}\n',
        False,
    ),
    (
        "the arm rule needs no helper at all: any let-else `return` in a test",
        "#[test]\nfn t() {\n"
        '    let Ok(text) = std::fs::read_to_string("justfile") else {\n'
        "        return; // packaged crate — not a failure\n    };\n"
        "    assert!(text.contains(\"x\"));\n}\n",
        True,
    ),
    (
        "`return Ok(())` from a Result test is the same pass",
        "#[test]\nfn t() -> Result<(), E> {\n"
        "    let Some(d) = dir() else {\n        return Ok(());\n    };\n    Ok(())\n}\n",
        True,
    ),
    (
        "`return Err(..)` from a Result test is a failure, not a pass",
        "#[test]\nfn t() -> Result<(), E> {\n"
        '    let Some(d) = dir() else {\n        return Err(E::new("no dir"));\n    };\n'
        "    Ok(())\n}\n",
        False,
    ),
    (
        "a let-else `return` inside a closure leaves the closure, not the test",
        "#[test]\nfn t() {\n    let h = std::thread::spawn(move || {\n"
        "        let Some(x) = rx.recv().ok() else { return };\n        use_it(x);\n    });\n"
        "    h.join().unwrap();\n}\n",
        False,
    ),
    (
        "a let-else `return` in a NON-test fn is that fn's own business",
        "fn helper() {\n    let Some(x) = f() else { return };\n    g(x);\n}\n",
        False,
    ),
    (
        "a `#[cfg(test)]` unit module in src/ is a test too",
        "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn t() {\n"
        "        let Ok(out) = run() else { return };\n        assert!(out);\n    }\n}\n",
        True,
    ),
    (
        "a braced string holding the shape is not code",
        "#[test]\nfn t() {\n"
        '    let msg = "let Some(r) = router() else { return };";\n'
        "    assert!(!msg.is_empty());\n}\n",
        False,
    ),
    (
        "`if let Some(..) = decliner() {..}` with no else is a pass on None",
        "fn router() -> Option<R> {\n    eprintln!(\"no\");\n    None\n}\n"
        "#[test]\nfn t() {\n    if let Some(r) = router() {\n        work(r);\n    }\n}\n",
        True,
    ),
    (
        "`if !decliner() { return; }` is the 1135 shape under any name",
        "fn ready() -> bool {\n    eprintln!(\"not ready\");\n    false\n}\n"
        "#[test]\nfn t() {\n    if !ready() {\n        return;\n    }\n    work();\n}\n",
        True,
    ),
    (
        "`if !decliner() { skip!(..) }` diverges",
        "fn ready() -> bool {\n    eprintln!(\"not ready\");\n    false\n}\n"
        "#[test]\nfn t() {\n    if !ready() {\n        nros_tests::skip!(\"x\");\n    }\n"
        "    work();\n}\n",
        False,
    ),
    (
        "`.expect(..)` on a decliner panics — a failure, not a pass",
        "fn router_locator() -> Option<String> {\n    eprintln!(\"no\");\n    None\n}\n"
        '#[test]\nfn t() {\n    let l = router_locator().expect("zenohd");\n    use_it(l);\n}\n',
        False,
    ),
    (
        "`?`-propagation makes the caller a decliner too, and ITS caller is judged",
        "fn router_locator() -> Option<String> {\n    eprintln!(\"no\");\n    None\n}\n"
        "fn open() -> Option<S> {\n    let l = router_locator()?;\n    S::open(&l)\n}\n"
        "#[test]\nfn t() {\n    if let Some(s) = open() {\n        s.work();\n    }\n}\n",
        True,
    ),
]


def self_test(quiet: bool = True) -> int:
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        for name, src, expect_flag in SELF_TESTS:
            f = Path(td) / "case.rs"
            f.write_text(src)
            got = bool(scan([f]))
            ok = got == expect_flag
            if not quiet or not ok:
                print(f"  [{'OK' if ok else 'FAIL'}] {name}")
            if not ok:
                failures += 1
                print(f"        expected flagged={expect_flag}, got {got}")
    if failures:
        print(f"\ncheck-test-precondition-guards self-test: {failures} case(s) FAILED")
        return 1
    if not quiet:
        print(
            f"\ncheck-test-precondition-guards self-test: {len(SELF_TESTS)} case(s) OK"
        )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--self-test",
        action="store_true",
        help="run the classifier's own cases verbosely and exit (they also run "
        "on every normal invocation)",
    )
    args = ap.parse_args()
    if args.self_test:
        return self_test(quiet=False)

    files = tracked_test_files()
    all_rs = tracked_rs_files()
    root = Path(
        subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                       text=True, check=True).stdout.strip()
    )
    arm, notes = apply_arm_baseline(scan([], all_rs), root)
    violations = assert_scope_is_the_whole_tree(files) + scan(files, []) + arm
    if violations:
        print("check-test-precondition-guards: FAIL\n")
        for v in violations:
            print(f"  {v}")
        print(
            "\nA `require_*`/`ensure_*`/`need_*`/`maybe_*` helper in a test file that\n"
            "returns `bool` or `Option<()>` hands its verdict to the caller. Issue 1135:\n"
            "four callers of one such helper wrote `if !guard() { return; }`, and a bare\n"
            "`return` from a `#[test]` is a PASS — so four ROS 2 param tests reported\n"
            "green on a host where our node never joined the graph.\n"
            "Return `()` and `nros_tests::skip!` (or `skip_class!`) inside the guard.\n"
            "A guard returning a real value (`Option<PathBuf>`, `TestResult<T>`) is fine\n"
            "— provided the CALLER's `else` arm skips or panics. Issue 1539: five\n"
            "`let Some(_router) = router() else { return };` in zenoh_integration.rs\n"
            "passed having run nothing on every host without zenohd."
        )
        return 1
    for n in notes:
        print(f"check-test-precondition-guards: note: {n}")
    dirs = {f.parent.as_posix() for f in files}
    outside = len([f for f in files if "packages/testing/" not in f.as_posix()])
    print(
        f"check-test-precondition-guards: OK ({len(files)} test files in "
        f"{len(dirs)} dirs; {outside} outside packages/testing/; `else`-arm rule "
        f"over {len(all_rs)} tracked .rs)"
    )
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test(quiet=False))
    # Always, not only behind the flag. A negative control that only runs when
    # someone remembers to ask for it decays into a comment, and a gate whose
    # classifier has quietly stopped classifying reports OK over nothing —
    # which is the same false green this gate exists to refuse.
    if self_test():
        sys.exit(1)
    sys.exit(main())
