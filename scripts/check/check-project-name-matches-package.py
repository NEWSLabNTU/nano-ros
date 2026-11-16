#!/usr/bin/env python3
"""check-project-name-matches-package — a package's `project()` is its package.xml `<name>`.

A component's generated entry points are named after its package:
`__nros_c_component_<package>_create` and the rest. The CMake side spells that
name from `PROJECT_NAME` (`nros_package_symbol`, cmake/NanoRosPackageXml.cmake),
while the metadata probe and the entry codegen spell it from `package.xml`. They
agree only when the two names are the same, which is ament's own convention.

dd4fa2c103 renamed two typed-carrier packages and left `project(c_typed_carrier)`.
The images built, because both of their halves used PROJECT_NAME. The probe could
not link (`undefined reference to `__nros_c_component_<package>_create'`), and
once a failed probe became fatal (#2608) that stopped every linux fixture build.

`nros_package_symbol` refuses the mismatch at configure. This gate says so
before anything configures, for every tracked package that names a component
symbol: a `CMakeLists.txt` calling one of the verbs below, beside a
`package.xml`. Buildless, about 0.2 s. Its matcher self-tests on every run.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The verbs whose expansion calls `nros_package_symbol`.
VERBS = re.compile(r"\b(nano_ros_auto_add_library|nano_ros_node_register|nros_components_register_node)\s*\(")
PROJECT = re.compile(r"^\s*project\s*\(\s*([A-Za-z0-9_.-]+)", re.M | re.I)
# Same shape as `nros_package_symbol`'s match, after XML comments are dropped.
NAME = re.compile(r"<name>\s*([^<\s]+)\s*</name>")
COMMENT = re.compile(r"<!--(?:[^-]|-[^-])*-->")
CMAKE_COMMENT = re.compile(r"#[^\n]*")


def mismatch(cmake_text: str, package_xml: str) -> tuple[str, str] | None:
    """(project, package) when a component-naming package disagrees, else None."""
    code = CMAKE_COMMENT.sub("", cmake_text)
    if not VERBS.search(code):
        return None
    p = PROJECT.search(code)
    n = NAME.search(COMMENT.sub("", package_xml))
    if not p or not n:
        return None
    return None if p.group(1) == n.group(1) else (p.group(1), n.group(1))


SELF_TESTS = [
    # (CMakeLists, package.xml, expect a mismatch)
    ("project(a_pkg C)\nnano_ros_auto_add_library(x STATIC s.c)\n", "<package><name>a_pkg</name></package>", False),
    ("project(c_typed_carrier LANGUAGES C CXX)\nnros_components_register_node(lib PLUGIN p::T)\n",
     "<package><name>nuttx_c_typed_carrier</name></package>", True),
    # A package that names no component symbol is not this rule's.
    ("project(other C)\nadd_executable(x x.c)\n", "<package><name>pkg</name></package>", False),
    # A verb that only appears in a comment does not count.
    ("project(other C)\n# nano_ros_node_register(x)\n", "<package><name>pkg</name></package>", False),
    # A commented-out <name> is not the package's name.
    ("project(b C)\nnano_ros_node_register(x)\n", "<package><!-- <name>old</name> --><name>b</name></package>", False),
]


def self_test() -> int:
    bad = [(c, x, w) for c, x, w in SELF_TESTS if (mismatch(c, x) is not None) != w]
    for c, x, w in bad:
        print(f"check-project-name-matches-package --self-test FAILED: want mismatch={w} for {c!r} / {x!r}")
    return 1 if bad else 0


def main() -> int:
    if self_test():
        return 1
    files = subprocess.run(
        ["git", "ls-files", "*CMakeLists.txt"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    if not files:
        print("check-project-name-matches-package: git listed no CMakeLists.txt (wrong root?)")
        return 1
    checked, found = 0, []
    for rel in files:
        if rel.startswith("third-party/") or "/third-party/" in rel:
            continue
        cml = ROOT / rel
        pxml = cml.parent / "package.xml"
        if not pxml.is_file():
            continue
        m = mismatch(cml.read_text(errors="replace"), pxml.read_text(errors="replace"))
        if VERBS.search(CMAKE_COMMENT.sub("", cml.read_text(errors="replace"))):
            checked += 1
        if m:
            found.append(f"{rel}: project({m[0]}) but package.xml names `{m[1]}`")
    if found:
        print("check-project-name-matches-package: a component's project() is not its package name:\n")
        for f in found:
            print(f"  {f}")
        print(
            "\nThe build names the component's symbols from project() and the metadata probe "
            "from package.xml, so they must be one name (ament's convention). Rename one."
        )
        return 1
    print(f"check-project-name-matches-package: OK ({checked} component package(s), each project() is its package name)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
