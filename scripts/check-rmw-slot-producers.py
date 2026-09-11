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
  consumer — some non-generated source reads `vtable.<slot>` / `vt->slot`

**CONSUMPTION IS ASKED FIRST (phase-428 W8).** A slot is covered when something
READS it; a backend BODY does not satisfy that. The order used to be the other
way — `produced` was tested before `consumed`, so a slot some backend filled was
`produced` whatever the runtime did with it, and a body no dispatch site can
reach counted as a working capability. That is issue 0800's own confusion
("the slot exists" reading as "the capability works") displaced one step: not a
declared slot with no body, but a body with no caller. Five slots were in that
state and none of them showed up anywhere — four `*_get_actual_qos` variants
cyclonedds filled when issue 0823 closed, plus `required_rx_bytes`, filled for
zenoh-pico by phase-403 W4 while the header itself says "Nothing calls this yet".

  produced      something reads it AND a backend fills it. Nothing to declare.
  default       read, no producer, and the header documents what a NULL slot
                means — so a caller gets a defined answer. The header IS the
                reason; this tool only checks it is there.
  unimplemented read, no producer, and NO documented NULL behaviour. A caller
                can reach it and the ABI does not say what happens. Must be
                declared, with a tracked issue.
  inert         NOTHING READS IT. Pure ABI surface, whether or not a backend
                fills it. Legitimate — an ABI that mirrors upstream reserves a
                slot's position and shape before anything calls it — but it
                must be a DECISION, so every inert slot belongs to a declared
                family with a reason. The report says which of them have a
                body, because an unreachable body is a different cost from an
                unreserved shape: it is code somebody wrote and tests.

`inert` is the one worth staring at. It was 35 of 74 when this tool was written
(2026-08-26) — half the vtable reserved rather than working, and before this
tool nothing said so. Under the old ordering it read 6 of 68 on 2026-09-04; the
consumption-first count is 11 of 68 on 2026-09-11. Re-run the report rather than
trusting either sentence; the number is a snapshot and the tool is not.

The families exist because 35 individual essays is how issue 0777 happened —
reasons written to fill a table, never checked. These slots are inert in groups,
for one reason per group, so the reason is written once where it is true. A
family is therefore only as honest as its membership: `--check` rejects a family
that still names a slot which has since gained a producer or a consumer,
BECAUSE the reason such a family carries was written about a slot that no longer
matches it. `get_serialization_format` left the `identity` family that way in
phase-421 W2 — the family's own reason said it was reserved "so a bridge image
linking two backends can [ask]", and once that bridge and a backend speaking
something other than CDR both existed, the reason had argued itself out.

Usage:
    scripts/check-rmw-slot-producers.py           # the report
    scripts/check-rmw-slot-producers.py --check   # fail on an unclassified slot
    scripts/check-rmw-slot-producers.py --self-test
"""

import argparse
import collections
import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from issue_status import refused_deferrals  # noqa: E402

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
    # phase-403 W4 — a Rust backend's OWN table, which spreads the adapter's
    # and overrides the slots only it can answer. The three globs above missed
    # this shape entirely: zenoh has filled the publisher-loan trio from
    # `nros-rmw-zenoh/src/lib.rs` since phase 124.A.4.b and all three still read
    # here as "no producer", i.e. the exact "the slot exists and nothing fills
    # it" confusion issue 0781 wrote this tool to end, inverted. Adding
    # `required_rx_bytes` to that same table without widening the scan would
    # have left its inert-family reason true-looking and false.
    "packages/rmw/*/*/src/lib.rs",
)

NULL_VALUES = {"nullptr", "NULL", "None", "0"}

# The graph family (phase-381). Called out per backend because it is the one
# family where the backends legitimately differ by an order of magnitude —
# zenoh reads `@ros2_lv` liveliness tokens and answers all of it, Cyclone reads
# the `ros_discovery_info` topic and answers node enumeration, XRCE has no graph
# at all — and because reading the ANY-backend aggregate as a per-backend claim
# is issue 1137.
GRAPH_SLOTS = {
    "get_node_names",
    "get_topic_names_and_types",
    "get_service_names_and_types",
    "get_publisher_names_and_types_by_node",
    "get_subscriber_names_and_types_by_node",
    "get_service_names_and_types_by_node",
    "get_client_names_and_types_by_node",
    "get_publishers_info_by_topic",
    "get_subscriptions_info_by_topic",
    "count_publishers",
    "count_subscribers",
    "node_get_graph_guard_condition",
}

# A NULL slot's documented behaviour, in the header, near the declaration.
NULL_DOC = re.compile(r"NULL (slot|function pointer|=)", re.IGNORECASE)

# How far back from a declaration its doc block can start. The longest real one
# is ~2 KB; the bound stops a neighbour's doc from being read as this slot's.
DOC_WINDOW = 2600

# Inert slots, grouped by why. Every inert slot must appear in exactly one
# family, and a family that names a slot which is no longer inert is stale.
Family = collections.namedtuple("Family", "members reason defer")
Family.__new__.__defaults__ = (None,)

INERT_FAMILIES = {
    "identity": Family(
        ("get_implementation_identifier",),
        "the identity that is load-bearing is the one a backend stamps into "
        "`rmw_gid_t`, which `rmw_compare_gids_equal` reads before the bytes — so "
        "what matters is that a backend has ONE spelling of its name, not that it "
        "can be asked for it. Reserved for the day a caller needs to ask which of "
        "several linked backends it is on. NOTE its former family-mate "
        "`get_serialization_format` left in phase-421 W2: the reason above was "
        "written for both, and it stopped being true for the format the moment "
        "uORB answered `\"uorb\"` where the others answer `\"cdr\"`",
    ),
    "capability-probe": Family(
        ("feature_supported",),
        "a generic probe with no caller. The capabilities the runtime actually "
        "branches on are each their own slot, answered by nullity or a dedicated "
        "probe, which is a narrower and checkable mechanism",
    ),
    "acks": Family(
        ("publisher_wait_for_all_acked",),
        "a blocking wait for reliable delivery to be acknowledged. Blocking is the "
        "problem: this ABI's waiting is decomposed into `has_data` / `drive_io` / "
        "`next_deadline_ms` so one executor can drive several backends, and a slot "
        "that blocks inside one backend does not fit that",
    ),
    "with-info-takes": Family(
        ("take_with_info", "take_loaned_message_with_info"),
        "metadata-carrying variants of takes whose plain forms are live. The "
        "runtime gets publisher GID and timestamps from the attachment on the "
        "message it already took, so it has never needed the variant",
    ),
    "graph-guard": Family(
        ("node_get_graph_guard_condition",),
        "a guard condition fired on graph change. Guard conditions here are a "
        "platform primitive the executor owns, not something a backend hands out, "
        "and nothing consumes graph change events",
    ),
    # The family below is a slot a backend FILLS. It became visible when
    # phase-428 W8 made consumption the first question; under the old ordering
    # a body was enough to read as covered.
    #
    # Its former family-mate `granted-qos-service-side` — the four
    # `{client,service}_{request,response}_*_get_actual_qos` — is GONE, and
    # deliberately not replaced by a narrower reason: issue 1327 gave the four
    # a consumer at `create_client` / `create_service` in `cffi/src/lib.rs`,
    # so they classify `produced` and a family entry for them would be the
    # stale-claim shape this table checks for.
    "rx-sizing": Family(
        ("required_rx_bytes",),
        "zenoh-pico fills it (phase-403 W4) and no dispatch site exists: the "
        "slot's own header block says so and names phase-403 W3/W5 as the owner "
        "of that site, which is still open work. Reserved deliberately — the "
        "header also records the 2026-08-31 ruling that the slot stays OPTIONAL "
        "permanently, so a NULL one is an answer and not a gap",
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
    # Rust struct literal, WRAPPED. The pattern above needs the value and its
    # comma on one line, so rustfmt breaking a long initializer —
    #     get_publisher_names_and_types_by_node: Some(
    #         get_publisher_names_and_types_by_node_trampoline::<R>,
    #     ),
    # — made a WIRED slot read as unwired. A false negative in the one gate
    # whose job is telling those apart, and it fires on exactly the slots with
    # the longest names. Captures the opening `Some(`, which is not a null
    # literal, so `None,` cannot match it (phase-381 W3).
    r"(?m)^\s*([a-z_0-9]+):\s*(Some\()\s*$",
)


def producers_in(text, slots):
    """Slots this initializer assigns something other than a null literal."""
    got = set()
    for rx in ASSIGN:
        for m in re.finditer(rx, text):
            if m.group(1) in slots and m.group(2).strip() not in NULL_VALUES:
                got.add(m.group(1))
    return got


CONSUME = r"(?:vtable|vtbl|\(\*vt\)|vt)\s*(?:\.|->)\s*{}\b"


def consumers_in(text, slots):
    """Slots this source READS off a vtable.

    Narrow on the RECEIVER, not on the operator. A looser pattern (any `.slot`
    or `->slot`) reported `create_node` as consumed because an unrelated C ops
    table in `orchestration_e2e` has a member of that name, and reported
    `destroy_node` as consumed for the same kind of reason — which would have
    hidden the leak this tool was written to find. Pinning the receiver to
    `vt` / `vtbl` / `vtable` keeps that out (`context->ops->publish` has
    receiver `ops`) while letting `->` back in: a C or C++ consumer calls
    through a POINTER, so `vt->take(...)` is the only spelling available to it
    and the `.`-only pattern could not see one. Measured 2026-09-11: widening
    it moves no slot today, because every arrow consumer is a smoke test for a
    slot the Rust runtime already reads — a closed false-negative, not a
    recount.
    """
    return {s for s in slots if re.search(CONSUME.format(re.escape(s)), text)}


def backend_of(rel):
    """The backend a producer file belongs to — `packages/rmw/<b>/...`.

    The Rust adapter is not a backend: it fills slots for EVERY `R:
    RustBackend`, so attributing its slots to one name would be a lie in both
    directions. It is reported under its own label.
    """
    if rel.endswith("rust_adapter.rs"):
        return "rust-adapter (every Rust backend)"
    parts = rel.split("/")
    if len(parts) > 2 and parts[0] == "packages" and parts[1] == "rmw":
        return parts[2]
    return rel


def producers_by_backend(slots):
    """backend -> the slots ITS vtable fills.

    Issue 1137 — the aggregate `produced` answers "does ANY backend fill this",
    which is what this tool documents and exactly what it should answer for a
    question about the ABI. It was then READ as a per-backend claim: the issue
    was filed saying "all twelve Cyclone graph slots are `produced`", and nine
    answering `UNSUPPORTED` live looked like nine regressions. Cyclone fills ONE
    of the twelve (`get_node_names`) and the other eleven are `nullptr` on
    purpose — phase-381 W5's stated scope, which lived only in prose. So the
    per-backend split is MEASURED here rather than argued anywhere, and the
    aggregate keeps meaning what it always meant.
    """
    out = {}
    for rel in _git("ls-files", *PRODUCER_GLOBS):
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except OSError:
            continue
        got = producers_in(text, slots)
        if got:
            out.setdefault(backend_of(rel), set()).update(got)
    return out


def _producers(slots):
    produced = set()
    for rel in _git("ls-files", *PRODUCER_GLOBS):
        try:
            produced |= producers_in(open(os.path.join(ROOT, rel), encoding="utf-8").read(), slots)
        except OSError:
            continue
    return produced


def _consumers(slots):
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
    return consumed


def scan():
    return scan_detail()[0]


def classify(slots, produced, consumed, documents_null):
    """The four kinds, with CONSUMPTION asked first (phase-428 W8).

    Split out of `scan()` so the self-test can drive it with planted sets: the
    ordering is the whole content of this function and a negative control over
    it must not need a tree to disagree with.
    """
    out = {}
    for s in slots:
        if s not in consumed:
            # Nothing reads it. A backend body does not change that — it only
            # changes what the inert slot COSTS, which the report says.
            out[s] = "inert"
        elif s in produced:
            out[s] = "produced"
        elif documents_null(s):
            out[s] = "default"
        else:
            out[s] = "unimplemented"
    return out


def scan_detail():
    """`(kinds, produced, consumed)` — for a caller that needs to say WHY a
    slot is inert. `scan()` stays `{slot: kind}` because that is what
    `rmw-api-parity` imports."""
    slots = header_slots()
    produced, consumed = _producers(slots), _consumers(slots)
    header = open(VTABLE_H, encoding="utf-8").read()

    def documents_null(slot):
        i = header.find("(*" + slot + ")")
        return i >= 0 and bool(NULL_DOC.search(header[max(0, i - DOC_WINDOW):i]))

    return classify(slots, produced, consumed, documents_null), produced, consumed


def self_test():
    bad = []

    slots = {"take", "publish", "set_log_severity"}
    if producers_in("/*take*/ nullptr,\n/*publish*/ &do_publish,\n", slots) != {"publish"}:
        bad.append("positional producer detection")
    if producers_in(".take = NULL,\n.publish = xrce_publish,\n", slots) != {"publish"}:
        bad.append("designated producer detection")
    if producers_in("    take: None,\n    publish: Some(t),\n", slots) != {"publish"}:
        bad.append("rust producer detection")
    # phase-381 W3 — rustfmt wraps a long initializer, and the wrapped form must
    # still read as WIRED. Both directions, because an arm that matched `None`
    # too would be worse than the blind spot it replaces.
    wrapped = "    publish: Some(\n        some_very_long_trampoline_name::<R>,\n    ),\n"
    if producers_in(wrapped, slots) != {"publish"}:
        bad.append("wrapped rust producer not detected")
    if producers_in("    take: None,\n", slots):
        bad.append("a None slot was read as produced")

    # The narrow consumer pattern, and the false positive it exists to avoid.
    if consumers_in("if let Some(f) = self.vtable.take {", slots) != {"take"}:
        bad.append("consumer detection missed `vtable.take`")
    if consumers_in("context->ops->publish(x)", slots):
        bad.append("an unrelated ops table was counted as a vtable consumer")

    # Per-backend attribution (issue 1137). The adapter must NOT be attributed
    # to a backend name — it fills slots for every Rust backend, so calling it
    # "zenoh" would be wrong in both directions.
    if backend_of("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp") != "cyclonedds":
        bad.append("a backend vtable was not attributed to its backend")
    if backend_of("packages/rmw/cffi/src/rust_adapter.rs") == "cffi":
        bad.append("the Rust adapter was attributed to a backend")

    # Every graph slot named here is a real slot. A typo would silently shrink
    # the family and make a backend look more complete than it is.
    for s in sorted(GRAPH_SLOTS):
        if s not in set(header_slots()):
            bad.append(f"GRAPH_SLOTS names {s}, which is not a slot")

    # Every family member is a real slot, and no slot is in two families.
    real = set(header_slots())
    seen = set()
    for fam, fam_def in INERT_FAMILIES.items():
        members = fam_def.members
        for m in members:
            if m not in real:
                bad.append(f"family {fam} names {m}, which is not a slot")
            if m in seen:
                bad.append(f"{m} appears in more than one family")
            seen.add(m)

    # phase-428 W8 — CONSUMPTION FIRST. The negative control for the ordering
    # itself, planted, because the tree is only ever one arrangement: a slot a
    # backend fills and nothing reads must classify `inert`, and the mutation
    # that reverses the two branches must fail here.
    planted = classify(
        ["filled_and_read", "filled_unread", "read_only_doc", "read_only_nodoc", "neither"],
        produced={"filled_and_read", "filled_unread"},
        consumed={"filled_and_read", "read_only_doc", "read_only_nodoc"},
        documents_null=lambda s: s == "read_only_doc",
    )
    want = {
        "filled_and_read": "produced",
        "filled_unread": "inert",
        "read_only_doc": "default",
        "read_only_nodoc": "unimplemented",
        "neither": "inert",
    }
    for slot, expect in want.items():
        if planted[slot] != expect:
            bad.append(
                f"classify: {slot} read as {planted[slot]!r}, want {expect!r}"
                + (" — a backend BODY must not satisfy the check"
                   if slot == "filled_unread" else "")
            )

    # A family may DEFER — "reserved until somebody does X" — and then the
    # `defer` field must name an OPEN issue (phase-428 W11). Structured, not
    # parsed out of the reason: a reason legitimately cites resolved issues,
    # because history is what a reason is made of, and only the `defer` claim
    # says the work is still tracked. The subject list is every family, so one
    # added later is covered with no edit here.
    for fam, fam_def in sorted(INERT_FAMILIES.items()):
        for num, why in refused_deferrals(fam_def.defer):
            bad.append(f"family {fam}: `defer = {num}` {why}")

    if bad:
        for b in bad:
            sys.stderr.write("check-rmw-slot-producers --self-test: " + b + "\n")
        return 2
    print(
        f"check-rmw-slot-producers --self-test: OK ({len(seen)} family member(s), "
        "13 case(s))"
    )
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    kinds, produced, _consumed = scan_detail()
    counts = {}
    for k in kinds.values():
        counts[k] = counts.get(k, 0) + 1
    total = len(kinds)

    print(f"# vtable slots: {total}\n")
    for k in ("produced", "default", "unimplemented", "inert"):
        print(f"  {k:<14} {counts.get(k, 0):>3}")

    # Per-backend, and the graph family called out — issue 1137. `produced`
    # above is an ANY-backend answer about the ABI; this is the per-backend one,
    # and it is the difference between "the slot works" and "the slot works on
    # the backend you are running".
    by_backend = producers_by_backend(list(kinds))
    if by_backend:
        print("\n## filled per backend (of {} slots)\n".format(len(kinds)))
        for name in sorted(by_backend):
            got = by_backend[name]
            graph = sorted(GRAPH_SLOTS & got)
            print(
                f"  {name:<32} {len(got):>3}   graph {len(graph)}/{len(GRAPH_SLOTS)}"
                + (f"  [{', '.join(graph)}]" if graph else "")
            )
        print(
            "\n  A slot this backend does not fill answers UNSUPPORTED at runtime,\n"
            "  which is a DECLARED 'cannot tell you' — never an empty result\n"
            "  (RFC-0035 / phase-381 W6). Do not read the aggregate above as a\n"
            "  per-backend capability: issue 1137 did, and nine correct\n"
            "  UNSUPPORTEDs were filed as nine bugs.\n"
            "\n"
            "  AND THIS TABLE STILL CANNOT ANSWER IT FOR A RUST BACKEND. The\n"
            "  adapter row is a trampoline per slot, non-NULL for EVERY\n"
            "  `R: RustBackend`, and the trampoline forwards to a `Session`\n"
            "  trait method whose default returns `Unsupported`. So a Rust\n"
            "  backend reads as filled here whether or not it implements the\n"
            "  method. Nothing static closes that gap — the answer is a live\n"
            "  call, which is what `graph_interop` is for."
        )

    for k in ("default", "unimplemented", "inert"):
        members = [s for s, v in kinds.items() if v == k]
        if not members:
            continue
        print(f"\n## {k} ({len(members)})\n")
        if k == "inert":
            print(
                "  Nothing READS these. `+body` marks one a backend nonetheless\n"
                "  fills: an unreachable implementation, which costs more than an\n"
                "  unreserved shape because somebody wrote and maintains it.\n"
            )
        for s in members:
            fam = next(
                (f for f, d in INERT_FAMILIES.items() if s in d.members), ""
            )
            body = "  +body" if (k == "inert" and s in produced) else ""
            print(f"  {s}{('  [' + fam + ']') if fam else ''}{body}")

    if not args.check:
        return 0

    rc = 0
    inert = {s for s, v in kinds.items() if v == "inert"}
    claimed = {m for d in INERT_FAMILIES.values() for m in d.members}

    undeclared = sorted(inert - claimed)
    if undeclared:
        rc = 1
        sys.stderr.write("\nERROR: inert slot in no declared family:\n")
        for s in undeclared:
            sys.stderr.write(f"  {s}\n")
        sys.stderr.write(
            "NOTHING READS IT — a backend filling it does not change that, it only\n"
            "adds an unreachable body. Put it in an INERT_FAMILIES group with the\n"
            "reason it is reserved (and a `defer =` issue if the reason is 'not\n"
            "yet'), or give it a consumer — but do not leave it undecided.\n"
        )

    stale = sorted(claimed - inert)
    if stale:
        rc = 1
        sys.stderr.write("\nERROR: a family claims a slot that is no longer inert:\n")
        for s in stale:
            sys.stderr.write(f"  {s}  (now: {kinds.get(s, 'not a slot')})\n")
        sys.stderr.write(
            "Something READS it now. Remove it — the reason it carries was\n"
            "written about a slot nothing could reach.\n"
        )

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
