#!/usr/bin/env python3
"""issue 0720 — a test's fixture binary name must be a real CMake target.

# What this protects

A test asks a resolver for an artifact BY NAME:

    build_threadx_rv64_rust_example_rmw("listener", "riscv64_threadx_rust_listener", Rmw::Cyclonedds)

The resolver joins that name onto the leaf's build directory. If the name is
wrong the path simply does not exist, and the test's `unwrap_or_else` turns
that into `nros_tests::skip!("… fixture missing (just … build-fixtures)")` —
a message that explains itself and blames the build. So a name that no target
produces reads as "nobody built the fixtures", forever, on a tree where they
were all built.

# The defect it was written for

phase-369 W4 (`7c455016f`, 2026-08-20) renamed the threadx-rv64 rust leaves'
CMake targets RMW-neutral — `riscv64_threadx_rust_listener_cyclonedds` became
`riscv64_threadx_rust_listener` — because the zenoh build dir was emitting an
ELF carrying the cyclone suffix. The rename updated the talker's test site and
missed the listener's, the only other consumer. From then on
`test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub` skipped every run, which
is the one test covering issue 0692's rust cyclone image: the image that
motivated the whole investigation was never actually executed by the suite that
reported on it. `check-skip-budget` saw it and printed `1 skip(s) —
capability=1`, which is a count, not a name.

# The rule

Every string literal handed to a cmake-leaf fixture resolver as its
`binary_name` must appear as a target in that leaf's `CMakeLists.txt`.

This is a STATIC check: it needs no build, so it fails on the rename commit
rather than on the next full sweep.

# Detection

The call sites are literal-argument calls, so the arguments are read straight
out of the source. A call whose `case` or `binary_name` is not a literal
(a variable, a `format!`) is reported as unchecked rather than skipped
silently — the count is printed, so a future refactor into variables cannot
quietly empty this gate.

Run: python3 scripts/check-fixture-binary-names.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# resolver name -> (leaf prefix, index of the `case` arg, index of `binary_name`)
RESOLVERS = {
    "build_threadx_rv64_rust_example_rmw": ("examples/qemu-riscv64-threadx/rust", 0, 1),
    "build_native_c_example_rmw": ("examples/native/c", 0, 1),
    "build_native_cpp_example_rmw": ("examples/native/cpp", 0, 1),
}

# The call head; the argument list is then read by balancing parens, because a
# `format!(…)` or a nested call inside the arguments defeats any regex that
# stops at the first `)`.
CALL_HEAD = re.compile(r"\b(" + "|".join(RESOLVERS) + r")\s*\(")
LITERAL = re.compile(r'^"([^"]*)"$')


def call_args(text: str, open_paren: int) -> list[str] | None:
    """Split the argument list starting at `text[open_paren] == "("`.

    Returns the top-level arguments, or None if the parens never close.
    """
    depth = 0
    args: list[str] = []
    start = open_paren + 1
    i = open_paren
    while i < len(text):
        ch = text[i]
        if ch == '"':
            # skip a string literal, honouring escapes
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                args.append(text[start:i])
                return [a.strip() for a in args if a.strip()]
        elif ch == "," and depth == 1:
            args.append(text[start:i])
            start = i + 1
        i += 1
    return None

# `project(<name> …)`, `add_executable(<name> …)`, and the nano-ros app wrappers,
# which all take the target name as their first argument.
TARGET_DECL = re.compile(
    r"^\s*(?:project|add_executable|add_library|nros_\w*_app|nano_ros_entry)\s*\(\s*([A-Za-z0-9_]+)",
    re.MULTILINE,
)


def targets_of(leaf: Path) -> set[str]:
    cml = leaf / "CMakeLists.txt"
    if not cml.is_file():
        return set()
    return set(TARGET_DECL.findall(cml.read_text()))


def main() -> int:
    bad: list[str] = []
    unchecked = 0
    checked = 0

    for src in sorted((ROOT / "packages/testing/nros-tests/tests").glob("*.rs")):
        text = src.read_text()
        for match in CALL_HEAD.finditer(text):
            fn = match.group(1)
            args = call_args(text, match.end() - 1)
            prefix, case_i, bin_i = RESOLVERS[fn]
            if args is None or len(args) <= bin_i:
                unchecked += 1
                continue
            case_m = LITERAL.match(args[case_i])
            bin_m = LITERAL.match(args[bin_i])
            if not case_m or not bin_m:
                unchecked += 1
                continue
            checked += 1
            leaf = ROOT / prefix / case_m.group(1)
            known = targets_of(leaf)
            if not known:
                bad.append(
                    f"{src.relative_to(ROOT)}: {fn}(\"{case_m.group(1)}\", …) — "
                    f"no CMakeLists.txt at {prefix}/{case_m.group(1)}"
                )
            elif bin_m.group(1) not in known:
                bad.append(
                    f"{src.relative_to(ROOT)}: {fn} asks for "
                    f'"{bin_m.group(1)}", which {prefix}/{case_m.group(1)}/CMakeLists.txt '
                    f"does not declare. It declares: {', '.join(sorted(known))}"
                )

    if bad:
        print("fixture binary names that no CMake target produces:", file=sys.stderr)
        for line in bad:
            print(f"  {line}", file=sys.stderr)
        print(
            "\nA name no target produces resolves to a missing path, which the test\n"
            "reports as a `fixture missing` SKIP — so this never fails, it just stops\n"
            "testing. See issue 0720.",
            file=sys.stderr,
        )
        return 1

    note = f", {unchecked} non-literal call(s) not checked" if unchecked else ""
    print(f"check-fixture-binary-names: OK ({checked} call site(s){note})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
