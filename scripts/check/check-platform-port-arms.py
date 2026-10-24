#!/usr/bin/env python3
"""Phase-513 R6 — every arm of a platform port defines the same ABI functions.

A platform port's C source may implement `<nros/platform.h>` twice, once per
arm of a preprocessor conditional. The Zephyr port is the case that motivated
this gate: `#ifdef CONFIG_POSIX_API` builds the lock and task functions on
pthreads, and its `#else` arm (issue 0566) builds them on `k_mutex` /
`k_condvar` / `k_thread`. The two arms are two implementations of ONE surface,
so a function only one of them defines is a hole in the other.

The kernel arm was eight functions short from issue 0566 until issue 2032 —
the six lock storage probes, `condvar_signal_from_isr` and `task_attr_init`.
Nothing noticed, because no caller reached the gap. Then phase-509's native
level runner called `nros_platform_mutex_rec_storage_size`, and on
`native_sim` the linker satisfied it from the POSIX port's `platform.o` that
`nros-rmw-xrce-cffi` bundles: 56 duplicate symbols in every Zephyr C++ XRCE
image. A missing function in one arm is not a link error in that arm. It
becomes one only when a caller arrives, and then it may name a different
file entirely.

`check-platform-abi-mirror` compares hand-written declarations against the
generated mirror. It has no view of which arm of a port defines what, so it
was green through all of this.

THE RULE. For every conditional group in a port's `src/*.c` that has more than
one arm (`#if` / `#elif` / `#else`), the set of `nros_platform_*` functions
DEFINED in each arm must be equal. An arm's set includes definitions inside
groups nested in it; a nested group is checked on its own too. A group with
one arm (`#ifdef X ... #endif`) is not compared: there is no second arm to be
short of.

A function legitimately present in one arm only — an ISR-only helper on an arm
that has interrupts, say — goes in `ALLOWED` with the reason. The table starts
empty and must say why for every row.
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import comments  # noqa: E402  phase-472 W3 — a comment is not code

# (path relative to the repo root, function name) -> why one arm alone has it.
ALLOWED: dict[tuple[str, str], str] = {}

_DIRECTIVE = re.compile(r"^\s*#\s*(if|ifdef|ifndef|elif|else|endif)\b(.*)$")
# A definition starts at column 0: a return type, then the name, then `(`.
# Declarations end in `;`, definitions reach `{` first.
_DEF_HEAD = re.compile(r"^(?:[A-Za-z_][^;{}=]*?\b)?(nros_platform_\w+)\s*\(")


@dataclass
class Arm:
    line: int
    cond: str
    defs: set[str] = field(default_factory=set)


@dataclass
class Group:
    line: int
    arms: list[Arm] = field(default_factory=list)


def scan(text: str) -> list[tuple[Group, list[Arm]]]:
    """Return every multi-arm group with its arms, innermost first.

    Each arm's `defs` holds the functions defined directly in it plus those of
    every group nested in it.
    """
    lines = comments.strip_comments(text, "c").split("\n")
    stack: list[Group] = []
    done: list[tuple[Group, list[Arm]]] = []
    top: set[str] = set()
    i = 0
    while i < len(lines):
        ln = lines[i]
        m = _DIRECTIVE.match(ln)
        if m:
            kind, rest = m.group(1), m.group(2).strip()
            if kind in ("if", "ifdef", "ifndef"):
                g = Group(i + 1)
                g.arms.append(Arm(i + 1, f"#{kind} {rest}"))
                stack.append(g)
            elif kind in ("elif", "else") and stack:
                stack[-1].arms.append(Arm(i + 1, f"#{kind} {rest}".strip()))
            elif kind == "endif" and stack:
                g = stack.pop()
                union = set().union(*(a.defs for a in g.arms))
                if len(g.arms) > 1:
                    done.append((g, g.arms))
                (stack[-1].arms[-1].defs if stack else top).update(union)
            i += 1
            continue
        h = _DEF_HEAD.match(ln)
        if h:
            # Walk the signature until it reaches `{` (definition) or `;`.
            j, sig = i, ln
            while "{" not in sig and ";" not in sig and j + 1 < len(lines):
                j += 1
                sig += lines[j]
            brace, semi = sig.find("{"), sig.find(";")
            if brace != -1 and (semi == -1 or brace < semi):
                (stack[-1].arms[-1].defs if stack else top).add(h.group(1))
        i += 1
    return done


def gaps(path: str, text: str) -> list[str]:
    out = []
    for g, arms in scan(text):
        union = set().union(*(a.defs for a in arms))
        for a in arms:
            for fn in sorted(union - a.defs):
                if (path, fn) in ALLOWED:
                    continue
                others = [b.line for b in arms if fn in b.defs]
                out.append(
                    f"{path}:{a.line}: arm `{a.cond}` of the group at line "
                    f"{g.line} does not define `{fn}`; the arm(s) at line(s) "
                    f"{', '.join(map(str, others))} do"
                )
    return out


def port_sources() -> list[str]:
    r = subprocess.run(
        ["git", "ls-files", "packages/platform/*/src/*.c"],
        cwd=REPO, capture_output=True, text=True, check=True,
    )
    return [p for p in r.stdout.split() if p]


def self_test() -> None:
    good = (
        "#ifdef A\n"
        "int8_t nros_platform_mutex_init(void* m) { return 0; }\n"
        "size_t\nnros_platform_mutex_storage_size(void)\n{ return 8; }\n"
        "#else\n"
        "int8_t nros_platform_mutex_init(void* m) {\n  return 0;\n}\n"
        "#if B\n"
        "size_t nros_platform_mutex_storage_size(void) { return 8; }\n"
        "#else\n"
        "size_t nros_platform_mutex_storage_size(void) { return 4; }\n"
        "#endif\n"
        "#endif\n"
    )
    assert gaps("x.c", good) == [], gaps("x.c", good)

    # The issue-2032 shape: the second arm is short one function.
    short = good.replace(
        "size_t nros_platform_mutex_storage_size(void) { return 4; }\n", ""
    )
    got = gaps("x.c", short)
    # Reported once, at the nested group; the outer arms both hold it.
    assert len(got) == 1, got
    assert "nros_platform_mutex_storage_size" in got[0], got

    # A declaration is not a definition; a comment is not code.
    decl = (
        "#ifdef A\nint8_t nros_platform_x(void) { return 0; }\n"
        "#else\nint8_t nros_platform_x(void);\n"
        "/* int8_t nros_platform_x(void) { return 0; } */\n#endif\n"
    )
    assert len(gaps("x.c", decl)) == 1, gaps("x.c", decl)

    # A one-arm group has no second arm to be short of.
    one = "#ifdef A\nint8_t nros_platform_x(void) { return 0; }\n#endif\n"
    assert gaps("x.c", one) == []

    # A weak definition counts, and so does an #elif arm.
    elif_ = (
        "#if A\n__attribute__((weak)) int8_t nros_platform_x(void) { return 0; }\n"
        "#elif B\nint8_t nros_platform_x(void) { return 1; }\n#else\n#endif\n"
    )
    got = gaps("x.c", elif_)
    assert len(got) == 1 and "#else" in got[0], got

    # ALLOWED silences exactly its row.
    ALLOWED[("x.c", "nros_platform_x")] = "selftest"
    try:
        assert gaps("x.c", elif_) == []
        assert gaps("y.c", elif_) != []
    finally:
        del ALLOWED[("x.c", "nros_platform_x")]


def main(argv: list[str]) -> int:
    self_test()
    if "--self-test" in argv:
        print("check-platform-port-arms: self-test ok")
        return 0
    paths = port_sources()
    if not paths:
        print("check-platform-port-arms: no port sources found; the glob is stale")
        return 1
    found = []
    for p in paths:
        found += gaps(p, (REPO / p).read_text())
    stale = [k for k in ALLOWED if k[0] not in paths]
    if found or stale:
        print("check-platform-port-arms: a port arm is short of the platform ABI")
        for f in found:
            print("  " + f)
        for k in stale:
            print(f"  ALLOWED names {k[0]}, which is not a port source any more")
        print(
            "  Define the function in every arm, or add an ALLOWED row with the\n"
            "  reason one arm alone has it (phase-513 R6, issue 2032)."
        )
        return 1
    print(f"check-platform-port-arms: {len(paths)} port source(s), every arm agrees")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
