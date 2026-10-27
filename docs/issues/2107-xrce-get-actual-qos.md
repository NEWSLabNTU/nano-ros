---
id: 2107
title: "XRCE leaves all six *_get_actual_qos slots NULL, though it knows the profile it created each entity with"
status: open
type: enhancement
area: [rmw, xrce, qos]
severity: low
found: 2026-10-27
related: [1329, 0823]
---

## What is missing

`publisher_get_actual_qos`, `subscription_get_actual_qos` and the four
service/client variants are NULL in `packages/rmw/xrce/nros-rmw-xrce/src/vtable.c`,
so the runtime answers UNSUPPORTED ("no read-back at all").

That overstates the limit. The client cannot read back what the Agent's DDS
entity was granted — there is no such request in the XRCE protocol — but every
policy this backend honours (`xrce_supported_qos_policies`: reliability,
durability VOLATILE/TRANSIENT_LOCAL, history, depth, avoid-ROS-namespace) is
sent explicitly in the BIN create (`uxr_buffer_create_*_bin`), and a policy it
does not honour is REFUSED at create (issue 1329), never silently dropped. So
for the honoured fields the created profile is the best answer the client has,
and for the rest the header's partial-answer rule applies: write `*_UNKNOWN`,
return OK.

Stock `rmw_microxrcedds` answers all six with the stored creation profile
(`*qos = custom_publisher->qos;` in `rmw_publisher.c`, likewise
`rmw_service.c` / `rmw_client.c`).

## To decide when doing it

- Whether an Agent-side override (an Agent run with a reference/XML profile)
  can change a BIN-created entity's QoS. If it can, the honoured fields are
  "requested", not "granted", and must be `UNKNOWN` too — measure against the
  pinned Agent before claiming them.
- One helper, six thunks (the header's own instruction).

## Acceptance

A `tests/` case in `nros-rmw-xrce` (no Agent needed: the per-entity state holds
the profile) for each of the six slots, plus `rmw-slot-producers`
counting six more xrce producers.
