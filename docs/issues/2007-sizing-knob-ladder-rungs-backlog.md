---
id: 2007
title: "15 sizing knobs still have no platform or board rung on the RFC-0049 ladder"
status: open
type: tech-debt
area: [config]
severity: low
found: 2026-10-23
related: [1033, 1722, 1808, 2006]
---

## Measured

Phase-400 W6 migrated build-script sizing knobs onto the RFC-0049 ladder, one
tenant at a time. `just check config-knob-census` on `main` (2026-10-23):
55 ladder knobs in 12 tenants, and **15** build-script names still classed
`sizing`, i.e. policy values with no derivation and no rung:

| knob | why it has no rung yet |
| --- | --- |
| `NROS_GRAPH_MAX_ENTITIES`, `NROS_ZENOH_GRAPH_CACHE_SIZE` | the peer graph's size; a property of the network, not of the image |
| `NROS_SERVICE_TIMEOUT_MS` | two readers (zenoh's `build.rs` and the C emitter in `nros-build-helpers`); needs one emission point before a rung |
| `NROS_XRCE_TRANSPORT_MTU`, `NROS_XRCE_SUBSCRIBER_RING_DEPTH`, `NROS_XRCE_BUFFER_SIZE` | transport and burst policy |
| `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` | see below: derivable only after a split |
| `NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE` | see below: no response bound exists |
| `NROS_SERVICE_INBOX_DEPTH`, `NROS_ACTION_INBOX_DEPTH`, `NROS_PARAM_SERVICE_INBOX_DEPTH` | ring depths that absorb bursts |
| `NROS_EXECUTOR_MAX_VIOLATIONS`, `NROS_EXECUTOR_RUNTIME_REMAPS` | executor policy |
| `NROS_ZENOH_READ_PRIORITY`, `NROS_ZENOH_LEASE_PRIORITY` | transport-band priorities; RFC-0071 D10 maps them onto the `rx`/`keepalive` roles, so the rung lands on the backend's row |

Each one should get a rung only when a platform or board needs a default
different from the built-in. A rung nobody uses adds a row to `explain` and
nothing else.

This is about a knob's REACH (which rungs it has). Whether a value can be
derived from what the image declares is issue 1808's question.

## The two XRCE service buffers are not plain backlog

Checked on 2026-10-23 because zenoh already derives its service inbox slot
(`NROS_SERVICE_INBOX_BYTES`) from the largest declared `_Request` bound.

- **Request buffer.** XRCE's `XRCE_SERVICE_REQUEST_BUFFER_SIZE` is one size
  shared by every service-server slot: user services, action servers, and the
  runtime's parameter and lifecycle services. Issue 1722 already RAISES it to
  the priced `set_parameters` request (`XRCE_PARAM_REQUEST_BYTES`). Deriving it
  DOWN to the declared servers' bound would under-size lifecycle's
  `ChangeState` request, whose `label` is an unbounded string. Zenoh could
  derive because phase-461 W2 moved the parameter and lifecycle families onto a
  ring of their own. XRCE needs the same split first.
- **Reply buffer.** A client receives the `_Response`, and the sizing
  descriptor has no response bound. `received_types_of` in
  `nros-cli-core/src/sizing_descriptor.rs` prices a `ServiceClient` row by its
  `_Request`, the same as a server row. So the reply slot cannot be derived
  until the descriptor states a response bound. Because zenoh's
  `SERVICE_FAMILY` includes client rows, a client's request bound also enters
  zenoh's server inbox maximum. That over-sizes the inbox; it never
  under-sizes it.

**Home:** none. Phase-400 was archived on 2026-10-23, and this list was the
remainder of its W6.
