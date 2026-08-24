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


# Contract symbols answered by a plain exported C FUNCTION rather than a vtable
# slot, because their answer must not vary by backend.
#
# This table VERIFIES rather than records: `compare()` greps the ABI headers for
# each declaration, so an entry whose function was never declared is reported as
# missing. A table that only recorded intent would be the vacuous-test failure
# one level up — a claim of coverage with nothing behind it.
ABI_FUNCTIONS = {
    "rmw_qos_profile_check_compatible": (
        "computes over two `rmw_qos_profile_t` values with no entity, no session and "
        "no transport. A per-backend answer would be a DEFECT, and the useful call "
        "sites (create-time validation, codegen, host tooling) have no vtable to "
        "dispatch through — some run before any backend has registered"
    ),
    "rmw_compare_gids_equal": (
        "a comparison of two values this ABI defines; see qos_profile_check_compatible"
    ),
}

# One slot answering SEVERAL upstream names.
#
# The name rule is mechanical (`rmw_take` -> `take`) precisely so no authored
# mapping has to be kept true, and this table is the one deliberate exception —
# so it carries the same burden as any declared deviation: a reason, and a
# self-test proving the target slot actually exists. Without that last part an
# alias becomes a way to make a MISSING slot invisible, which is the opposite of
# what this tool is for.
GROUPED_SYMBOLS = {
    # Upstream split `_with_enclaves` off `rmw_get_node_names` only because
    # appending to a fixed out-parameter list would have broken its ABI. A
    # visitor has no such list, so the enclave is a fourth argument, NULL where
    # untracked.
    "rmw_get_node_names_with_enclaves": "get_node_names",
    # Our `publish` and `take` ALREADY deal in serialized bytes — the payload
    # crossing this seam is CDR, written by `nros-serdes` above it. So these
    # three upstream names describe what our slots do; a separate slot could
    # only ever forward to the same one. The mechanical name rule attached our
    # slots to the wrong namesake, and renaming them `publish_serialized_message`
    # would be worse than recording the grouping.
    "rmw_publish_serialized_message": "publish",
    "rmw_take_serialized_message": "take",
    "rmw_take_serialized_message_with_info": "take_with_info",
    # `create_session` / `destroy_session` ARE upstream's init/shutdown, in the
    # shape an image that opens exactly one session needs. Recorded here rather
    # than left in ADDED as "no upstream equivalent", which the sibling parity
    # file contradicted by naming these very slots as our answers.
    "rmw_init": "create_session",
    "rmw_shutdown": "destroy_session",
    "rmw_context_fini": "destroy_session",
}

# Slots we add that upstream has no equivalent for.
ADDED = {
    "create_session": (
        "upstream's `rmw_init` + context, in the shape an image that opens exactly ONE "
        "session needs — grouped onto it in GROUPED_SYMBOLS rather than claimed as "
        "having no upstream equivalent"
    ),
    "destroy_session": "upstream's `rmw_shutdown` + `rmw_context_fini`; there is no second teardown phase because the session shell is caller-owned",
    "drive_io": "no background transport thread is assumed; the caller donates the CPU that does I/O",
    "next_deadline_ms": "lets a caller-driven loop sleep exactly until the backend's next internal event",
    "set_wake_callback": (
        "wake the executor from the transport's own thread or ISR, without a wait-set. "
        "SESSION-scoped: the per-entity `*_set_on_new_*_callback` slots are the upstream "
        "family, and this is not a substitute for them"
    ),
    "ping_session": "liveness of the SESSION, which upstream expresses through a context that cannot fail",
    "has_data": "a poll that allocates nothing, for a loop with no wait-set",
    "has_request": "as above, service side",
    "publish_streamed": "publish a payload larger than any single buffer the target can hold",
    "subscription_supports_in_place": "probe: can this backend hand out its receive buffer directly",
    "process_raw_in_place": "dispatch from the transport's own buffer — no copy on a target with no spare RAM",
}

# RETURN-type differences, declared. Separate from ARG_DEVIATIONS because the
# return is the channel a caller detects failure through, so a difference here
# is a different KIND of claim from a parameter difference.
RET_DEVIATIONS = {
    # The four `create_*` slots: upstream RETURNS the entity pointer and signals
    # failure with NULL; ours returns a status and writes the entity through an
    # OUT parameter. Same decision as their ARG_DEVIATIONS entry — no runtime
    # allocation, so the caller owns the storage — and returning a status is
    # strictly more informative than NULL.
    "node_get_graph_guard_condition": (
        "registers a callback and returns a status, where upstream returns the guard "
        "condition itself — see the ARG_DEVIATIONS entry"
    ),
    "create_node": "node is an OUT parameter; the return carries the status (no runtime allocation) — as create_publisher",
    "create_publisher": "entity is an OUT parameter; the return carries the status (no runtime allocation)",
    "create_subscription": "as create_publisher",
    "create_service": "as create_publisher",
    "create_client": "as create_publisher",
    # The six `void` slots are GONE (2026-08-24). They are recorded here as a
    # deleted entry rather than an absence, because the reason they carried was
    # the shape a bad deviation takes: "cleanup is best-effort" described the
    # behaviour without justifying it, and re-reading it in W5 is what settled
    # that `void` was never a target constraint. All six now return `rmw_ret_t`
    # like upstream.
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
        "pre-size — see the allocation note on `publish`"
    ),
    "take_request": (
        "upstream takes (service, rmw_service_info_t *, void *ros_request, bool *taken); ours "
        "takes bytes (buf/buf_len/out_len) because there is no typesupport on target, and a "
        "bare `int64_t *seq_out` in place of the info struct — an RTOS reply needs the sequence "
        "number and nothing else in it. NOTE issue 0778: on cyclonedds this int64 is not a "
        "sequence at all but an index into a 32-entry table released only by send_response, "
        "so a request taken and never answered leaks a slot"
    ),
    "take_response": (
        "same two deviations as `take_request`, client side"
    ),
    # ---- Entity lifecycle ----
    # The session/node difference and the OUT-parameter shape are one decision
    # applied consistently, described in the phase doc: an image opens ONE
    # session, there is no typesupport indirection on target, and nothing is
    # allocated at create time.
    "create_node": (
        "no `rmw_context_t *`: an image has one session and reaches it directly. "
        "Node is an OUT parameter, as every other create here"
    ),
    "create_publisher": ("session not node; baked pkg/type strings not typesupport; entity is an OUT parameter (no runtime allocation)"),
    "create_subscription": ("as create_publisher"),
    "create_service": ("as create_publisher"),
    "create_client": ("as create_publisher"),
    # The `const`-only deviations are GONE (2026-08-24). Fifteen slots took a
    # NON-const handle where upstream takes `const`; W5 checked every backend
    # for a write through that pointer and found none — all four `*_data_mut`
    # uses in the Rust adapter are in `destroy_*`, and no C backend touches the
    # struct — so the deviation described nothing. The entry stays as this
    # comment because "listed so it stays visible instead of settling in as
    # permanent" was the right instinct and it worked.
    # ---- No node object to destroy through ----
    "destroy_publisher": ("upstream takes (node, entity); an image has no node object, so the entity alone identifies it"),
    "destroy_subscription": ("as destroy_publisher"),
    "destroy_service": ("as destroy_publisher"),
    "destroy_client": ("as destroy_publisher"),
    # ---- Data plane ----
    "publish": (
        "bytes (`const uint8_t *`, `size_t`) rather than a typed `const void *`, because "
        "codegen bakes the type and there is no typesupport on target; and no allocation argument: upstream's pre-sizes a per-entity allocator the CALLER owns, and this ABI has no allocator to hand one. NOT because 'pools are baked' — that clause was FALSE and is retired (issue 0777): cyclonedds ddsrt_malloc/calloc per publish AND per take, zenoh mallocs inside zenoh-pico, and the cffi shim vec![]s per fallback loan. Only uORB preallocates"
    ),
    "publish_loaned_message": ("a length instead of upstream's allocation argument: the loan is a byte slot, and the backend needs to know how much of it was written"),
    "borrow_loaned_message": ("upstream loans a typed message via `void **`; ours reserves a byte slot of a requested size and reports the granted capacity plus an opaque token, because the payload is bytes and the backend owns the buffer until it is committed or discarded"),
    # ---- Events ----
    "publisher_event_init": ("upstream fills an `rmw_event_t` the caller then polls with `rmw_take_event`; ours registers a CALLBACK directly, because an RTOS executor has no wait-set to poll an event handle from. The extra `uint32_t` is the QoS-policy filter and `void *` the callback context"),
    "subscription_event_init": ("as publisher_event_init"),
    "get_node_names": (
        "visitor instead of an allocating `rcutils_string_array_t` pair; session "
        "not node; and the `enclave` argument is what lets ONE slot answer both "
        "`rmw_get_node_names` and `rmw_get_node_names_with_enclaves` — upstream "
        "split those only because appending to a fixed out-parameter list would "
        "have broken its ABI, which a visitor has no equivalent of"
    ),
    "get_topic_names_and_types": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_service_names_and_types": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_publisher_names_and_types_by_node": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_subscriber_names_and_types_by_node": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_service_names_and_types_by_node": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_client_names_and_types_by_node": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_publishers_info_by_topic": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "get_subscriptions_info_by_topic": ('upstream returns an ALLOCATING `rmw_names_and_types_t` / `rcutils_string_array_t`; ours streams through a visitor callback, because there is no allocator at this seam and the ROS graph has no bound the CALLER can know — a buffer shape would make a 128 KiB image reserve for the worst graph it might ever meet. Session, not node, as everywhere else'),
    "count_publishers": (
        "session, not node — an image has no node object to count through"
    ),
    "count_subscribers": ("as count_publishers"),
    "node_get_graph_guard_condition": (
        "upstream RETURNS a guard condition for the caller to add to a wait set; we "
        "have no wait set and guard conditions are an executor concept here, so this "
        "registers a callback instead — the `set_wake_callback` shape. The callback "
        "is an EDGE with no payload: saying WHAT changed means buffering it, which "
        "is the graph cache a small target cannot afford"
    ),
    "take_with_info": (
        "the same two deviations `take` declares — bytes rather than a typed "
        "`void *ros_message` because there is no typesupport on target, and no "
        "allocation argument — see `publish`"
    ),
    "take_loaned_message_with_info": (
        "the same deviations `take_loaned_message` declares — a byte view plus an "
        "opaque release token instead of a typed loan"
    ),
    "publisher_wait_for_all_acked": (
        "`uint32_t timeout_ms` for upstream's by-value `rmw_time_t`: every duration "
        "in this ABI is u32 milliseconds (issue 0241), one width and one unit, so a "
        "per-call time struct would be the only one of its kind"
    ),
    "send_request": (
        "bytes rather than a typed `const void *`, and no `int64_t *sequence_id` "
        "OUT parameter: upstream hands the caller the sequence it assigned, while "
        "ours is a fire-and-forget publish whose reply is matched by "
        "`take_response`. W5 verdict: this is a GAP, not a deviation — filed as issue "
        "0778. Nothing matches the reply, because there is nothing to match it BY, and "
        "each backend invented an unsafe policy to cope (cyclone abandons the first "
        "request, zenoh assumes idempotence)"
    ),
    "send_response": (
        "bytes plus a bare `int64_t` sequence rather than upstream's "
        "`rmw_request_id_t *`: an RTOS reply needs the sequence and nothing else "
        "in that struct, the same deviation `take_request` declares on the way in"
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

# A `gap` whose reason names a TRACKED ISSUE is deferred, not forgotten, and
# `--check` must not treat it as a red — otherwise the only way to put this
# gate on the `just check` line is to stop tracking the gap, which is exactly
# backwards. The issue id is the whole requirement: a bare "not yet" would let
# anything sit here, while `issue 0776` is a file somebody has to close.
_ISSUE_REF = re.compile(r"\bissue[ -]?(\d{4})\b", re.I)
DEFERRED = {
    k: _ISSUE_REF.search(why).group(1)
    for k, (bucket, why) in _parity.MAP.items()
    if bucket == "gap" and _ISSUE_REF.search(why)
}


def vtable_slots():
    """{slot: [param types]} from the vtable header.

    Comments are stripped first: the header documents each slot in prose that
    names other slots, and a prose mention must not read as a declaration
    (issue 0719's trap).
    """
    src = open(VTABLE, encoding="utf-8").read()
    body = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    body = re.sub(r"(?m)//.*$", " ", body)

    # Only the vtable STRUCT's members are slots. Phase 376 W4 added
    # file-scope visitor typedefs (`rmw_node_visit_fn` and friends) which match
    # the same `(*name)(` shape, and scanning the whole file reported all three
    # as undeclared extra slots — a tool defect that reads as a finding.
    body = body[body.index("typedef struct nros_rmw_vtable_t {"):]
    body = body[: body.index("} nros_rmw_vtable_t;")]

    slots = {}
    rets = {}
    # `[*\s]*` after the type name: a slot may RETURN a pointer
    # (`const char *(*get_implementation_identifier)(void)`). Without it the
    # regex silently skipped every pointer-returning slot, so two slots that had
    # already landed were reported as MISSING — a tool defect that reads as a
    # gap in the ABI.
    for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_ ]*?[\w])[*\s]*\(\s*\*\s*([a-z_0-9]+)\s*\)\s*\(", body):
        # Keep the pointer in the return type: the capture group deliberately
        # stops at the last word so `const char *(*slot)(…)` parses at all, so
        # the `*`s have to be put back or every pointer-returning slot reports a
        # spurious `const char` vs `const char *` difference.
        # Between the type's last word and the opening `(` of `(*slot)` — the
        # slot's OWN `*` lives after that paren and must not be counted.
        gap = m.group(0)[m.end(1) - m.start(0) :]
        gap = gap[: gap.index("(")]
        ret = " ".join((m.group(1) + " " + "*" * gap.count("*")).split())
        name = m.group(2)
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
        rets.setdefault(name, ret)
    return slots, rets


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
        name, ret, params = parts[0], parts[1], parts[2]
        if name in contract:
            sigs[name] = (
                " ".join(ret.split()),
                [p.strip() for p in params.split(",") if p.strip()],
            )
    return sigs


def compare():
    slots, rets = vtable_slots()
    up = upstream_signatures()
    if up is None:
        return None

    # Callback PARAMETERS caught by the slot regex are not slots.
    for probably_a_param in ("cb", "chunk_cb", "size_cb"):
        slots.pop(probably_a_param, None)

    missing, arg_diff, matched, declared, ret_diff, grouped, abi_fns = (
        [], [], [], [], [], [], [])
    deferred = []

    # What the headers actually DECLARE, so the table above cannot claim a
    # function nobody wrote.
    hdr = ""
    inc = os.path.join(ROOT, "packages", "core", "nros-rmw-abi", "include", "nros")
    for fn in sorted(os.listdir(inc)):
        if fn.endswith(".h"):
            hdr += open(os.path.join(inc, fn), encoding="utf-8").read()
    hdr = re.sub(r"/\*.*?\*/", " ", hdr, flags=re.S)
    declared_fns = {
        n for n in ABI_FUNCTIONS if re.search(r"\b%s\s*\(" % re.escape(n), hdr)
    }
    for name, (up_ret, params) in sorted(up.items()):
        if name in DECLINED:
            continue
        if name in ABI_FUNCTIONS:
            if name in declared_fns:
                abi_fns.append(name)
            else:
                missing.append((name, name, params))
            continue
        slot = GROUPED_SYMBOLS.get(name) or name[len("rmw_"):]
        if name in GROUPED_SYMBOLS:
            # An alias is satisfied by its target existing; the target's own
            # entry is what checks the signature.
            if slot in slots:
                grouped.append((name, slot))
                continue
        if slot not in slots:
            (deferred if name in DEFERRED else missing).append((name, slot, params))
            continue
        # Phase 376 W5 — the RETURN type is part of a signature. This tool did
        # not parse it, so six slots returned `void` where upstream returns
        # `rmw_ret_t` — a difference nobody had declared, on the axis that
        # decides whether a caller can detect failure at all — and `--check` was
        # about to join `just check` blind to it.
        ret_ok = rets.get(slot, "") == up_ret
        if slots[slot] == params and ret_ok:
            matched.append(slot)
        elif slot in ARG_DEVIATIONS and (ret_ok or slot in RET_DEVIATIONS):
            declared.append(slot)
        elif not ret_ok and slot not in RET_DEVIATIONS:
            ret_diff.append((slot, up_ret, rets.get(slot, "?")))
        else:
            arg_diff.append((slot, params, slots[slot]))

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
        "deferred": deferred,
        "arg_diff": arg_diff,
        "matched": matched,
        "abi_fns": abi_fns,
        "grouped": grouped,
        "declared": declared,
        "ret_diff": ret_diff,
        "undeclared_extra": undeclared_extra,
    }


def self_test():
    bad = []
    slots, _rets = vtable_slots()
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
    slots_now, _r = vtable_slots()
    for sym, target in GROUPED_SYMBOLS.items():
        if target not in slots_now:
            bad.append(
                f"{sym}: grouped onto {target!r}, which is NOT a slot — an alias to a "
                "missing slot hides the gap it should report"
            )
    if not ADDED:
        bad.append("ADDED is empty — every RTOS-only slot must carry its reason")
    for slot, why in ADDED.items():
        if not why.strip():
            bad.append(f"{slot}: an addition with no reason is prose")
    # The deferral rule, both ways: a `gap` reason without an issue id is NOT
    # deferred, it is a red. Without this the rule degrades into "any gap is
    # fine", which is the failure mode that would make `--check` on the
    # `just check` line worth nothing.
    for probe, want in (("issue 0776. the bound is not computed", True),
                        ("issue-0776 tracked", True),
                        ("not yet, no bound is computed", False),
                        ("tracked in issue 77", False)):
        if bool(_ISSUE_REF.search(probe)) != want:
            bad.append(f"issue-ref rule: {probe!r} should be deferred={want}")
    if not DEFERRED:
        bad.append("no deferred gap resolved — the parity MAP link is broken")

    # A deviation entry for a slot that no longer deviates is the same drift
    # the parity MAP had (45 stale entries, two directions). Three ARG entries
    # and six RET entries survived their own fix here, still explaining a
    # difference the header had stopped having — and the RET ones read
    # "NOT a target constraint, W5 candidate to change", which is a to-do
    # item that had been done. Declarations are only worth reading if a stale
    # one is impossible.
    up = upstream_signatures()
    if up is not None:
        our_args, our_rets = vtable_slots()
        # Per SLOT, not per bucket: a slot can legitimately hold an ARG entry
        # and a stale RET entry at once (`destroy_publisher` did — it still
        # drops upstream's node argument, which kept it in the "declared"
        # bucket and hid that its return had stopped differing). So compare
        # each axis against the header directly.
        up_by_slot = {}
        for name, (uret, uargs) in up.items():
            up_by_slot[GROUPED_SYMBOLS.get(name) or name[len("rmw_"):]] = (uret, uargs)
        for s in sorted(ARG_DEVIATIONS):
            u = up_by_slot.get(s)
            if u and s in our_args and our_args[s] == u[1]:
                bad.append(
                    f"ARG_DEVIATIONS[{s!r}] explains an argument difference the header "
                    "no longer has"
                )
        for s in sorted(RET_DEVIATIONS):
            u = up_by_slot.get(s)
            if u and s in our_rets and our_rets[s] == u[0]:
                bad.append(
                    f"RET_DEVIATIONS[{s!r}] explains a return-type difference the header "
                    "no longer has"
                )

    if bad:
        for b in bad:
            sys.stderr.write("rmw-abi-shape --self-test: " + b + "\n")
        return 2
    print(f"rmw-abi-shape --self-test: OK ({len(slots)} slot(s) parsed, 7 case(s))")
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
    # Phase 376 W5 — three numbers, not two. This used to print ONE, counting a
    # slot with a DECLARED deviation as "matching name + args", so the headline
    # said 24 exact matches when 20 of those were declared differences. A
    # measurement that folds "identical" into "different but explained" cannot
    # answer the question the campaign is actually asking.
    print(f"  slots identical to upstream : {len(r['matched'])}")
    print(f"  name matches, args DECLARED : {len(r['declared'])}")
    print(f"  answered by a GROUPED slot  : {len(r['grouped'])}")
    print(f"  plain ABI functions         : {len(r['abi_fns'])}")
    print(f"  slots present, args differ : {len(r['arg_diff'])}")
    print(f"  UNDECLARED return-type diff: {len(r['ret_diff'])}")
    print(f"  no slot at all             : {len(r['missing'])}")
    print(f"  deferred, issue tracked    : {len(r['deferred'])}")
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

    if r["ret_diff"]:
        print(f"## return type differs, undeclared ({len(r['ret_diff'])})")
        for slot, want, got in r["ret_diff"]:
            print(f"  {slot:44s} upstream {want!r}, ours {got!r}")
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

    if r["deferred"]:
        print(f"## deferred, tracked by an issue ({len(r['deferred'])})")
        for name, slot, params in r["deferred"]:
            print(f"  {slot:44s} <- issue {DEFERRED[name]}")
        print()

    if r["missing"]:
        print(f"## no slot ({len(r['missing'])})")
        for name, slot, params in r["missing"]:
            print(f"  {slot:44s} <- {name}({', '.join(params)})")
        print()

    rc = 0
    if args.check and (
        r["missing"] or r["arg_diff"] or r["ret_diff"] or r["undeclared_extra"]
        or r["vendor_types"]
    ):
        sys.stderr.write(
            "rmw-abi-shape: the vtable does not mirror upstream.\n"
            "  Every contract symbol needs a slot named after it, with matching\n"
            "  args — or an entry in ARG_DEVIATIONS / ADDED / the parity table's\n"
            "  `declined` bucket, carrying the RTOS reason for the difference.\n"
            "  A gap that is real but not yet closed goes in the parity table's\n"
            "  `gap` bucket with a TRACKED ISSUE ID in the reason (`issue 0776`),\n"
            "  which is deferral with a name on it rather than an exemption.\n"
            "  Signatures must also be vendor-free: see TYPE_TARGET.\n"
        )
        rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
