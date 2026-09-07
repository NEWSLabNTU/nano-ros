#!/usr/bin/env python3
"""`cmake_language(DEFER ... CALL fn <args>)` must not pass an unexpanded `${...}`.

Issue 1132. DEFER stores its CALL arguments UNEXPANDED and expands them when the
deferred call runs, in the deferred directory's scope. A function-local variable
is gone by then, so the callee receives an EMPTY STRING:

    function(caller _tgt)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL callee "${_tgt}")
    endfunction()
    caller("my_target_name")
    -- DEFERRED CALLEE GOT: []

Measured on cmake 3.22, this repo's floor.

WHY THIS IS A GATE AND NOT A REVIEW NOTE.

The failure is silent in the worst way: the callee's own `if(NOT TARGET "")`
guard returns cleanly, which is indistinguishable from a guard that correctly
skipped a non-target. The one site this gate was written for
(`_nros_node_register_apply_config_header_deps`) had been a no-op for its whole
life, and the build stayed correct anyway because the STRONGER edge beside it --
an eager `OBJECT_DEPENDS` file dependency -- did the real work. So nothing
failed, nothing warned, and nothing asserted the mechanism ran. That is the
issue 0196 shape: a mechanism nobody can observe is indistinguishable from one
that was never written.

THE TWO CORRECT IDIOMS, both already in this tree:

  * no-argument call, state travels by GLOBAL property (preferred) --
    `_nros_node_register_schedule_inventory`, `NanoRosEntityFacts.cmake`;
  * `cmake_language(EVAL CODE "cmake_language(DEFER ... CALL fn [[${target}]])")`
    -- `NanoRosEntry.cmake`. This one works because EVAL expands the argument
    into a LITERAL before DEFER ever stores it. It is the only way to defer a
    call that genuinely needs an argument.

The EVAL form is not flagged: its top-level statement is `cmake_language(EVAL
CODE "...")`, and the DEFER inside is part of a quoted string, not a token of
the statement this gate reads.

Run: python3 scripts/check-deferred-call-args.py [--self-test]
"""

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]



def strip_comments(text):
    """Blank out `#` comments, preserving offsets so line numbers stay true.

    Needed because this gate's own docstring-adjacent cmake comments quote the
    broken form to explain it -- and the first version of this gate reported
    the COMMENT that describes the bug as an instance of the bug. A scanner
    that cannot tell code from prose about code will keep finding itself.
    """
    out = list(text)
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            i = j + 1
            continue
        if text.startswith("[[", i):
            j = text.find("]]", i + 2)
            i = n if j == -1 else j + 2
            continue
        if c == "#":
            j = text.find("\n", i)
            end = n if j == -1 else j
            for k in range(i, end):
                out[k] = " "
            i = end
            continue
        i += 1
    return "".join(out)


def tokenize(text, start):
    """Tokens of the command invocation whose `(` is at `text[start]`.

    Quoted strings and bracket arguments come back as ONE token each, which is
    what keeps the `EVAL CODE "...DEFER..."` form from reading as a DEFER
    statement. Returns (tokens, index just past the closing paren), or
    (None, None) if the parens never balance.
    """
    tokens = []
    cur = ""
    depth = 0
    i = start
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            cur += text[i : j + 1]
            i = j + 1
            continue
        if c == "[" and text.startswith("[[", i):
            j = text.find("]]", i + 2)
            if j == -1:
                return None, None
            cur += text[i : j + 2]
            i = j + 2
            continue
        if c == "(":
            depth += 1
            if depth == 1:
                i += 1
                continue
        if c == ")":
            depth -= 1
            if depth == 0:
                if cur.strip():
                    tokens.append(cur.strip())
                return tokens, i + 1
        if c == "#" and depth >= 1:
            j = text.find("\n", i)
            i = n if j == -1 else j
            continue
        if c.isspace() and depth == 1:
            if cur.strip():
                tokens.append(cur.strip())
            cur = ""
            i += 1
            continue
        cur += c
        i += 1
    return None, None


def offenders_in(text, rel):
    """(rel, line, token) for every DEFER CALL argument carrying a `${`."""
    bad = []
    text = strip_comments(text)
    needle = "cmake_language"
    i = 0
    while True:
        i = text.find(needle, i)
        if i == -1:
            return bad
        j = i + len(needle)
        while j < len(text) and text[j].isspace():
            j += 1
        if j >= len(text) or text[j] != "(":
            i += len(needle)
            continue
        tokens, end = tokenize(text, j)
        if tokens is None:
            i += len(needle)
            continue
        if "DEFER" in tokens and "CALL" in tokens:
            k = tokens.index("CALL")
            # tokens[k+1] is the callee NAME; the arguments follow it.
            for tok in tokens[k + 2 :]:
                if "${" in tok:
                    line = text.count("\n", 0, i) + 1
                    bad.append((rel, line, tok))
        i = end if end is not None else i + len(needle)


def cmake_files():
    """The TRACKED cmake files, from the git index.

    `git ls-files`, not a filesystem walk. `check-no-tracked-file-find` states
    the measurement behind that rule -- 7m36s against 0.8s for the same 232
    paths -- and it caught the first version of this gate, which walked and then
    pruned. Pruning is not the fix: the walk still stats every directory it
    considers pruning, and it descends into build/ and target/ first.

    The index also settles what to skip without a SKIP list: build artifacts and
    agent worktrees are untracked, so they are simply not in it. A SKIP list
    compared against the wrong kind of path is its own recurring bug here.
    """
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "*.cmake", "CMakeLists.txt", "*/CMakeLists.txt"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for rel in sorted(f for f in out.split("\0") if f):
        p = REPO / rel
        if p.is_file():
            yield p, Path(rel)


def self_test():
    cases = [
        # (source, expected offender count, what it stands for)
        (
            'function(f _t)\n'
            '    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL g "${_t}")\n'
            'endfunction()\n',
            1,
            "the issue 1132 shape -- a function-local passed to a deferred call",
        ),
        (
            'cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL g)\n',
            0,
            "no-argument call: the preferred idiom, state via GLOBAL property",
        ),
        (
            'cmake_language(EVAL CODE\n'
            '    "cmake_language(DEFER DIRECTORY \\"${d}\\" CALL g [[${target}]])")\n',
            0,
            "the EVAL CODE form -- expands to a literal BEFORE defer stores it",
        ),
        (
            'cmake_language(DEFER CALL g "plain-literal")\n',
            0,
            "a literal argument is fine; it survives re-expansion unchanged",
        ),
        (
            'cmake_language(DEFER # a comment mentioning ${x}\n'
            '    CALL g)\n',
            0,
            "a comment inside the invocation is not an argument",
        ),
        (
            'cmake_language(DEFER CALL g "${a}" "${b}")\n',
            2,
            "every offending argument is reported, not just the first",
        ),
        (
            '# cmake_language(DEFER DIRECTORY "${d}" CALL g "${_t}") is the BROKEN form\n'
            'cmake_language(DEFER CALL g)\n',
            0,
            "a comment quoting the broken form is prose, not an instance of it",
        ),
    ]
    bad = []
    for src, want, why in cases:
        got = len(offenders_in(src, "<self-test>"))
        if got != want:
            bad.append(f"expected {want} offender(s), got {got}: {why}")
    if bad:
        print("check-deferred-call-args SELF-TEST FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    print(f"check-deferred-call-args self-test: OK ({len(cases)} cases)")
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if self_test():
        return 1
    scanned = 0
    bad = []
    for path, rel in cmake_files():
        scanned += 1
        bad.extend(offenders_in(path.read_text(encoding="utf-8", errors="replace"), rel))
    if not scanned:
        # A gate that scans nothing passes for the wrong reason. Three gates in
        # this repo have shipped in that state.
        print("check-deferred-call-args: FAILED -- scanned 0 cmake files.", file=sys.stderr)
        return 1
    if not bad:
        print(f"check-deferred-call-args: OK -- {scanned} cmake file(s), no deferred CALL passes an unexpanded reference.")
        return 0
    print("check-deferred-call-args: a deferred CALL argument will arrive EMPTY (issue 1132):", file=sys.stderr)
    for rel, line, tok in bad:
        print(f"  {rel}:{line}: CALL argument {tok}", file=sys.stderr)
    print("", file=sys.stderr)
    print("  DEFER expands CALL arguments when the deferred call RUNS, in the", file=sys.stderr)
    print("  deferred directory's scope. A function-local is gone by then, and the", file=sys.stderr)
    print("  callee's own `if(NOT TARGET \"\")` guard then returns cleanly -- the", file=sys.stderr)
    print("  mechanism becomes a no-op that looks like a guard doing its job.", file=sys.stderr)
    print("", file=sys.stderr)
    print("  Either travel by a GLOBAL property and defer a NO-ARGUMENT call, or", file=sys.stderr)
    print("  wrap it in `cmake_language(EVAL CODE ...)` so the argument is expanded", file=sys.stderr)
    print("  into a literal before DEFER stores it. Both idioms are in cmake/.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
