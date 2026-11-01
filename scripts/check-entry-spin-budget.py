#!/usr/bin/env python3
"""phase-521 W6 (RFC-0113 D2) — no literal spin budget in an entry loop.

An entry loop is `loop { spin_once(budget) }`. The executor ends each park at
its next deadline or on its wake object, so the budget is not a cadence: it
bounds only a park nothing else bounds. Every entry used to pass 10 ms, which
woke an idle tier 100 times a second whatever its park could do. The budget now
comes from the executor (`Executor::entry_spin_budget`, its runtime and C/C++
forms `NodeDispatchRuntime::entry_spin_budget_ms` and
`nros_cpp_entry_spin_budget_ms`), which knows whether every source of work can
end the park.

This gate reads the entry funnels below and refuses a spin call whose budget is
an integer literal. A file that no longer exists is a failure: a gate over a
path that moved checks nothing (issue 0196's reach rule).

`--self-test` runs the matcher over synthetic lines.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The entry funnels: the code that owns an image's forever-spin.
ENTRY_FILES = [
    "packages/api/nros/src/lib.rs",
    "packages/core/nros-macros/src/main_macro.rs",
    "packages/boards/nros-board-freertos/src/entry.rs",
    "packages/boards/nros-board-threadx/src/entry.rs",
    "packages/boards/nros-board-nuttx/src/lib.rs",
    "packages/boards/nros-board-mps2-an385/src/entry.rs",
    "packages/boards/nros-board-esp32-qemu/src/board_entry.rs",
    "packages/boards/nros-board-common/c/nros_rtos_run_components.c",
    "packages/api/nros-cpp/include/rclcpp/detail/main.hpp",
    "packages/api/nros-cpp/include/rclcpp/detail/driven_future.hpp",
]

# Lines carrying one of these are exempt, with the reason in the marker's own
# comment: the RTIC dispatch loop (RTIC's own tick wakes it) and the bounded
# external-observer step, whose step is the observer's sampling period.
EXEMPT_MARKER = "entry-spin-budget: exempt"

PATTERNS = [
    # Rust: spin_once(<literal>) or spin_once(Duration::from_millis(<literal>))
    re.compile(r"\bspin_once\(\s*(?:[^,()]*::)?from_millis\(\s*\d+\s*\)\s*\)"),
    re.compile(r"\bspin_once\(\s*\d+\s*\)"),
    # Two-argument forms: spin_once(&mut rt, <literal>), nros_cpp_spin_once(h, <literal>)
    re.compile(r"\bspin_once\(\s*[^,()]+,\s*\d+\s*\)"),
    # nros_cpp_spin_for(h, bound, <literal>)
    re.compile(r"\bspin_for\(\s*[^,()]+,\s*[^,()]+,\s*\d+\s*\)"),
]


def offending(text):
    """Yield (line_no, line) for each literal-budget spin call in `text`.

    Calls split over lines are joined first, so `spin_once(\n &mut crt,\n 10,\n)`
    is caught; comments are not code and are skipped.
    """
    lines = text.splitlines()
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith(("//", "/*", "*", "#")):
            continue
        if "spin_once(" not in line and "spin_for(" not in line:
            continue
        window = " ".join(l.strip() for l in lines[i : i + 6])
        call = window[window.find("spin_") :]
        # Close the call at its matching parenthesis.
        depth = 0
        end = None
        for j, ch in enumerate(call):
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    end = j + 1
                    break
        call = call[:end] if end else call
        call = re.sub(r",\s*\)$", ")", call)
        if EXEMPT_MARKER in " ".join(lines[max(0, i - 3) : i + 1]):
            continue
        if any(p.search(call) for p in PATTERNS):
            yield i + 1, stripped


def self_test():
    bad = [
        "let _ = runtime.spin_once(::core::time::Duration::from_millis(10));",
        "let _ = runtime.runtime.spin_once(10);",
        "NodeDispatchRuntime::spin_once(&mut crt, 10) {",
        "NodeDispatchRuntime::spin_once(\n    &mut crt,\n    10,\n) {",
        "int32_t last = (int32_t)nros_cpp_spin_once(storage, 10);",
        "out = (int32_t)nros_cpp_spin_for(storage, bound_ms, 10);",
        "(void)nros_cpp_spin_once(exec, 10);",
    ]
    good = [
        "let _ = runtime.spin_once(budget);",
        "NodeDispatchRuntime::spin_once(&mut crt, NodeDispatchRuntime::entry_spin_budget_ms(&crt)) {",
        "NodeDispatchRuntime::spin_once(\n    &mut crt,\n    deadline.budget_ms(x),\n) {",
        "(void)nros_cpp_spin_once(exec, nros_cpp_entry_spin_budget_ms(exec));",
        "// spin_once(10) in a comment",
        "// entry-spin-budget: exempt (RTIC owns the tick)\nspin_once(&mut c, 10);",
    ]
    failures = 0
    for case in bad:
        if not list(offending(case)):
            print(f"self-test: missed {case!r}")
            failures += 1
    for case in good:
        hits = list(offending(case))
        if hits:
            print(f"self-test: false positive {case!r}: {hits}")
            failures += 1
    return failures


def main():
    if "--self-test" in sys.argv:
        n = self_test()
        print("check-entry-spin-budget self-test:", "OK" if n == 0 else f"{n} FAILED")
        return 1 if n else 0
    if self_test():
        print("check-entry-spin-budget: self-test FAILED")
        return 1
    problems = []
    for rel in ENTRY_FILES:
        path = ROOT / rel
        if not path.exists():
            problems.append(f"{rel}: listed entry funnel is missing (moved? update ENTRY_FILES)")
            continue
        for line_no, line in offending(path.read_text()):
            problems.append(f"{rel}:{line_no}: literal spin budget: {line}")
    if problems:
        print("check-entry-spin-budget: FAIL")
        for p in problems:
            print("  " + p)
        print(
            "  An entry asks the executor for its budget (`entry_spin_budget`,\n"
            "  `NodeDispatchRuntime::entry_spin_budget_ms`, `nros_cpp_entry_spin_budget_ms`),\n"
            "  which parks until the next deadline when every source of work can end\n"
            "  the park (phase-521 W6, RFC-0113 D2)."
        )
        return 1
    print(f"check-entry-spin-budget: OK — {len(ENTRY_FILES)} entry funnels ask the executor")
    return 0


if __name__ == "__main__":
    sys.exit(main())
