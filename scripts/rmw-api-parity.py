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

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# `librmw_zenoh_cpp.so`, not `librmw_dds_common__rosidl_typesupport_fastrtps_cpp.so`.
IMPL_LIB = re.compile(r"librmw_[a-z0-9]+_cpp\.so")
CONTRACT = os.path.join(ROOT, "docs", "reference", "rmw-implementation-contract.txt")

# Where we answer each symbol. `(bucket, detail)`.
#
# Written against ROS 2 Humble's rmw. A symbol NOT in this table is a hard
# failure under --check: that is the whole mechanism.
MAP = {
    # ---- Entity lifecycle: the vtable's core ----
    "rmw_create_publisher": ("vtable", "create_publisher"),
    "rmw_destroy_publisher": ("vtable", "destroy_publisher"),
    "rmw_create_subscription": ("vtable", "create_subscription"),
    "rmw_destroy_subscription": ("vtable", "destroy_subscription"),
    "rmw_create_service": ("vtable", "create_service"),
    "rmw_destroy_service": ("vtable", "destroy_service"),
    "rmw_create_client": ("vtable", "create_client"),
    "rmw_destroy_client": ("vtable", "destroy_client"),
    # ---- Data plane ----
    "rmw_publish": ("vtable", "publish_raw"),
    "rmw_take": ("vtable", "try_recv_raw"),
    "rmw_take_with_info": ("vtable", "try_recv_raw + MESSAGE_INFO_TABLE"),
    "rmw_take_sequence": ("vtable", "try_recv_sequence (burst-take, phase 124.D.1)"),
    "rmw_send_request": ("vtable", "send_request_raw"),
    "rmw_take_request": ("vtable", "try_recv_request"),
    "rmw_send_response": ("vtable", "send_reply"),
    "rmw_take_response": ("vtable", "try_recv_reply_raw"),
    "rmw_service_server_is_available": ("vtable", "service_server_is_available"),
    "rmw_publisher_assert_liveliness": ("vtable", "assert_publisher_liveliness"),
    # ---- Zero-copy / loaned ----
    "rmw_borrow_loaned_message": ("vtable", "pub_loan"),
    "rmw_return_loaned_message_from_publisher": ("vtable", "pub_discard"),
    "rmw_publish_loaned_message": ("vtable", "pub_commit"),
    "rmw_take_loaned_message": ("vtable", "sub_borrow"),
    "rmw_return_loaned_message_from_subscription": ("vtable", "sub_release"),
    "rmw_take_loaned_message_with_info": ("vtable", "sub_borrow + MESSAGE_INFO_TABLE"),
    # ---- Events ----
    "rmw_publisher_event_init": ("vtable", "register_publisher_event"),
    "rmw_subscription_event_init": ("vtable", "register_subscription_event"),
    "rmw_take_event": ("vtable", "the event callback delivers; no polled take"),
    "rmw_event_set_callback": ("vtable", "register_*_event takes the callback directly"),
    "rmw_subscription_set_on_new_message_callback": ("vtable", "set_wake_callback"),
    # ---- Answered a layer up or down ----
    "rmw_init": ("vtable", "create_session — grouped"),
    "rmw_shutdown": ("vtable", "destroy_session — grouped"),
    "rmw_context_fini": ("vtable", "destroy_session — grouped; no second teardown phase"),
    "rmw_init_options_init": (
        "declined",
        "upstream needs the init/copy/fini trio because its options OWN heap and carry "
        "an rcutils_allocator_t, which cannot cross this seam; ours is a build-time POD. "
        "Does NOT decide security_options / discovery_options, which we answer nowhere",
    ),
    "rmw_init_options_copy": ("declined", "as rmw_init_options_init — \"copy\" is `=`"),
    "rmw_init_options_fini": ("declined", "as rmw_init_options_init — \"fini\" is nothing"),
    "rmw_create_node": ("vtable", "create_node"),
    "rmw_destroy_node": ("vtable", "destroy_node"),
    "rmw_wait": (
        "declined",
        "has_data/has_request + drive_io + set_wake_callback + next_deadline_ms ARE "
        "this, decomposed. A vtable `wait` would add only the BLOCK, moved from the "
        "platform into a backend that can only block on its own handles — while one "
        "executor drives sessions from several backends, timers fire off the platform "
        "clock, and guard conditions fire from an ISR",
    ),
    "rmw_create_wait_set": (
        "declined",
        "the executor's arena entry table IS the set, allocated once; a per-wait set "
        "would be heap on the spin path",
    ),
    "rmw_destroy_wait_set": ("declined", "as rmw_create_wait_set"),
    "rmw_create_guard_condition": (
        "declined",
        "EntryKind::GuardCondition; no transport variation, and once `wait` is "
        "declined there is no backend consumer",
    ),
    "rmw_destroy_guard_condition": ("declined", "as rmw_create_guard_condition"),
    "rmw_trigger_guard_condition": (
        "declined",
        "GuardConditionHandle::trigger -> the platform wake primitive; ISR-safety is a "
        "platform-ABI guarantee no backend makes",
    ),
    "rmw_serialize": (
        "declined",
        "codegen, not a backend concern: CDR for an IDL type is fixed by ROS interop, "
        "so a per-backend answer would be a defect. Upstream's parameters are also two "
        "things this ABI already declined — a typesupport pointer and an "
        "`rmw_serialized_message_t`, which is an `rcutils_uint8_array_t` carrying an "
        "ALLOCATOR, at a seam with no allocator",
    ),
    "rmw_deserialize": ("declined", "as rmw_serialize"),
    "rmw_get_serialized_message_size": (
        "gap",
        "the old reason — \"generated per type; the bound is baked\" — was FALSE: "
        "nros-serdes declares only serialize/deserialize/deserialize_borrowed, no "
        "generated crate emits a size constant, and buffers are sized by env knobs "
        "(NROS_SUBSCRIPTION_BUFFER_SIZE). `report_dropped_take` says outright that it "
        "cannot name the size that would have worked",
    ),
    "rmw_publish_serialized_message": ("vtable", "publish — grouped; our payload IS CDR"),
    "rmw_take_serialized_message": ("vtable", "take — grouped; our payload IS CDR"),
    "rmw_take_serialized_message_with_info": ("vtable", "take_with_info — grouped"),
    "rmw_get_serialization_format": ("layer", "CDR, fixed per build"),
    "rmw_get_implementation_identifier": ("layer", "nros_rmw_descriptor_t (check-rmw-descriptors)"),
    "rmw_feature_supported": ("layer", "a NULL vtable slot IS the feature probe"),
    "rmw_init_publisher_allocation": ("declined", "no runtime allocation to pre-size; pools are baked"),
    "rmw_fini_publisher_allocation": ("declined", "as above"),
    "rmw_init_subscription_allocation": ("declined", "as above"),
    "rmw_fini_subscription_allocation": ("declined", "as above"),
    # ---- Graph / introspection ----
    "rmw_get_node_names": ("gap", "cyclone backend has graph.cpp; no vtable slot, so no portable answer"),
    "rmw_get_node_names_with_enclaves": ("gap", "same slot, enclave field unpopulated"),
    "rmw_get_topic_names_and_types": ("gap", "no vtable slot"),
    "rmw_get_service_names_and_types": ("gap", "no vtable slot"),
    "rmw_get_publisher_names_and_types_by_node": ("gap", "no vtable slot"),
    "rmw_get_subscriber_names_and_types_by_node": ("gap", "no vtable slot"),
    "rmw_get_service_names_and_types_by_node": ("gap", "no vtable slot"),
    "rmw_get_client_names_and_types_by_node": ("gap", "no vtable slot"),
    "rmw_get_publishers_info_by_topic": ("gap", "no vtable slot"),
    "rmw_get_subscriptions_info_by_topic": ("gap", "no vtable slot"),
    "rmw_count_publishers": ("gap", "no vtable slot"),
    "rmw_count_subscribers": ("gap", "no vtable slot"),
    "rmw_node_get_graph_guard_condition": ("gap", "no graph-change notification"),
    "rmw_publisher_count_matched_subscriptions": ("gap", "service_server_available is the only matched-count we expose"),
    "rmw_subscription_count_matched_publishers": ("gap", "as above"),
    # ---- QoS introspection ----
    "rmw_publisher_get_actual_qos": ("gap", "requested QoS is baked; the GRANTED profile is never read back"),
    "rmw_subscription_get_actual_qos": ("gap", "as above"),
    "rmw_client_request_publisher_get_actual_qos": ("gap", "as above"),
    "rmw_client_response_subscription_get_actual_qos": ("gap", "as above"),
    "rmw_service_request_subscription_get_actual_qos": ("gap", "as above"),
    "rmw_service_response_publisher_get_actual_qos": ("gap", "as above"),
    "rmw_qos_profile_check_compatible": (
        "layer",
        "a plain exported ABI function, not a vtable slot: its answer must not vary "
        "by backend, and the useful call sites (create-time validation, codegen, host "
        "tooling) have no vtable and may run before a backend registers. Declared in "
        "nros/rmw_entity.h, defined once in nros-rmw-cffi",
    ),
    # ---- Identity ----
    "rmw_get_gid_for_publisher": ("gap", "cyclone graph.cpp has GIDs; not exposed portably"),
    "rmw_compare_gids_equal": ("layer", "a plain exported ABI function; see rmw_qos_profile_check_compatible"),
    # ---- Declined: RTOS design ----
    "rmw_publisher_get_network_flow_endpoints": (
        "declined",
        "enumerates OS-level flows (DSCP, multicast egress); zenoh-pico/XRCE have no such notion",
    ),
    "rmw_subscription_get_network_flow_endpoints": ("declined", "as above"),
    "rmw_subscription_set_content_filter": (
        "declined",
        "content filtering is a DDS-only expression evaluator; would bloat every non-DDS backend",
    ),
    "rmw_subscription_get_content_filter": ("declined", "as above"),
    "rmw_set_log_severity": (
        "declined",
        "log level is a build-time constant (nros_log); a runtime setter implies a mutable global",
    ),
    "rmw_publisher_wait_for_all_acked": (
        "gap",
        "reliable backends know their unacked count; needed for clean shutdown",
    ),
    "rmw_client_set_on_new_response_callback": ("gap", "wake callback exists for subs only"),
    "rmw_service_set_on_new_request_callback": ("gap", "wake callback exists for subs only"),
}

BUCKETS = ("vtable", "layer", "declined", "gap")


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
    if bad:
        for b in bad:
            sys.stderr.write("rmw-api-parity --self-test: " + b + "\n")
        return 2
    print(f"rmw-api-parity --self-test: OK ({len(MAP)} mapping(s), 2 case(s))")
    return 0


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
    for bucket in ("gap", "declined"):
        items = [r for r in rows if r[0] == bucket]
        if not items:
            continue
        print(f"## {bucket} ({len(items)})")
        for _b, sym, detail in items:
            print(f"  {sym:52s} {detail}")
        print()

    rc = 0
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
