#!/usr/bin/env python3
"""A matcher-predicate must be fed a HERE-STRING, never a pipe.  Issue 1077.

    if ! printf '%s' "$out" | grep -q "status=refused"; then

Bash's builtin `printf` flushes PER LINE, and a `-q` grep exits at the FIRST
match.  So whenever the needle is on any line but the last, grep can close the
read end while the writer still has a line to send; the writer takes SIGPIPE,
and `set -o pipefail` promotes that 141 to the pipeline's status.  `if ! …`
then reads a SUCCESSFUL match as a MISS.  Traced on 2026-09-05:

    write(1, "    demo/msg/Open (unbounded)\\n", 30) = -1 EPIPE (Broken pipe)
    --- SIGPIPE ---  +++ killed by SIGPIPE +++     PIPESTATUS=(141 0)

Measured at 60 of 3000 calls (2%) on a three-line haystack on an IDLE 12-core
host — and 13 of 300 runs of the gate that carried it, against 0 of 300 for the
here-string.  That was `check-fast` flaking on four different gates over four
runs, every flake wearing a considered assertion message that pointed at real
code: "a subscribed type with no bound did NOT refuse", "first pass did not
read the placeholder".  Issue 0876's shape — a red that is not a verdict, and
articulate enough to survive a reviewer's judgement.

SIBLING, NOT DUPLICATE.  `check-grep-q-error-conflation` (issue 0726) is about
the same call's EXIT STATUS: grep answers 0 / 1 / >=2 and `if ! …` folds the
error into "no match", so that gate routes the sites through `nros_grep_q`.
It explicitly skips any line naming `nros_grep_q` — which is right for the
status question and blind to this one, because

    printf '%s' "$out" | nros_grep_q PAT

still puts a writer behind a reader that exits early, and still returns 141.
Worse, the helper's fatal path (`exit 2` on a tool failure) runs in the
PIPELINE'S SUBSHELL and cannot stop the caller.  So the two rules are
independent: use `nros_grep_q`, AND feed it `<<<`.

WHAT IS FLAGGED: a pipeline whose LAST stage is a matcher predicate that can
stop reading before EOF (`grep -q`, `grep -m N`, `nros_grep_q`), in a position
where the pipeline's STATUS is read (`if`/`elif`/`while`/`until`/`!`/`&&`/
`; then`/`|| continue`), in a file that enables `pipefail`.

Two shapes are deliberately NOT flagged.  A pipeline in VALUE position
(`x="$(a | head -1)"`) discards its status, so the value it produces is
unaffected; `head` is only a hazard when a predicate reads its status, and
`… | head -1 | grep -q X` is caught by its last stage.  And a pipeline whose
status is already defused (`… || true`) has said so on purpose.

THE FIX is to capture and then match without a pipe:

    out="$(some-command 2>&1)"
    if ! nros_grep_q 'needle' <<<"$out"; then …

A here-string has no writer PROCESS — bash fills the descriptor before grep is
started — so the race cannot exist.  Measured 0 of 5000.

There is NO allowlist (issue 2090).  There was one, a ratchet of 30 sites
that predated this gate and were said to "sit outside the `check-fast` lane
where this was measured".  Two of them were in `check-artifact-identity-budget`,
whose self-test runs IN `check-fast`, and that gate's case (c) went red on main
three times over two weeks with exactly this race: the identity-budget gate's
`! printf '%s\n' "$rlibs" | grep -q …` read a match as a miss, took its
"crate not rebuilt" widening, and dropped the wording the case asserts.  A
ratchet is only as true as the claim that each entry is harmless, and nobody
re-checks that claim, so every site was converted and the list deleted.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import workflow_commands  # noqa: E402  phase-472 W1 — the CI shell population
import shell_statements  # noqa: E402  issue 1738
from shell_statements import statements  # noqa: E402

# A last stage that stops reading before EOF: a MATCHER PREDICATE, whose only
# product is an exit status, so it stops the moment it knows the answer.
EARLY_EXIT_PREDICATE = re.compile(
    r"^\s*(?:"
    r"nros_grep_q\b"                                # the sanctioned helper,
    r"|nros_git_grep_q\b"                           # piped into anyway
    r"|(?:command\s+)?(?:git\s+)?grep\s+(?:"
    r"-[A-Za-z]*q[A-Za-z]*\b|--quiet\b"             # grep -q / -qE / -qF / -qx
    r"|-[A-Za-z]*m[A-Za-z]*\s*\d|--max-count"       # grep -m N
    r")"
    r")"
)

# The pipeline's exit status is READ rather than discarded. Decided per
# STATEMENT (`scripts/lib/shell_statements.py`, issue 1738): a one-line
# `if …; then …; fi` used to hand the whole tail (`; then echo n; fi; }`) to
# `last_pipeline_stage`, so the matcher was never the last stage. A bare `||`
# is absent on purpose: `… || true` / `… || :` is the status being defused
# deliberately, and `… || continue` reaches us through its own arm.
STATUS_HEAD = re.compile(r"^(?:if|elif|while|until|!)\s")
OR_CONSUMER = re.compile(r"^(?:continue|break|return|exit|fail)\b")
CONTINUED = re.compile(r"\\\s*$")  # continued into an && / ; then


def status_consumed(stmts, k: int, line: str) -> bool:
    st = stmts[k]
    if STATUS_HEAD.match(st.text):
        return True
    nxt = stmts[k + 1] if k + 1 < len(stmts) else None
    if nxt is not None and nxt.lead.split()[:1] in (["then"], ["do"]):
        return True  # `cond; then` — a condition continued from the line above
    if "&&" in (st.sep_before, st.sep_after):
        return True
    if st.sep_after == "||" and nxt is not None and OR_CONSUMER.match(nxt.text):
        return True
    return nxt is None and bool(CONTINUED.search(line))


def last_pipeline_stage(line: str) -> str:
    """The text after the final single `|`, ignoring `||` and quoted text.

    Crude on purpose: this reads one LINE of shell, not a parse tree. It is
    enough to tell `a | head -1 | sed …` (a value) from `a | head -1 | grep -q X`
    (a predicate), and to keep a `|` inside a pattern — `grep -qE 'a|b'` — from
    reading as a pipe. Those two are the distinctions that decide whether a
    SIGPIPE on the writer can be mistaken for a verdict.
    """
    out: list[str] = []
    i = 0
    quote = ""          # "'" or '"' while inside a quoted run
    while i < len(line):
        c = line[i]
        if quote:
            # Inside single quotes bash has no escapes at all; inside double
            # quotes a backslash escapes the next byte.
            if quote == '"' and c == "\\" and i + 1 < len(line):
                out.append(c)
                out.append(line[i + 1])
                i += 2
                continue
            if c == quote:
                quote = ""
            out.append(c)
            i += 1
            continue
        if c == "\\" and i + 1 < len(line):
            out.append(c)
            out.append(line[i + 1])
            i += 2
            continue
        if c in ("'", '"'):
            quote = c
            out.append(c)
            i += 1
            continue
        if c == "|":
            if i + 1 < len(line) and line[i + 1] == "|":
                out.append("||")
                i += 2
                continue
            out.append("\x00")
            i += 1
            continue
        out.append(c)
        i += 1
    joined = "".join(out)
    if "\x00" not in joined:
        return ""
    return joined.rsplit("\x00", 1)[1]


def flags(line: str) -> bool:
    """Does this ONE line carry the defect, in any of its statements?"""
    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        return False
    stmts = statements(line)
    for k, st in enumerate(stmts):
        stage = last_pipeline_stage(st.text)
        if stage and EARLY_EXIT_PREDICATE.match(stage) and status_consumed(stmts, k, line):
            return True
    return False


# The detector is a regex over one line of shell, so it can stop matching
# without anyone noticing and report a clean sweep over a class it no longer
# sees — issue 0196's rule failing in miniature. These hold it to BOTH
# directions on the exact shapes that motivated it.
SELF_TEST = (
    # (line, expected-to-be-flagged, why)
    ("""if ! printf '%s' "$_o_out" | grep -q "status=refused"; then""", True,
     "the literal line from issue 1077"),
    ("""elif printf '%s' "$out" | grep -qE 'is required|must also be'; then""", True,
     "an alternation `|` inside the PATTERN is not a pipe"),
    ("""    if ! "$m" --version | head -1 | grep -q "4.4"; then""", True,
     "an early-exit predicate behind an early-exit `head`"),
    ("""    printf '%s' "$b" | grep -qE 'add_executable\\(' || continue""", True,
     "`|| continue` reads the status just as `if` does"),
    ("""if ! printf '%s' "$out" | nros_grep_q 'needle'; then""", True,
     "the 0726 helper does not survive being piped into either"),
    ("""x="$(grep -E '^t' "$f" | head -1 | sed 's/a/b/')" """, False,
     "value position: the status is discarded and the value is unaffected"),
    ("""ver="$(qemu --version | head -1)" """, False,
     "value position with an early-exit last stage: still only a value"),
    ("""    variant="$(grep -oE 'x' "$h" | head -1 || true)" """, False,
     "`|| true` is the status defused on purpose"),
    ("""if ! nros_grep_q "status=refused" <<<"$_o_out"; then""", False,
     "the FIX must not trip the gate that asked for it"),
    # issue 1738 — one-line compounds: the matcher is the last stage of its
    # STATEMENT, not of the line.
    ("""_f() { if ! printf '%s' "$1" | grep -q x; then echo n; fi; }""", True,
     "a one-line `if` inside a one-line function"),
    ("""for f in a b; do printf '%s' "$f" | grep -q x && echo hit; done""", True,
     "`&&` after the matcher inside a one-line loop"),
    ("""if [ -n "$x" ]; then printf '%s' "$x" | grep -q y || true; fi""", False,
     "`|| true` still defuses it inside a one-line `if`"),
    ("""if true; then v="$(a | grep -m1 x)"; fi""", False,
     "value position inside a one-line `if`"),
)


def self_test() -> list[str]:
    problems: list[str] = list(shell_statements.self_test())
    if not any(f.startswith(".github/actions/") for f in tracked_shell()):
        problems.append("the population does not reach .github/actions/ (phase-472 W1)")
    for line, expected, why in SELF_TEST:
        if flags(line) != expected:
            problems.append(
                f"expected {'a HIT' if expected else 'no hit'} ({why}) on: {line.strip()}"
            )
    return problems


def tracked_shell() -> list[str]:
    """Tracked shell we author: `*.sh`, plus the `just` recipe bodies, which
    are bash too and run in the same fan-out."""
    # phase-472 W1 — CI shell too: a workflow or composite action `run:` is
    # bash under `-o pipefail` on the runner, the exact setting issue 1077 is
    # about, and `.github/` was never read.
    out = subprocess.run(
        ["git", "ls-files", "*.sh", "*.just", "justfile", *workflow_commands.ci_pathspecs()],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return sorted(
        f for f in out
        if "/third-party/" not in f and "/generated/" not in f
    )


def scan(root: Path) -> list[str]:
    """Return the violations."""
    me = Path(__file__).resolve()
    violations: list[str] = []
    for rel in tracked_shell():
        path = root / rel
        if path.resolve() == me:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        # Only a file that turns on pipefail can suffer this: without it the
        # pipeline reports the matcher's status alone, which is the right
        # answer whatever the writer did.
        #
        # A CI step with an explicit `shell: bash` runs `bash -eo pipefail {0}`
        # (GitHub's own expansion), and a composite action MUST name its shell,
        # so a CI file that says `shell: bash` has pipefail on without the word
        # appearing (phase-472 W1).
        if "pipefail" not in text and not (
            rel.startswith(".github/") and re.search(r"^\s*(?:-\s*)?shell:\s*bash\s*$", text, re.M)
        ):
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            if not flags(line):
                continue
            stripped = line.strip()
            violations.append(f"{rel}:{lineno}: {stripped[:110]}")
    return violations


def main() -> int:
    problems = self_test()
    if problems:
        print(
            "check-pipefail-sigpipe-assertions: SELF-TEST FAILED — the detector "
            "no longer recognises the shape it exists to find, so a green here "
            "would mean nothing.",
            file=sys.stderr,
        )
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1

    root = Path(__file__).resolve().parent.parent
    violations = scan(root)

    if violations:
        print(
            "check-pipefail-sigpipe-assertions: FAILED (issue 1077) — "
            f"{len(violations)} pipeline(s) whose status is read can report a "
            "MATCH as a MISS.",
            file=sys.stderr,
        )
        print(file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        print(
            "\n"
            "  Under `set -o pipefail` a matcher that exits early (grep -q,\n"
            "  grep -m, nros_grep_q) can kill the writer with SIGPIPE, and 141\n"
            "  then becomes the pipeline's status — so a SUCCESSFUL match reads\n"
            "  as a failure, intermittently, under load.\n"
            "\n"
            "  Capture first, then match with a HERE-STRING:\n"
            "\n"
            "      out=\"$(some-command 2>&1)\"\n"
            "      if ! nros_grep_q 'needle' <<<\"$out\"; then ...\n"
            "\n"
            "  `nros_grep_q` is scripts/lib/grep-q.sh (issue 0726). Piping INTO\n"
            "  it does not help: the helper is then a subshell, so its own\n"
            "  fatal path cannot stop the caller either.",
            file=sys.stderr,
        )
        return 1

    print(
        "check-pipefail-sigpipe-assertions: OK "
        "(no status-consuming pipeline into an early-exit matcher)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
