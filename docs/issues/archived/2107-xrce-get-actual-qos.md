---
id: 2107
title: "XRCE leaves all six *_get_actual_qos slots NULL, though it knows the profile it created each entity with"
status: resolved
type: enhancement
area: [rmw, xrce, qos]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [1329, 0823, 2120]
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

## Resolution (2026-10-27)

All six slots are filled: two helpers in `src/session.c`
(`xrce_actual_qos_topic`, `xrce_actual_qos_service`) and six thunks in
`src/vtable.c`. Each writes only what is known and leaves the rest as the
caller pre-loaded it (`NROS_RMW_QOS_PROFILE_UNKNOWN`).

### Measured — can an Agent-side profile override a BIN-created entity?

No. Against the pinned Agent (2.4.3, Fast-DDS 2.14.6), reading each
Agent-side endpoint's DISCOVERED QoS from a stock `rmw_fastrtps_cpp` peer
(rclpy `get_{publishers,subscriptions}_info_by_topic(..., no_mangle=True)` on
`rt/chatter`, `rq/add_two_intsRequest`, `rr/add_two_intsReply`), with the XRCE
talker, listener, service server and service client fixtures:

- **Agent with a loopback-only profile:** talker writer and listener reader
  were RELIABLE / VOLATILE, deadline and lifespan infinite, liveliness
  AUTOMATIC, lease infinite. This matches what the create sent plus Fast-DDS
  defaults.
- **Agent with an XML default profile** for `data_writer`, `data_reader`,
  `publisher` and `subscriber` (BEST_EFFORT, TRANSIENT_LOCAL, deadline 3 s,
  lifespan 4 s, MANUAL_BY_TOPIC, lease 5 s): the talker and listener endpoints
  were unchanged.
  - Positive control: the same file under a stock `ros2 topic pub` with
    `system_default` QoS DID come out BEST_EFFORT / TRANSIENT_LOCAL /
    deadline 3 s. So the file parsed and its defaults apply wherever Fast-DDS
    consults them.
  - Why the Agent's entities ignore it: the Agent copies
    `DATAWRITER_QOS_DEFAULT` into a local before applying the BIN flags
    (`FastDDSDataWriter::create_by_bin`), and Fast-DDS substitutes the XML
    default only for the sentinel's address.

So for pub/sub the honoured fields are **granted**, not just requested:

| field | reported value |
| --- | --- |
| reliability, durability, history | what was sent |
| depth | what was sent; UNKNOWN when 0 was sent, because the field is then absent from the CREATE and SEDP carries no depth |
| deadline, lifespan | INFINITE |
| liveliness | AUTOMATIC, lease INFINITE |
| `avoid_ros_namespace_conventions` | as created (it is client-side naming) |

### Measured — a requester/replier does not hold what was asked

The client library discards a requester's or replier's QoS
(`create_entities_bin.c`: `(void) qos;`), and the Agent builds both endpoints
from fixed attributes. For services created with the services-default profile
(RELIABLE / VOLATILE), and the same under the XML override:

| endpoint | reliability | durability |
| --- | --- | --- |
| request writer, reply writer | RELIABLE | TRANSIENT_LOCAL |
| request reader, reply reader | RELIABLE | VOLATILE |

All four had infinite deadline and lifespan, and AUTOMATIC liveliness with an
infinite lease.

- The four service/client slots report that profile.
- History and depth stay UNKNOWN: they were never sent and are not on the
  wire.
- Stock `rmw_microxrcedds` reports the stored creation profile here, which
  for a service is not what the Agent built.
- That a different request is admitted and silently not applied is filed as
  issue 2120.

### Tests

- `tests/actual_qos.c` (`nros_rmw_xrce_actual_qos`, `just check rmw-xrce`).
  Covers all six slots:
  - a publisher's sent BEST_EFFORT / TRANSIENT_LOCAL / KEEP_LAST(7) / avoid-ROS;
  - a subscription's SYSTEM_DEFAULT with depth 0, which reports depth UNKNOWN;
  - KEEP_ALL, which reports no depth;
  - the fixed service/client profile with UNKNOWN history and depth;
  - NULL arguments.
- Negative control: answering VOLATILE for service writers, and always
  writing the depth, failed three checks.

There is no committed live case. The slots read local state. The Agent-side
facts they rest on are the measurement above, run by hand with the fixtures
listed.
