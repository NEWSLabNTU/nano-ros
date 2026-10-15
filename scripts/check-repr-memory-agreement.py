#!/usr/bin/env python3
"""phase-432 W1.2 — the two artifacts that describe ONE memory agree about it.

WHAT THIS GATES, AND WHAT IT DELIBERATELY DOES NOT
--------------------------------------------------
W1.1 deleted `TargetProfile`: codegen models no target at all. Removing a model
of the target without adding a MEASUREMENT of it is strictly worse than either,
so this is the measurement.

The pair is chosen by asking which two artifacts a single byte range is read
through, not by which two look similar. Both come out of the C++ pack:

  * `packs/cpp/message.hpp.jinja`      -> `struct pkg::msg::Name {...}`
  * `packs/cpp/message_types.rs.jinja` -> `#[repr(C)] pub struct pkg_msg_name_t`

The generated C++ code hands a `const void*` to ITS C++ object to the Rust FFI
glue, which reinterprets it as the `repr(C)` struct. The C++ compiler and Rust
`repr(C)` follow the SAME ABI, so the two agree BY CONSTRUCTION unless the two
pack templates SPELL the same lowered field differently (`rclcpp::FixedString`
/ `FixedSequence` / `HeapSequence` / `Span` against `cpp_repr_c_type`). That
spelling is the whole hazard, and it is what this measures.

NOT gated, on purpose:

  * The C pack (`packs/c`). Until phase-492 W7 its header described the same
    memory as the C++ header (both inline `char[N]`), so this gate compiled the
    C header as a stand-in for the C++ one. Since W7 the C pack takes rosidl's
    member shape (`rosidl_runtime_c__String`, `__Sequence`: pointer + size +
    capacity over caller-owned bytes), so its memory partner is upstream ROS
    `rosidl_generator_c`, NOT this repo's C++ pack. C and C++ nodes exchange
    CDR, never memory, so the two are SUPPOSED to differ.
  * The IDIOMATIC Rust pack (`packs/nros/*.jinja`, what `nros generate rust`
    emits) carries no `repr(C)` anywhere. It shares CDR with the headers -- a
    wire format, target-independent by specification -- and NOT memory.
  * The `packs/rmw/*.jinja` pack also emits `repr(C)`, but its memory partner
    is likewise upstream ROS `rosidl_generator_c`
    (`crate::rosidl_runtime_rs::String` is the `{char*, size, cap}` trio),
    NOT this repo's C++ pack.

HOW IT MEASURES
---------------
Neither side is asked to describe the other. Each side is asked, in its own
language, for numbers:

    C++   extern "C" char probe__X__size[sizeof(T) + 1];
          extern "C" char probe__X__f__a[__builtin_offsetof(T, a) + 1];
    Rust  #[no_mangle] static probe__X__size: [u8; size_of::<X>() + 1]
          #[no_mangle] static probe__X__f__a: [u8; offset_of!(X, a) + 1]

Both are cross-compiled for the SAME non-host target (32-bit `armv7a-none-eabi`
/ `arm-none-eabi-g++ -march=armv7-a`) to object files -- no linking, no
execution -- and the symbol SIZES are compared with `nm --print-size`. The `+ 1`
exists only so a zero offset still produces a sized symbol.

A 32-bit target is the point, not a convenience: `size_t` and pointers are 4
bytes there and 8 on the host, so the RFC-0033 `heap` containers
(`HeapSequence<T>` / `HeapString` vs `{*mut T, usize, usize}`) are measured
where the answer actually depends on the target.

The generated headers include the per-build config header for the
codegen-version anchor (RFC-0090) and nothing else from it; the probe supplies
a two-macro stand-in, so the C++ side needs no runtime build.

WHAT A MISMATCH LOOKS LIKE
--------------------------
Three properties per struct pair, so a fault has somewhere to land:

  * total size + alignment   -- a width or capacity that differs
  * every top-level offset   -- a reordering, or an earlier field's size
  * every sub-field offset of a container field (`FixedSequence {size, data}`,
    `HeapSequence {data, size, capacity}`, `Span {ptr, len}`) -- the container
    shape, which a top-level offset cannot see because the container is ONE
    field. The sub-field NAMES come from the Rust glue's container struct, so a
    C++ container whose member is spelled differently is a compile error
    naming it.

Field NAMES are compared through the compiler: every offset probe on the C++
side names the Rust field, so a pack that renames a field fails to compile
with the name in the diagnostic rather than reporting a number.

CORPUS
------
`packages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus` -- the same
in-tree corpus `codegen_fingerprint` hashes, so a shape added for one is covered
by the other. It needs no ROS install and no ament index. Both storage arms are
run: `inline` (empty resolver -- fixed arrays) and `configured`
(`nros-codegen.toml` -- `heap` pointers and `borrowed` views).

The packs reach this gate through the BUILT `nros` binary (`include_str!`
bundles them), exactly as `codegen_fingerprint` does -- so a stale CLI would
answer for sources that are no longer in the tree. `nros_cli_usable` in the
recipe is what makes that a SKIP rather than a wrong verdict.

`--self-test` is the negative control: it re-runs the comparator over a
deliberately perturbed reading and requires it to report, and pairs a Rust
struct name with its C++ type through the same mapping the gate uses. A gate
that can only say OK is not evidence.
"""

from __future__ import annotations

import contextlib
import io
import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CORPUS = REPO / "packages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus"
CPP_INCLUDES = [
    REPO / "packages/api/nros-cpp/include",
    REPO / "packages/api/nros-c/include",
    REPO / "packages/platform/nros-platform-api/include",
]

# One target, named in one place. 32-bit so pointer/size_t width is exercised.
RUST_TARGET = "armv7a-none-eabi"
CXX_FLAGS = ["-march=armv7-a", "-std=c++17", "-fno-exceptions", "-fno-rtti", "-fno-common"]

PROBE = "nros_repr_probe"

# The per-build config header the generated `.hpp` files include for the
# codegen-version anchor (RFC-0090). A probe compiles no runtime, so it supplies
# exactly the two macros that anchor reads -- for whatever version the BUILT
# `nros` stamped, read back from the generated output.
CONFIG_STUB = "#define NROS_CODEGEN_VERSION {v}\n#define NROS_CODEGEN_VERSION_MIN {v}\n"


# --------------------------------------------------------------------------
# extraction
# --------------------------------------------------------------------------


def _balanced_body(text: str, open_at: int) -> tuple[str, int]:
    """The `{...}` starting at `open_at`, and the index just past its `}`."""
    depth = 0
    for i in range(open_at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_at + 1 : i], i + 1
    raise SystemExit(f"{PROBE}: unbalanced braces at offset {open_at}")


def extract_rust_structs(text: str) -> dict[str, dict]:
    """`#[repr(C)] pub struct NAME { pub f: T, ... }` blocks, by NAME.

    `types` records each field's Rust type spelling, so a field whose type is
    another extracted struct (a container) can be probed member by member.
    """
    out: dict[str, dict] = {}
    for m in re.finditer(r"#\[repr\(C\)\]\s*(?:#\[[^\]]*\]\s*)*pub struct\s+(\w+)\s*\{", text):
        body, _ = _balanced_body(text, m.end() - 1)
        pairs = re.findall(r"^\s*pub\s+(\w+)\s*:\s*(.+?),?\s*$", body, re.M)
        out[m.group(1)] = {
            "decl": text[m.start() : m.end()] + body + "}",
            "fields": [f for f, _ in pairs],
            "types": {f: t.strip() for f, t in pairs},
        }
    return out


def rust_snake(name: str) -> str:
    """The C++ pack's file/struct stem rule (`rosidl_codegen::utils::to_snake_case`)."""
    out, prev_lower = [], False
    for i, ch in enumerate(name):
        if ch.isupper():
            if i > 0 and prev_lower:
                out.append("_")
            out.append(ch.lower())
            prev_lower = False
        else:
            out.append(ch)
            prev_lower = ch.islower()
    return "".join(out)


PARTS = {"srv": ("Request", "Response"), "action": ("Goal", "Result", "Feedback")}


def cpp_type_map(pkg: str, interfaces: list[str]) -> dict[str, str | None]:
    """Rust glue struct name -> the C++ type it mirrors.

    `pkg_msg_name_t` -> `pkg::msg::Name`, `pkg_msg_name_t_view` -> `NameView`;
    a service/action PART is a nested struct: `pkg_srv_name_request_t` ->
    `pkg::srv::Name::Request`. Derived from the interface FILES, so a struct
    that maps to nothing is a struct this gate cannot pair -- reported, never
    skipped.

    A service/action part's `_view` maps to None: the C++ pack emits no view
    type for a service or action part (only a message has `NameView`), so the
    Rust glue's `_t_view` there mirrors nothing C++ holds and no memory is
    shared. Known by RULE rather than listed, and never compared.
    """
    c_pkg = pkg.replace("-", "_")
    out: dict[str, str] = {}
    for f in interfaces:
        path = Path(f)
        kind, name = path.suffix[1:], path.stem
        stem = f"{c_pkg}_{kind}_{rust_snake(name)}"
        if kind == "msg":
            out[f"{stem}_t"] = f"{c_pkg}::msg::{name}"
            out[f"{stem}_t_view"] = f"{c_pkg}::msg::{name}View"
        else:
            for part in PARTS[kind]:
                key = f"{stem}_{part.lower()}_t"
                out[key] = f"{c_pkg}::{kind}::{name}::{part}"
                out[f"{key}_view"] = None
    return out


# --------------------------------------------------------------------------
# corpus generation
# --------------------------------------------------------------------------


def interface_files() -> list[str]:
    files = (
        sorted(CORPUS.glob("msg/*.msg"))
        + sorted(CORPUS.glob("srv/*.srv"))
        + sorted(CORPUS.glob("action/*.action"))
    )
    if not files:
        raise SystemExit(f"{PROBE}: no interface files under {CORPUS}")
    return [str(p) for p in files]


# issue 1735 — a THIRD arm, HEAP STRINGS. The corpus config puts no `string`
# field in `heap` mode, so the C++ pack's `nros_cpp_heap_str_t` (the heap STRING
# container) was never measured: swapping its `size`/`capacity` passed while the
# same swap on the heap SEQUENCE failed. Written by this gate into its own work
# dir rather than added to the corpus, because the corpus feeds
# `codegen_fingerprint` and moving it stales every workspace fixture.
HEAP_STRINGS_TOML = """[fields]
"fingerprint-corpus/Shapes.text"        = { cap = 32, mode = "heap" }
"fingerprint-corpus/Probe_Request.note" = { cap = 32, mode = "heap" }
"fingerprint-corpus/Probe_Goal.name"    = { cap = 16, mode = "heap" }
"fingerprint-corpus/Shapes.seq_prim"    = { cap = 16, mode = "heap" }
"""
# The container each arm must actually reach, by its C++-pack type name: an
# arm whose output holds none of it measured nothing about that container.
ARM_MUST_REACH = {"heap-strings": "nros_cpp_heap_str_t"}


def generate(nros: str, work: Path, arm: str, configured) -> Path:
    """Emit the C++ pack (header + its Rust glue) for one storage arm."""
    out = work / f"{arm}-cpp"
    out.mkdir(parents=True, exist_ok=True)
    args = {
        "package_name": "fingerprint-corpus",
        "output_dir": str(out),
        "interface_files": interface_files(),
        "dependencies": [],
        "ros_edition": "humble",
    }
    if isinstance(configured, str):
        cfg = work / f"{arm}-nros-codegen.toml"
        cfg.write_text(configured)
        args["codegen_config"] = str(cfg)
    elif configured:
        args["codegen_config"] = str(CORPUS / "nros-codegen.toml")
    af = work / f"{arm}-cpp-args.json"
    af.write_text(json.dumps(args))
    r = subprocess.run(
        [nros, "codegen", "--language", "cpp", "--args-file", str(af)],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        raise SystemExit(
            f"{PROBE}: `nros codegen --language cpp` failed for the {arm} arm:\n"
            f"{r.stdout}{r.stderr}"
        )
    return out


# --------------------------------------------------------------------------
# probe emission
# --------------------------------------------------------------------------


def container_subs(rust_structs: dict[str, dict], pair_names: set[str]) -> dict[str, dict]:
    """Per pair: field -> the member names of its CONTAINER type, if any.

    A field whose Rust type is another extracted struct that is not itself a
    paired message (`..._t_seq_prim_seq_t`, `nros_cpp_borrow_t`, ...) is a
    container; its members are probed one by one, because a swapped
    `size`/`capacity` leaves every top-level offset unchanged.
    """
    out: dict[str, dict] = {}
    for name in pair_names:
        info = rust_structs[name]
        subs = {}
        for f in info["fields"]:
            t = info["types"].get(f, "")
            if t in rust_structs and t not in pair_names:
                subs[f] = rust_structs[t]["fields"]
        out[name] = subs
    return out


def cpp_probe_tu(
    headers: list[Path], pairs: list[tuple[str, str]], subs: dict[str, dict],
    rust_structs: dict[str, dict], arm: str,
) -> str:
    lines = ["// generated by scripts/check-repr-memory-agreement.py - do not edit"]
    for h in headers:
        lines.append(f'#include "{h}"')
    lines.append('extern "C" {')
    for r_name, cpp in pairs:
        lines.append(f"char {sym(arm, r_name, 'size')}[sizeof({cpp}) + 1];")
        lines.append(f"char {sym(arm, r_name, 'align')}[alignof({cpp}) + 1];")
        for f in rust_structs[r_name]["fields"]:
            lines.append(
                f"char {sym(arm, r_name, 'f', f)}[__builtin_offsetof({cpp}, {f}) + 1];"
            )
            for sub in subs[r_name].get(f, []):
                lines.append(
                    f"char {sym(arm, r_name, 'f', f, sub)}"
                    f"[__builtin_offsetof({cpp}, {f}.{sub}) + 1];"
                )
    lines.append("}")
    return "\n".join(lines) + "\n"


def rust_probe_crate(structs: dict[str, dict], pairs: list[tuple[str, str]], arm: str) -> str:
    lines = [
        "// generated by scripts/check-repr-memory-agreement.py - do not edit",
        "#![no_std]",
        "#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]",
        "",
    ]
    for name in sorted(structs):
        lines.append(structs[name]["decl"])
    lines.append("")
    for r_name, _cpp in pairs:
        for kind, expr in (
            ("size", f"core::mem::size_of::<{r_name}>()"),
            ("align", f"core::mem::align_of::<{r_name}>()"),
        ):
            s = sym(arm, r_name, kind)
            lines.append(
                f"#[unsafe(no_mangle)] pub static {s}: [u8; {expr} + 1] = [0; {expr} + 1];"
            )
    return "\n".join(lines) + "\n"


def rust_field_probes(
    rust_structs: dict[str, dict], pairs: list[tuple[str, str]],
    subs: dict[str, dict], arm: str,
) -> list[str]:
    """Offset probes, from the SAME field and container-member names the C++
    probe names, so a member one side lacks is a compile error naming it."""
    out = []
    for r_name, _cpp in pairs:
        for f in rust_structs[r_name]["fields"]:
            for path, sfx in [(f, (f,))] + [
                (f"{f}.{sub}", (f, sub)) for sub in subs[r_name].get(f, [])
            ]:
                expr = f"core::mem::offset_of!({r_name}, {path})"
                s = sym(arm, r_name, "f", *sfx)
                out.append(
                    f"#[unsafe(no_mangle)] pub static {s}: [u8; {expr} + 1] = [0; {expr} + 1];"
                )
    return out


def sym(arm: str, struct: str, kind: str, *rest: str) -> str:
    parts = [PROBE, arm.replace("-", "_"), struct, kind, *rest]
    return "__".join(parts)


# --------------------------------------------------------------------------
# compile + read
# --------------------------------------------------------------------------


def nm_sizes(nm: str, obj: Path) -> dict[str, int]:
    r = subprocess.run(
        [nm, "--print-size", "--defined-only", str(obj)], capture_output=True, text=True
    )
    if r.returncode != 0:
        raise SystemExit(f"{PROBE}: {nm} failed on {obj}:\n{r.stderr}")
    out = {}
    for line in r.stdout.splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[3].startswith(PROBE):
            out[parts[3]] = int(parts[1], 16)
    return out


def run(cmd: list[str], what: str) -> None:
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"{PROBE}: {what} failed:\n{' '.join(cmd)}\n{r.stdout}{r.stderr}")


# --------------------------------------------------------------------------
# comparison
# --------------------------------------------------------------------------


def compare(c_vals: dict[str, int], r_vals: dict[str, int]) -> list[str]:
    """Every probe present on both sides must report the same number.

    A probe missing from ONE side is reported too: it means a struct or field
    exists in one pack and not the other, which is a spelling mismatch that
    happens to be invisible to arithmetic.
    """
    problems = []
    for name in sorted(set(c_vals) | set(r_vals)):
        cv, rv = c_vals.get(name), r_vals.get(name)
        # `name` came from the UNION, so at least one side has it; the type
        # checker cannot see that, and spelling it out is cheaper than an
        # ignore comment that would also hide a real None later.
        if cv is None and rv is not None:
            problems.append(f"  {name}: absent from the C++ probe, {rv - 1} in Rust")
        elif rv is None and cv is not None:
            problems.append(f"  {name}: {cv - 1} in C++, absent from the Rust probe")
        elif cv is None or rv is None:
            problems.append(f"  {name}: absent from BOTH probes — unreachable")
        elif cv != rv:
            problems.append(f"  {name}: C++ says {cv - 1}, Rust says {rv - 1}")
    return problems


# --------------------------------------------------------------------------
# driver
# --------------------------------------------------------------------------


def check_arm(
    arm: str, nros: str, cxx: str, nm: str, work: Path, configured
) -> tuple[int, int, list[str]]:
    cpp_dir = generate(nros, work, arm, configured)
    must = ARM_MUST_REACH.get(arm)
    # walk-ok: `cpp_dir` is this run's generated pack output, untracked.
    pack_rs = sorted(cpp_dir.rglob("*_types.rs"))
    if must and not any(must in rs.read_text(encoding="utf-8", errors="replace")
                        for rs in pack_rs):
        raise SystemExit(
            f"{PROBE}: the {arm} arm generated no `{must}` field - it exists to "
            "measure that container, and a comparison that never reached it is not a pass"
        )

    rust_structs: dict[str, dict] = {}
    for rs in pack_rs:
        for name, info in extract_rust_structs(rs.read_text(encoding="utf-8")).items():
            prev = rust_structs.get(name)
            if prev and prev["decl"] != info["decl"]:
                raise SystemExit(
                    f"{PROBE}: two different `{name}` declarations in the C++ pack output"
                )
            rust_structs[name] = info

    # Pair every Rust glue struct that mirrors a C++ type; the rest must be
    # CONTAINERS some paired struct embeds. A struct that is neither is one the
    # gate cannot account for, which is a failure, not a skip.
    type_map = cpp_type_map("fingerprint-corpus", interface_files())
    pairs = sorted((r, type_map[r]) for r in rust_structs if type_map.get(r))
    pair_names = {r for r, _ in pairs}
    no_cpp_side = {r for r in rust_structs if r in type_map and type_map[r] is None}
    subs = container_subs(rust_structs, pair_names)
    embedded = {
        rust_structs[p]["types"][f] for p in pair_names for f in rust_structs[p]["fields"]
    }
    # Containers reachable only through an unpaired view are accounted for too.
    embedded |= {
        rust_structs[v]["types"][f] for v in no_cpp_side for f in rust_structs[v]["fields"]
    }
    orphans = sorted(
        r for r in rust_structs
        if r not in pair_names and r not in embedded and r not in no_cpp_side
    )
    if not pairs:
        raise SystemExit(
            f"{PROBE}: the {arm} arm paired no Rust glue struct with a C++ type - "
            "a corpus that pairs nothing cannot agree about anything"
        )
    if orphans:
        return len(pairs), 0, [
            f"  {r}: a repr(C) struct in the C++ pack's Rust glue that mirrors no C++\n"
            "      message type and that no paired struct embeds. Teach `cpp_type_map`\n"
            "      its C++ name rather than dropping it."
            for r in orphans
        ]

    # The stamped codegen version, read back from the output, for the stub.
    version = None
    # walk-ok: same — `cpp_dir` is this run's generated pack output, untracked.
    headers = sorted(cpp_dir.rglob("*.hpp"))
    for h in headers:
        m = re.search(r"#define NROS_EMITTED_CODEGEN_VERSION (\d+)", h.read_text(encoding="utf-8"))
        if m:
            version = m.group(1)
            break
    if version is None:
        raise SystemExit(f"{PROBE}: the {arm} arm's headers carry no codegen-version stamp")
    stub = work / f"{arm}-stub" / "rclc" / "detail"  # phase-501 W2
    stub.mkdir(parents=True, exist_ok=True)
    (stub / "nros_config_generated.h").write_text(CONFIG_STUB.format(v=version))

    cxx_src = work / f"{arm}-probe.cpp"
    cxx_src.write_text(cpp_probe_tu(headers, pairs, subs, rust_structs, arm))
    cxx_obj = work / f"{arm}-probe-cpp.o"
    incs = [f"-I{work / f'{arm}-stub'}", f"-I{cpp_dir}"] + [f"-I{d}" for d in CPP_INCLUDES]
    run([cxx, "-c", *CXX_FLAGS, *incs, "-o", str(cxx_obj), str(cxx_src)], f"the {arm} C++ probe")

    rs_src = work / f"{arm}-probe.rs"
    rs_text = rust_probe_crate(rust_structs, pairs, arm)
    rs_text += "\n".join(rust_field_probes(rust_structs, pairs, subs, arm)) + "\n"
    rs_src.write_text(rs_text)
    rs_obj = work / f"{arm}-probe-rs.o"
    run(
        [
            "rustc",
            "--edition",
            "2024",
            "--target",
            RUST_TARGET,
            "--crate-type",
            "rlib",
            "--emit",
            "obj",
            "-C",
            "opt-level=0",
            "-o",
            str(rs_obj),
            str(rs_src),
        ],
        f"the {arm} Rust probe",
    )

    c_vals = nm_sizes(nm, cxx_obj)
    r_vals = nm_sizes(nm, rs_obj)
    if not c_vals or not r_vals:
        raise SystemExit(
            f"{PROBE}: the {arm} arm read {len(c_vals)} C++ and {len(r_vals)} Rust probe "
            "symbols - a comparison over nothing is not a pass"
        )
    return len(pairs), len(c_vals), compare(c_vals, r_vals)


def self_test() -> int:
    """The negative control: the comparator must SAY SO when readings differ.

    A gate whose only demonstrated behaviour is printing OK has not been shown
    to have any behaviour at all.
    """
    base = {f"{PROBE}__x__S__size": 41, f"{PROBE}__x__S__f__a": 9}
    fails = 0

    if compare(base, dict(base)):
        print("self-test: identical readings were reported as a mismatch", file=sys.stderr)
        fails += 1

    perturbed = dict(base)
    perturbed[f"{PROBE}__x__S__f__a"] = 5
    if not compare(base, perturbed):
        print("self-test: a differing field offset was NOT reported", file=sys.stderr)
        fails += 1

    if not compare(base, {f"{PROBE}__x__S__size": 41}):
        print("self-test: a probe missing from one side was NOT reported", file=sys.stderr)
        fails += 1

    # The pairing rule: a Rust glue name maps to the C++ type the pack emits for
    # it, and a name the corpus does not produce maps to nothing (so it would be
    # reported as an orphan, never silently compared against something else).
    m = cpp_type_map("p-q", ["x/msg/LaserScan.msg", "x/srv/AddTwoInts.srv"])
    want = {
        "p_q_msg_laser_scan_t": "p_q::msg::LaserScan",
        "p_q_msg_laser_scan_t_view": "p_q::msg::LaserScanView",
        "p_q_srv_add_two_ints_request_t": "p_q::srv::AddTwoInts::Request",
        "p_q_srv_add_two_ints_request_t_view": None,
    }
    for k, v in want.items():
        if k not in m or m[k] != v:
            print(f"self-test: `{k}` paired with {m.get(k)!r}, not {v!r}", file=sys.stderr)
            fails += 1
    if "p_q_msg_laser_scan_t_seq_t" in m:
        print("self-test: a container name was paired with a C++ message type", file=sys.stderr)
        fails += 1

    if fails == 0:
        print("repr-memory-agreement self-test OK (3 comparator cases, 2 pairing cases)")
    return 1 if fails else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    # The recipe resolves it (`nros_cli_bin --checkout-only`, issue 1850);
    # only `--self-test` runs without one.
    ap.add_argument("--nros")
    ap.add_argument("--cxx", default="arm-none-eabi-g++")
    ap.add_argument("--nm", default="arm-none-eabi-nm")
    ap.add_argument("--keep", action="store_true", help="keep the probe sources for inspection")
    ap.add_argument("--self-test", action="store_true", help="negative control, no toolchain")
    args = ap.parse_args()

    if args.self_test:
        return self_test()
    if not args.nros:
        ap.error("--nros is required (the recipe passes `nros_cli_bin --checkout-only`)")
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    with contextlib.redirect_stdout(io.StringIO()) as _selftest_out:
        _selftest_rc = self_test()
    if _selftest_rc:
        sys.stdout.write(_selftest_out.getvalue())
        return _selftest_rc

    for tool in (args.cxx, args.nm):
        if shutil.which(tool) is None:
            raise SystemExit(f"{PROBE}: `{tool}` not on PATH (the recipe should have skipped)")

    tmp_root = REPO / "tmp"
    tmp_root.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="repr-memory-agreement.", dir=tmp_root))
    try:
        total_pairs = total_probes = 0
        problems: list[str] = []
        for arm, configured in (("inline", False), ("configured", True),
                                ("heap-strings", HEAP_STRINGS_TOML)):
            pairs, probes, probs = check_arm(
                arm, args.nros, args.cxx, args.nm, work, configured
            )
            total_pairs += pairs
            total_probes += probes
            problems += [f"  [{arm}] {p.strip()}" for p in probs]
        if problems:
            print(
                "repr memory agreement FAILED - the C++ pack's header and its "
                f"repr(C) Rust glue describe different memory on {RUST_TARGET}:",
                file=sys.stderr,
            )
            for p in problems:
                print(p, file=sys.stderr)
            print(
                "\n  Both render from ONE lowered field. A disagreement is the two\n"
                "  C++ pack filters (`cpp_type`/`cpp_array_suffix` vs `cpp_repr_c_type`)\n"
                "  spelling it differently - fix the filter, not the number.",
                file=sys.stderr,
            )
            return 1
        print(
            f"repr memory agreement OK ({total_pairs} struct pairs, "
            f"{total_probes} probes, target {RUST_TARGET}, 3 storage arms)"
        )
        return 0
    finally:
        if args.keep:
            print(f"  probe sources kept in {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
