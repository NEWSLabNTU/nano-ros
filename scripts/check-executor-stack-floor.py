#!/usr/bin/env python3
"""The executor's main-stack floor must be EMITTED, and guarded, by every producer.

Issue 0961: `Executor::open_in` builds the executor as a VALUE in its own frame
and `nros_cpp_init` moves it again, so an image whose main thread is smaller
than that overflows in a function PROLOGUE — before any statement runs, and
therefore before any check written inside those functions could execute. Four
bring-up sessions went into finding that once.

The answer is a compile-time check in the CALLER's translation unit:

    #define NROS_EXECUTOR_MAIN_STACK_MIN <2 x the executor VALUE size>
    #if defined(CONFIG_MAIN_STACK_SIZE) && !defined(NROS_STACK_MIN_ACKNOWLEDGE)
    #if CONFIG_MAIN_STACK_SIZE < NROS_EXECUTOR_MAIN_STACK_MIN
    #error ...
    #endif
    #endif

It exists in all four producers. Nothing kept it there — no gate, no test — so
this file is that. It is a SOURCE check on the emitters, deliberately: the
generated headers are build artifacts, and a gate in an affordability tier may
not resolve one (CLAUDE.md). What is checkable without a build is that every
producer still emits the define and still wraps it in the guard, which is the
regression that would silently disarm the whole thing.

Two failure modes it is aimed at:

* a producer stops emitting the define — the `#if` then compares against an
  undefined identifier, which the preprocessor reads as 0, so the guard passes
  for EVERY stack size and the check is silently gone;
* a producer keeps the define and drops the `#if`/`#error` — the number is then
  documentation, which is what this issue already had and what cost the four
  sessions.

The VALUE is not asserted here. It comes from a probe of the real type and moves
whenever `MAX_CBS` / `MAX_NODES` move, which is the point; pinning it would make
this gate a second, staler SSoT.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# phase-472 W3 — the guard is read as CODE: a comparison or `#error` that sits
# in a comment guards nothing, and used to count.
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import comments  # noqa: E402
import per_item  # noqa: E402  phase-472 W6 — one verdict per emitted header

# Every producer of a header a caller compiles against. Two Rust emitters (the
# C++ one inlines the text, the C one substitutes into the templates below) and
# the two templates themselves.
PRODUCERS = [
    "packages/tooling/nros-build-helpers/src/cpp.rs",
    "packages/api/nros-c/templates/nros_config_generated.h.template",
    "packages/api/nros-c/templates/nros_config_generated_exact.h.template",
]

# `c.rs` reaches the header THROUGH the templates, so it is checked for the
# substitution rather than the guard text.
C_EMITTER = "packages/tooling/nros-build-helpers/src/c.rs"
C_SUBSTITUTION = "@EXECUTOR_STACK_MIN@"

DEFINE = "NROS_EXECUTOR_MAIN_STACK_MIN"
OPT_OUT = "NROS_STACK_MIN_ACKNOWLEDGE"


class Failure(Exception):
    pass


def check_text(name: str, text: str) -> list[str]:
    """Every producer must DEFINE the floor and GUARD on it."""
    problems = []
    # A `.rs` producer emits the header from string literals (kept); a template
    # is C. Either way its comments are prose.
    text = comments.strip_comments(text, "rust" if name.endswith(".rs") else "c")

    # `[^\S\n]` not `\s`: the latter matches the NEWLINE, so a valueless
    # `#define NROS_EXECUTOR_MAIN_STACK_MIN` followed by the `#if` line would
    # satisfy `\s+\S` and pass. The selftest below catches exactly that, and
    # did — the first version of this regex was wrong.
    define_re = re.compile(rf"#define[^\S\n]+{DEFINE}[^\S\n]+\S")
    if not define_re.search(text):
        problems.append(
            f"{name}: does not `#define {DEFINE}`.\n"
            f"    Without the define the `#if` below compares against an undefined\n"
            f"    identifier, which the preprocessor evaluates as 0 — so the guard\n"
            f"    passes for every stack size and the check is silently gone."
        )

    # phase-472 W6 — PER EMITTED HEADER. `cpp.rs` holds two inline emitters,
    # and a file-level search let the first one's guard stand for the second:
    # disarming the second `#if` left this gate green. Each define is judged on
    # the text from it to the next define.
    segs = per_item.segments(text, define_re) or [(None, text)]
    for k, (m, seg) in enumerate(segs, 1):
        where = (f"{name} (emitter {k}, line {per_item.line_of(text, m.start())})"
                 if m is not None and len(segs) > 1 else name)
        if f"CONFIG_MAIN_STACK_SIZE < {DEFINE}" not in seg:
            problems.append(
                f"{where}: defines {DEFINE} but does not COMPARE against it\n"
                f"    (`#if CONFIG_MAIN_STACK_SIZE < {DEFINE}`).\n"
                f"    A number nobody checks is documentation. That is the state issue 0961\n"
                f"    was filed about, and it cost four bring-up sessions."
            )
        if "#error" not in seg:
            problems.append(f"{where}: has the comparison but no `#error` — it cannot fail.")
        if OPT_OUT not in seg:
            problems.append(
                f"{where}: no `{OPT_OUT}` escape hatch.\n"
                f"    An image that builds its executor off the main thread is measured\n"
                f"    against a stack this header cannot see, and needs a way to say so."
            )

    return problems


def run() -> int:
    problems = []
    checked = 0

    for rel in PRODUCERS:
        p = REPO / rel
        if not p.is_file():
            raise Failure(
                f"{rel}: missing. This gate names its producers explicitly, so a\n"
                "renamed or deleted one is a failure rather than a silent pass."
            )
        problems += check_text(rel, p.read_text())
        checked += 1

    c = REPO / C_EMITTER
    if not c.is_file():
        raise Failure(f"{C_EMITTER}: missing.")
    if C_SUBSTITUTION not in comments.strip_comments(c.read_text(), "rust"):
        problems.append(
            f"{C_EMITTER}: does not substitute `{C_SUBSTITUTION}`.\n"
            f"    It reaches the header through the templates rather than inlining the\n"
            f"    text, so the substitution IS its half of the guard."
        )
    checked += 1

    if problems:
        print(
            f"check-executor-stack-floor: {len(problems)} problem(s)\n", file=sys.stderr
        )
        for pr in problems:
            print(f"  [FAIL] {pr}", file=sys.stderr)
        print(
            "\n  Issue 0961. The floor is a NECESSARY condition, not a sufficient one —\n"
            "  it is roughly a third of the true frame cost — but an image below it\n"
            "  provably cannot boot, and the whole value is that the failure arrives at\n"
            "  compile time naming the number instead of as a prologue overflow.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-executor-stack-floor: OK ({checked} producer(s) define "
        f"{DEFINE} and guard on it)"
    )
    return 0


def selftest() -> int:
    per_item.self_test()  # the shared helper's own controls (phase-472 W6)
    good = (
        f"#define {DEFINE} 3104\n"
        f"#if defined(CONFIG_MAIN_STACK_SIZE) && !defined({OPT_OUT})\n"
        f"#if CONFIG_MAIN_STACK_SIZE < {DEFINE}\n"
        '#error "too small"\n'
        "#endif\n#endif\n"
    )
    assert check_text("t", good) == [], check_text("t", good)

    # NEGATIVE: each way the guard can be disarmed must be CAUGHT.
    no_define = good.replace(f"#define {DEFINE} 3104\n", "")
    assert any("does not `#define" in p for p in check_text("t", no_define))

    no_compare = good.replace(f"#if CONFIG_MAIN_STACK_SIZE < {DEFINE}\n", "")
    assert any("does not COMPARE" in p for p in check_text("t", no_compare))

    no_error = good.replace('#error "too small"\n', "")
    assert any("no `#error`" in p for p in check_text("t", no_error))

    comments.self_test()
    # phase-472 W3 — a guard in a COMMENT is no guard.
    commented = good.replace(
        f"#if CONFIG_MAIN_STACK_SIZE < {DEFINE}\n",
        f"#if 0 /* CONFIG_MAIN_STACK_SIZE < {DEFINE} */\n",
    )
    assert any("does not COMPARE" in p for p in check_text("t", commented))
    assert any("no `#error`" in p for p in check_text(
        "t", good.replace('#error "too small"\n', '// #error "too small"\n')))

    no_optout = good.replace(f" && !defined({OPT_OUT})", "").replace(OPT_OUT, "")
    assert any("escape hatch" in p for p in check_text("t", no_optout))

    # phase-472 W6 — two emitters in one file: the second's guard is its own.
    two = good + "\n" + good.replace(f"#if CONFIG_MAIN_STACK_SIZE < {DEFINE}\n", "#if 0\n")
    assert any("emitter 2" in p and "does not COMPARE" in p for p in check_text("t", two)), \
        check_text("t", two)
    assert check_text("t", good + "\n" + good) == []
    # A define whose value is empty is not a define worth having.
    empty = good.replace(f"#define {DEFINE} 3104", f"#define {DEFINE}")
    assert any("does not `#define" in p for p in check_text("t", empty))

    print("check-executor-stack-floor: selftest OK")
    return 0


if __name__ == "__main__":
    try:
        if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
            sys.exit(selftest())
        selftest()  # on the NORMAL path too, so the controls cannot rot
        sys.exit(run())
    except Failure as exc:
        print(f"check-executor-stack-floor: {exc}", file=sys.stderr)
        sys.exit(1)
