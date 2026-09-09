#!/usr/bin/env python3
"""A status you mean to INSPECT may not be captured by a bare assignment.  Issue 1249.

    out="$(cmd)"        # <- under `set -e`, a non-zero `cmd` ENDS THE SCRIPT HERE
    rc=$?               # <- never runs; and if it did it would read 0
    if [ "$rc" -eq 2 ]; then ...

Two independent defects landed on 2026-09-09 wearing exactly this shape, in
two different files, neither author aware of the other:

  * `scripts/build/workspace-fixtures-build.sh` (PR #780).  A `find` was handed
    a doubled path that has never existed, exited 1 with no output, and
    `set -euo pipefail` took the whole script down AT THE ASSIGNMENT -- so the
    `if [ -n "$_sf_elf" ]` written directly beneath it, whose entire purpose was
    to tolerate an absent elf, had never executed once.  The workspace fixture
    build died on its first entry with `error: recipe ... failed on line 234`
    and no message of its own; a stack-floor gate had therefore never run.

  * `.githooks/pre-push` (PR #798).  `out="$("$reach" --changed "$sha" 2>&1)"`
    followed by `rc=$?` and a three-outcome `case`.  The rc=2 arm existed to say
    "could not ask the remotes" and LET THE PUSH THROUGH.  Because the hook runs
    under `set -euo pipefail`, a `$reach` exiting 2 killed the hook at the
    assignment instead: every such push was REFUSED WITH ZERO OUTPUT, which
    teaches `--no-verify` and so bypasses every other guard in the hook.

Same idiom, same day, same silence.  That is a class, not two bugs.

WHY IT IS INVISIBLE ON REVIEW.  The handling code is right there, three lines
down, correct, and reads as coverage.  Nothing in the diff says the shell will
never reach it.  And the failure has no signature of its own: `set -e` prints
nothing, so the symptom is the caller's generic "recipe failed" or, worse, an
exit status with no output at all.  Both authors above wrote a considered
comment about the failure mode they were handling, above code that could not
run.

WHAT IS FLAGGED -- two rules, and each keys on the author's OWN evidence of
intent, never on the mere presence of a bare assignment:

  RULE 1 (status).  A bare `var="$(cmd)"` whose IMMEDIATELY NEXT statement is a
  pure status capture (`rc=$?`).  This needs no judgement about `cmd`: after a
  bare assignment under `set -e`, `$?` is either unreachable or a constant 0, so
  the line is dead or lying whatever ran.  Zero false positives by construction.

  RULE 2 (emptiness).  A bare assignment whose head command reports "nothing
  found" as a NON-ZERO EXIT, followed by a test of that same variable for
  emptiness.  `find` on a missing root, `grep` with no match, `command -v` on an
  absent tool: the empty case the author is handling is exactly the case the
  shell will not survive to hand them.  The command list (`EMPTY_IS_NONZERO`) is
  deliberately closed and small -- see its comment.

WHAT IS NOT FLAGGED, ON PURPOSE.  **Not every bare assignment is a bug.**

  * `ver="$(awk ... f)"` then `[ -n "$ver" ]`.  awk/sed/basename/printf report
    "nothing matched" as exit 0 WITH EMPTY OUTPUT, so the emptiness test is
    reachable and the code is correct as written.  If the input file is missing
    awk does exit non-zero -- and aborting is then the RIGHT answer, because a
    missing input is a broken precondition, not a result.  This is the negative
    control the gate must keep passing.
  * `dir="$(mktemp -d)"`, `root="$(git rev-parse --show-toplevel)"` with no
    inspection after them.  A failure there SHOULD abort; that is what `set -e`
    is for, and rewriting these would be the mass-rewrite this gate exists to
    avoid.
  * Anything already spelled safely: `x="$(cmd)" || rc=$?`, `x="$(cmd)" || true`,
    `if x="$(cmd)"; then`.  An assignment inside an `if` condition is exempt from
    `set -e` by the shell itself, which is why that spelling is a fix.
  * A recipe or file that has turned errexit OFF (`set +e`).  `just workspace
    doctor` does exactly this on purpose and reads `rustup_rc=$?` legitimately.

THE FIX, either spelling:

    rc=0
    out="$(cmd)" || rc=$?

    if out="$(cmd)"; then ...; else ...; fi

A NOTE ON `local`.  `local out="$(cmd)"` does not abort -- `local` is the
command, and its own status (0) is what `set -e` and a following `$?` both see.
That is the same lie with a quieter delivery, so RULE 1 flags it too and names
the distinction.  Declare first, assign on its own line, then capture.

EXEMPTIONS have ONE spelling: the `ALLOWLIST` below, keyed by line text, with a
reason on every entry.  Per-site subsets are how issue 0442 happened.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Exemptions.  ONE spelling, keyed by the assignment's LINE TEXT (not a line
# number -- an insertion above a listed site must not re-point every entry).
# A reason is mandatory.  A stale entry is a FAILURE, not a silent no-op: it
# silences a line that has moved and reads as though it still guards something.
# ---------------------------------------------------------------------------
ALLOWLIST: dict[str, tuple[tuple[str, str], ...]] = {}

# Head commands whose "nothing found" IS a non-zero exit, so an emptiness test
# written after them can never see the case it handles.  Closed and small on
# purpose -- every entry is a command where empty-output and failure are the
# SAME event:
#
#   find      a search root that does not exist  -> 1, no output   (PR #780)
#   grep      no match                           -> 1, no output
#   ls        a path that does not exist         -> 2, no output
#   command -v / which / type / hash   absent    -> 1, no output
#   readlink / realpath   a path that is not there -> 1, no output
#   pkg-config            module not installed   -> 1, no output
#
# NOT in here, and deliberately: awk, sed, basename, dirname, cut, tr, printf.
# Those answer "nothing" with exit 0, so the author's emptiness test is live and
# the code is right.  Adding one of them would turn this gate into the
# mass-rewrite it exists to avoid.
EMPTY_IS_NONZERO = (
    "find",
    "grep", "egrep", "fgrep", "rg",
    "ls",
    "command", "which", "type", "hash",
    "readlink", "realpath",
    "pkg-config",
)

# A bare assignment whose right-hand side is a command substitution. `local`
# and friends are captured so RULE 1 can name the masking variant.
#
# `arm` allows a `case` PATTERN in front of it. That is not decoration: the real
# PR #798 defect was `*[!0]*) out="$("$reach" --changed "$sha" 2>&1)" ;;`, and a
# detector anchored at column-plus-indent could not see it. Nothing else may
# precede an assignment here -- a `&&`/`||`/`;` prefix means the status is
# already being read, which NOT_STATEMENT_START rejects.
ASSIGN = re.compile(
    r"^(?P<indent>[ \t]*)"
    r"(?P<arm>[^ \t()&|;]+\)[ \t]+)?"
    r"(?P<decl>(?:local|declare|typeset|export|readonly)[ \t]+(?:-[A-Za-z]+[ \t]+)*)?"
    r"(?P<var>[A-Za-z_][A-Za-z0-9_]*)="
    r"(?P<rhs>\"?\$\(|\"?`)"
)

# Block CLOSERS, skipped when looking for the statement whose `$?` reads the
# assignment. `rc=$?` after `fi` / `esac` / `done` reads the last command the
# compound ran, which is the assignment -- and that is exactly how PR #798 was
# written: three arms of a `case`, the `rc=$?` beneath `esac`. Anchoring on
# "the literally next line" made the gate blind to the file it was written for
# (measured: the mutation survived).
#
# `else` and `elif` are NOT here on purpose: skipping them would pair an
# assignment in the THEN branch with a `$?` that belongs to the ELSE branch's
# last command. The else-branch assignment is flagged on its own.
BLOCK_CLOSER = re.compile(r"^[ \t]*(?:fi|esac|done|\}|;;&?|;&|\))[ \t]*(?:#.*)?$")

# The statement is not at statement position -- it is a condition, a loop head,
# or the right arm of an operator, all of which the shell already exempts.
NOT_STATEMENT_START = re.compile(r"^[ \t]*(?:if|elif|while|until|!|then|do|&&|\|\|)\b")

# A pure status capture: the whole statement is `[local ]var=$?`.
STATUS_CAPTURE = re.compile(
    r"^[ \t]*(?:local[ \t]+|declare[ \t]+)?[A-Za-z_][A-Za-z0-9_]*=\$\?[ \t]*(?:;.*)?$"
)

# errexit on / off. Parsed rather than pattern-matched so `set -uo pipefail`
# (no `e`) and `set -o errexit` both land on the right answer.
SET_DIRECTIVE = re.compile(r"^[ \t]*set[ \t]+(?P<args>[-+].*)$")

# `#!/usr/bin/env bash` opening a `just` recipe body. Without a shebang a just
# recipe runs each LINE in its own shell, so a `$?` on the next line reads a
# different process entirely -- a distinct defect, and not this one.
BASH_SHEBANG = re.compile(r"^[ \t]*#![ \t]*(?:/usr/bin/env[ \t]+bash|/bin/bash|/usr/bin/bash)\b")

# A `just` recipe header at column 0 (`name:`, `name arg:`, `name: dep`).
JUST_RECIPE_HEADER = re.compile(r"^@?[a-zA-Z_][a-zA-Z0-9_-]*(?:[ \t]+[^:]*)?:(?:[ \t].*)?$")


def set_options(args: str) -> dict[str, bool]:
    """Parse one `set` line into {option-name: on}.

    Bash bundles short flags and lets `-o` take its argument from the NEXT
    token, so `set -euo pipefail` is `errexit`, `nounset` and `pipefail` in one
    line -- the spelling this repo uses almost everywhere. A parser that only
    recognised a standalone `-o` read `-euo pipefail` as neither, which quietly
    switched RULE 2 to its narrow (no-pipefail) reading for 8 of 8 real sites.
    """
    letters = {"e": "errexit", "u": "nounset", "x": "xtrace", "f": "noglob"}
    tokens = args.split()
    out: dict[str, bool] = {}
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if not tok.startswith(("-", "+")):
            i += 1
            continue
        on = tok.startswith("-")
        for ch in tok[1:]:
            if ch == "o":
                if i + 1 < len(tokens) and not tokens[i + 1].startswith(("-", "+")):
                    out[tokens[i + 1]] = on
                    i += 1
            elif ch in letters:
                out[letters[ch]] = on
        i += 1
    return out


def errexit_delta(args: str) -> bool | None:
    """Does this `set` line turn errexit on (True), off (False) or neither?"""
    return set_options(args).get("errexit")


def pipefail_delta(args: str) -> bool | None:
    """Does this `set` line turn pipefail on (True), off (False) or neither?

    Load-bearing for RULE 2, not decoration. WITHOUT pipefail a pipeline's
    status is its LAST stage's, so `v="$(grep x f | cut -d'"' -f2)"` cannot
    abort however grep exits -- `cut` answers 0 -- and the emptiness test below
    it is perfectly live. Flagging those would be the mass-rewrite this gate is
    written to avoid.
    """
    return set_options(args).get("pipefail")


def join_logical(lines: list[str], start: int) -> tuple[str, int]:
    """Join backslash continuations and an unbalanced `$(` run.

    Returns (joined text, index of the line AFTER the statement). Crude on
    purpose: this reads shell as text, not as a parse tree. It only has to tell
    where the assignment ENDS so the next statement can be identified.
    """
    text = lines[start]
    i = start
    while i + 1 < len(lines):
        stripped = text.rstrip()
        unbalanced = text.count("$(") + text.count("`") % 2 > text.count(")")
        if not stripped.endswith("\\") and not unbalanced:
            break
        i += 1
        text += "\n" + lines[i]
    return text, i + 1


def pipeline_heads(rhs: str) -> list[str]:
    """First word of each pipeline stage inside the command substitution.

    Quote-aware so an alternation inside a pattern (`grep -E 'a|b'`) is not read
    as a pipe -- the same distinction `check-pipefail-sigpipe-assertions` draws.
    """
    # strip to the inside of the outermost $( ... )
    start = rhs.find("$(")
    if start < 0:
        start = rhs.find("`")
        if start < 0:
            return []
        body = rhs[start + 1:]
    else:
        body = rhs[start + 2:]
    stages: list[str] = []
    cur: list[str] = []
    quote = ""
    depth = 0
    i = 0
    while i < len(body):
        c = body[i]
        if quote:
            if quote == '"' and c == "\\" and i + 1 < len(body):
                i += 2
                continue
            if c == quote:
                quote = ""
            cur.append(c)
            i += 1
            continue
        if c == "\\" and i + 1 < len(body):
            i += 2
            continue
        if c in ("'", '"'):
            quote = c
            i += 1
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            if depth == 0:
                break
            depth -= 1
        elif c == "|" and depth == 0:
            if i + 1 < len(body) and body[i + 1] == "|":
                cur.append("||")
                i += 2
                continue
            stages.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    stages.append("".join(cur))

    heads: list[str] = []
    for stage in stages:
        words = stage.split()
        # skip env assignments and wrappers that pass the status through
        k = 0
        while k < len(words) and (
            re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", words[k])
            or words[k] in ("env", "timeout", "nice", "ionice", "stdbuf", "cd")
        ):
            k += 1
            if k and words[k - 1] == "timeout" and k < len(words):
                k += 1  # timeout's duration argument
        if k < len(words):
            heads.append(Path(words[k].strip('"\'')).name)
    return heads


def empties_var(line: str, var: str) -> bool:
    """Does this statement test `var` for emptiness, or `case` on it?"""
    v = re.escape(var)
    return bool(
        re.search(r"\[\[?[ \t]+-[nz][ \t]+\"?\$\{?" + v + r"\b", line)
        or re.search(r"^[ \t]*(?:if|elif)?[ \t]*case[ \t]+\"?\$\{?" + v + r"\b", line)
    )


class Finding(tuple):
    pass


def scan_text(rel: str, text: str) -> list[tuple[int, str, str, str]]:
    """Return (lineno, rule, statement, evidence) for each violation in one file."""
    lines = text.splitlines()
    # A workflow `run:` block is `bash -e {0}` by GitHub's own default, so
    # errexit is on for every line of every run block whether or not anyone
    # wrote `set -e`. Nothing else in the file is shell.
    is_workflow = rel.startswith(".github/workflows/")
    is_just = rel.endswith(".just") or Path(rel).name == "justfile"

    findings: list[tuple[int, str, str, str]] = []
    # `errexit`/`pipefail` are tracked in LINE ORDER, which is what a reader
    # sees. bash's own options are process-global rather than block-scoped, so
    # this is an approximation; it is exact for the shapes that matter here
    # (a prologue near the top, and `just workspace doctor`'s deliberate
    # `set +e`).
    errexit = is_workflow
    pipefail = False
    just_errexit = False
    just_pipefail = False
    in_just_body = False

    i = 0
    while i < len(lines):
        line = lines[i]

        if is_just:
            if line and not line[0].isspace() and not line.startswith("#"):
                # left a recipe body; a header opens a new one
                in_just_body = bool(JUST_RECIPE_HEADER.match(line))
                just_errexit = False
                just_pipefail = False
                i += 1
                continue
            if in_just_body and BASH_SHEBANG.match(line):
                just_errexit = False
                just_pipefail = False

        m_set = SET_DIRECTIVE.match(line)
        if m_set:
            delta = errexit_delta(m_set.group("args"))
            if delta is not None:
                if is_just:
                    just_errexit = delta
                else:
                    errexit = delta
            pf = pipefail_delta(m_set.group("args"))
            if pf is not None:
                if is_just:
                    just_pipefail = pf
                else:
                    pipefail = pf
            i += 1
            continue

        active = just_errexit if is_just else errexit
        pipefail_active = just_pipefail if is_just else pipefail
        stripped = line.strip()
        if not active or not stripped or stripped.startswith("#"):
            i += 1
            continue

        m = ASSIGN.match(line)
        if not m or NOT_STATEMENT_START.match(line):
            i += 1
            continue

        statement, nxt = join_logical(lines, i)
        # Already spelled safely, or its status is deliberately discarded.
        if re.search(r"\|\||&&", statement.split("=", 1)[1]):
            i = nxt
            continue

        var = m.group("var")
        decl = (m.group("decl") or "").strip()

        # --- next effective statement ---
        #
        # Two searches, because the two rules ask different questions. The
        # EMPTINESS test must be the very next statement (an emptiness check
        # further away is about something else). The `$?` read may sit behind
        # any number of block closers, because that is what `rc=$?` after
        # `esac` means: the status of the last command the compound ran.
        j = nxt
        while j < len(lines) and (not lines[j].strip() or lines[j].strip().startswith("#")):
            j += 1
        following = lines[j] if j < len(lines) else ""

        k = j
        while k < len(lines) and (
            not lines[k].strip()
            or lines[k].strip().startswith("#")
            or BLOCK_CLOSER.match(lines[k])
        ):
            k += 1
        after_closers = lines[k] if k < len(lines) else ""
        if not STATUS_CAPTURE.match(following) and STATUS_CAPTURE.match(after_closers):
            following, j = after_closers, k

        rule = evidence = None
        if STATUS_CAPTURE.match(following):
            if decl in ("local", "declare", "typeset", "export", "readonly"):
                rule = "status-masked"
                evidence = (
                    f"{j + 1}: {following.strip()}  -- `{decl}` is the command, so "
                    f"$? is ALWAYS 0 here"
                )
            else:
                rule = "status"
                evidence = f"{j + 1}: {following.strip()}  -- unreachable under set -e"
        elif empties_var(following, var):
            heads = pipeline_heads(statement.split("=", 1)[1])
            # Without pipefail only the LAST stage decides the status.
            candidates = heads if pipefail_active else heads[-1:]
            hazard = next((h for h in candidates if h in EMPTY_IS_NONZERO), None)
            if hazard:
                rule = "emptiness"
                evidence = (
                    f"{j + 1}: {following.strip()}  -- `{hazard}` reports "
                    f"'nothing found' as a non-zero exit"
                )

        if rule:
            first = statement.splitlines()[0].strip()
            allowed = {t for t, _ in ALLOWLIST.get(rel, ())}
            if first not in allowed:
                findings.append((i + 1, rule, first, evidence or ""))

        i = nxt

    return findings


# ---------------------------------------------------------------------------
# Self-test. Runs on the NORMAL path, every invocation: a negative control
# nobody runs decays into a comment (check-board-tiers.py). The two REAL cases
# are here in both shapes -- the before-shape must FAIL, the after-shape must
# PASS -- because a detector that stops recognising the class it was written for
# would otherwise report a clean sweep over a class it can no longer see.
# ---------------------------------------------------------------------------
SELF_TEST: tuple[tuple[str, str, int, str], ...] = (
    (
        "PR #780 BEFORE — workspace-fixtures-build.sh, the shape that shipped",
        "scripts/build/fixture.sh",
        1,
        """#!/usr/bin/env bash
set -euo pipefail
build_one() {
    local _sf_elf
    _sf_elf="$(find "$NROS_REPO_ROOT/$dir/$out_root" -type f -name "$entry" \\
        -path "*/$row_profile_dir/*" -print -quit 2>/dev/null)"
    if [ -n "$_sf_elf" ]; then
        python3 scripts/check-stack-floor.py --board-for-row "$platform" "$_sf_elf"
    fi
}
""",
    ),
    (
        "PR #780 AFTER — `|| true`, because an absent elf is a legitimate state",
        "scripts/build/fixture.sh",
        0,
        """#!/usr/bin/env bash
set -euo pipefail
build_one() {
    local _sf_elf
    _sf_elf="$(find "$NROS_REPO_ROOT/$dir/$out_root" -type f -name "$entry" \\
        -path "*/$row_profile_dir/*" -print -quit 2>/dev/null || true)"
    if [ -n "$_sf_elf" ]; then
        python3 scripts/check-stack-floor.py --board-for-row "$platform" "$_sf_elf"
    fi
}
""",
    ),
    (
        # VERBATIM the shape that shipped: three `case` arms, the `rc=$?`
        # beneath `esac`. An earlier draft of this self-test used a flattened
        # two-line version and PASSED while the real file's mutation SURVIVED —
        # the distance between the assignment and the `$?`, and the `case`
        # pattern in front of it, are the whole difficulty.
        "PR #798 BEFORE — .githooks/pre-push, the rc=2 arm that had never run",
        ".githooks/pre-push",
        1,
        """#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
    case "$remote_sha" in
        *[!0]*) out="$("$reach" --changed "$remote_sha" 2>&1)" ;;
        *)
            if [ -n "$base" ]; then
                out="$("$reach" --changed "$base" 2>&1)"
            else
                out="$("$reach" 2>&1)"
            fi
            ;;
    esac
    rc=$?
    if [ "$rc" -eq 2 ]; then
        echo "pre-push: submodule reachability NOT verified (no network)." >&2
        continue
    fi
done
""",
    ),
    (
        "PR #798 AFTER — `rc=0; ... || rc=$?` on every arm, `rc=$?` deleted",
        ".githooks/pre-push",
        0,
        """#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
    rc=0
    case "$remote_sha" in
        *[!0]*) out="$("$reach" --changed "$remote_sha" 2>&1)" || rc=$? ;;
        *)
            if [ -n "$base" ]; then
                out="$("$reach" --changed "$base" 2>&1)" || rc=$?
            else
                out="$("$reach" 2>&1)" || rc=$?
            fi
            ;;
    esac
    if [ "$rc" -eq 2 ]; then
        echo "pre-push: submodule reachability NOT verified (no network)." >&2
        continue
    fi
done
""",
    ),
    (
        "a `$?` behind a block closer still reads the assignment (`fi`, `done`)",
        "scripts/thing.sh",
        1,
        """#!/usr/bin/env bash
set -euo pipefail
if [ -n "$base" ]; then
    out="$(some-probe "$base")"
fi
rc=$?
echo "$rc"
""",
    ),
    (
        "NEGATIVE CONTROL — a failure that SHOULD abort is correct as a bare assignment",
        "scripts/thing.sh",
        0,
        """#!/usr/bin/env bash
set -euo pipefail
repo_root="$(git rev-parse --show-toplevel)"
tmpdir="$(mktemp -d)"
cd "$repo_root"
""",
    ),
    (
        "NEGATIVE CONTROL — awk answers 'nothing' with exit 0, so `[ -n ]` is live",
        ".github/workflows/docs.yml",
        0,
        """      run: |
        ver="$(awk '/^\\[tool\\.mdbook\\]/{f=1;next} f && /^version/{print;exit}' nros-sdk-index.toml)"
        [ -n "$ver" ] || { echo "could not read the version" >&2; exit 1; }
""",
    ),
    (
        "NEGATIVE CONTROL — errexit turned OFF on purpose (`just workspace doctor`)",
        "just/workspace.just",
        0,
        """doctor:
    #!/usr/bin/env bash
    set +e
    list_out="$(timeout 5s rustup toolchain list 2>/dev/null)"
    rustup_rc=$?
    echo "$rustup_rc"
""",
    ),
    (
        "NEGATIVE CONTROL — an assignment inside `if` is exempt from set -e by the shell",
        "scripts/thing.sh",
        0,
        """#!/usr/bin/env bash
set -euo pipefail
if out="$(some-probe)"; then
    echo "$out"
else
    echo "probe declined" >&2
fi
""",
    ),
    (
        "`local x=$(cmd)` does not abort — it makes $? a constant 0, same lie",
        "scripts/thing.sh",
        1,
        """#!/usr/bin/env bash
set -euo pipefail
probe() {
    local out="$(some-probe --ask)"
    local rc=$?
    [ "$rc" -eq 0 ] || return 1
}
""",
    ),
    (
        "a `grep` whose empty result is inspected — the #780 shape, other command",
        "scripts/thing.sh",
        1,
        """#!/usr/bin/env bash
set -euo pipefail
hits="$(grep -n 'needle' "$file")"
if [ -z "$hits" ]; then
    echo "nothing to do"
fi
""",
    ),
    (
        "a GitHub `run:` block is `bash -e {0}` with no `set -e` written anywhere",
        ".github/workflows/gate.yml",
        1,
        """      run: |
        found="$(command -v some-tool)"
        if [ -z "$found" ]; then
          echo "not installed" >&2
        fi
""",
    ),
)


def self_test() -> list[str]:
    problems: list[str] = []
    for why, rel, expected, body in SELF_TEST:
        got = len(scan_text(rel, body))
        if got != expected:
            problems.append(f"expected {expected} finding(s), got {got}: {why}")
    return problems


def tracked_shell() -> list[str]:
    # `nros_clear_inherited_git_env` FIRST (issues 0986/0988): this gate is on
    # the fast line, so the `pre-push` hook reaches it, and a push from a linked
    # worktree exports `GIT_DIR` — which overrides both a path argument and
    # `git -C`. A `git ls-files` answering for the wrong repository would report
    # a clean sweep over a file set this gate never read.
    sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
    from git_hook_env import nros_clear_inherited_git_env  # noqa: PLC0415

    nros_clear_inherited_git_env()
    out = subprocess.run(
        ["git", "-C", str(Path(__file__).resolve().parent.parent),
         "ls-files", "*.sh", "*.just", "justfile", ".githooks/*", ".github/workflows/*"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return sorted(
        f for f in out
        if "/third-party/" not in f and "/generated/" not in f and not f.startswith("tmp/")
    )


def main() -> int:
    problems = self_test()
    if problems:
        print(
            "check-set-e-bare-assignment: SELF-TEST FAILED — the detector no "
            "longer recognises the shape it exists to find, so a green here "
            "would mean nothing.",
            file=sys.stderr,
        )
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1

    root = Path(__file__).resolve().parent.parent
    me = Path(__file__).resolve()

    violations: list[str] = []
    seen: set[tuple[str, str]] = set()
    examined = 0
    for rel in tracked_shell():
        path = root / rel
        if path.resolve() == me:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        examined += 1
        allowed = {t for t, _ in ALLOWLIST.get(rel, ())}
        for lineno, rule, statement, evidence in scan_text(rel, text):
            if statement in allowed:
                seen.add((rel, statement))
                continue
            violations.append(f"{rel}:{lineno}  [{rule}]\n      {statement}\n      -> {evidence}")

    stale = sorted(
        f"{rel}: {line}   ({why})"
        for rel, entries in ALLOWLIST.items()
        for line, why in entries
        if (rel, line) not in seen
    )
    if stale:
        print(
            "check-set-e-bare-assignment: the allowlist names "
            f"{len(stale)} line(s) that no longer match. A stale entry silences "
            "a line that has MOVED and reads as though it still guards "
            "something — delete it, or re-point it:",
            file=sys.stderr,
        )
        for site in stale:
            print(f"    {site}", file=sys.stderr)
        return 1

    if violations:
        print(
            "check-set-e-bare-assignment: FAILED (issue 1249) — "
            f"{len(violations)} command substitution(s) whose status is meant "
            "to be inspected, written as a bare assignment under `set -e`.",
            file=sys.stderr,
        )
        print(file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        print(
            "\n"
            "  Under `set -e` a bare `var=\"$(cmd)\"` whose command exits non-zero\n"
            "  ENDS THE SCRIPT AT THE ASSIGNMENT, with no message of its own. The\n"
            "  handling written beneath it — the `$?` read, the emptiness test,\n"
            "  the `case` on the status — never runs. Both PR #780 and PR #798\n"
            "  shipped this on the same day, each with a correct comment above\n"
            "  code the shell could not reach.\n"
            "\n"
            "  Capture the status, or put the assignment where the shell exempts it:\n"
            "\n"
            "      rc=0\n"
            "      out=\"$(cmd)\" || rc=$?\n"
            "\n"
            "      if out=\"$(cmd)\"; then ...; else ...; fi\n"
            "\n"
            "  If the failure genuinely SHOULD abort, it is not this class — but\n"
            "  then delete the handling below it, because it is dead code that\n"
            "  reads as coverage.\n"
            "\n"
            "  `local out=\"$(cmd)\"` is the quiet variant: `local` is the command,\n"
            "  so nothing aborts and `$?` is a constant 0. Declare first, assign\n"
            "  on its own line, then capture.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-set-e-bare-assignment: OK ({examined} shell file(s); no "
        "inspected command substitution written as a bare assignment under "
        f"set -e; {sum(len(v) for v in ALLOWLIST.values())} allowlisted)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
