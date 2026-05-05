#!/usr/bin/env python3
"""
Phase 117.X.1 — `.msg` / `.srv` → ROS-shaped Cyclone DDS IDL.

Drives ROS 2's stock `rosidl_adapter` (msg2idl.py / srv2idl.py)
to convert a ROS interface file into an IDL file, then post-
processes the IDL to inject the `dds_::` namespace and trailing
`_` mangling that `rmw_cyclonedds_cpp` uses for its
`dds_topic_descriptor_t::m_typename` field.

Result: the descriptor produced by `idlc -t -l c <output>.idl`
has `m_typename` exactly equal to `<pkg>::<msg|srv>::dds_::<Type>_`
— the same string stock RMW emits — so a nano-ros publisher and
an `rclcpp` subscriber on the same topic name match without
translation.

Usage:
    msg_to_cyclone_idl.py
        --pkg-name <name>
        --pkg-dir  <path/to/pkg/with/package.xml>
        --output-dir <where to write *.idl>
        --interface <path/to/Foo.msg|Foo.srv> [...]

Output layout:
    <output-dir>/
        <Type>.idl   # mangled, ready for `idlc -t -l c`

The script prints absolute paths of generated IDL files to stdout,
one per line, suitable for use as a CMake custom-command output
list.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


# Default location of rosidl_adapter scripts. Override via env
# `NROS_ROSIDL_ADAPTER_BIN_DIR` if a non-Humble distro is in use.
DEFAULT_ADAPTER_BIN = Path(
    os.environ.get(
        "NROS_ROSIDL_ADAPTER_BIN_DIR",
        "/opt/ros/humble/lib/rosidl_adapter",
    )
)


def find_adapter(name: str) -> Path:
    p = DEFAULT_ADAPTER_BIN / name
    if not p.is_file():
        sys.exit(
            f"error: rosidl_adapter helper {name} not found at {p}.\n"
            "Set NROS_ROSIDL_ADAPTER_BIN_DIR to the directory containing\n"
            "msg2idl.py / srv2idl.py, or install ros-humble-rosidl-adapter."
        )
    return p


def run_adapter(adapter_script: Path, pkg_dir: Path, rel_iface: Path) -> Path:
    """Invoke msg2idl.py / srv2idl.py, returning the produced .idl path
    (relative to pkg_dir)."""
    # rosidl_adapter expects to be invoked from inside the package
    # directory with the interface file path relative to the package.
    cmd = [sys.executable, str(adapter_script), str(rel_iface)]
    result = subprocess.run(
        cmd,
        cwd=pkg_dir,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        sys.exit(
            f"error: {adapter_script.name} failed on "
            f"{pkg_dir}/{rel_iface} (exit {result.returncode})"
        )
    # rosidl_adapter writes alongside the input with .idl extension.
    expected = (pkg_dir / rel_iface).with_suffix(".idl")
    if not expected.is_file():
        sys.exit(f"error: expected {expected} not produced by adapter")
    return expected


# Patterns that match the canonical rosidl_adapter output. Examples:
#
#   module my_msgs {
#     module msg {
#       struct MyString {
#         string data;
#       };
#     };
#   };
#
# We need to wrap every `struct <Name>` in a `module dds_` block
# and rename to `<Name>_`. Multiple structs in one file (e.g. a
# .srv generates Request + Response in a single .idl) are all
# rewritten.

_STRUCT_RE = re.compile(
    r"(?P<indent>[ \t]*)struct\s+(?P<name>[A-Za-z_]\w*)\s*\{",
)


def mangle_idl(src: str) -> str:
    """Rewrite a rosidl_adapter IDL string by inserting `module dds_ { … }`
    around every top-level (within its enclosing namespace) struct and
    suffixing the struct name with `_`. The rosidl_adapter output is
    well-behaved enough that a line-oriented rewrite is sufficient — no
    full IDL parser needed.
    """
    out_lines: list[str] = []
    nesting: list[str] = []  # stack of opened wrapper indents
    for line in src.splitlines(keepends=False):
        m = _STRUCT_RE.match(line)
        if m:
            indent = m.group("indent")
            name = m.group("name")
            out_lines.append(f"{indent}module dds_ {{")
            # Re-indent the struct line itself by 2 spaces under the
            # new wrapper.
            new_indent = indent + "  "
            rest = line[len(m.group(0)):]  # everything after `struct <Name> {`
            out_lines.append(f"{new_indent}struct {name}_ {{{rest}")
            nesting.append(indent)
            continue

        # Track when we exit the struct so we can close the wrapper.
        if nesting and line.rstrip() == nesting[-1] + "};":
            indent = nesting.pop()
            # Close the original `};` with the extra indent we added,
            # then close the wrapper at the original indent.
            out_lines.append(f"{indent}  }};")
            out_lines.append(f"{indent}}};")
            continue

        # Indent every line that's currently inside a wrapped struct
        # by an extra two spaces so the struct body's indentation
        # stays consistent.
        if nesting:
            out_lines.append("  " + line)
        else:
            out_lines.append(line)

    return "\n".join(out_lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Convert .msg/.srv to Cyclone-DDS-shaped IDL.",
    )
    ap.add_argument("--pkg-name", required=True)
    ap.add_argument("--pkg-dir", required=True, type=Path)
    ap.add_argument("--output-dir", required=True, type=Path)
    ap.add_argument(
        "--interface",
        action="append",
        required=True,
        help="Path to .msg / .srv (relative to --pkg-dir or absolute). "
             "Repeatable.",
    )
    args = ap.parse_args()

    if not args.pkg_dir.is_dir():
        sys.exit(f"error: --pkg-dir {args.pkg_dir} is not a directory")
    if not (args.pkg_dir / "package.xml").is_file():
        sys.exit(
            f"error: {args.pkg_dir}/package.xml not found "
            "(rosidl_adapter requires it)"
        )

    args.output_dir.mkdir(parents=True, exist_ok=True)

    msg2idl = find_adapter("msg2idl.py")
    srv2idl = find_adapter("srv2idl.py")

    out_paths: list[Path] = []

    for iface in args.interface:
        iface_path = Path(iface)
        if not iface_path.is_absolute():
            iface_path = (args.pkg_dir / iface_path).resolve()
        rel = iface_path.relative_to(args.pkg_dir.resolve())

        if iface_path.suffix == ".msg":
            adapter = msg2idl
        elif iface_path.suffix == ".srv":
            adapter = srv2idl
        else:
            sys.exit(
                f"error: unsupported interface extension "
                f"{iface_path.suffix} (expected .msg or .srv)"
            )

        # rosidl_adapter writes <input>.idl next to the input. We may
        # not have write access there, so copy the .msg to a temp
        # workspace that mirrors the package layout, run the adapter,
        # then move the result into --output-dir + post-process.
        with tempfile.TemporaryDirectory() as tmp:
            scratch = Path(tmp) / args.pkg_name
            scratch.mkdir()
            shutil.copy(args.pkg_dir / "package.xml", scratch / "package.xml")
            (scratch / rel.parent).mkdir(parents=True, exist_ok=True)
            shutil.copy(iface_path, scratch / rel)

            generated_idl = run_adapter(adapter, scratch, rel)
            raw = generated_idl.read_text()

        mangled = mangle_idl(raw)

        out_idl = args.output_dir / iface_path.with_suffix(".idl").name
        out_idl.write_text(mangled)
        out_paths.append(out_idl.resolve())

    for p in out_paths:
        print(p)
    return 0


if __name__ == "__main__":
    sys.exit(main())
