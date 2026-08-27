#!/usr/bin/env python3
"""Which vtable slots anything actually writes, and which anything reads.

Issue 0781 found the subscription loan pair declared, documented, and filled by
no backend — "the slot exists" reading as "the capability works". Counting the
rest of the vtable the same way turned up 42 slots with no producer, which is
not by itself a defect: plenty of slots are optional and the runtime has a
defined answer for a NULL one. The number was useless because it mixed kinds.

So this splits by the two questions that actually differ, both derived from the
tree rather than declared:

  producer — some backend's vtable initializer assigns it something non-NULL
  consumer — some non-generated source reads `vtable.<slot>` / `(*vt).<slot>`

and classifies every slot:

  produced      a backend fills it. Nothing to declare.
  default       consumed, no producer, and the header documents what a NULL
                slot means — so a caller gets a defined answer. The header IS
                the reason; this tool only checks it is there.
  unimplemented consumed, no producer, and NO documented NULL behaviour. A
                caller can reach it and the ABI does not say what happens.
                Must be declared, with a tracked issue.
  inert         no producer AND no consumer. Nothing in the tree writes or
                reads it: pure ABI surface. Legitimate — an ABI that mirrors
                upstream reserves a slot's position and shape before anything
                fills it — but it must be a DECISION, so every inert slot
                belongs to a declared family with a reason.

`inert` is the one worth staring at: as of 2026-08-26 it is 35 of 74. Half the
vtable is reserved rather than working, and before this tool nothing said so.

The families exist because 35 individual essays is how issue 0777 happened —
reasons written to fill a table, never checked. These slots are inert in groups,
for one reason per group, so the reason is written once where it is true.

Usage:
    scripts/check-rmw-slot-producers.py           # the report
    scripts/check-rmw-slot-producers.py --check   # fail on an unclassified slot
    scripts/check-rmw-slot-producers.py --self-test
"""

import argparse
import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VTABLE_H = os.path.join(
    ROOT, "packages", "core", "nros-rmw-abi", "include", "nros", "rmw_vtable.h"
)

# Backend vtable initializers. Positional (`/*name*/ value,`) for the C++ ones,
# designated for XRCE and the Rust adapter. The adapter counts: it is what fills
# slots for every `R: RustBackend`, and issue 0781 turned on exactly that.
PRODUCER_GLOBS = (
    "packages/rmw/*/*/src/vtable.c",
    "packages/rmw/*/*/src/vtable.cpp",
    "packages/rmw/cffi/src/rust_adapter.rs",
)

NULL_VALUES = {"nullptr", "NULL", "None", "0"}

# A NULL slot's documented behaviour, in the header, near the declaration.
NULL_DOC = re.compile(r"NULL (slot|function pointer|=)", re.IGNORECASE)

# How far back from a declaration its doc block can start. The longest real one
# is ~2 KB; the bound stops a neighbour's doc from being read as this slot's.
DOC_WINDOW = 2600

# Inert slots, grouped by why. Every inert slot must appear in exactly one
# family, and a family that names a slot which is no longer inert is stale.
INERT_FAMILIES = {
    "identity": (
        ("get_implementation_identifier", "get_serialization_format"),
        "the runtime answers both without asking a backend — the registry name "
        "and `\"cdr\"` — and no backend has yet wanted to say otherwise. Reserved "
        "so a bridge image linking two backends can, since that is the case where "
        "a per-backend answer stops being decoration",
    ),
    "capability-probe": (
        ("feature_supported",),
        "a generic probe with no caller. The capabilities the runtime actually "
        "branches on are each their own slot, answered by nullity or a dedicated "
        "probe, which is a narrower and checkable mechanism",
    ),
    "gid": (
        ("get_gid_for_publisher",),
        "publisher GIDs travel in the message attachment and are compared there; "
        "nothing asks a backend for one out of band",
    ),
    "matched-counts": (
        ("publisher_count_matched_subscriptions", "subscription_count_matched_publishers"),
        "discovery-introspection counts. Nothing in the executor or the C/C++ API "
        "surfaces them, so no consumer exists to give them meaning",
    ),
    "actual-qos": (
        (
            "client_request_publisher_get_actual_qos",
            "client_response_subscription_get_actual_qos",
            "service_request_subscription_get_actual_qos",
            "service_response_publisher_get_actual_qos",
        ),
        "reading back the QoS a backend actually granted after negotiation. The "
        "publisher and subscription halves are LIVE on cyclonedds since issue 0823 "
        "(`read_entity_qos`); these four are the client/service entities, which need "
        "the handle behind a client or service and have no consumer yet — "
        "phase-393 W1. Deleting them would re-hide what 0823 measured: the runtime "
        "reporting the QoS it asked for as the QoS it got",
    ),
    "acks": (
        ("publisher_wait_for_all_acked",),
        "a blocking wait for reliable delivery to be acknowledged. Blocking is the "
        "problem: this ABI's waiting is decomposed into `has_data` / `drive_io` / "
        "`next_deadline_ms` so one executor can drive several backends, and a slot "
        "that blocks inside one backend does not fit that",
    ),
    "with-info-takes": (
        ("take_with_info", "take_loaned_message_with_info"),
        "metadata-carrying variants of takes whose plain forms are live. The "
        "runtime gets publisher GID and timestamps from the attachment on the "
        "message it already took, so it has never needed the variant",
    ),
    "on-new-callbacks": (
        (
            "service_set_on_new_request_callback",
            "client_set_on_new_response_callback",
            "subscription_set_on_new_message_callback",
        ),
        "upstream's per-entity readiness callbacks. `set_wake_callback` is this "
        "ABI's answer and it is per-SESSION, which is the shape an executor "
        "driving several backends can use; the per-entity trio is reserved for "
        "parity",
    ),
    "graph-queries": (
        (
            "get_node_names",
            "get_topic_names_and_types",
            "get_service_names_and_types",
            "get_publisher_names_and_types_by_node",
            "get_subscriber_names_and_types_by_node",
            "get_service_names_and_types_by_node",
            "get_client_names_and_types_by_node",
            "get_publishers_info_by_topic",
            "get_subscriptions_info_by_topic",
        ),
        "introspecting the ROS graph. Cyclone's `graph.cpp` PUBLISHES this node's "
        "participant info so other nodes can see it, and that is the half an "
        "embedded image needs; reading the graph back means holding a discovered "
        "view of every peer, which is unbounded memory on a target. Reserved, and "
        "the distinction is worth keeping straight — `graph.cpp` existing has been "
        "read as these being implemented",
    ),
    "entity-counts": (
        ("count_publishers", "count_subscribers"),
        "as graph-queries: they need the same discovered view",
    ),
    "graph-guard": (
        ("node_get_graph_guard_condition",),
        "a guard condition fired on graph change. Guard conditions here are a "
        "platform primitive the executor owns, not something a backend hands out, "
        "and nothing consumes graph change events",
    ),
    "content-filter": (
        ("subscription_set_content_filter", "subscription_get_content_filter"),
        "DDS content-filtered topics. Landed in W4 for shape parity; no backend "
        "implements filtering and the runtime never asks",
    ),
    "network-flow": (
        ("publisher_get_network_flow_endpoints", "subscription_get_network_flow_endpoints"),
        "reporting the transport's endpoints. Landed in W4 for shape parity; "
        "diagnostic only, and nothing diagnoses",
    ),
}


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, os.path.join(ROOT, path))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def header_slots():
    return _load("_order", "scripts/check-vtable-positional-order.py").header_field_order()


def _git(*args):
    return subprocess.run(
        ["git", "-C", ROOT, *args], capture_output=True, text=True, check=False
    ).stdout.split()


ASSIGN = (
    r"/\*\s*([a-z_0-9]+)\s*\*/\s*([^,\n]+),",       # positional, annotated
    r"\.\s*([a-z_0-9]+)\s*=\s*([^,\n]+),",           # C designated
    r"(?m)^\s*([a-z_0-9]+):\s*([^,\n]+),",           # Rust struct literal
)


def producers_in(text, slots):
    """Slots this initializer assigns something other than a null literal."""
    got = set()
    for rx in ASSIGN:
        for m in re.finditer(rx, text):
            if m.group(1) in slots and m.group(2).strip() not in NULL_VALUES:
                got.add(m.group(1))
    return got


CONSUME = r"(?:vtable|\(\*vt\)|vt)\s*\.\s*{}\b"


def consumers_in(text, slots):
    """Slots this source READS off a vtable.

    Deliberately narrow. A looser pattern (any `.slot` or `->slot`) reported
    `create_node` as consumed because an unrelated C ops table in
    `orchestration_e2e` has a member of that name, and reported `destroy_node`
    as consumed for the same kind of reason — which would have hidden the leak
    this tool was written to find.
    """
    return {s for s in slots if re.search(CONSUME.format(re.escape(s)), text)}


def scan():
    slots = header_slots()

    produced = set()
    for rel in _git("ls-files", *PRODUCER_GLOBS):
        try:
            produced |= producers_in(open(os.path.join(ROOT, rel), encoding="utf-8").read(), slots)
        except OSError:
            continue

    consumed = set()
    for rel in _git("ls-files", "packages"):
        if not rel.endswith((".rs", ".c", ".cpp")):
            continue
        if "generated.rs" in rel or "/src/vtable." in rel or "rust_adapter.rs" in rel:
            continue
        try:
            consumed |= consumers_in(
                open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read(), slots
            )
        except OSError:
            continue

    header = open(VTABLE_H, encoding="utf-8").read()

    def documents_null(slot):
        i = header.find("(*" + slot + ")")
        return i >= 0 and bool(NULL_DOC.search(header[max(0, i - DOC_WINDOW):i]))

    out = {}
    for s in slots:
        if s in produced:
            out[s] = "produced"
        elif s not in consumed:
            out[s] = "inert"
        elif documents_null(s):
            out[s] = "default"
        else:
            out[s] = "unimplemented"
    return out


def self_test():
    bad = []

    slots = {"take", "publish", "set_log_severity"}
    if producers_in("/*take*/ nullptr,\n/*publish*/ &do_publish,\n", slots) != {"publish"}:
        bad.append("positional producer detection")
    if producers_in(".take = NULL,\n.publish = xrce_publish,\n", slots) != {"publish"}:
        bad.append("designated producer detection")
    if producers_in("    take: None,\n    publish: Some(t),\n", slots) != {"publish"}:
        bad.append("rust producer detection")

    # The narrow consumer pattern, and the false positive it exists to avoid.
    if consumers_in("if let Some(f) = self.vtable.take {", slots) != {"take"}:
        bad.append("consumer detection missed `vtable.take`")
    if consumers_in("context->ops->publish(x)", slots):
        bad.append("an unrelated ops table was counted as a vtable consumer")

    # Every family member is a real slot, and no slot is in two families.
    real = set(header_slots())
    seen = set()
    for fam, (members, _reason) in INERT_FAMILIES.items():
        for m in members:
            if m not in real:
                bad.append(f"family {fam} names {m}, which is not a slot")
            if m in seen:
                bad.append(f"{m} appears in more than one family")
            seen.add(m)

    if bad:
        for b in bad:
            sys.stderr.write("check-rmw-slot-producers --self-test: " + b + "\n")
        return 2
    print(f"check-rmw-slot-producers --self-test: OK ({len(seen)} family member(s), 5 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    kinds = scan()
    counts = {}
    for k in kinds.values():
        counts[k] = counts.get(k, 0) + 1
    total = len(kinds)

    print(f"# vtable slots: {total}\n")
    for k in ("produced", "default", "unimplemented", "inert"):
        print(f"  {k:<14} {counts.get(k, 0):>3}")

    for k in ("default", "unimplemented", "inert"):
        members = [s for s, v in kinds.items() if v == k]
        if not members:
            continue
        print(f"\n## {k} ({len(members)})\n")
        for s in members:
            fam = next(
                (f for f, (ms, _r) in INERT_FAMILIES.items() if s in ms), ""
            )
            print(f"  {s}{('  [' + fam + ']') if fam else ''}")

    if not args.check:
        return 0

    rc = 0
    inert = {s for s, v in kinds.items() if v == "inert"}
    claimed = {m for ms, _r in INERT_FAMILIES.values() for m in ms}

    undeclared = sorted(inert - claimed)
    if undeclared:
        rc = 1
        sys.stderr.write("\nERROR: inert slot in no declared family:\n")
        for s in undeclared:
            sys.stderr.write(f"  {s}\n")
        sys.stderr.write(
            "Nothing writes or reads it. Put it in an INERT_FAMILIES group with the\n"
            "reason it is reserved, or wire it — but do not leave it undecided.\n"
        )

    stale = sorted(claimed - inert)
    if stale:
        rc = 1
        sys.stderr.write("\nERROR: a family claims a slot that is no longer inert:\n")
        for s in stale:
            sys.stderr.write(f"  {s}  (now: {kinds.get(s, 'not a slot')})\n")
        sys.stderr.write("Remove it — the reason it carries has stopped being true.\n")

    unimpl = sorted(s for s, v in kinds.items() if v == "unimplemented")
    if unimpl:
        rc = 1
        sys.stderr.write("\nERROR: reachable slot with no producer and no documented NULL:\n")
        for s in unimpl:
            sys.stderr.write(f"  {s}\n")
        sys.stderr.write(
            "A caller can reach this and the header does not say what a NULL slot\n"
            "does. Document the NULL behaviour, or fill the slot.\n"
        )

    if rc == 0:
        print("\ncheck-rmw-slot-producers --check: OK (every slot is classified)")
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
