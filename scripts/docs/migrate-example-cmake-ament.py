#!/usr/bin/env python3
"""Migrate canonical native example CMakeLists to the RFC-0048 ament shape (phase-287 W6).

CONSERVATIVE. Only rewrites a leaf whose body — after the W1/W2a bootstrap
prelude — is EXACTLY the canonical role shape:

    [optional interface-gen line: nros_find_interfaces(...) OR nano_ros_generate_interfaces/nros_generate_interfaces(${PROJECT_NAME}...)]
    nano_ros_entry(NAME <n> SOURCES <srcs...> DEPLOY native)
    [nano_ros_link(<n>)  OR  nros_platform_link_app(<n>)]

Anything bespoke — extra add_library/add_executable, extra target_link_libraries,
find_package(Threads), target_compile_options, a NAME that differs from the entry,
multiple entries — makes the leaf SKIPPED (left untouched, reported) for manual
migration. Only native `DEPLOY native` leaves are eligible; embedded / zephyr /
workspace shapes are out of scope for this transform.

Rewrites the CMakeLists to:

    cmake_minimum_required(VERSION 3.24)
    project(<proj> LANGUAGES C CXX)
    set(CMAKE_<C|CXX>_STANDARD <n>)          # preserved from the original
    set(CMAKE_<C|CXX>_STANDARD_REQUIRED ON)
    find_package(nano_ros REQUIRED)
    find_package(<dep> REQUIRED)             # one per package.xml <depend>
    nano_ros_add_executable(<name> <sources>)
    ament_target_dependencies(<name> <deps>) # when deps present
    install(TARGETS <name> DESTINATION lib/${PROJECT_NAME})
    ament_package()

and updates package.xml: `<build_type>cmake` -> `ament_cmake`, and inserts
`<nano_ros deploy="native"/>` into `<export>` when absent.

Usage:
    scripts/docs/migrate-example-cmake-ament.py --dry-run
    scripts/docs/migrate-example-cmake-ament.py            # apply
    scripts/docs/migrate-example-cmake-ament.py <path>...  # restrict
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# The W1/W2a bootstrap prelude: from the leading comment / guard through
# `nano_ros_bootstrap()` (+ trailing `# --- end nano-ros bootstrap ---`).
_PRELUDE = re.compile(
    r"(?:^[ \t]*#[^\n]*\n)*"                       # leading comment block
    r"^[ \t]*if\s*\(\s*NOT\s+DEFINED\s+NANO_ROS_ROOT.*?"
    r"nano_ros_bootstrap\s*\(\s*\)[ \t]*\n"
    r"(?:^[ \t]*#[ \t]*---[ \t]*end nano-ros bootstrap[^\n]*\n)?",
    re.M | re.S,
)

_PROJECT = re.compile(r"^\s*project\s*\(\s*([A-Za-z0-9_]+)\b(?P<rest>[^)]*)\)", re.M)
_STD = re.compile(
    r"^\s*set\s*\(\s*CMAKE_(C|CXX)_STANDARD\s+(\d+)\s*\)\s*$", re.M)
_ENTRY = re.compile(
    r"nano_ros_entry\s*\(\s*"
    r"NAME\s+(?P<name>[A-Za-z0-9_]+)\s+"
    r"SOURCES\s+(?P<srcs>(?:[^)]*?))\s+"
    r"DEPLOY\s+(?P<deploy>[A-Za-z0-9_ ]+?)\s*\)",
    re.S,
)
_LINK = re.compile(r"^\s*(?:nano_ros_link|nros_platform_link_app)\s*\(\s*([A-Za-z0-9_]+)\s*\)\s*$", re.M)
_IFACE = re.compile(r"^\s*(?:nros_find_interfaces|nano_ros_generate_interfaces|nros_generate_interfaces)\s*\([^\n]*\)\s*$", re.M)

# Bespoke markers that disqualify a leaf from the conservative transform.
_BESPOKE = re.compile(
    r"add_library\s*\(|add_executable\s*\(|target_link_libraries\s*\(|"
    r"target_compile_options\s*\(|target_include_directories\s*\(|"
    r"find_package\s*\(\s*(?!nano_ros\b)|add_subdirectory\s*\(|"
    r"target_compile_definitions\s*\(",
)


def _depends(pkgxml: Path) -> list[str]:
    if not pkgxml.exists():
        return []
    body = pkgxml.read_text()
    deps = re.findall(r"<depend>\s*([A-Za-z0-9_-]+)\s*</depend>", body)
    # Interface/tooling packages are not link deps.
    return [d for d in deps if not re.match(r"^(rosidl|ament|rclcpp|rcl|rmw)", d)]


def _std_line(text: str) -> tuple[str, str] | None:
    m = _STD.search(text)
    if not m:
        return None
    return m.group(1), m.group(2)


def _transform_cmake(text: str, pkgxml: Path) -> tuple[str | None, str]:
    """Return (new_text, reason). new_text is None when skipped."""
    if "nano_ros_add_executable" in text:
        return None, "already-ament"
    if "nano_ros_bootstrap(" not in text:
        return None, "not-bootstrap-shape"
    # Own-interface package (generates its OWN msg/srv from ${PROJECT_NAME}) is the
    # nano_ros_generate_interfaces (§5) case, not a plain executable — dropping its
    # generate line would lose the bindings. Handle separately.
    if re.search(r"generate_interfaces\s*\(\s*\$\{PROJECT_NAME\}", text) \
       or (pkgxml.exists() and "rosidl_interface_packages" in pkgxml.read_text()):
        return None, "own-msg-pkg"

    body_wo_prelude = _PRELUDE.sub("", text)

    m_entry = _ENTRY.search(text)
    if not m_entry:
        return None, "no-entry"
    if m_entry.group("deploy").strip() != "native":
        return None, "non-native-deploy"
    name = m_entry.group("name")
    srcs = " ".join(m_entry.group("srcs").split())

    # Strip the constructs we understand from the prelude-less body; whatever
    # remains (besides project/cmake_min/std/comments/blank) must be empty, else
    # the leaf is bespoke.
    residue = body_wo_prelude
    residue = _ENTRY.sub("", residue)
    residue = _LINK.sub("", residue)
    residue = _IFACE.sub("", residue)
    residue = re.sub(r"^\s*cmake_minimum_required\s*\([^\n]*\)\s*$", "", residue, flags=re.M)
    residue = _PROJECT.sub("", residue)
    residue = _STD.sub("", residue)
    residue = re.sub(r"^\s*set\s*\(\s*CMAKE_(C|CXX)_STANDARD_REQUIRED\s+ON\s*\)\s*$", "", residue, flags=re.M)
    residue = re.sub(r"^\s*#[^\n]*$", "", residue, flags=re.M)  # comments
    residue = residue.strip()
    if residue:
        return None, "bespoke-residue"
    if _BESPOKE.search(body_wo_prelude):
        return None, "bespoke-marker"

    # Project name + language list.
    m_proj = _PROJECT.search(text)
    if not m_proj:
        return None, "no-project"
    proj = m_proj.group(1)

    std = _std_line(text)
    deps = _depends(pkgxml)

    out = []
    out.append("cmake_minimum_required(VERSION 3.24)")
    out.append(f"project({proj} LANGUAGES C CXX)")
    out.append("")
    if std:
        lang, ver = std
        out.append(f"set(CMAKE_{lang}_STANDARD {ver})")
        out.append(f"set(CMAKE_{lang}_STANDARD_REQUIRED ON)")
        out.append("")
    out.append("find_package(nano_ros REQUIRED)")
    for d in deps:
        out.append(f"find_package({d} REQUIRED)")
    out.append("")
    out.append(f"nano_ros_add_executable({name} {srcs})")
    if deps:
        out.append(f"ament_target_dependencies({name} {' '.join(deps)})")
    out.append("")
    out.append(f"install(TARGETS {name} DESTINATION lib/${{PROJECT_NAME}})")
    out.append("ament_package()")
    return "\n".join(out) + "\n", "migrated"


def _transform_pkgxml(pkgxml: Path) -> bool:
    if not pkgxml.exists():
        return False
    body = pkgxml.read_text()
    orig = body
    body = re.sub(r"<build_type>\s*cmake\s*</build_type>",
                  "<build_type>ament_cmake</build_type>", body)
    if "<nano_ros" not in body:
        # Insert the tuple right after <build_type>, else at the start of an
        # existing <export>, else synthesize a whole <export> block before
        # </package> (ament packages carry a build_type export — a nano-ros leaf
        # that had none gets the canonical one now).
        if re.search(r"<build_type>[^<]*</build_type>", body):
            body = re.sub(r"(<build_type>[^<]*</build_type>)",
                          r'\1\n    <nano_ros deploy="native"/>', body, count=1)
        elif "<export>" in body:
            body = body.replace("<export>", '<export>\n    <nano_ros deploy="native"/>', 1)
        elif "</package>" in body:
            block = ("  <export>\n"
                     "    <build_type>ament_cmake</build_type>\n"
                     '    <nano_ros deploy="native"/>\n'
                     "  </export>\n")
            body = body.replace("</package>", block + "</package>", 1)
    if body != orig:
        pkgxml.write_text(body)
        return True
    return False


def leaves() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "examples/native/**/CMakeLists.txt"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()
    return [REPO / p for p in out if "/build" not in p]


def main() -> int:
    args = sys.argv[1:]
    dry = "--dry-run" in args
    targets = [REPO / a for a in args if not a.startswith("--")]
    files = [f for f in leaves() if not targets or f in targets]

    # --pkgxml-only: (re)apply the package.xml <nano_ros> tuple to every leaf that
    # already carries the ament CMake shape (fixes leaves migrated before the
    # pkgxml transform learned to synthesize a missing <export>).
    if "--pkgxml-only" in args:
        n = 0
        for f in files:
            if "nano_ros_add_executable" not in f.read_text():
                continue
            pkgxml = f.parent / "package.xml"
            if not dry and _transform_pkgxml(pkgxml):
                n += 1
                print(f"  pkgxml: {pkgxml.relative_to(REPO)}")
        print(f"\n{'DRY-RUN ' if dry else ''}{n} package.xml updated.")
        return 0

    migrated = skipped = 0
    reasons: dict[str, int] = {}
    for f in files:
        pkgxml = f.parent / "package.xml"
        new, reason = _transform_cmake(f.read_text(), pkgxml)
        if new is None:
            skipped += 1
            reasons[reason] = reasons.get(reason, 0) + 1
            if reason not in ("already-ament",):
                print(f"  SKIP [{reason}]: {f.relative_to(REPO)}")
            continue
        migrated += 1
        print(f"  migrate: {f.relative_to(REPO)}")
        if not dry:
            f.write_text(new)
            _transform_pkgxml(pkgxml)

    print(f"\n{'DRY-RUN ' if dry else ''}{migrated} migrated, {skipped} skipped "
          f"({', '.join(f'{k}={v}' for k, v in sorted(reasons.items()))}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
