#!/usr/bin/env python3
"""Generate book/src/reference/rmw-feature-matrix.md from backend sources.

The per-RMW capability story used to live as prose scattered over four
pages, and it drifted in BOTH directions (Cyclone services were
documented as unsupported a year after service.cpp landed; zenoh manual
liveliness was documented as a no-op while the shim wired it). This
generator derives the wired/NULL facts from the artifacts that cannot
lie:

  * the two C vtables (positional `/*slot*/ value` in cyclonedds's
    vtable.cpp, designated `.slot = value` in xrce's vtable.c),
  * the zenoh shim's Rust trait overrides (presence of the `fn` in the
    backend source — the trait default is the unsupported fallback),
  * the QoS masks (`supported_qos_policies` in the zenoh shim; the
    CFFI layer's blanket mask for the C-ABI backends, which is a
    runtime-side ASSUMPTION — TODO 115.K.2.x in rmw/cffi/src/lib.rs).

Node-layer rows (actions, params, lifecycle) are static data with
citations: they are built on pub/sub + services in nros-node, not on a
per-backend slot.

Run:  python3 scripts/gen-rmw-feature-matrix.py [--check]
Gate: just check-rmw-feature-matrix
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "book", "src", "reference", "rmw-feature-matrix.md")

CYCLONE_VTABLE = "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp"
XRCE_VTABLE = "packages/rmw/xrce/nros-rmw-xrce/src/vtable.c"
ZENOH_SRC_DIR = "packages/rmw/zenoh/nros-rmw-zenoh/src"
ZENOH_SESSION = "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs"
CFFI_LIB = "packages/rmw/cffi/src/lib.rs"

# slot-name -> (row label, which C-ABI slot(s) must be non-NULL,
#               regex for the zenoh Rust override)
FEATURES = [
    ("Publish / subscribe", ["create_publisher", "create_subscription"],
     r"fn (create_publisher|publish_raw)"),
    ("Services (server side)", ["create_service", "try_recv_request", "send_reply"],
     r"fn (create_service|send_reply)"),
    ("Service clients", ["create_client", "send_request_raw", "try_recv_reply_raw"],
     r"fn (create_client|send_request)"),
    ("Server-availability probe", ["service_server_available"],
     r"fn server_available"),
    ("Status events (deadline / liveliness / lost)",
     ["register_subscription_event", "register_publisher_event"],
     r"fn register_event_callback"),
    ("Manual liveliness assert", ["assert_publisher_liveliness"],
     r"fn assert_liveliness"),
    ("Event-driven wake (`set_wake_callback`)", ["set_wake_callback"],
     r"fn set_wake_callback"),
    ("Deadline hint (`next_deadline_ms`)", ["next_deadline_ms"],
     r"fn next_deadline_ms"),
    ("Zero-copy loan API", ["pub_loan"], r"fn (pub_loan|loan)"),
    ("Batch receive (`try_recv_sequence`)", ["try_recv_sequence"],
     r"fn try_recv_sequence"),
    ("Streamed publish", ["publish_streamed"], r"fn publish_streamed"),
    ("Connectivity ping", ["ping_session"], r"fn ping_session"),
]

# Node-layer features: not a backend slot; static rows with the rule.
NODE_LAYER = [
    ("Actions", "yes", "yes", "untested",
     "Built in `nros-node` on pub/sub + services. Cyclone has both since "
     "`service.cpp`, but no action example runs on it in CI — see "
     "[known limitations](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/cyclonedds-known-limitations.md)."),
    ("Parameters (+ param services)", "yes", "yes", "yes",
     "Node-layer services (RFC-0004); available wherever services are."),
    ("Lifecycle (REP-2002)", "yes", "yes", "yes",
     "Node-layer state machine + services (`lifecycle-services` feature)."),
]

QOS_ORDER = [
    "CORE", "DURABILITY_TRANSIENT_LOCAL", "DEADLINE", "LIFESPAN",
    "LIVELINESS_AUTOMATIC", "LIVELINESS_MANUAL_BY_TOPIC",
    "LIVELINESS_MANUAL_BY_NODE", "LIVELINESS_LEASE",
    "AVOID_ROS_NAMESPACE_CONVENTIONS",
]


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
        return fh.read()


def parse_designated(text):
    """xrce shape: `.slot = value,` -> {slot: bool(wired)}"""
    out = {}
    for m in re.finditer(r"^\s*\.([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([A-Za-z_][A-Za-z0-9_]*)",
                         text, re.M):
        out[m.group(1)] = m.group(2) != "NULL"
    return out


def parse_positional(text):
    """cyclone shape: `/*slot*/ value,` (comment names the slot).

    A value can be an identifier that is itself a typed-nullptr constant
    (`constexpr … (*kFoo)(…) = nullptr;` — the Phase-108 deferred event
    hooks are spelled exactly that way), so resolve those to NULL too:
    treating any non-`nullptr` identifier as wired reported Cyclone's
    status events as implemented, which is the drift this generator
    exists to prevent.
    """
    null_consts = set(re.findall(
        r"constexpr[^;=]*\(\s*\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)[^;=]*=\s*nullptr\s*;",
        text, re.S))
    out = {}
    for m in re.finditer(r"/\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\*/\s*([A-Za-z_:][A-Za-z0-9_:]*)",
                         text):
        val = m.group(2)
        out[m.group(1)] = val != "nullptr" and val not in null_consts
    return out


def zenoh_has(pattern):
    rx = re.compile(pattern)
    for dirpath, _dirs, files in os.walk(os.path.join(ROOT, ZENOH_SRC_DIR)):
        for f in files:
            if f.endswith(".rs"):
                if rx.search(open(os.path.join(dirpath, f), encoding="utf8").read()):
                    return True
    return False


def qos_mask(text, fn_name):
    """Collect QosPolicyMask::X terms inside fn `fn_name`'s body."""
    m = re.search(rf"fn {fn_name}[^{{]*\{{", text)
    if not m:
        return None
    depth, i, start = 1, m.end(), m.end()
    while depth and i < len(text):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        i += 1
    body = text[start:i]
    return sorted(set(re.findall(r"QosPolicyMask::([A-Z0-9_]+)", body)))


def cell(v):
    return {True: "wired", False: "—"}[v]


def render():
    cyc = parse_positional(read(CYCLONE_VTABLE))
    xrce = parse_designated(read(XRCE_VTABLE))
    zen_qos = qos_mask(read(ZENOH_SESSION), "supported_qos_policies")
    cffi_qos = qos_mask(read(CFFI_LIB), "supported_qos_policies")

    missing = [s for _l, slots, _z in FEATURES for s in slots
               if s not in cyc or s not in xrce]
    if missing:
        # A renamed/removed slot must fail the generator, not silently
        # render a `—` for a feature that merely moved.
        sys.exit(f"gen-rmw-feature-matrix: slot(s) not found in a vtable: {missing}")

    lines = [
        "<!-- GENERATED by scripts/gen-rmw-feature-matrix.py — do not edit by hand.",
        "     Regenerate: python3 scripts/gen-rmw-feature-matrix.py",
        "     Gated by:   just check-rmw-feature-matrix -->",
        "",
        "# Per-RMW Feature Matrix",
        "",
        "One table per question the capability pages used to answer in",
        "contradictory prose. **Derived from the backend sources** — the two",
        "C vtables (`" + os.path.basename(CYCLONE_VTABLE) + "`,",
        "`" + os.path.basename(XRCE_VTABLE) + "`) and the zenoh shim's trait",
        "overrides — so a backend gaining or losing a slot moves this page in",
        "the same commit or fails the gate.",
        "",
        "`wired` = the backend implements it. `—` = not wired: the runtime",
        "surfaces `UNSUPPORTED` or falls back where a fallback exists (the",
        "vtable comments name which).",
        "",
        "## Session / entity capabilities",
        "",
        "| Capability | Zenoh | XRCE-DDS | Cyclone DDS |",
        "|---|---|---|---|",
    ]
    for label, slots, zpat in FEATURES:
        zen = zenoh_has(zpat)
        xr = all(xrce[s] for s in slots)
        cy = all(cyc[s] for s in slots)
        lines.append(f"| {label} | {cell(zen)} | {cell(xr)} | {cell(cy)} |")

    lines += [
        "",
        "## Node-layer features",
        "",
        "These live in `nros-node` on top of pub/sub + services — they are",
        "not backend slots, so the rule, not a vtable, decides the row:",
        "",
        "| Feature | Zenoh | XRCE-DDS | Cyclone DDS | Rule |",
        "|---|---|---|---|---|",
    ]
    for label, z, x, c, why in NODE_LAYER:
        lines.append(f"| {label} | {z} | {x} | {c} | {why} |")

    lines += [
        "",
        "## QoS policies",
        "",
        "A backend advertises the policies it can enforce via",
        "`supported_qos_policies()`; requesting an unadvertised policy fails",
        "entity creation loudly (`INCOMPATIBLE_QOS`) — **no silent",
        "downgrade**. Per-policy semantics: [RMW vs upstream §7](../design/rmw-vs-upstream.md).",
        "",
        "| Policy | Zenoh | XRCE-DDS / Cyclone DDS (via C ABI) ¹ |",
        "|---|---|---|",
    ]
    for pol in QOS_ORDER:
        z = "✓" if pol in (zen_qos or []) else "—"
        c = "✓" if pol in (cffi_qos or []) else "—"
        lines.append(f"| `{pol}` | {z} | {c} |")
    lines += [
        "",
        "¹ The C-ABI backends' mask is asserted by the **runtime shim**",
        "(`packages/rmw/cffi/src/lib.rs`), not reported by the backend — the",
        "vtable has no `supported_qos_policies` slot yet (TODO 115.K.2.x).",
        "Treat the column as the runtime's assumption; the backend's own",
        "enforcement happens at entity creation.",
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
