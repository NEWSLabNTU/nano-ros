#!/usr/bin/env python3
"""Every build road that generates per-message type support also generates the
runtime's own interface types, through the ONE helper.

Issue 2704. A backend that resolves types through a descriptor table (Cyclone
DDS) creates an entity only for a type whose descriptor is linked into the
image. An application's interface packages reach that table through the road's
per-message typesupport generator; the runtime's own entities (the parameter
services, `/rosout`, `/parameter_events`, the lifecycle services) are typed
from packages the application never names (`rcl_interfaces`,
`lifecycle_msgs`), and reach it only through `nros_runtime_interface_types()`
(`cmake/NanoRosRuntimeInterfaceTypes.cmake`, issue 2304). The top-level cmake
road called it; the Zephyr module road did not, and a Zephyr image on Cyclone
booted without its parameter services.

The roads that can build a Cyclone image (2026-11-18 sweep):

  * top-level cmake  `CMakeLists.txt` -- native/posix, FreeRTOS (incl. the
                     `integrations/freertos` and `integrations/s32ds` shells),
                     ThreadX, PX4 (`find_package(nano_ros)` bootstraps it),
                     the NuttX workspace road: all `add_subdirectory(<nano-ros>)`;
  * Zephyr module    `zephyr/CMakeLists.txt` -- C, C++ and generated Rust
                     applications alike link the C++ Cyclone backend;
  * cargo            no cmake: `nros-node` registers each type's descriptor
                     at create time (`rmw_type_registry::register_type`)
                     through the backend's Rust registrar, so the runtime's
                     types are covered there by construction;
  * NuttX guest-make `integrations/nuttx` -- builds no Cyclone image at all
                     (issue 2705), so it has no per-message road to hold.

What this gate checks:

  1. Every cmake file that RUNS a per-message typesupport generator (calls
     `${NROS_RMW_PER_MESSAGE_HOOK}` or `_nros_zephyr_cyclonedds_typesupport`)
     belongs to a road in ROADS, and each road's root calls
     `nros_runtime_interface_types`. A new generator with no road here fails:
     add the road and give it the call, through the one helper.
  2. The axis -> package table the CLI's configure-time refusal reads
     (`backend_link::RUNTIME_INTERFACE_PACKAGES`) names the same packages per
     axis as the cmake helper generates.

Buildless; self-test first.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

HELPER = "cmake/NanoRosRuntimeInterfaceTypes.cmake"
RUST_TABLE = "packages/cli/nros-cli-core/src/orchestration/backend_link.rs"

# generator file -> the road root that must call the helper.
ROADS: dict[str, str] = {
    "cmake/NanoRosGenerateInterfaces.cmake": "CMakeLists.txt",
    "cmake/NanoRosRuntimeInterfaceTypes.cmake": "CMakeLists.txt",
    "zephyr/cmake/nros_cyclonedds_typesupport.cmake": "zephyr/CMakeLists.txt",
    "zephyr/cmake/nros_generate_interfaces.cmake": "zephyr/CMakeLists.txt",
}

GENERATOR_CALL = re.compile(
    r"cmake_language\(\s*CALL\s+\$\{(NROS_RMW_PER_MESSAGE_HOOK|_hook)\}"
    r"|^\s*_nros_zephyr_cyclonedds_typesupport\(",
    re.M,
)
HELPER_CALL = re.compile(r"\bnros_runtime_interface_types\b(?!\.cmake)")
COMMENT = re.compile(r"#.*$", re.M)

CMAKE_AXIS = re.compile(
    r'if\((?P<cond>[^\n]*IN_LIST features[^\n]*)\)\s*\n\s*list\(APPEND _pkgs (?P<pkgs>[^)]*)\)'
)
RUST_ROW = re.compile(r'\(\s*"(?P<axis>[a-z_]+)"\s*,\s*&\[(?P<pkgs>[^\]]*)\]\s*\)')


def strip(text: str) -> str:
    return COMMENT.sub("", text)


def generators(files: dict[str, str]) -> list[str]:
    return sorted(
        p for p, t in files.items() if p.endswith(".cmake") and GENERATOR_CALL.search(strip(t))
    )


def check_roads(files: dict[str, str]) -> list[str]:
    errs = []
    for gen in generators(files):
        root = ROADS.get(gen)
        if root is None:
            errs.append(
                f"{gen}: runs a per-message typesupport generator but belongs to no "
                "road in ROADS -- add its road root and make that root call "
                "nros_runtime_interface_types()"
            )
            continue
        body = files.get(root)
        if body is None:
            errs.append(f"{gen}: its road root {root} is gone")
        elif not HELPER_CALL.search(strip(body).replace("NanoRosRuntimeInterfaceTypes", "")):
            errs.append(
                f"{root}: generates per-message type support ({gen}) but never calls "
                "nros_runtime_interface_types(), so a descriptor-table backend gets no "
                "descriptor for the runtime's own entities"
            )
    for gen, root in ROADS.items():
        if gen not in files:
            errs.append(f"ROADS names {gen}, which is gone: update the table")
    return errs


def cmake_table(text: str) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for m in CMAKE_AXIS.finditer(strip(text)):
        pkgs = {p.strip('"').split(":")[0] for p in m.group("pkgs").split()}
        for axis in re.findall(r'"([a-z_]+)" IN_LIST features', m.group("cond")):
            out.setdefault(axis, set()).update(pkgs)
    return out


def rust_table(text: str) -> dict[str, set[str]]:
    start = text.index("RUNTIME_INTERFACE_PACKAGES")
    end = text.index("];", start)
    out: dict[str, set[str]] = {}
    for m in RUST_ROW.finditer(text[start:end]):
        out[m.group("axis")] = set(re.findall(r'"([a-z_]+)"', m.group("pkgs")))
    return out


def check_tables(helper: str, rust: str) -> list[str]:
    c, r = cmake_table(helper), rust_table(rust)
    if not c or not r:
        return [f"could not read the axis tables (cmake {c}, rust {r})"]
    if c != r:
        return [
            f"{HELPER} generates {dict(sorted(c.items()))} but {RUST_TABLE}'s "
            f"RUNTIME_INTERFACE_PACKAGES says {dict(sorted(r.items()))}: keep them equal"
        ]
    return []


def self_test() -> bool:
    helper = (
        'if("param_services" IN_LIST features OR "rosout" IN_LIST features)\n'
        '    list(APPEND _pkgs "builtin_interfaces" "rcl_interfaces:builtin_interfaces")\n'
        "endif()\n"
        'if("lifecycle" IN_LIST features)\n'
        '    list(APPEND _pkgs "lifecycle_msgs")\n'
        "endif()\n"
        "cmake_language(CALL ${_hook} TARGET x)\n"
    )
    rust_ok = (
        "pub const RUNTIME_INTERFACE_PACKAGES: &[(&str, &[&str])] = &[\n"
        '    ("param_services", &["builtin_interfaces", "rcl_interfaces"]),\n'
        '    ("rosout", &["builtin_interfaces", "rcl_interfaces"]),\n'
        '    ("lifecycle", &["lifecycle_msgs"]),\n];\n'
    )
    if check_tables(helper, rust_ok):
        return False
    if not check_tables(helper, rust_ok.replace('("lifecycle", &["lifecycle_msgs"]),\n', "")):
        return False
    base = {
        "cmake/NanoRosGenerateInterfaces.cmake": "cmake_language(CALL ${NROS_RMW_PER_MESSAGE_HOOK} X)\n",
        "cmake/NanoRosRuntimeInterfaceTypes.cmake": helper,
        "zephyr/cmake/nros_cyclonedds_typesupport.cmake": "  _nros_zephyr_cyclonedds_typesupport(x)\n",
        "zephyr/cmake/nros_generate_interfaces.cmake": "  _nros_zephyr_cyclonedds_typesupport(x)\n",
        "CMakeLists.txt": "nros_runtime_interface_types(a)\n",
        "zephyr/CMakeLists.txt": "nros_runtime_interface_types(a)\n",
    }
    if check_roads(base):
        return False
    # The pre-fix Zephyr road: a generator, no call (only a comment naming it).
    bad = dict(base)
    bad["zephyr/CMakeLists.txt"] = "# nros_runtime_interface_types is the cmake road's\n"
    if not check_roads(bad):
        return False
    # A new generator with no road.
    new = dict(base)
    new["cmake/board/NewRoad.cmake"] = "  _nros_zephyr_cyclonedds_typesupport(x)\n"
    if not check_roads(new):
        return False
    return True


def main() -> int:
    if not self_test():
        print("check-runtime-interface-types-roads: self-test FAILED")
        return 1
    files: dict[str, str] = {}
    for p in list(REPO.glob("cmake/**/*.cmake")) + list(REPO.glob("zephyr/**/*.cmake")) + list(
        REPO.glob("integrations/**/*.cmake")
    ) + list(REPO.glob("packages/rmw/**/*.cmake")):
        rel = p.relative_to(REPO).as_posix()
        if "/target/" in rel or "/build" in rel:
            continue
        files[rel] = p.read_text(encoding="utf-8", errors="replace")
    for root in set(ROADS.values()):
        files[root] = (REPO / root).read_text(encoding="utf-8")
    if not generators(files):
        print("check-runtime-interface-types-roads: found no per-message generator, refusing to pass")
        return 1
    errs = check_roads(files) + check_tables(
        files[HELPER], (REPO / RUST_TABLE).read_text(encoding="utf-8")
    )
    if errs:
        print("check-runtime-interface-types-roads: FAIL\n")
        for e in errs:
            print(f"  {e}")
        return 1
    print(
        f"check-runtime-interface-types-roads: OK ({len(generators(files))} generator file(s) "
        f"on {len(set(ROADS.values()))} road(s); axis tables agree)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
