#!/usr/bin/env python3
"""Phase 376 W2 — our vtable against upstream's ABI, slot by slot and arg by arg.

`rmw-api-parity.py` answers "do we have the capability, somewhere". This answers
the stricter question the campaign actually set:

> Our ABI should look mostly identical to the official ABI except the RTOS
> revision. The revision can be done by adding or removing items, or fixing
> args. All RMW functions should go into the C vtable, generic over all
> backends.

So the target state is mechanical, and therefore checkable:

* every symbol in the implementation contract has a vtable slot named exactly
  the upstream name minus its `rmw_` prefix (`rmw_take` -> `take`);
* that slot's parameters match upstream's, or the difference is DECLARED with
  its RTOS reason;
* every slot with no upstream counterpart is a DECLARED addition, with a reason.

Nothing here is a matter of taste: a rename is a rename, an argument is present
or it is not. What needs judgement — whether a deviation is justified — is
exactly what the declaration tables below record, and what review looks at.

# Why "minus the prefix" rather than keeping `rmw_`

The slots live inside `nros_rmw_vtable_t`, so the type already carries the
namespace; a `rmw_` on every member would stutter. More usefully, the mechanical
rule means the comparison needs no authored name mapping at all — a mapping is a
place for a mistake to hide, and this one would have 88 entries.

Usage:
    scripts/rmw-abi-shape.py            # the report
    scripts/rmw-abi-shape.py --check    # non-zero if an undeclared deviation exists
    scripts/rmw-abi-shape.py --self-test
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VTABLE = os.path.join(ROOT, "packages", "core", "nros-rmw-abi", "include", "nros", "rmw_vtable.h")
SIGS = os.path.join(ROOT, "docs", "reference", "rmw-implementation-signatures.txt")
CONTRACT = os.path.join(ROOT, "docs", "reference", "rmw-implementation-contract.txt")

# ---------------------------------------------------------------------------
# Declared deviations. Everything here is a deliberate difference from upstream
# and needs a reason that is about the TARGET, not about our convenience.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# The vtable is a GENERIC RMW ABI, so its signatures carry no vendor name.
# `nros_rmw_publisher_t` says who wrote the header, which is the one thing a
# backend author does not need to know: the whole point of the seam is that a
# backend implements RMW, not nano-ros. The struct TAGS may stay ours — the
# typedef names are the surface.
#
# Target spelling for each type currently in a slot signature. A type with an
# upstream counterpart takes upstream's name; the RTOS-only ones take the same
# shape without a vendor prefix.
TYPE_TARGET = {
    "nros_rmw_ret_t": "rmw_ret_t",
    "nros_rmw_publisher_t": "rmw_publisher_t",
    "nros_rmw_subscription_t": "rmw_subscription_t",
    "nros_rmw_service_t": "rmw_service_t",
    "nros_rmw_client_t": "rmw_client_t",
    "nros_rmw_qos_t": "rmw_qos_profile_t",
    "nros_rmw_event_kind_t": "rmw_event_type_t",
    "nros_rmw_event_callback_t": "rmw_event_callback_t",
    "nros_rmw_publisher_options_t": "rmw_publisher_options_t",
    "nros_rmw_subscription_options_t": "rmw_subscription_options_t",
    # RTOS-only: no upstream counterpart, so the generic name is simply the
    # unprefixed one. `session` is the concept upstream splits into context and
    # node, which an image that opens exactly one of them does not need split.
    "nros_rmw_session_t": "rmw_session_t",
}

# Including our header and upstream's `rmw/rmw.h` in ONE translation unit would
# then be a redefinition. No TU in this repo does — a target image never links
# real rmw, and every host-side consumer reaches the backend through Rust — but
# it is the hazard the rename creates and it should fail loudly rather than
# produce two types of the same name. See the phase doc for the `#error` guard.
VENDOR_PREFIX = "nros_"


# Slots we add that upstream has no equivalent for.
ADDED = {
    "create_session": "upstream splits context/init/node; an RTOS image has ONE session and opens it once",
    "destroy_session": "pair of the above",
    "drive_io": "no background transport thread is assumed; the caller donates the CPU that does I/O",
    "next_deadline_ms": "lets a caller-driven loop sleep exactly until the backend's next internal event",
    "set_wake_callback": "wake the executor from the transport's own thread or ISR, without a wait-set",
    "ping_session": "liveness of the SESSION, which upstream expresses through a context that cannot fail",
    "has_data": "a poll that allocates nothing, for a loop with no wait-set",
    "has_request": "as above, service side",
    "publish_streamed": "publish a payload larger than any single buffer the target can hold",
    "subscription_supports_in_place": "probe: can this backend hand out its receive buffer directly",
    "process_raw_in_place": "dispatch from the transport's own buffer — no copy on a target with no spare RAM",
}

# Parameter differences on slots that DO correspond to an upstream function.
ARG_DEVIATIONS = {
    # Keyed by slot name. Each entry is a difference from upstream's parameter
    # list that a target constraint forces, and the reason must be about the
    # TARGET rather than about our convenience.
    "take": (
        "upstream takes (sub, void *ros_message, bool *taken, allocation); ours takes "
        "(sub, buf, buf_len, size_t *out_len, bool *taken) — there is no typesupport "
        "indirection on target, so the payload is BYTES and the caller owns the buffer, "
        "which means it needs the length back; the allocation argument has nothing to "
        "pre-size because pools are baked"
    ),
    "take_request": (
        "upstream takes (service, rmw_service_info_t *, void *ros_request, bool *taken); ours "
        "takes bytes (buf/buf_len/out_len) because there is no typesupport on target, and a "
        "bare `int64_t *seq_out` in place of the info struct — an RTOS reply needs the sequence "
        "number and nothing else in it"
    ),
    "take_response": (
        "same two deviations as `take_request`, client side"
    ),
    "take_sequence": (
        "upstream takes (sub, count, message_sequence, info_sequence, size_t *taken, allocation); "
        "ours takes a contiguous byte block plus a per-slot length array, because there is no "
        "typed message sequence on target and no allocator to pre-size"
    ),
    "take_loaned_message": (
        "upstream loans a typed `void **loaned_message`; ours is a byte view plus an opaque "
        "token to release, because there is no typesupport and the backend owns the buffer "
        "until `sub_release`"
    ),
    "service_server_is_available": (
        "upstream takes (node, client, bool *); an image has no node object — "
        "the client reaches its session directly, so the node parameter would be "
        "a pointer with nothing to point at"
    ),
}

# Contract symbols we deliberately do not implement at all.
from importlib import util as _util  # noqa: E402  (kept beside its use)

_spec = _util.spec_from_file_location("_parity", os.path.join(ROOT, "scripts", "rmw-api-parity.py"))
_parity = _util.module_from_spec(_spec)
_spec.loader.exec_module(_parity)
DECLINED = {k for k, (bucket, _why) in _parity.MAP.items() if bucket == "declined"}


def vtable_slots():
    """{slot: [param types]} from the vtable header.

    Comments are stripped first: the header documents each slot in prose that
    names other slots, and a prose mention must not read as a declaration
    (issue 0719's trap).
    """
    src = open(VTABLE, encoding="utf-8").read()
    body = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    body = re.sub(r"(?m)//.*$", " ", body)

    slots = {}
    for m in re.finditer(r"\(\s*\*\s*([a-z_0-9]+)\s*\)\s*\(", body):
        name = m.group(1)
        depth = 1
        params = ""
        for ch in body[m.end():]:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    break
            params += ch
        # A nested function-pointer parameter (`set_wake_callback`'s `cb`) is
        # matched by the same regex; it is a parameter, not a slot.
        slots.setdefault(name, _norm(params))
    return slots


def _norm(raw):
    """Same normalisation the inventory applies, so both sides are comparable."""
    sys.path.insert(0, os.path.join(ROOT, "scripts"))
    spec = _util.spec_from_file_location("_inv", os.path.join(ROOT, "scripts", "rmw-api-inventory.py"))
    mod = _util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.normalise_params(raw)


def upstream_signatures():
    """{name: [param types]} for the implementation contract only."""
    if not os.path.exists(SIGS) or not os.path.exists(CONTRACT):
        return None
    contract = {
        l.strip() for l in open(CONTRACT, encoding="utf-8") if l.strip() and not l.startswith("#")
    }
    sigs = {}
    for line in open(SIGS, encoding="utf-8"):
        if line.startswith("#") or not line.strip():
            continue
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 3:
            continue
        name, _ret, params = parts[0], parts[1], parts[2]
        if name in contract:
            sigs[name] = [p.strip() for p in params.split(",") if p.strip()]
    return sigs


def compare():
    slots = vtable_slots()
    up = upstream_signatures()
    if up is None:
        return None

    # Callback PARAMETERS caught by the slot regex are not slots.
    for probably_a_param in ("cb", "chunk_cb", "size_cb"):
        slots.pop(probably_a_param, None)

    missing, arg_diff, matched = [], [], []
    for name, params in sorted(up.items()):
        if name in DECLINED:
            continue
        slot = name[len("rmw_"):]
        if slot not in slots:
            missing.append((name, slot, params))
            continue
        if slots[slot] != params and slot not in ARG_DEVIATIONS:
            arg_diff.append((slot, params, slots[slot]))
        else:
            matched.append(slot)

    expected = {n[len("rmw_"):] for n in up} | set(ADDED)
    undeclared_extra = sorted(s for s in slots if s not in expected)

    # Vendor-named types in the signatures — the "generic flavour" rule.
    vendor_types = {}
    for slot, params in slots.items():
        for p in params:
            for tok in re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", p):
                if tok.startswith(VENDOR_PREFIX):
                    vendor_types.setdefault(tok, set()).add(slot)

    return {
        "vendor_types": vendor_types,
        "slots": slots,
        "upstream": up,
        "missing": missing,
        "arg_diff": arg_diff,
        "matched": matched,
        "undeclared_extra": undeclared_extra,
    }


def self_test():
    bad = []
    slots = vtable_slots()
    if "create_publisher" not in slots:
        bad.append("vtable parse found no create_publisher slot")
    if "cb" in slots and len(slots.get("cb", [])) == 0:
        pass  # popped in compare(); parsing it here is fine
    # A prose mention must not become a slot.
    probe = re.sub(r"/\*.*?\*/", " ", "/* calls (*take)(x) internally */ int (*real)(int a);", flags=re.S)
    names = re.findall(r"\(\s*\*\s*([a-z_0-9]+)\s*\)\s*\(", probe)
    if names != ["real"]:
        bad.append(f"a slot named only in a COMMENT must not count: {names}")
    r = compare()
    if r is not None:
        for tname in r["vendor_types"]:
            if tname not in TYPE_TARGET:
                bad.append(f"{tname}: in a signature with no target spelling in TYPE_TARGET")
    if not ADDED:
        bad.append("ADDED is empty — every RTOS-only slot must carry its reason")
    for slot, why in ADDED.items():
        if not why.strip():
            bad.append(f"{slot}: an addition with no reason is prose")
    if bad:
        for b in bad:
            sys.stderr.write("rmw-abi-shape --self-test: " + b + "\n")
        return 2
    print(f"rmw-abi-shape --self-test: OK ({len(slots)} slot(s) parsed, 3 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    r = compare()
    if r is None:
        sys.stderr.write(
            f"rmw-abi-shape: missing {SIGS} or {CONTRACT}.\n"
            "  Regenerate in the distrobox:\n"
            "    scripts/rmw-api-inventory.py --signatures > docs/reference/rmw-implementation-signatures.txt\n"
        )
        return 2

    total = len(r["upstream"]) - len(DECLINED & set(r["upstream"]))
    print("rmw ABI shape — our vtable against upstream, name and args")
    print(f"  contract symbols to mirror : {total}")
    print(f"  slots matching name + args : {len(r['matched'])}")
    print(f"  slots present, args differ : {len(r['arg_diff'])}")
    print(f"  no slot at all             : {len(r['missing'])}")
    print(f"  declared RTOS additions    : {len(ADDED)}")
    print(f"  UNDECLARED extra slots     : {len(r['undeclared_extra'])}")
    print(f"  vendor-named types in sigs : {len(r['vendor_types'])}")
    print()

    if r["vendor_types"]:
        print(f"## vendor-named types in slot signatures ({len(r['vendor_types'])})")
        print("   the vtable is a generic RMW ABI; a backend implements RMW, not nano-ros")
        for tname in sorted(r["vendor_types"]):
            target = TYPE_TARGET.get(tname, "?? no target spelling recorded")
            uses = len(r["vendor_types"][tname])
            print(f"  {tname:34s} -> {target:30s} ({uses} slot(s))")
        print()

    if r["arg_diff"]:
        print(f"## args differ ({len(r['arg_diff'])})")
        for slot, want, got in r["arg_diff"]:
            print(f"  {slot}")
            print(f"      upstream: ({', '.join(want)})")
            print(f"      ours:     ({', '.join(got)})")
        print()

    if r["undeclared_extra"]:
        print(f"## extra slots with no declaration ({len(r['undeclared_extra'])})")
        for s in r["undeclared_extra"]:
            print(f"  {s}")
        print()

    if r["missing"]:
        print(f"## no slot ({len(r['missing'])})")
        for name, slot, params in r["missing"]:
            print(f"  {slot:44s} <- {name}({', '.join(params)})")
        print()

    rc = 0
    if args.check and (
        r["missing"] or r["arg_diff"] or r["undeclared_extra"] or r["vendor_types"]
    ):
        sys.stderr.write(
            "rmw-abi-shape: the vtable does not mirror upstream.\n"
            "  Every contract symbol needs a slot named after it, with matching\n"
            "  args — or an entry in ARG_DEVIATIONS / ADDED / the parity table's\n"
            "  `declined` bucket, carrying the RTOS reason for the difference.\n"
            "  Signatures must also be vendor-free: see TYPE_TARGET.\n"
        )
        rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
