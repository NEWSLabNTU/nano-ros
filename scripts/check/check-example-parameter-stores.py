#!/usr/bin/env python3
"""No shipped example may demonstrate a parameter store `ros2 param` cannot see.

THE CLASS, AND WHY IT NEEDED A GATE
-----------------------------------
nano-ros has one parameter store that the six `rcl_interfaces/srv/*` servers
read: the `nros_params::ParameterServer` the executor owns. It also has a
CALLER-STORAGE store in C — `nros_parameter_server_t` over an
`nros_parameter_t[]` the caller supplies — which is purely node-local and which
nothing joins to the first. RFC-0089 §"Parameters" and RFC-0019/0020 make the
Rust store the SSoT; phase-426 W4 deleted the two node-owned C++ stores and,
with the second half of W4, `nros::ParameterServer<Cap>`, the C++ wrapper over
the caller-storage one.

What survived that deletion, for four months, was the demonstration. Three
files under `examples/` still built a caller-storage store:

  * `examples/native/cpp/parameters` — its ENTIRE body was
    `nros::ParameterServer<8>`, so the shipped answer to "how do I use
    parameters in C++" was the defect.
  * `examples/native/c/parameters` — the same in C.
  * `examples/native/c/custom-transport-loopback` — an `nros_parameter_server_t`
    initialised and never touched again.

An example is COPIED OUT (RFC-0026), so a wrong one is not a wrong line in one
file — it is the starting point of every program written from it. And nothing
caught this: W4's acceptance was `check-cpp-capability-layout` plus "a parameter
declared through `rclcpp::Node::declare_parameter` is visible to `ros2 param
get`", both of which were true of the node API while the examples went on using
a different one.

WHAT THIS REFUSES
-----------------
Any reference, from a file under `examples/`, to the caller-storage parameter
family. The C++ half (`nros::ParameterServer<...>`) is a compile error now that
the class is gone; the C half is a live API with legitimate callers elsewhere,
so it is this gate that keeps it out of the examples.

It is NOT a ban on the API. `packages/api/nros-c` implements it and
`packages/api/nros-c/tests/compile/param_entry_points.c` pins its entry points;
both are outside this gate's reach on purpose. The rule is about what we SHOW.

The replacement, which the message names, is:
  C   — `nros_executor_declare_param_*` / `_get_param_*` / `_set_param_*`
        (and the `_on` spellings for a named node).
  C++ — `rclcpp::Node::declare_parameter<T>` / `get_parameter<T>` /
        `set_parameter<T>` / `has_parameter`.
  Rust— `Executor::declare_parameter` / `NodeCtx::parameter`.

Run: python3 scripts/check/check-example-parameter-stores.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXAMPLES = REPO / "examples"

SOURCE_SUFFIXES = {".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".rs"}

# The caller-storage family. `nros_parameter_t` alone is NOT here: it is the
# element type, and naming it is only a problem when a server is built over it,
# which the `nros_parameter_server_*` entries already catch.
BANNED = re.compile(
    r"\b("
    r"nros_parameter_server_t"
    r"|nros_parameter_server_(?:init|fini|get_zero_initialized|set_callback|get_count)"
    r"|nros_parameter_(?:declare|get|set)_[a-z_]+"
    r"|nros_parameter_(?:has|get_type)"
    r"|nros::ParameterServer"
    r")\b"
)

REPLACEMENT = (
    "Use the store the parameter services read:\n"
    "  C    nros_executor_declare_param_*/_get_param_*/_set_param_*  "
    "(`_on` names a node)\n"
    "  C++  rclcpp::Node::declare_parameter<T>/get_parameter<T>/set_parameter<T>\n"
    "  Rust Executor::declare_parameter / NodeCtx::parameter\n"
    "See examples/native/{c,cpp}/parameters for a worked pair."
)


def tracked_example_sources() -> list[Path]:
    """Sources under `examples/` that a clone would see.

    `git ls-files --cached --others --exclude-standard` rather than a walk: a
    build tree under an example leaf holds copies of nros-c's own headers, which
    DO declare the family, and flagging those would make the gate's verdict
    depend on whether anyone had built. `--others` is there so a NEW example
    file is checked before it is staged — a gate a contributor only meets after
    `git add` is one they meet too late.
    """
    out = subprocess.run(
        [
            "git",
            "-C",
            str(REPO),
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "examples",
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [
        REPO / name
        for name in out.split("\0")
        if name and Path(name).suffix in SOURCE_SUFFIXES
    ]


def offenders(paths: list[Path]) -> list[tuple[Path, int, str]]:
    found: list[tuple[Path, int, str]] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "nros_parameter" not in text and "ParameterServer" not in text:
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            stripped = line.strip()
            # A comment saying the store is GONE is the opposite of the defect,
            # and the examples carry exactly that. Only CODE counts.
            if stripped.startswith(("//", "/*", "*", "#", "///")):
                continue
            match = BANNED.search(line)
            if match:
                found.append((path, lineno, match.group(1)))
    return found


def self_test() -> None:
    """Negative control: the pattern must fire on the code that was there.

    A gate over a family nobody uses any more prints the same OK whether its
    regex works or not (RFC-0089 §"A gate whose subject disappears passes
    vacuously"), so the shapes the three examples actually carried are checked
    here, on every run.
    """
    must_match = [
        "    nros::ParameterServer<8> params;",
        "    nros_parameter_server_t params = nros_parameter_server_get_zero_initialized();",
        '    if (nros_parameter_declare_bool(&params, "verbose", false) != NROS_RET_OK) return 1;',
        "    (void)nros_parameter_server_init(&app.params, app.param_storage, 4);",
        '    if (nros_parameter_get_string(&params, "topic_name", topic, sizeof(topic))) {}',
        '    if (params.has_parameter("missing")) return 5;'.replace(
            "params.has_parameter", "nros_parameter_has"
        ),
    ]
    must_not_match = [
        '    nros_executor_declare_param_bool_on(&e, &n, "verbose", false);',
        '    nros_executor_get_param_integer(&e, "rate", &out);',
        '    node.declare_parameter<double>("ctrl_period", 0.15);',
        '    let v = ctx.parameter::<i64>("publish_period_ms");',
    ]
    for line in must_match:
        if not BANNED.search(line):
            raise SystemExit(
                "check-example-parameter-stores: SELF-TEST FAILED — the pattern "
                f"does not match a known offender:\n  {line}"
            )
    for line in must_not_match:
        if BANNED.search(line):
            raise SystemExit(
                "check-example-parameter-stores: SELF-TEST FAILED — the pattern "
                f"matches the sanctioned spelling:\n  {line}"
            )


def main() -> int:
    self_test()
    paths = tracked_example_sources()
    if not paths:
        print(
            "check-example-parameter-stores: FAIL — no tracked example sources "
            "found; the scan would pass vacuously."
        )
        return 1
    found = offenders(paths)
    if found:
        print("check-example-parameter-stores: FAIL")
        print("  A shipped example reaches the CALLER-STORAGE parameter store. Its")
        print("  contents are invisible to `ros2 param list|get|set`, because the six")
        print("  `rcl_interfaces/srv/*` servers read the executor's store and nothing")
        print('  joins the two (RFC-0089 §"Parameters", phase-426 W4).')
        print("")
        for path, lineno, symbol in found:
            print(f"  {path.relative_to(REPO)}:{lineno}: {symbol}")
        print("")
        for line in REPLACEMENT.splitlines():
            print(f"  {line}")
        return 1
    print(
        f"check-example-parameter-stores: OK ({len(paths)} example "
        "source(s), none reaches the caller-storage store)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
