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




def _guard_typedefs(pkg_name: str, text: str) -> str:
    """CPP-guard `typedef` lines per (package, typedef-name).

    rosidl emits array typedefs like `typedef double double__36[36];` into
    EVERY file whose struct uses that array shape; two such files in one
    package (PoseWithCovariance + TwistWithCovariance) redeclare the name
    in the same reopened module and idlc 0.10.5 errors ("Declaration
    'double__36' collides"). Package-scoped so another package's own
    module still gets its copy.
    """
    out = []
    for line in text.split("\n"):
        m = re.match(r"^(\s*)typedef\s+\S.*?(\w+)\s*\[[^\]]*\]\s*;\s*$", line)
        if m:
            tag = f"NROS_TD_{pkg_name}_{m.group(2)}".upper().replace("-", "_")
            out.append(f"#ifndef {tag}")
            out.append(f"#define {tag}")
            out.append(line)
            out.append(f"#endif /* {tag} */")
        else:
            out.append(line)
    return "\n".join(out)


def _guard_wrap(pkg_name: str, out_idl: Path, text: str) -> str:
    """Wrap an emitted IDL in a C-preprocessor include guard.

    idlc runs a CPP pass over `#include` but (0.10.5) does NOT dedupe a
    diamond include — `Control.idl -> {Lateral,Longitudinal} -> Time.idl`
    re-declares `Time_` and idlc aborts in `delete_const_expr`
    ("Declaration 'Time_' collides"). Standard guards fix every diamond
    (phase-292 W2, ASI wall #8 follow-on).
    """
    tag = f"NROS_IDL_GUARD_{pkg_name}_{out_idl.stem}".upper().replace("-", "_")
    text = _guard_typedefs(pkg_name, text)
    return f"#ifndef {tag}\n#define {tag}\n{text}#endif /* {tag} */\n"


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

# Composite messages reference nested message types by their
# rosidl_adapter scoped name, e.g. `std_msgs::msg::MultiArrayDimension`
# or `builtin_interfaces::msg::Time`. Those referenced types are
# themselves mangled into a `dds_` sub-module with a `_`-suffixed name
# (see `mangle_idl`), so the reference must be rewritten to match —
# `std_msgs::msg::dds_::MultiArrayDimension_` — or idlc fails to resolve
# the scoped name. Match only `<pkg>::<msg|srv|action>::<Type>` triples,
# which is exactly how rosidl_adapter renders cross-type references.
_SCOPED_REF_RE = re.compile(
    r"\b([A-Za-z_]\w*)::(msg|srv|action)::([A-Za-z_]\w*)\b",
)


def _mangle_scoped_refs(line: str) -> str:
    """Rewrite nested type references to their mangled `dds_::<Type>_`
    form. A reference already pointing at `dds_::` is left alone."""
    def _sub(m: "re.Match[str]") -> str:
        pkg, kind, ty = m.group(1), m.group(2), m.group(3)
        return f"{pkg}::{kind}::dds_::{ty}_"

    # Skip references that already carry the `dds_` segment to keep the
    # rewrite idempotent.
    if "::dds_::" in line:
        return line
    return _SCOPED_REF_RE.sub(_sub, line)


# Phase 117.X.3 / 117.12.B — leading fields injected into every
# `_Request_` / `_Response_` struct so the wire CDR matches stock
# `rmw_cyclonedds_cpp`'s `cdds_request_header_t { uint64_t guid;
# int64_t seq; }` layout (16 bytes total, see upstream
# `src/serdata.hpp:73-77`). We inline the two primitive fields rather
# than declare a nested struct so each IDL stays self-contained — the
# wire bytes are identical either way.
SERVICE_HEADER_FIELDS = [
    "unsigned long long rmw_writer_guid;",
    "long long rmw_sequence_number;",
]


def mangle_idl(src: str, inject_service_header: bool = False) -> str:
    """Rewrite a rosidl_adapter IDL string by inserting `module dds_ { … }`
    around every top-level (within its enclosing namespace) struct and
    suffixing the struct name with `_`. The rosidl_adapter output is
    well-behaved enough that a line-oriented rewrite is sufficient — no
    full IDL parser needed.

    When ``inject_service_header`` is true, also injects the 16-byte
    request-id header fields (`unsigned long long rmw_writer_guid` +
    `long long rmw_sequence_number`) as the first two fields of every
    rewritten struct. Used for `.srv` inputs to make the wire CDR
    match stock `rmw_cyclonedds_cpp`'s `cdds_request_header_t` layout.
    """
    out_lines: list[str] = []
    nesting: list[str] = []  # stack of opened wrapper indents
    for line in src.splitlines(keepends=False):
        # Mangle nested type references before any structural handling.
        # The struct-open / wrapper / `};` lines carry no scoped refs,
        # so this only rewrites member-field declarations.
        line = _mangle_scoped_refs(line)
        m = _STRUCT_RE.match(line)
        if m:
            indent = m.group("indent")
            name = m.group("name")
            out_lines.append(f"{indent}module dds_ {{")
            # Re-indent the struct line itself by 2 spaces under the
            # new wrapper.
            new_indent = indent + "  "
            field_indent = new_indent + "  "
            rest = line[len(m.group(0)):]  # everything after `struct <Name> {`
            out_lines.append(f"{new_indent}struct {name}_ {{{rest}")
            if inject_service_header:
                for hdr in SERVICE_HEADER_FIELDS:
                    out_lines.append(f"{field_indent}{hdr}")
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


# A field declaration line in rosidl_adapter IDL output, e.g.
# `int32 order;` or `sequence<int32> sequence;`. Used to lift the base
# Goal/Result/Feedback struct bodies out of msg2idl output so the action
# synthesizer can reuse rosidl's type mapping without reimplementing it.
_FIELD_DECL_RE = re.compile(r"^\s*[A-Za-z_][\w:<>, ]*\s+[A-Za-z_]\w*;\s*$")


def _action_msg_fields(pkg_name: str, msg2idl: Path, body: str) -> list[str]:
    """Run msg2idl on a synthetic `.msg` carrying @p body and return the
    generated struct's field-declaration lines (rosidl-typed IDL). Empty
    sections (e.g. an action with no feedback fields) yield ``[]``."""
    if not body.strip():
        return []
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp) / pkg_name
        (scratch / "msg").mkdir(parents=True)
        # rosidl_adapter requires a package.xml; a minimal stub suffices
        # because we only consume the struct body, not package metadata.
        (scratch / "package.xml").write_text(
            f'<?xml version="1.0"?>\n<package format="3">'
            f"<name>{pkg_name}</name><version>0.0.0</version>"
            "<description>x</description>"
            "<maintainer email=\"x@example.com\">x</maintainer>"
            "<license>x</license></package>\n"
        )
        msg_path = scratch / "msg" / "ActionSection.msg"
        msg_path.write_text(body if body.endswith("\n") else body + "\n")
        idl = run_adapter(msg2idl, scratch, Path("msg/ActionSection.msg"))
        raw = idl.read_text()
    return [_escape_member(line.strip()) for line in raw.splitlines() if _FIELD_DECL_RE.match(line)]


# IDL reserved words that can legally appear as ROS field names but
# collide with the grammar as member identifiers (Cyclone 0.10.5's idlc
# rejects them). `Fibonacci_Result` has a field literally named
# `sequence`. Escape such members with a leading `_` (IDL escaped
# identifier) — the wire CDR is positional, so the rename is invisible.
_IDL_RESERVED = {
    "sequence", "string", "wstring", "long", "short", "double", "float",
    "char", "wchar", "boolean", "octet", "struct", "union", "enum",
    "module", "interface", "typedef", "const", "fixed", "native", "any",
    "void", "in", "out", "inout", "switch", "case", "default", "unsigned",
}


def _escape_member(decl: str) -> str:
    m = re.match(r"^(?P<type>.*\s)(?P<name>[A-Za-z_]\w*);\s*$", decl)
    if not m:
        return decl
    name = m.group("name")
    if name in _IDL_RESERVED:
        return f"{m.group('type')}_{name};"
    return decl


def synthesize_action_idl(pkg_name: str, action_path: Path, msg2idl: Path) -> str:
    """Synthesize the Cyclone IDL for a ROS `.action`, matching the nros
    action layer's wire framing (NOT stock rmw_cyclonedds_cpp):

      - `goal_id` is a fixed `octet[16]`, matching ROS 2 UUID layout.
        The Cyclone backend strips/reinserts the nano-ros raw-layer
        4-byte length prefix at the service/topic boundary.
      - send_goal / get_result Request+Response carry the 16-byte service
        header (`rmw_writer_guid` + `rmw_sequence_number`) inlined first,
        like a `.srv`. The feedback message has no header.
      - The accept reply / get_result reply / feedback are assembled by
        the action layer from primitives (`bool accepted` + `int32 sec`
        + `uint32 nanosec`; `int8 status` + result; goal_id + feedback),
        so the wrappers nest the base structs / inline those primitives.

    Emits all eight types in the `<pkg>::action::dds_::` namespace:
    `<A>_{Goal,Result,Feedback}_`, `<A>_SendGoal_{Request,Response}_`,
    `<A>_GetResult_{Request,Response}_`, `<A>_FeedbackMessage_`.
    """
    stem = action_path.stem
    text = action_path.read_text()
    sections: list[list[str]] = [[]]
    for line in text.splitlines():
        if line.strip() == "---":
            sections.append([])
        else:
            sections[-1].append(line)
    while len(sections) < 3:
        sections.append([])
    goal_fields = _action_msg_fields(pkg_name, msg2idl, "\n".join(sections[0]))
    result_fields = _action_msg_fields(pkg_name, msg2idl, "\n".join(sections[1]))
    feedback_fields = _action_msg_fields(pkg_name, msg2idl, "\n".join(sections[2]))

    hdr = [
        "unsigned long long rmw_writer_guid;",
        "long long rmw_sequence_number;",
    ]
    ns = f"{pkg_name}::action::dds_"

    def struct(name: str, fields: list[str]) -> list[str]:
        out = [f"      struct {name} {{"]
        out += [f"        {f}" for f in fields]
        out.append("      };")
        return out

    body: list[str] = []
    body += struct(f"{stem}_Goal_", goal_fields or ["uint8 structure_needs_at_least_one_member;"])
    body += struct(f"{stem}_Result_", result_fields or ["uint8 structure_needs_at_least_one_member;"])
    body += struct(f"{stem}_Feedback_", feedback_fields or ["uint8 structure_needs_at_least_one_member;"])
    body += struct(
        f"{stem}_SendGoal_Request_",
        hdr + ["octet goal_id[16];", f"{ns}::{stem}_Goal_ goal;"],
    )
    body += struct(
        f"{stem}_SendGoal_Response_",
        hdr + ["boolean accepted;", "int32 stamp_sec;", "uint32 stamp_nanosec;"],
    )
    body += struct(
        f"{stem}_GetResult_Request_",
        hdr + ["octet goal_id[16];"],
    )
    body += struct(
        f"{stem}_GetResult_Response_",
        hdr + ["int8 status;", f"{ns}::{stem}_Result_ result;"],
    )
    body += struct(
        f"{stem}_FeedbackMessage_",
        ["octet goal_id[16];", f"{ns}::{stem}_Feedback_ feedback;"],
    )

    lines = [
        f"// Auto-synthesized by msg_to_cyclone_idl.py from {pkg_name}/action/{stem}.action.",
        "// Uses ROS 2 UUID-compatible fixed goal_id storage; the Cyclone",
        "// backend adapts the nano-ros raw length prefix at runtime.",
        f"module {pkg_name} {{",
        "  module action {",
        "    module dds_ {",
    ]
    lines += body
    lines += ["    };", "  };", "};", ""]
    return "\n".join(lines)


def action_type_names(pkg_name: str, stem: str) -> list[str]:
    """The eight registrable type names an action emits, in the order the
    backend looks them up."""
    ns = f"{pkg_name}::action::dds_"
    return [
        f"{ns}::{stem}_Goal_",
        f"{ns}::{stem}_Result_",
        f"{ns}::{stem}_Feedback_",
        f"{ns}::{stem}_SendGoal_Request_",
        f"{ns}::{stem}_SendGoal_Response_",
        f"{ns}::{stem}_GetResult_Request_",
        f"{ns}::{stem}_GetResult_Response_",
        f"{ns}::{stem}_FeedbackMessage_",
    ]


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
            iface_path = args.pkg_dir / iface_path
        # Resolve symlinks on BOTH sides before computing the package-
        # relative path. The Zephyr west workspace exposes the repo through a
        # `nano-ros` symlink, so an ABSOLUTE `--interface` arg arrives
        # symlinked while `pkg_dir.resolve()` follows the link — `relative_to`
        # then raised "not in the subpath / one relative one absolute" (the
        # absolute branch used to skip the resolve, so only pkg_dir was
        # canonicalised). Consistent resolution keeps `rel` = the package-
        # relative interface path for the scratch-dir adapter copy below.
        iface_path = iface_path.resolve()
        rel = iface_path.relative_to(args.pkg_dir.resolve())

        if iface_path.suffix == ".action":
            # Actions are synthesized directly (rosidl ships no
            # action2idl that emits the SendGoal/GetResult/FeedbackMessage
            # wrappers, and the nros wire framing diverges from stock —
            # see `synthesize_action_idl`).
            mangled = synthesize_action_idl(args.pkg_name, iface_path, msg2idl)
            out_idl = args.output_dir / iface_path.with_suffix(".idl").name
            out_idl.write_text(_guard_wrap(args.pkg_name, out_idl, mangled))
            out_paths.append(out_idl.resolve())
            continue

        if iface_path.suffix == ".msg":
            adapter = msg2idl
        elif iface_path.suffix == ".srv":
            adapter = srv2idl
        else:
            sys.exit(
                f"error: unsupported interface extension "
                f"{iface_path.suffix} (expected .msg, .srv, or .action)"
            )

        # rosidl_adapter writes <input>.idl next to the input. We may
        # not have write access there, so copy the .msg to a temp
        # workspace that mirrors the package layout, run the adapter,
        # then move the result into --output-dir + post-process.
        with tempfile.TemporaryDirectory() as tmp:
            scratch = Path(tmp) / args.pkg_name
            scratch.mkdir()
            # rosidl_adapter derives the IDL's top-level `module <pkg>` from
            # the package.xml `<name>` in its CWD — NOT from a CLI flag. Copying
            # the caller's real package.xml breaks whenever `--pkg-name` differs
            # from that `<name>`: an example that bundles a `std_msgs/String`
            # under its OWN package dir (`--pkg-name std_msgs --pkg-dir
            # <example>`, the example's `<name>` = the node target) emitted the
            # descriptor as `<target>_msg_dds__String__desc` while the register
            # TU (driven by `--pkg-name`) referenced `std_msgs_msg_dds__String__desc`
            # → undefined-reference at link. Synthesise a minimal package.xml
            # whose `<name>` IS `--pkg-name` so the module, the descriptor
            # symbol, and the register TU all agree (matches the action path
            # above). msg2idl reads only `<name>`, so a stub suffices.
            (scratch / "package.xml").write_text(
                f'<?xml version="1.0"?>\n<package format="3">'
                f"<name>{args.pkg_name}</name><version>0.0.0</version>"
                "<description>x</description>"
                '<maintainer email="x@example.com">x</maintainer>'
                "<license>x</license></package>\n"
            )
            (scratch / rel.parent).mkdir(parents=True, exist_ok=True)
            shutil.copy(iface_path, scratch / rel)

            generated_idl = run_adapter(adapter, scratch, rel)
            raw = generated_idl.read_text()

        # `.srv` files produce a Request + Response struct pair; both
        # carry the request-id header in their wire CDR per stock
        # `rmw_cyclonedds_cpp` convention. `.msg` files don't.
        inject_header = (iface_path.suffix == ".srv")
        mangled = mangle_idl(raw, inject_service_header=inject_header)

        out_idl = args.output_dir / iface_path.with_suffix(".idl").name
        out_idl.write_text(_guard_wrap(args.pkg_name, out_idl, mangled))
        out_paths.append(out_idl.resolve())

    for p in out_paths:
        print(p)
    return 0


if __name__ == "__main__":
    sys.exit(main())
