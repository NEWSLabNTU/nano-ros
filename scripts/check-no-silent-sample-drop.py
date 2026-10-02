#!/usr/bin/env python3
"""issue 0737 — an example's message callback may not drop a sample silently.

A subscription callback that does

    if (deserialize(&msg, data, len) != 0) {
        return;
    }

turns "the message was rejected here" into "no output", and those two are the
same observation from outside the process. That ambiguity is what made 0737
cost two hosts a full investigation each: the sample was discovered, matched,
stored and TAKEN — Cyclone's own trace printed `take: returning 1` — while the
only symptom available to anyone was an absence of `Received:` lines. Every
layer below the callback had to be cleared by hand before the callback itself
became a suspect, and one line of output would have skipped all of it.

These are EXAMPLES, which makes it worse twice over: users copy them, and tests
GREP them. A silent arm in code a test reads for its verdict is the same defect
as a test that reports PASS on an unmet precondition, one level out.

So: a `return` inside a failed-deserialize arm must be preceded by a print in
the same arm. Not a rule about error handling — dropping is often right — a
rule about SAYING SO.

Run: python3 scripts/check-no-silent-sample-drop.py
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.tracked import tracked  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SCOPE = ["examples"]
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402
import per_item  # noqa: E402
# The arm opener: any `_deserialize(...)` used as a failure test.
OPENER = re.compile(r"_deserialize\s*\(.*\)\s*!=\s*0\s*\)\s*\{")
# `log_error`/`log_warn` are the post-phase-417 Rust spellings; `nros_error`/
# `nros_warn` stay listed because the C API and older prose still use them.
SAYS_SOMETHING = re.compile(
    r"\b(printf|fprintf|puts|log_error|log_warn|nros_error|nros_warn|NROS_LOG|std::cerr|nros_log_emit\w*)\b"
)


def offenders(path: Path):
    """Per CALL over the comment-stripped file — issue 1615 (W6).

    The opener was matched per LINE, so a `_deserialize(&msg, data,\n len)`
    split across lines (clang-format does that) was never an arm at all. The
    arm is now the balanced `{…}` after `if (<call with _deserialize> != 0)`.
    """
    try:
        raw = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    lang = comments.lang_for(path) or "c"
    code = comments.strip_comments(raw, lang)
    out = []
    for m in OPENER_ML.finditer(code):
        brace = code.find("{", m.end() - 1)
        end = per_item.block_end(code, brace)
        arm = code[brace + 1:end - 1]
        if "return" in arm and not SAYS_SOMETHING.search(arm) and not calls_local_sink(arm, code):
            out.append(f"{path.relative_to(ROOT)}:{code.count(chr(10), 0, m.start()) + 1}")
    return out


def calls_local_sink(arm, code):
    """A file-local helper whose own body reaches a sink (`emit()` in the
    no-libc bare-metal leaves wraps `nros_log_emit_at`) says something too."""
    for name in set(re.findall(r"\b([A-Za-z_]\w*)\s*\(", arm)):
        m = re.search(r"\b" + re.escape(name) + r"\s*\([^;{]*\)\s*\{", code)
        if m:
            body = code[m.end() - 1:per_item.block_end(code, m.end() - 1)]
            if SAYS_SOMETHING.search(body):
                return True
    return False


# `_deserialize(` … `) != 0) {` with the argument list allowed to span lines.
OPENER_ML = re.compile(r"_deserialize\s*\([^;{}]*?\)\s*!=\s*0\s*\)\s*\{", re.S)


def main() -> int:
    files = [
        f
        for d in SCOPE
        for f in tracked(ROOT / d)
        if f.suffix in (".c", ".cpp", ".cc", ".h", ".hpp")
    ]
    bad = [o for f in files for o in offenders(f)]
    if bad:
        print("check-no-silent-sample-drop: FAIL\n", file=sys.stderr)
        for b in bad:
            print(f"  {b}: a rejected sample returns with no output", file=sys.stderr)
        print(
            "\n  Say what was dropped and why. From outside the process a silent\n"
            "  `return` and a message that never arrived are the SAME observation,\n"
            "  and issue 0737 spent two hosts' investigations on that ambiguity.\n"
            "  One `fprintf(stderr, ...)` in the arm is the whole fix.",
            file=sys.stderr,
        )
        return 1
    print(f"check-no-silent-sample-drop: OK ({len(files)} example source file(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
