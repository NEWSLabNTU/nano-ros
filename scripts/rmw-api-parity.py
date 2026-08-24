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
    "rmw_publish": ("vtable", "publish"),
    "rmw_take": ("vtable", "take"),
    "rmw_take_with_info": ("vtable", "take_with_info"),
    "rmw_take_sequence": ("vtable", "take_sequence (burst-take, phase 124.D.1)"),
    "rmw_send_request": ("vtable", "send_request"),
    "rmw_take_request": ("vtable", "take_request"),
    "rmw_send_response": ("vtable", "send_response"),
    "rmw_take_response": ("vtable", "take_response"),
    "rmw_service_server_is_available": ("vtable", "service_server_is_available"),
    "rmw_publisher_assert_liveliness": ("vtable", "publisher_assert_liveliness"),
    # ---- Zero-copy / loaned ----
    "rmw_borrow_loaned_message": ("vtable", "borrow_loaned_message"),
    "rmw_return_loaned_message_from_publisher": ("vtable", "return_loaned_message_from_publisher"),
    "rmw_publish_loaned_message": ("vtable", "publish_loaned_message"),
    "rmw_take_loaned_message": ("vtable", "take_loaned_message"),
    "rmw_return_loaned_message_from_subscription": ("vtable", "return_loaned_message_from_subscription"),
    "rmw_take_loaned_message_with_info": ("vtable", "take_loaned_message_with_info"),
    # ---- Events ----
    "rmw_publisher_event_init": ("vtable", "publisher_event_init"),
    "rmw_subscription_event_init": ("vtable", "subscription_event_init"),
    "rmw_take_event": (
        "declined",
        "upstream polls an event because the WAIT SET said one was ready, and the wait "
        "set is declined here — a poll would be blind. And upstream's poll exists to "
        "move status handling off the notification context onto a safe one; our "
        "callback already runs on the safe one, from inside drive_io on the executor "
        "thread, never an ISR or a transport thread",
    ),
    "rmw_event_set_callback": (
        "declined",
        "fused into publisher_event_init / subscription_event_init, which take the "
        "callback at init time; there is no rmw_event_t handle to attach one to later. "
        "Costs the ability to replace or clear a callback afterwards, which upstream has",
    ),
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
        "issue 0776. The old reason — \"generated per type; the bound is baked\" — was FALSE: "
        "nros-serdes declares only serialize/deserialize/deserialize_borrowed, no "
        "generated crate emits a size constant, and buffers are sized by env knobs "
        "(NROS_SUBSCRIPTION_BUFFER_SIZE). `report_dropped_take` says outright that it "
        "cannot name the size that would have worked",
    ),
    "rmw_publish_serialized_message": ("vtable", "publish — grouped; our payload IS CDR"),
    "rmw_take_serialized_message": ("vtable", "take — grouped; our payload IS CDR"),
    "rmw_take_serialized_message_with_info": ("vtable", "take_with_info — grouped"),
    "rmw_get_serialization_format": ("vtable", "get_serialization_format — CDR today, but the ANSWER is the backend's"),
    "rmw_get_implementation_identifier": ("vtable", "get_implementation_identifier — also in `nros_rmw_descriptor_t` (check-rmw-descriptors)"),
    "rmw_feature_supported": ("vtable", "feature_supported — a NULL slot is still the structural probe; this answers the named `rmw_feature_t` values"),
    "rmw_init_publisher_allocation": (
        "declined",
        "upstream pre-sizes a per-entity `rcutils_allocator_t` the CALLER owns, and this "
        "ABI has no allocator to hand one — there is nothing for the argument to point "
        "at. NOT because 'pools are baked': that clause was FALSE (issue 0777) — "
        "cyclonedds allocates per publish and per take, zenoh inside zenoh-pico, the cffi "
        "shim per fallback loan; only uORB preallocates",
    ),
    "rmw_fini_publisher_allocation": ("declined", "as above"),
    "rmw_init_subscription_allocation": ("declined", "as above"),
    "rmw_fini_subscription_allocation": ("declined", "as above"),
    # ---- Graph / introspection ----
    "rmw_get_node_names": (
        "vtable",
        "get_node_names — W4. A VISITOR (`rmw_node_visit_fn`), not an out-array: there is "
        "no allocator at this seam to hand back `rcutils_string_array_t` with",
    ),
    "rmw_get_node_names_with_enclaves": ("vtable", "get_node_names — grouped; the visitor carries `enclave`"),
    "rmw_get_topic_names_and_types": ("vtable", "get_topic_names_and_types — `rmw_names_and_types_visit_fn`"),
    "rmw_get_service_names_and_types": ("vtable", "get_service_names_and_types — visitor"),
    "rmw_get_publisher_names_and_types_by_node": ("vtable", "get_publisher_names_and_types_by_node — visitor"),
    "rmw_get_subscriber_names_and_types_by_node": ("vtable", "get_subscriber_names_and_types_by_node — visitor"),
    "rmw_get_service_names_and_types_by_node": ("vtable", "get_service_names_and_types_by_node — visitor"),
    "rmw_get_client_names_and_types_by_node": ("vtable", "get_client_names_and_types_by_node — visitor"),
    "rmw_get_publishers_info_by_topic": ("vtable", "get_publishers_info_by_topic — `rmw_topic_endpoint_info_visit_fn`"),
    "rmw_get_subscriptions_info_by_topic": ("vtable", "get_subscriptions_info_by_topic — endpoint-info visitor"),
    "rmw_count_publishers": ("vtable", "count_publishers"),
    "rmw_count_subscribers": ("vtable", "count_subscribers"),
    "rmw_node_get_graph_guard_condition": ("vtable", "node_get_graph_guard_condition"),
    "rmw_publisher_count_matched_subscriptions": ("vtable", "publisher_count_matched_subscriptions"),
    "rmw_subscription_count_matched_publishers": ("vtable", "subscription_count_matched_publishers"),
    # ---- QoS introspection ----
    "rmw_publisher_get_actual_qos": (
        "vtable",
        "publisher_get_actual_qos — W4. ALL-OR-NOTHING: a backend that can determine four policies and not the fifth returns UNSUPPORTED and writes nothing, because `rmw_qos_profile_t` has no UNKNOWN sentinel to report a partial answer with. Owed by W5",
    ),
    "rmw_subscription_get_actual_qos": ("vtable", "subscription_get_actual_qos — as above"),
    "rmw_client_request_publisher_get_actual_qos": ("vtable", "client_request_publisher_get_actual_qos — as above"),
    "rmw_client_response_subscription_get_actual_qos": ("vtable", "client_response_subscription_get_actual_qos — as above"),
    "rmw_service_request_subscription_get_actual_qos": ("vtable", "service_request_subscription_get_actual_qos — as above"),
    "rmw_service_response_publisher_get_actual_qos": ("vtable", "service_response_publisher_get_actual_qos — as above"),
    "rmw_qos_profile_check_compatible": (
        "layer",
        "a plain exported ABI function, not a vtable slot: its answer must not vary "
        "by backend, and the useful call sites (create-time validation, codegen, host "
        "tooling) have no vtable and may run before a backend registers. Declared in "
        "nros/rmw_entity.h, defined once in nros-rmw-cffi",
    ),
    # ---- Identity ----
    "rmw_get_gid_for_publisher": ("vtable", "get_gid_for_publisher"),
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
    "rmw_set_log_severity": ("vtable", "set_log_severity"),
    "rmw_publisher_wait_for_all_acked": ("vtable", "publisher_wait_for_all_acked"),
    "rmw_client_set_on_new_response_callback": ("vtable", "client_set_on_new_response_callback"),
    "rmw_service_set_on_new_request_callback": ("vtable", "service_set_on_new_request_callback"),
}

BUCKETS = ("vtable", "layer", "declined", "gap")


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
