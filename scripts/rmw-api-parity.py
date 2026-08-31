#!/usr/bin/env python3
"""Phase 376 — what upstream rmw requires of an implementation, and where we answer it.

# What is being compared, and why not the obvious thing

The obvious comparison is `rmw.h` against `nros/rmw_vtable.h`, and it is the
wrong one twice over.

**Upstream's headers overstate the contract.** The `rmw` package declares 177
`RMW_PUBLIC` functions, but most are utilities rmw itself DEFINES — allocators,
error handling, `names_and_types` init/fini, qos string conversions, the
`validate_*` helpers. An implementation does not supply those; it links them.
Comparing against 177 would manufacture ~90 phantom gaps.

**Our header understates ours.** `rmw_vtable.h` is the BACKEND seam — the part a
zenoh or Cyclone backend plugs into. Plenty of what upstream calls rmw lives one
layer up in `nros-node` (the executor's wait, guard conditions), one layer down
in the backend (graph queries in `nros-rmw-cyclonedds/src/graph.cpp`), or in
codegen (serialize/deserialize). Comparing only the vtable would manufacture
gaps for things we ship.

So the contract is taken EMPIRICALLY: the `rmw_*` symbols a real implementation
DEFINES. `librmw_fastrtps_cpp.so` and `librmw_zenoh_cpp.so` export the same 88 —
two independent implementations, byte-identical symbol sets — which is a much
better definition of "what an rmw must provide" than any reading of the headers.
That number is the check, not a constant here: `--contract` re-derives it.

# The mapping is authored, and that is the point

Each of the 88 is classified below with WHERE we answer it, or why we do not.
The tool's job is not to guess the mapping — it is to make an unclassified
symbol impossible to ignore, so that when upstream grows one, or a distro bump
changes the set, somebody has to decide rather than the diff quietly aging.

Buckets:
  vtable   — a slot in `nros/rmw_vtable.h`
  layer    — we answer it, elsewhere (named)
  declined — deliberately absent, with the RTOS reason
  gap      — missing, and it should not be

Usage:
    scripts/rmw-api-parity.py                 # report against the recorded contract
    scripts/rmw-api-parity.py --contract      # re-derive from an installed impl (needs ROS)
    scripts/rmw-api-parity.py --check         # fail on anything unclassified
    scripts/rmw-api-parity.py --self-test
"""

import argparse
import os
import subprocess
import sys

import re
import importlib.util as _util

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# `librmw_zenoh_cpp.so`, not `librmw_dds_common__rosidl_typesupport_fastrtps_cpp.so`.
IMPL_LIB = re.compile(r"librmw_[a-z0-9]+_cpp\.so")
CONTRACT = os.path.join(ROOT, "docs", "reference", "rmw-implementation-contract.txt")

# Where we answer each symbol. `(bucket, detail)`.
#
# Written against ROS 2 Humble's rmw. A symbol NOT in this table is a hard
# failure under --check: that is the whole mechanism.
MAP_TOML = os.path.join(ROOT, "docs", "reference", "rmw-api-map.toml")


def _load_map():
    """`{symbol: (where, detail)}` from the authored map.

    Read from a FILE rather than held here, so the gate and
    `gen-rmw-api-comparison.py` cannot disagree: two copies of a map is how a
    document ends up describing a tree that moved.

    `detail` is the nano-ros name for a slot or a global, and the reason for
    everything else — the shape `check_against_vtable` already validates.
    """
    # `tomllib` is stdlib only on 3.11+; CI runs the `ros:humble` image, whose
    # python3 is 3.10. Every other TOML reader in `scripts/` already carries
    # this fallback — see `fixtures-manifest.py`, `fixture-inventory.py`,
    # `check-cargo-profile-mirror.sh`.
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:
        import tomli as tomllib

    with open(MAP_TOML, "rb") as fh:
        raw = tomllib.load(fh)
    out = {}
    for sym, row in raw.items():
        # `[[arg_rule]]` is a sibling array in the same file — the ARG
        # attribution the document uses, not a contract symbol. Skipped by
        # shape rather than by name so a future sibling table needs no edit
        # here.
        if not isinstance(row, dict) or "where" not in row:
            continue
        where = row["where"]
        detail = row.get("nano") or row.get("reason", "")
        out[sym] = (where, detail)
        MAP_ROWS[sym] = row
    return out


# The full authored row, for consumers that need more than `(where, detail)` —
# `status` / `answers` / `issue`. Same dict, read once, so the document and the
# gate still cannot disagree.
MAP_ROWS = {}
MAP = _load_map()

# What we DID about a symbol, as opposed to WHICH SURFACE answers it (`where`).
# `same` and `re-shaped` are DERIVED from the signatures and must never be
# authored; authoring them would let the map assert a match the types deny.
STATUSES = ("same", "re-shaped", "re-mapped", "not-supported", "not-implemented")
DERIVED_ONLY = ("same", "re-shaped")


def check_status():
    """The `status` axis: vocabulary, and the rules that keep it honest.

    Added because `where` was doing two jobs. `declined` covered `rmw_wait`,
    decomposed into five live slots, and `rmw_init_publisher_allocation`, where
    nothing crosses the seam — opposite facts under one word.
    """
    bad = []
    for sym, row in sorted(MAP_ROWS.items()):
        status, where = row.get("status"), row["where"]
        if status is None:
            if where in ("layer", "declined"):
                bad.append(
                    f"{sym}: `where = \"{where}\"` needs an explicit `status` — "
                    "absent from the vtable says nothing about whether the "
                    "capability is answered"
                )
            continue
        if status not in STATUSES:
            bad.append(f"{sym}: unknown status {status!r}; expected one of {STATUSES}")
            continue
        if status in DERIVED_ONLY:
            bad.append(
                f"{sym}: `status = \"{status}\"` is DERIVED from the signatures "
                "and must not be authored"
            )
        if status == "re-mapped" and not row.get("answers"):
            bad.append(
                f"{sym}: `re-mapped` must name what answers it — add `answers = [...]`. "
                "\"Answered somewhere\" that cannot be queried is prose, not a map"
            )
        if status == "not-implemented" and not row.get("issue"):
            bad.append(
                f"{sym}: `not-implemented` must carry `issue = NNNN`. Without one a "
                "gap is indistinguishable from a decision, and silence turns the "
                "first into the second"
            )
        if status == "not-supported" and row.get("issue"):
            bad.append(f"{sym}: `not-supported` is a decision, so it takes no `issue`")
    return bad

# `global` is distinct from `vtable`: a slot is per-BACKEND, a global is
# defined ONCE for the image. Conflating them hid which of the two a
# symbol actually lands on (phase-393 followup).
BUCKETS = ("vtable", "global", "layer", "declined", "gap")


# The MAP is authored, and an authored table drifts the moment the thing it
# describes moves under it. It did, twice, silently:
#
#   - W3.b RENAMED 17 slots (`try_recv_raw` -> `take`, `send_reply` ->
#     `send_response`, …). The buckets stayed right, the details named slots
#     that no longer existed.
#   - W4 LANDED 28 slots — the whole graph/introspection family, all six
#     `*_get_actual_qos`, the two service-side callbacks — and the MAP still
#     read `("gap", "no vtable slot")` for every one of them.
#
# So this file claimed 26 gaps while `rmw-abi-shape` found ONE symbol without a
# slot: two tools over one question, disagreeing by 25 symbols, each green. The
# report is the artifact people quote, which makes a stale one worse than none.
#
# The check below is the structural fix. It reads the same header
# `rmw-abi-shape` parses, so the two tools cannot disagree again without one of
# them going red.
def vtable_slot_names():
    """Slot names in `nros/rmw_vtable.h`, via rmw-abi-shape's own parser.

    Importing rather than re-implementing is deliberate: a SECOND parser for
    one header is how the two tools would drift a third time. The header's
    tricky shapes (pointer-returning slots, nested callback parameters) already
    cost that parser two rounds of fixes; they are not worth relearning here.
    """
    spec = _util.spec_from_file_location(
        "_rmw_abi_shape", os.path.join(ROOT, "scripts", "rmw-abi-shape.py")
    )
    mod = _util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    slots, _rets = mod.vtable_slots()
    return set(slots), getattr(mod, "GROUPED_SYMBOLS", {}), getattr(mod, "ABI_FUNCTIONS", {})


def check_against_vtable():
    """[(symbol, complaint)] — MAP claims that the header contradicts."""
    slots, grouped, abi_fns = vtable_slot_names()
    bad = []
    for sym, (bucket, detail) in sorted(MAP.items()):
        named = re.split(r"[ ,(]", detail.strip())[0]
        if bucket == "vtable":
            if named not in slots:
                bad.append((sym, f"detail names {named!r}, which is no slot in rmw_vtable.h"))
                continue
            # …and it must be the RIGHT slot. The mechanical rule is the whole
            # naming contract: slot = upstream name minus `rmw_`. Anything else
            # has to be a DECLARED grouping. Checking only that the name
            # resolves let `rmw_subscription_set_on_new_message_callback` go on
            # pointing at `set_wake_callback` — a real slot, the wrong one, and
            # a record the header already called untrue.
            mechanical = sym[4:] if sym.startswith("rmw_") else sym
            if named != mechanical and grouped.get(sym) != named:
                bad.append((
                    sym,
                    f"detail names slot {named!r}, but the mechanical name is "
                    f"{mechanical!r} and no GROUPED_SYMBOLS row redirects it",
                ))
            continue
        # The other direction: a slot EXISTS and the MAP still says we do not
        # answer it there. `gap` is the one that matters — it is the bucket
        # people read as a to-do list.
        if sym in abi_fns:
            # A plain exported ABI function is legitimately `layer`: it is not
            # a slot BECAUSE its answer must not vary by backend.
            continue
        slot = sym[4:] if sym.startswith("rmw_") else sym
        if slot in slots or sym in grouped:
            bad.append((sym, f"bucket {bucket!r}, but slot {slot!r} exists in rmw_vtable.h"))
    return bad


def read_contract():
    if not os.path.exists(CONTRACT):
        return None
    out = []
    for line in open(CONTRACT, encoding="utf-8"):
        line = line.strip()
        if line and not line.startswith("#"):
            out.append(line)
    return out


def derive_contract():
    """The symbols a real implementation DEFINES. Ground truth over prose."""
    # `librmw_<impl>_cpp.so` and nothing else. A substring test on "fastrtps_cpp"
    # also matches `librmw_dds_common__rosidl_typesupport_fastrtps_cpp.so`, which
    # is a typesupport library with ZERO `rmw_*` symbols — and since the contract
    # is an INTERSECTION, one such member empties it. That produced a contract of
    # 0 and an "extras" list claiming each real implementation had 88 private
    # symbols, which is the shape of a bug that reads as a finding.
    libs = []
    for prefix in os.environ.get("AMENT_PREFIX_PATH", "").split(os.pathsep):
        if not prefix:
            continue
        libdir = os.path.join(prefix, "lib")
        if not os.path.isdir(libdir):
            continue
        for f in sorted(os.listdir(libdir)):
            if IMPL_LIB.fullmatch(f):
                libs.append(os.path.join(libdir, f))
    if not libs:
        return None, "no librmw_<impl>_cpp.so found — source a ROS install first"

    sets = {}
    for lib in libs:
        try:
            out = subprocess.run(
                ["nm", "-D", "--defined-only", lib],
                capture_output=True, text=True, check=False,
            ).stdout
        except OSError as e:
            return None, f"nm failed: {e}"
        syms = set()
        for line in out.splitlines():
            parts = line.split()
            if parts and parts[-1].startswith("rmw_"):
                syms.add(parts[-1])
        sets[os.path.basename(lib)] = syms

    # A library that exports no `rmw_*` at all is not an implementation, whatever
    # its name says. Belt and braces with IMPL_LIB above: an intersection is only
    # as honest as its weakest member.
    sets = {n: s for n, s in sets.items() if s}
    if not sets:
        return None, "found candidate libraries, none exporting rmw_* symbols"
    names = sorted(sets)
    common = set.intersection(*sets.values())
    report = [f"{n}: {len(sets[n])}" for n in names]
    # A symbol only ONE implementation defines is that implementation's private
    # extension, not the contract — say so rather than silently intersecting.
    extras = {n: sorted(sets[n] - common) for n in names if sets[n] - common}
    return (sorted(common), {"per_lib": report, "extras": extras})


def report(contract):
    counts = {b: 0 for b in BUCKETS}
    unclassified = []
    rows = []
    for sym in contract:
        entry = MAP.get(sym)
        if entry is None:
            unclassified.append(sym)
            continue
        counts[entry[0]] += 1
        rows.append((entry[0], sym, entry[1]))
    stale = sorted(set(MAP) - set(contract))
    return counts, unclassified, stale, rows


def self_test():
    bad = []
    # Every mapped bucket is a known one.
    for sym, (bucket, detail) in MAP.items():
        if bucket not in BUCKETS:
            bad.append(f"{sym}: unknown bucket {bucket!r}")
        if not detail.strip():
            bad.append(f"{sym}: empty reason — a classification with no reason is prose")
    # An unclassified symbol must be reported, not absorbed.
    counts, unclassified, _stale, _rows = report(["rmw_publish", "rmw_brand_new_thing"])
    if unclassified != ["rmw_brand_new_thing"]:
        bad.append(f"unclassified detection broken: {unclassified}")
    if counts["vtable"] != 1:
        bad.append(f"expected the mapped symbol counted: {counts}")
    # The header cross-check, in both directions, against the real header —
    # a self-test over a fixture would not have caught either drift, because
    # both were the fixture and the header diverging.
    for sym, complaint in check_against_vtable():
        bad.append(f"{sym}: {complaint}")
    # The `status` axis: vocabulary, and the rules that stop a gap from
    # decaying into a decision by silence.
    bad.extend(check_status())
    if bad:
        for b in bad:
            sys.stderr.write("rmw-api-parity --self-test: " + b + "\n")
        return 2
    n_status = sum(1 for r in MAP_ROWS.values() if r.get("status"))
    print(
        f"rmw-api-parity --self-test: OK ({len(MAP)} mapping(s), "
        f"{n_status} authored status(es), 2 case(s))"
    )
    return 0



def _slot_kinds():
    """`{slot: produced|default|unimplemented|inert}` from the producer scan."""
    spec = _util.spec_from_file_location(
        "_producers", os.path.join(ROOT, "scripts", "check-rmw-slot-producers.py")
    )
    mod = _util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.scan()


def _slot_for(symbol):
    """The vtable slot a contract symbol is answered by.

    The name rule is mechanical (`rmw_take` -> `take`); `GROUPED_SYMBOLS` in
    rmw-abi-shape.py is the authored exception list for one slot answering
    several upstream names.
    """
    try:
        spec = _util.spec_from_file_location(
            "_shape", os.path.join(ROOT, "scripts", "rmw-abi-shape.py")
        )
        mod = _util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        grouped = getattr(mod, "GROUPED_SYMBOLS", {})
    except Exception:  # noqa: BLE001
        grouped = {}
    return grouped.get(symbol) or symbol[len("rmw_"):]


def _producer_note(contract):
    """`vtable` means a SLOT exists, never that a backend fills it.

    Issue 0800: the loan trio and `set_log_severity` were each counted answered
    here while no backend implemented them, because the bucket answers "where
    do we answer this" and a declared slot IS where. Rather than overload the
    bucket — `check_against_vtable` would reject a `gap` for a slot that
    exists, correctly — the second dimension is printed beside it.

    Issue 0785 is why this is now resolved per SYMBOL and not only per slot.
    That issue found `rmw_get_node_names_with_enclaves` counted as answered
    while nothing in this ABI can carry an enclave, and asked for the answered
    column to stop over-counting. It generalises: the same question asked of
    every contract symbol shows most of the `vtable` column resting on slots
    nothing writes and nothing reads.
    """
    try:
        kinds = _slot_kinds()
    except Exception as exc:  # noqa: BLE001 - a report must not die on its footnote
        return f"  (slot-producer breakdown unavailable: {exc})"

    counts = {}
    for k in kinds.values():
        counts[k] = counts.get(k, 0) + 1

    inert_syms = []
    live_syms = 0
    for sym in contract:
        entry = MAP.get(sym)
        if not entry or entry[0] != "vtable":
            continue
        kind = kinds.get(_slot_for(sym))
        if kind == "inert":
            inert_syms.append(sym)
        else:
            live_syms += 1

    lines = [
        "  NOTE  `vtable` counts a SLOT, not a backend that fills one. Of "
        f"{len(kinds)} slots:",
        f"        {counts.get('produced', 0)} filled by some backend, "
        f"{counts.get('default', 0)} NULL with documented behaviour, "
        f"{counts.get('inert', 0)} written and read by nothing.",
        "        `just check rmw-slot-producers` is that dimension (issue 0800).",
        "",
        f"  Of the {live_syms + len(inert_syms)} contract symbol(s) in the `vtable` "
        f"column, {live_syms} are answered by a slot something",
        f"  writes or reads, and {len(inert_syms)} by an INERT one (issue 0785). "
        "An inert slot is a reserved shape,",
        "  not a working capability — see the declared families in "
        "check-rmw-slot-producers.py.",
    ]
    if inert_syms:
        lines.append("")
        lines.append(f"## answered by an inert slot ({len(inert_syms)})")
        for sym in inert_syms:
            lines.append(f"  {sym:52s} -> {_slot_for(sym)}")
    return "\n".join(lines)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--contract", action="store_true", help="re-derive from an installed impl")
    ap.add_argument("--check", action="store_true", help="fail on unclassified symbols")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    if args.contract:
        derived, info = derive_contract()
        if derived is None:
            sys.stderr.write(f"rmw-api-parity: {info}\n")
            return 2
        for line in info["per_lib"]:
            print(f"# {line}")
        for lib, extra in info["extras"].items():
            print(f"# {lib} defines {len(extra)} symbol(s) no sibling does: {', '.join(extra)}")
        print(f"# contract (defined by every implementation): {len(derived)}")
        for s in derived:
            print(s)
        return 0

    contract = read_contract()
    if contract is None:
        sys.stderr.write(
            f"rmw-api-parity: no recorded contract at {CONTRACT}.\n"
            "  Generate it where a ROS install exists:\n"
            "    scripts/rmw-api-parity.py --contract > docs/reference/rmw-implementation-contract.txt\n"
        )
        return 2

    counts, unclassified, stale, rows = report(contract)
    total = len(contract)

    print(f"rmw implementation contract: {total} symbol(s)")
    for b in BUCKETS:
        print(f"  {b:9s} {counts[b]:3d}")
    print()
    print(_producer_note(contract))
    print()
    for bucket in ("gap", "declined"):
        items = [r for r in rows if r[0] == bucket]
        if not items:
            continue
        print(f"## {bucket} ({len(items)})")
        for _b, sym, detail in items:
            print(f"  {sym:52s} {detail}")
        print()

    rc = 0
    drift = check_against_vtable()
    if drift:
        sys.stderr.write(
            f"rmw-api-parity: {len(drift)} mapping(s) contradict nros/rmw_vtable.h:\n"
        )
        for sym, complaint in drift:
            sys.stderr.write(f"  {sym:52s} {complaint}\n")
        sys.stderr.write(
            "\n  A slot was renamed or landed and this table did not follow. Fix the\n"
            "  MAP entry — the report above is what people quote for \"what do we\n"
            "  answer?\", and a stale one is worse than no table at all.\n"
        )
        rc = 1 if args.check else rc
    if unclassified:
        sys.stderr.write(
            f"rmw-api-parity: {len(unclassified)} contract symbol(s) with no classification:\n"
        )
        for s in unclassified:
            sys.stderr.write(f"  {s}\n")
        sys.stderr.write(
            "\n  Upstream grew a symbol, or the distro moved. Decide where we answer\n"
            "  it — a vtable slot, another layer, or a declined RTOS reason — and\n"
            "  add it to MAP. An unclassified symbol is the one case this tool\n"
            "  exists to prevent: a parity claim that quietly stopped being true.\n"
        )
        rc = 1 if args.check else rc
    if stale:
        sys.stderr.write(
            f"rmw-api-parity: {len(stale)} mapping(s) for symbol(s) not in the contract "
            "(upstream removed them, or the name is misspelled):\n"
        )
        for s in stale:
            sys.stderr.write(f"  {s}\n")
        rc = 1 if args.check else rc
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
