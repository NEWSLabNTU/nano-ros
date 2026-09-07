#!/usr/bin/env python3
"""The boot report's Rust layout and its Python decoder must agree.

phase-412. `scripts/read-boot-report.py` decodes a memory dump positionally --
twenty little-endian u32s in a fixed order. Nothing in either language forces
that order to match `BootReport` in
`packages/core/nros-node/src/boot_report.rs`, and a mismatch does not fail: it
DECODES, and prints an arena size that is really a callback count.

That is the failure mode this gate exists for. A wrong number that looks like a
number is worse than no number, and this record's whole purpose is to be
believed about a board that cannot otherwise be asked.

Checked here:

* the struct's field order equals the script's `FIELDS`
* `Snapshot`, which the Rust tests read through, has the same order again --
  so a test asserting on the record exercises the layout the script assumes
* `MAGIC` and `VERSION` are the same constants on both sides
* the `Stage` ladder -- every variant's NAME and NUMBER -- equals the
  decoder's `STAGES` table (issue 1036)

Not checked here: the size the RECORD reports at runtime. That comes from
`size_of` on the target and is asserted by the Rust test, which is the only
side that can see it.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RUST = REPO / "packages/core/nros-node/src/boot_report.rs"
ALLOC_RUST = REPO / "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs"
PY = REPO / "scripts/read-boot-report.py"


def fail(msg: str) -> int:
    print(f"check-boot-report-layout: {msg}", file=sys.stderr)
    return 1


def rust_struct_fields(text: str, name: str, ty: str) -> list[str]:
    """Field names of `struct <name>`, in declaration order."""
    m = re.search(rf"\n\s*pub struct {name} \{{\n(.*?)\n\s*\}}\n", text, re.S)
    if not m:
        raise SystemExit(f"{RUST}: no `pub struct {name}` found")
    fields = []
    for line in m.group(1).splitlines():
        line = line.strip()
        if not line or line.startswith("//") or line.startswith("///"):
            continue
        fm = re.match(rf"(?:pub )?([a-z_][a-z0-9_]*): {re.escape(ty)},", line)
        if fm:
            fields.append(fm.group(1))
    return fields


def py_fields(text: str, name: str = "FIELDS") -> list[str]:
    m = re.search(rf"\n{name} = \(\n(.*?)\n\)\n", text, re.S)
    if not m:
        raise SystemExit(f"{PY}: no `{name}` tuple found")
    return re.findall(r'"([a-z_][a-z0-9_]*)"', m.group(1))


def rust_stage_variants(text: str) -> dict[int, str]:
    """`Stage` variant name by discriminant, from the Rust enum.

    Issue 1036. `stage` is the record's single most useful field and the decoder
    renders it through a hand-written table, so an inserted or renumbered
    variant makes the script name the WRONG PHASE -- confidently, with a
    plausible string, which is the same failure mode the field order has and
    the reason this gate exists. The enum's own doc comment says inserting a
    stage renumbers the rest; this is what notices when only one side did.
    """
    m = re.search(r"\npub enum Stage \{\n(.*?)\n\}\n", text, re.S)
    if not m:
        raise SystemExit(f"{RUST}: no `pub enum Stage` found")
    out: dict[int, str] = {}
    for line in m.group(1).splitlines():
        vm = re.match(r"\s*([A-Z][A-Za-z0-9]*) = (\d+),\s*$", line)
        if vm:
            out[int(vm.group(2))] = vm.group(1)
    return out


def py_stage_names(text: str) -> dict[int, str]:
    """Leading variant name of each entry in the decoder's `STAGES` table."""
    m = re.search(r"\nSTAGES = \{\n(.*?)\n\}\n", text, re.S)
    if not m:
        raise SystemExit(f"{PY}: no `STAGES` dict found")
    out: dict[int, str] = {}
    for line in m.group(1).splitlines():
        sm = re.match(r'\s*(\d+): "([A-Za-z0-9]+)', line)
        if sm:
            out[int(sm.group(1))] = sm.group(2)
    return out


def diff_stages(rust: dict[int, str], py: dict[int, str], quiet: bool = False) -> int:
    if rust == py:
        return 0
    if quiet:
        return 1
    rc = fail("stage ladder: boot_report.rs and read-boot-report.py disagree")
    for n in sorted(set(rust) | set(py)):
        r, p = rust.get(n), py.get(n)
        if r != p:
            print(f"  stage {n}: Rust has {r!r}, decoder has {p!r}", file=sys.stderr)
    return rc


def const_u32(text: str, name: str) -> int:
    m = re.search(rf"\npub const {name}: u32 = (0x[0-9a-fA-F_]+|\d+);", text)
    if not m:
        raise SystemExit(f"{RUST}: no `pub const {name}: u32` found")
    return int(m.group(1).replace("_", ""), 0)


def py_const(text: str, name: str) -> int:
    m = re.search(rf"\n{name} = (0x[0-9a-fA-F]+|\d+)[ \t]*(?:#[^\n]*)?\n", text)
    if not m:
        raise SystemExit(f"{PY}: no `{name}` found")
    return int(m.group(1), 0)


def diff(
    label: str,
    a: list[str],
    b: list[str],
    a_name: str,
    b_name: str,
    quiet: bool = False,
) -> int:
    """Compare two field orders. `quiet` suppresses the report, for the
    selftest's deliberate mismatches -- printing those on a PASSING run
    would train a reader to skip exactly the output that matters."""
    if a == b:
        return 0
    if quiet:
        return 1
    rc = fail(f"{label}: {a_name} and {b_name} disagree")
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            print(f"  word {i}: {a_name} has `{x}`, {b_name} has `{y}`", file=sys.stderr)
    if len(a) != len(b):
        print(
            f"  {a_name} has {len(a)} fields, {b_name} has {len(b)}: "
            f"extra {sorted(set(a) ^ set(b))}",
            file=sys.stderr,
        )
    return rc


def main() -> int:
    rust = RUST.read_text()
    py = PY.read_text()

    record = rust_struct_fields(rust, "BootReport", "AtomicU32")
    snapshot = rust_struct_fields(rust, "Snapshot", "u32")
    script = py_fields(py)

    if not record:
        return fail("parsed zero fields out of `BootReport` -- the gate is blind")

    rc = 0
    rc |= diff("record vs decoder", record, script, "BootReport", "read-boot-report.py")
    rc |= diff("record vs snapshot", record, snapshot, "BootReport", "Snapshot")

    for name, py_name in (("MAGIC", "MAGIC"), ("VERSION", "KNOWN_VERSION")):
        r = const_u32(rust, name)
        p = py_const(py, py_name)
        if r != p:
            rc |= fail(f"{name}: boot_report.rs has {r:#x}, read-boot-report.py has {p:#x}")

    stages = rust_stage_variants(rust)
    if len(stages) < 2:
        rc |= fail("parsed fewer than two `Stage` variants -- the gate is blind")
    rc |= diff_stages(stages, py_stage_names(py))

    # The SECOND record. Same positional decode, same drift, same gate -- it
    # lives in another crate only because nros-node depends on that crate and
    # the call would otherwise be a cycle, which changes nothing about how
    # wrongly a reordered field decodes.
    alloc_rust = ALLOC_RUST.read_text()
    alloc_record = rust_struct_fields(alloc_rust, "SubscriberAllocReport", "AtomicU32")
    alloc_script = py_fields(py, "ALLOC_FIELDS")
    if not alloc_record:
        rc |= fail("parsed zero fields out of `SubscriberAllocReport` -- blind")
    rc |= diff("alloc record vs decoder", alloc_record, alloc_script,
               "SubscriberAllocReport", "read-boot-report.py")
    for name, py_name in (("SUBSCRIBER_ALLOC_MAGIC", "ALLOC_MAGIC"),
                          ("SUBSCRIBER_ALLOC_VERSION", "ALLOC_VERSION")):
        r = const_u32(alloc_rust, name)
        p = py_const(py, py_name)
        if r != p:
            rc |= fail(f"{name}: subscriber.rs has {r:#x}, decoder has {p:#x}")

    if rc == 0:
        print(
            f"check-boot-report-layout: OK (BootReport {len(record)} fields / "
            f"{len(record) * 4} bytes, {len(stages)} stages; "
            f"SubscriberAllocReport {len(alloc_record)} fields / "
            f"{len(alloc_record) * 4} bytes; all agreed with the decoder)"
        )
    return rc


def self_test() -> int:
    """Negative controls: the gate must FAIL on each drift it exists to catch.

    Run on the normal path, not behind a flag. A gate that only ever sees the
    agreeing case reports OK for both reasons -- because the files agree, and
    because it stopped being able to tell.
    """
    ok = True

    def expect(name: str, got, want) -> None:
        nonlocal ok
        if got != want:
            ok = False
            print(f"  self-test FAIL {name}: {got!r} != {want!r}")

    rust = RUST.read_text()
    py = PY.read_text()
    record = rust_struct_fields(rust, "BootReport", "AtomicU32")
    snapshot = rust_struct_fields(rust, "Snapshot", "u32")
    script = py_fields(py)

    # The parsers must actually find something. A regex that silently matches
    # nothing turns every comparison below into `[] == []`, which passes.
    expect("parses the record", len(record) > 5, True)
    expect("parses the snapshot", len(snapshot) > 5, True)
    expect("parses the decoder", len(script) > 5, True)

    # Reordering two words is the drift that DECODES rather than failing, and
    # so is the whole reason this gate exists.
    swapped = list(script)
    swapped[0], swapped[1] = swapped[1], swapped[0]
    expect("catches a reorder", diff("t", record, swapped, "a", "b", quiet=True) != 0, True)

    # A field added on one side only.
    expect("catches a missing field", diff("t", record, script[:-1], "a", "b", quiet=True) != 0, True)

    # And the agreeing case must still pass, or the gate fails everything.
    expect("passes when they agree", diff("t", record, script, "a", "b", quiet=True), 0)

    # The stage ladder, same three controls. A renumber is the drift that
    # DECODES: the script prints a stage name for a number the image never
    # meant, which is worse than refusing.
    rust_stages = rust_stage_variants(rust)
    py_stages = py_stage_names(py)
    expect("parses the stage enum", len(rust_stages) > 2, True)
    expect("parses the stage table", len(py_stages) > 2, True)
    renumbered = dict(py_stages)
    lo, hi = sorted(renumbered)[:2]
    renumbered[lo], renumbered[hi] = renumbered[hi], renumbered[lo]
    expect("catches a renumbered stage", diff_stages(rust_stages, renumbered, quiet=True) != 0, True)
    dropped = {k: v for k, v in py_stages.items() if k != max(py_stages)}
    expect("catches a missing stage", diff_stages(rust_stages, dropped, quiet=True) != 0, True)
    expect("passes when the stages agree", diff_stages(rust_stages, py_stages, quiet=True), 0)

    print("check-boot-report-layout --self-test: " + ("OK" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    # The selftest runs on the NORMAL path -- a negative control nobody runs
    # decays into a comment, which is what `check-gate-selftests` enforces.
    rc = self_test()
    sys.exit(rc if rc or "--self-test" in sys.argv else main())
