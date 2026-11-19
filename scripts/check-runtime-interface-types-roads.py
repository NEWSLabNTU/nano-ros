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
  * NuttX guest-make `integrations/nuttx` -- builds no Cyclone image: it
                     refuses one at configure time (issue 2705), so it has
                     no per-message road to hold.

What this gate checks:

  1. Every cmake file that RUNS a per-message typesupport generator (calls
     `${NROS_RMW_PER_MESSAGE_HOOK}` or `_nros_zephyr_cyclonedds_typesupport`)
     belongs to a road in ROADS, and each road's root calls
     `nros_runtime_interface_types`. A new generator with no road here fails:
     add the road and give it the call, through the one helper.
  2. ONE table. The runtime-owned axis -> packages -> registered types live
     in `packages/core/nros-orchestration-ir/data/runtime-interface-types.json`
     only: the cmake helper reads it (`string(JSON)`), the CLI's refusal and
     the cargo road's count read it (`nros_orchestration_ir::
     runtime_interface_types`). Neither the helper nor `backend_link.rs` may
     spell a runtime package list of its own again.
  3. The table's `registers` column is what nros-node creates: the types
     named at the executor's own creation sites (`create_param_srv::<..>`,
     the `/parameter_events` and `/rosout` writers, `create_lc_srv::<..>`,
     `create_lc_pub::<..>` in `executor/spin.rs`) equal the axis's
     `registers`, so a new runtime-owned entity cannot be created with a type
     the cargo count (and the table) does not have.

Buildless; self-test first.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

HELPER = "cmake/NanoRosRuntimeInterfaceTypes.cmake"
RUST_REFUSAL = "packages/cli/nros-cli-core/src/orchestration/backend_link.rs"
TABLE = "packages/core/nros-orchestration-ir/data/runtime-interface-types.json"
SPIN = "packages/core/nros-node/src/executor/spin.rs"

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

# A runtime package list spelled outside the table: a literal package name in
# the helper's code, or a Rust `(axis, &[pkgs])` row in the refusal's file.
RUNTIME_PKGS = ("builtin_interfaces", "rcl_interfaces", "lifecycle_msgs")
RUST_ROW = re.compile(r'\(\s*"(?:param_services|rosout|lifecycle)"\s*,\s*&\[')

# The executor's own creation sites in nros-node, per axis: (axis, ROS package,
# kind, regex whose group 1 is the type name).
CREATION_SITES = (
    (
        "param_services",
        "rcl_interfaces",
        "srv",
        re.compile(r"create_param_srv::<(\w+)>\("),
    ),
    (
        "param_services",
        "rcl_interfaces",
        "msg",
        re.compile(
            r"fn create_param_event_pub\b[^{]*\{\s*type Event = nros_rcl_interfaces::msg::(\w+);"
        ),
    ),
    (
        "rosout",
        "rcl_interfaces",
        "msg",
        re.compile(
            r"fn create_rosout_pub\b[^{]*\{\s*type Log = nros_rcl_interfaces::msg::(\w+);"
        ),
    ),
    ("lifecycle", "lifecycle_msgs", "srv", re.compile(r"create_lc_srv::<(\w+)>\(")),
    (
        "lifecycle",
        "lifecycle_msgs",
        "msg",
        re.compile(r"create_lc_pub::<nros_lifecycle_msgs::msg::(\w+)>\("),
    ),
)


def strip(text: str) -> str:
    return COMMENT.sub("", text)


def generators(files: dict[str, str]) -> list[str]:
    return sorted(
        p
        for p, t in files.items()
        if p.endswith(".cmake") and GENERATOR_CALL.search(strip(t))
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
        elif not HELPER_CALL.search(
            strip(body).replace("NanoRosRuntimeInterfaceTypes", "")
        ):
            errs.append(
                f"{root}: generates per-message type support ({gen}) but never calls "
                "nros_runtime_interface_types(), so a descriptor-table backend gets no "
                "descriptor for the runtime's own entities"
            )
    for gen, root in ROADS.items():
        if gen not in files:
            errs.append(f"ROADS names {gen}, which is gone: update the table")
    return errs


def check_single_source(helper: str, rust: str) -> list[str]:
    errs = []
    code = strip(helper)
    if "runtime-interface-types.json" not in code or "string(JSON" not in code:
        errs.append(f"{HELPER}: does not read the runtime interface table ({TABLE})")
    for pkg in RUNTIME_PKGS:
        if re.search(rf"\b{pkg}\b", code):
            errs.append(
                f"{HELPER}: spells `{pkg}` in its code; the runtime packages come from "
                f"{TABLE} only"
            )
    if RUST_ROW.search(rust):
        errs.append(
            f"{RUST_REFUSAL}: spells an axis -> package row; read "
            "nros_orchestration_ir::runtime_interface_types instead"
        )
    return errs


def created_types(spin: str) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for axis, pkg, kind, rx in CREATION_SITES:
        for name in rx.findall(spin):
            out.setdefault(axis, set()).add(f"{pkg}/{kind}/{name}")
    return out


def check_registers(table: dict, spin: str) -> list[str]:
    created = created_types(spin)
    errs = []
    axes = {a["axis"]: set(a["registers"]) for a in table["axes"]}
    for axis in sorted(set(axes) | set(created)):
        want, got = axes.get(axis, set()), created.get(axis, set())
        if not got:
            errs.append(
                f"{SPIN}: found no creation site for axis `{axis}`; update CREATION_SITES "
                "in this gate to the executor's code"
            )
        elif want != got:
            errs.append(
                f"axis `{axis}`: nros-node creates {sorted(got)} but {TABLE} registers "
                f"{sorted(want)}: keep them equal (the cargo road sizes the Cyclone tables "
                "from the table)"
            )
    return errs


def self_test() -> bool:
    helper = (
        'file(READ "${_dir}/runtime-interface-types.json" _json)\n'
        'string(JSON _n LENGTH "${_json}" axes)\n'
        "cmake_language(CALL ${_hook} TARGET x)\n"
    )
    if check_single_source(helper, "fn f() {}\n"):
        return False
    # The pre-table helper spelled its own list.
    if not check_single_source(
        helper + 'list(APPEND _pkgs "lifecycle_msgs")\n', "fn f() {}\n"
    ):
        return False
    if not check_single_source(helper, '("lifecycle", &["lifecycle_msgs"]),\n'):
        return False
    table = {
        "axes": [
            {"axis": "rosout", "registers": ["rcl_interfaces/msg/Log"]},
            {"axis": "lifecycle", "registers": ["lifecycle_msgs/srv/GetState"]},
        ]
    }
    spin = (
        "fn create_rosout_pub(\n    exec: &mut X,\n) -> R {\n"
        "    type Log = nros_rcl_interfaces::msg::Log;\n}\n"
        "let h = create_lc_srv::<GetState>(\n"
    )
    if check_registers(table, spin):
        return False
    # A fourth runtime-owned type created but not in the table.
    if not check_registers(table, spin + "let h = create_lc_srv::<ChangeState>(\n"):
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
    bad["zephyr/CMakeLists.txt"] = (
        "# nros_runtime_interface_types is the cmake road's\n"
    )
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
    tracked = (
        subprocess.run(
            [
                "git",
                "ls-files",
                "-z",
                "--",
                "cmake",
                "zephyr",
                "integrations",
                "packages/rmw",
            ],
            cwd=REPO,
            capture_output=True,
            check=True,
        )
        .stdout.decode()
        .split("\0")
    )
    for rel in tracked:
        if rel.endswith(".cmake"):
            files[rel] = (REPO / rel).read_text(encoding="utf-8", errors="replace")
    for root in set(ROADS.values()):
        files[root] = (REPO / root).read_text(encoding="utf-8")
    if not generators(files):
        print(
            "check-runtime-interface-types-roads: found no per-message generator, refusing to pass"
        )
        return 1
    table = json.loads((REPO / TABLE).read_text(encoding="utf-8"))
    errs = (
        check_roads(files)
        + check_single_source(
            files[HELPER], (REPO / RUST_REFUSAL).read_text(encoding="utf-8")
        )
        + check_registers(table, (REPO / SPIN).read_text(encoding="utf-8"))
    )
    if errs:
        print("check-runtime-interface-types-roads: FAIL\n")
        for e in errs:
            print(f"  {e}")
        return 1
    print(
        f"check-runtime-interface-types-roads: OK ({len(generators(files))} generator file(s) "
        f"on {len(set(ROADS.values()))} road(s); one runtime interface table, "
        f"its registers equal nros-node's creation sites)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
