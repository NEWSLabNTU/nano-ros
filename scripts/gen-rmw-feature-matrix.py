#!/usr/bin/env python3
"""Generate book/src/reference/rmw-feature-matrix.md from backend sources.

The per-RMW capability story used to live as prose scattered over four
pages, and it drifted in BOTH directions (Cyclone services were
documented as unsupported a year after service.cpp landed; zenoh manual
liveliness was documented as a no-op while the shim wired it — and later
the reverse, once phase-467 Row 7 made it answer UNSUPPORTED under
MANUAL_* while a `fn` presence check still read "wired"). This
generator derives the wired/NULL facts from the artifacts that cannot
lie:

  * the three C vtables (positional `/*slot*/ value` in cyclonedds's and
    uORB's vtable.cpp, designated `.slot = value` in xrce's vtable.c). A
    slot filled with a function whose whole body is `return
    NROS_RMW_RET_UNSUPPORTED;` counts as NOT wired — uORB fills its
    service slots exactly that way, and a non-NULL pointer that refuses is
    the same answer as NULL to a caller,
  * the zenoh shim's Rust trait overrides (presence of EVERY named `fn` in
    the backend source, not ending in an `Unsupported` refusal — the trait
    default is the unsupported fallback), plus any slot the zenoh crate
    fills in its own `NrosRmwVtable` literal (the `lending` loan slots),
  * each backend's OWN `supported_qos_policies` answer: the zenoh shim's
    `QoSPolicyMask` expression and, for a C backend, the function its
    vtable puts in the `supported_qos_policies` slot,
  * the runtime cells of `matrix::CELLS` and `interop::CELLS` for the
    node-layer rows' "tested" verdict — not an authored list, which is
    what said Cyclone actions were untested while two interop cells and
    seven matrix cells ran them.

A capability may have more than one legitimate slot shape: status events
are delivered either by a backend CALLBACK (`*_event_init`) or by the
runtime POLLING the backend (`*_take_event`, issue 1164 — Cyclone's
shape). A row lists every accepted shape, and a backend is wired when it
fills any one of them completely.

Run:  python3 scripts/gen-rmw-feature-matrix.py [--check]
Gate: just check rmw-feature-matrix (derived fast lane)
"""

import os
import re
import sys

import sys as _sys
from pathlib import Path as _Path

_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "book", "src", "reference", "rmw-feature-matrix.md")

# The canonical slot list: EMPTY_VTABLE in the cffi crate names EVERY field
# (phase-376 W4 made that a load-bearing property). Parsed as the authority so
# a slot rename/addition surfaces here as a diff, never as a silent miss.
CFFI_EMPTY = "packages/rmw/cffi/src/lib.rs"
ZENOH_SRC_DIR = "packages/rmw/zenoh/nros-rmw-zenoh/src"
ZENOH_SESSION = "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs"
QOS_MASK_DEF = "packages/core/nros-rmw/src/traits.rs"
MATRIX_CELLS = "packages/testing/nros-tests/src/matrix.rs"
INTEROP_CELLS = "packages/testing/nros-tests/src/interop.rs"

# column label -> (vtable source, initializer shape, backend source dir,
#                  `Rmw::` variant in the test cells)
C_BACKENDS = [
    ("XRCE-DDS", "packages/rmw/xrce/nros-rmw-xrce/src/vtable.c", "designated",
     "packages/rmw/xrce/nros-rmw-xrce/src", "Xrce"),
    ("Cyclone DDS", "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp",
     "positional", "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src", "Cyclonedds"),
    ("uORB", "packages/rmw/uorb/nros-rmw-uorb/src/vtable.cpp", "positional",
     "packages/rmw/uorb/nros-rmw-uorb/src", "Uorb"),
]
ZENOH_RMW = "Zenoh"


def any_of(*shapes):
    """A row accepted in more than one slot shape (each a list of slots)."""
    return [list(s) for s in shapes]


# (row label, accepted C-ABI slot shapes, zenoh regexes — ALL must match)
# Slot names follow the phase-376 upstream-rmw spellings.
FEATURES = [
    ("Publish / subscribe", any_of(["create_publisher", "create_subscription"]),
     [r"fn create_publisher", r"fn create_subscription", r"fn publish_raw"]),
    ("Services (server side)",
     any_of(["create_service", "take_request", "send_response"]),
     [r"fn create_service", r"fn send_response"]),
    ("Service clients", any_of(["create_client", "send_request", "take_response"]),
     [r"fn create_client", r"fn send_request"]),
    # phase-428 W13: the zenoh trait method is `service_is_ready` (rclcpp's
    # name); `server_available` was the pre-#638 spelling.
    ("Server-availability probe", any_of(["service_server_is_available"]),
     [r"fn (server_available|service_is_ready)"]),
    # Issue 1164: callback (`*_event_init`) OR runtime-polled (`*_take_event`).
    ("Status events (deadline / liveliness / lost)",
     any_of(["subscription_event_init", "publisher_event_init"],
            ["subscription_take_event", "publisher_take_event"]),
     [r"fn register_event_callback"]),
    # Issue 1972: the Iron+ kinds ride the SAME slots, so the slot shape alone
    # cannot tell them apart from the Tier-1 set. The fourth element is the
    # extra evidence a C backend must carry: a `case` arm WITH A BODY for each
    # matched kind in its sources (a wrong-side `case X:` that falls through to
    # the caller-error default has no `{`). zenoh's evidence is its trait code
    # naming the kinds.
    ("Matched / incompatible-type events (Iron+)",
     any_of(["subscription_event_init", "publisher_event_init"],
            ["subscription_take_event", "publisher_take_event"]),
     [r"EventKind::PublicationMatched", r"EventKind::SubscriptionMatched"],
     [r"case\s+NROS_RMW_EVENT_PUBLICATION_MATCHED\s*:\s*\{",
      r"case\s+NROS_RMW_EVENT_SUBSCRIPTION_MATCHED\s*:\s*\{"]),
    ("Manual liveliness assert", any_of(["publisher_assert_liveliness"]),
     [r"fn assert_liveliness"]),
    ("Event-driven wake (`set_wake_callback`)", any_of(["set_wake_callback"]),
     [r"fn set_wake_callback"]),
    ("Deadline hint (`next_deadline_ms`)", any_of(["next_deadline_ms"]),
     [r"fn next_deadline_ms"]),
    ("Zero-copy loan API", any_of(["borrow_loaned_message"]),
     [r"fn (pub_loan|borrow_loaned)"]),
    ("Batch receive (`take_sequence`)", any_of(["take_sequence"]),
     [r"fn take_sequence"]),
    ("Streamed publish", any_of(["publish_streamed"]), [r"fn publish_streamed"]),
    ("Connectivity ping", any_of(["ping_session"]), [r"fn ping_session"]),
    # ---- phase-376 W4 parity surface (declared slots; wiring in flight) ----
    ("Identity / feature probe",
     any_of(["get_implementation_identifier", "feature_supported"]),
     [r"fn implementation_identifier", r"fn feature_supported"]),
    # Two rows, not one: zenoh answers the GID (`Publisher::get_gid`) and no
    # matched count, so a merged row read "—" for a GID that is served.
    ("Publisher GID", any_of(["get_gid_for_publisher"]),
     [r"fn (get_gid|gid_for_publisher)\b"]),
    ("Matched-endpoint counts",
     any_of(["publisher_count_matched_subscriptions",
             "subscription_count_matched_publishers"]),
     [r"fn count_matched_subscriptions", r"fn count_matched_publishers"]),
    ("Actual-QoS read-back", any_of(["publisher_get_actual_qos"]),
     [r"fn (get_actual_qos|actual_qos)"]),
    ("Wait-for-acked", any_of(["publisher_wait_for_all_acked"]),
     [r"fn wait_for_all_acked"]),
    ("Take-with-info", any_of(["take_with_info"]), [r"fn take_with_info"]),
    # Issue 1940 put this row back. phase-407 had retired the slots (issue 0960
    # DECLINED the trio) and the row went with them, because a row names a slot
    # the canonical vtable has. A Rust backend answers through the adapter, so
    # its evidence is the trait method the adapter forwards to. All three
    # slots: a backend that wakes subscriptions alone is not "wired" here.
    ("Entity new-data callbacks",
     any_of(["subscription_set_on_new_message_callback",
             "service_set_on_new_request_callback",
             "client_set_on_new_response_callback"]),
     [r"fn register_waker"]),
    ("Graph introspection (names/types/counts)",
     any_of(["get_node_names", "get_topic_names_and_types", "count_publishers"]),
     [r"fn get_node_names", r"fn get_topic_names"]),
    # Issue 1978 — the Jazzy contract's four ADOPT slots, one row per question.
    ("Service-graph counts (`count_clients` / `count_services`)",
     any_of(["count_clients", "count_services"]),
     [r"fn count_clients", r"fn count_services"]),
    # The zenoh evidence is the same `fn get_gid` spelling the publisher row
    # reads — the client's override lives beside it in `service.rs`.
    ("Client GID", any_of(["get_gid_for_client"]), [r"fn get_gid\b"]),
    ("Event-kind probe (`event_type_is_supported`)",
     any_of(["event_type_is_supported"]),
     [r"fn event_type_is_supported"]),
]

# Node-layer features: built in `nros-node` on pub/sub + services, so they
# exist wherever a backend has both. "tested" comes from the test cells.
# (row label, `Workload::` variant, rule text)
NODE_LAYER = [
    ("Actions", "Action",
     "Built in `nros-node` on pub/sub + services (goal / cancel / result "
     "services + feedback / status topics)."),
    ("Parameters (+ param services)", "Params",
     "Node-layer services (RFC-0004); available wherever services are."),
    ("Lifecycle (REP-2002)", "Lifecycle",
     "Node-layer state machine + services (`lifecycle-services` feature)."),
]
NODE_LAYER_NEEDS = ["Publish / subscribe", "Services (server side)", "Service clients"]


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
        return fh.read()


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", " ", text)


def canonical_slots():
    """Every vtable slot, parsed from cffi's EMPTY_VTABLE literal (which is
    required to name every field). The authority for rename detection."""
    text = read(CFFI_EMPTY)
    m = re.search(r"pub const EMPTY_VTABLE[^{]*\{(.*?)\};", text, re.S)
    if not m:
        sys.exit("gen-rmw-feature-matrix: EMPTY_VTABLE not found in cffi lib.rs")
    return set(re.findall(r"([a-z_][a-z0-9_]*)\s*:\s*None", m.group(1)))


def parse_designated(text):
    """xrce shape: `.slot = value,` -> {slot: value-or-None}.

    A slot ABSENT from a designated initializer is implicitly NULL —
    phase-376's new parity slots are exactly that, so absence means
    unwired, never an error (the canonical-slot check catches renames).
    """
    out = {}
    for m in re.finditer(r"^\s*\.([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([A-Za-z_][A-Za-z0-9_]*)",
                         text, re.M):
        out[m.group(1)] = None if m.group(2) == "NULL" else m.group(2)
    return out


def parse_positional(text):
    """cyclone / uORB shape: `/*slot*/ value,` (comment names the slot).

    A value can be an identifier that is itself a typed-nullptr constant
    (`constexpr … (*kFoo)(…) = nullptr;` — Cyclone's `*_event_init` hooks
    are spelled exactly that way), so resolve those to NULL too: treating
    any non-`nullptr` identifier as wired reported Cyclone's callback
    event hooks as implemented.
    """
    null_consts = set(re.findall(
        r"constexpr[^;=]*\(\s*\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)[^;=]*=\s*nullptr\s*;",
        text, re.S))
    out = {}
    for m in re.finditer(r"/\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\*/\s*([A-Za-z_:][A-Za-z0-9_:]*)",
                         text):
        val = m.group(2)
        out[m.group(1)] = None if (val == "nullptr" or val in null_consts) else val
    return out


def function_bodies(src_dir):
    """{function name: body text, comments stripped} over a C/C++ source dir."""
    bodies = {}
    for path in tracked(os.path.join(ROOT, src_dir)):
        if path.suffix not in (".c", ".cpp", ".h", ".hpp"):
            continue
        text = strip_comments(path.read_text(encoding="utf8"))
        for m in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(([^;{}()]|\([^()]*\))*\)\s*\{",
                             text):
            name = m.group(1)
            if name in ("if", "for", "while", "switch", "return", "sizeof"):
                continue
            depth, i = 1, m.end()
            while depth and i < len(text):
                depth += {"{": 1, "}": -1}.get(text[i], 0)
                i += 1
            bodies.setdefault(name, text[m.end():i - 1])
    return bodies


_STUB_STMT = re.compile(r"\(void\)\s*[A-Za-z_][A-Za-z0-9_]*|return\s+(NROS_)?RMW_RET_UNSUPPORTED")


def is_unsupported_stub(body):
    """A body made only of `(void)x;` and `return …UNSUPPORTED;`."""
    stmts = [s.strip() for s in body.split(";") if s.strip()]
    return bool(stmts) and all(_STUB_STMT.fullmatch(s) for s in stmts) and any(
        "UNSUPPORTED" in s for s in stmts)


def c_backend_fills(vtable, shape, src_dir):
    """{slot: wired?} for one C backend, with refusing stubs counted as NULL."""
    text = read(vtable)
    raw = parse_designated(text) if shape == "designated" else parse_positional(text)
    bodies = function_bodies(src_dir)
    out = {}
    for slot, fn in raw.items():
        if fn is None:
            out[slot] = False
            continue
        body = bodies.get(fn.split("::")[-1])
        out[slot] = not (body is not None and is_unsupported_stub(body))
    return out, raw, bodies


_RUST_REFUSAL_TAIL = re.compile(r"Err\(\s*(TransportError|Self::Error)::Unsupported\s*\)\s*;?\s*$")


def zenoh_has(patterns):
    """Every pattern names an override whose body does not END in a refusal.

    The Rust twin of `is_unsupported_stub`: zenoh's `assert_liveliness`
    exists, returns `Ok(())` for an AUTOMATIC publisher (nothing to assert)
    and `Err(TransportError::Unsupported)` for a MANUAL one — so presence of
    the `fn` read as "wired" for the one case the row is about.
    """
    # Tracked crate sources — index lookup, not a walk (issue 0721).
    texts = [strip_comments(p.read_text(encoding="utf8"))
             for p in tracked(os.path.join(ROOT, ZENOH_SRC_DIR), suffix=".rs")]

    def served(pat):
        for t in texts:
            for m in re.finditer(pat, t):
                body = body_after(t[m.start():], r"[^{;]*\{")
                if body is None or not _RUST_REFUSAL_TAIL.search(body[:-1]):
                    return True
        return False

    return all(served(pat) for pat in patterns)


def zenoh_slot_fills():
    """Slots the zenoh crate fills in a Rust `NrosRmwVtable` literal itself
    (`slot: Some(fn)`), on top of the adapter base — its loan slots, behind
    the `lending` feature. A real slot fill, read like a C vtable's."""
    fills = set()
    for p in tracked(os.path.join(ROOT, ZENOH_SRC_DIR), suffix=".rs"):
        fills |= set(re.findall(r"\b([a-z_][a-z0-9_]*):\s*Some\(",
                                strip_comments(p.read_text(encoding="utf8"))))
    return fills


def qos_policy_bits():
    """The policy bits in definition order, from `QoSPolicyMask`'s consts."""
    bits = re.findall(r"pub const ([A-Z0-9_]+): Self = Self\(1 << \d+\);", read(QOS_MASK_DEF))
    if not bits:
        sys.exit("gen-rmw-feature-matrix: no QoSPolicyMask bits parsed")
    return bits


def expand_core(names):
    s = set(names)
    if "CORE" in s:
        s |= {"RELIABILITY", "DURABILITY_VOLATILE", "HISTORY", "DEPTH"}
    return s - {"CORE", "NONE"}


def body_after(text, header_rx):
    m = re.search(header_rx, text)
    if not m:
        return None
    depth, i = 1, m.end()
    while depth and i < len(text):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        i += 1
    return text[m.end():i]


def zenoh_qos():
    body = body_after(read(ZENOH_SESSION), r"fn supported_qos_policies[^{]*\{")
    if body is None:
        sys.exit("gen-rmw-feature-matrix: zenoh supported_qos_policies not found")
    return expand_core(re.findall(r"QoSPolicyMask::([A-Z0-9_]+)", strip_comments(body)))


def c_qos(raw, bodies, label):
    fn = raw.get("supported_qos_policies")
    if fn is None:
        return set()  # NULL slot: the backend claims no policy
    body = bodies.get(fn.split("::")[-1])
    if body is None:
        sys.exit(f"gen-rmw-feature-matrix: {label}'s supported_qos_policies "
                 f"body `{fn}` not found in its sources")
    return expand_core(re.findall(r"NROS_RMW_QOS_POLICY_([A-Z0-9_]+)", body))


def tested_cells():
    """{(Rmw variant, Workload variant)} with at least one Runtime cell."""
    out = set()
    for rel in (MATRIX_CELLS, INTEROP_CELLS):
        text = re.sub(r"//[^\n]*", "", read(rel))
        for m in re.finditer(r"\b(?:cell|c)\(\s*(\w+),\s*(\w+),\s*(\w+),\s*(\w+),"
                             r"\s*(\w+),\s*(\w+)", text):
            if m.group(6) == "Runtime":
                out.add((m.group(3), m.group(4)))
    if not out:
        sys.exit("gen-rmw-feature-matrix: no Runtime cells parsed from the test cells")
    return out


def cell(v):
    return {True: "wired", False: "—"}[v]


def render():
    canon = canonical_slots()
    missing = sorted({s for _l, shapes, *_rest in FEATURES for sh in shapes for s in sh
                      if s not in canon})
    if missing:
        # A renamed/removed slot must fail the generator, not silently
        # render a `—` for a feature that merely moved. Absence from a
        # BACKEND's initializer is fine (implicitly NULL); absence from the
        # canonical EMPTY_VTABLE list is a rename.
        sys.exit(f"gen-rmw-feature-matrix: slot(s) not in the canonical "
                 f"vtable (EMPTY_VTABLE): {missing}")

    backends = []
    c_src_text = {}
    for label, vtable, shape, src_dir, rmw in C_BACKENDS:
        fills, raw, bodies = c_backend_fills(vtable, shape, src_dir)
        backends.append((label, fills, c_qos(raw, bodies, label), rmw))
        c_src_text[label] = "\n".join(
            strip_comments(p.read_text(encoding="utf8"))
            for p in tracked(os.path.join(ROOT, src_dir))
            if p.suffix in (".c", ".cpp", ".h", ".hpp"))

    cols = ["Zenoh"] + [b[0] for b in backends]
    lines = [
        "<!-- GENERATED by scripts/gen-rmw-feature-matrix.py — do not edit by hand.",
        "     Regenerate: python3 scripts/gen-rmw-feature-matrix.py",
        "     Gated by:   just check rmw-feature-matrix -->",
        "",
        "# Per-RMW Feature Matrix",
        "",
        "One table per question the capability pages used to answer in",
        "contradictory prose. **Derived from the backend sources** — the C",
        "vtables (" + ", ".join(f"`{os.path.basename(b[1])}` ({b[0]})" for b in C_BACKENDS)
        + ") and the zenoh shim's trait",
        "overrides — so a backend gaining or losing a slot moves this page in",
        "the same commit or fails the gate.",
        "",
        "`wired` = the backend implements it. `—` = not wired: the runtime",
        "surfaces `UNSUPPORTED` or falls back where a fallback exists (the",
        "vtable comments name which). A slot filled with a function that only",
        "returns `UNSUPPORTED` (uORB's service stubs) is `—`, not `wired`.",
        "Status events count as wired whether the backend calls back",
        "(`*_event_init`) or the runtime polls it (`*_take_event`). The rows",
        "below the parity marker are the phase-376 W4 surface — slots declared",
        "in the ABI whose backend wiring is the in-flight campaign; all-dash",
        "rows there mean *declared, not yet wired anywhere*, and they flip",
        "automatically as backends land implementations.",
        "",
        "## Session / entity capabilities",
        "",
        "| Capability | " + " | ".join(cols) + " |",
        "|---|" + "---|" * len(cols),
    ]
    wired_rows = {}
    zen_fills = zenoh_slot_fills()
    for label, shapes, zpat, *c_extra in FEATURES:
        c_pats = c_extra[0] if c_extra else []
        row = {"Zenoh": zenoh_has(zpat)
               or any(all(s in zen_fills for s in sh) for sh in shapes)}
        for blabel, fills, _q, _r in backends:
            row[blabel] = any(all(fills.get(s, False) for s in sh) for sh in shapes) and all(
                re.search(p, c_src_text[blabel]) for p in c_pats)
        wired_rows[label] = row
        lines.append(f"| {label} | " + " | ".join(cell(row[c]) for c in cols) + " |")

    tested = tested_cells()
    rmw_of = {"Zenoh": ZENOH_RMW, **{b[0]: b[3] for b in backends}}
    lines += [
        "",
        "## Node-layer features",
        "",
        "These live in `nros-node` on top of pub/sub + services — they are",
        "not backend slots, so a backend has them exactly when it has those.",
        "`tested` = at least one `Runtime` cell for that backend and workload",
        "in `matrix::CELLS` or `interop::CELLS` (`packages/testing/nros-tests/src/`);",
        "`untested` = available, but no runtime cell runs it; `—` = the",
        "backend lacks pub/sub or services, so the feature cannot exist there.",
        "",
        "| Feature | " + " | ".join(cols) + " | Rule |",
        "|---|" + "---|" * len(cols) + "---|",
    ]
    for label, workload, why in NODE_LAYER:
        vals = []
        for c in cols:
            if not all(wired_rows[n][c] for n in NODE_LAYER_NEEDS):
                vals.append("—")
            elif (rmw_of[c], workload) in tested:
                vals.append("tested")
            else:
                vals.append("untested")
        lines.append(f"| {label} | " + " | ".join(vals) + f" | {why} |")

    qos = {"Zenoh": zenoh_qos(), **{b[0]: b[2] for b in backends}}
    lines += [
        "",
        "## QoS policies",
        "",
        "A backend advertises the policies it can enforce via",
        "`supported_qos_policies()`; requesting an unadvertised policy fails",
        "entity creation loudly (`INCOMPATIBLE_QOS`) — **no silent",
        "downgrade**. Each column is that backend's OWN answer: the zenoh",
        "shim's `Session::supported_qos_policies`, and for a C backend the",
        "function its vtable puts in the `supported_qos_policies` slot.",
        "Per-policy semantics: [RMW vs upstream §7](../design/rmw-vs-upstream.md).",
        "",
        "| Policy | " + " | ".join(cols) + " |",
        "|---|" + "---|" * len(cols),
    ]
    for pol in qos_policy_bits():
        lines.append(f"| `{pol}` | " + " | ".join("✓" if pol in qos[c] else "—" for c in cols)
                     + " |")
    lines += [
        "",
        "## Related",
        "",
        "- [Choosing an RMW Backend](../user-guide/rmw-choosing.md) — the",
        "  decision tree",
        "- [Backend Reference](../user-guide/rmw-backends.md) — architecture,",
        "  footprint, transports per backend",
        "- [Support Status](support-status.md) — versions, pins, and CI tiers",
        "",
    ]
    return "\n".join(lines)


def main():
    content = render()
    if "--check" in sys.argv:
        on_disk = open(OUT, encoding="utf8").read() if os.path.exists(OUT) else ""
        if on_disk != content:
            sys.exit("check-rmw-feature-matrix: STALE — regenerate with "
                     "python3 scripts/gen-rmw-feature-matrix.py and commit.")
        print("rmw-feature-matrix OK")
        return
    with open(OUT, "w", encoding="utf8") as fh:
        fh.write(content)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
