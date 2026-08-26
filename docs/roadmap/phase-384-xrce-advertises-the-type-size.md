# Phase 384 — XRCE advertises the type's size, instead of inheriting the Agent's 1028

**Status (2026-08-26). W4 RUN FIRST — premise CONFIRMED by measurement. W1-W3,
W5 open.** Depends on phase-380, which built the number this phase transmits.

## Why

The Agent registers every topic we create with a hardcoded max serialized size:

```cpp
// third-party/xrce/agent/src/cpp/types/TopicPubSubType.cpp:23
TopicPubSubType::TopicPubSubType(bool with_key) {
    m_typeSize = 1024 + 4 /*encapsulation*/;
```

One occurrence, no derivation from the actual type. Our registration supplies no
size for it to use, so 1028 is what every DDS peer discovers for every nano-ros
XRCE topic.

For a message of 1024 bytes or less that is harmless over-provisioning, which is
why nothing has noticed. For anything larger it is simply WRONG, and the failure
mode is the one issue 0757 already documented on another transport: the sample is
received, ACKed, and discarded, with the subscription looking healthy from every
outside probe. 0757's case was 13.4 KiB Autoware trajectories — comfortably over
the cliff — and `packages/testing/nros-bench/large-msg-xrce` exists precisely
because large XRCE payloads are a known-hazardous area.

Until phase-380 there was nothing better to send: nothing in this tree computed a
message's serialized size. Now `M::MAX_SERIALIZED_SIZE_XCDR2` is a compile-time
constant for every bounded type, so the number the Agent is guessing is one we
can state.

## This is NOT the fix for issue 0741, and the evidence is already in that issue

0741 (`test_xrce_service_ros2_client`, a 28-byte reply refused into a 15-byte
history) is the investigation that produced this idea, and it is worth being
explicit that this phase does not close it:

* **The Agent's registration was already excluded there.** Its §"Agent-side
  investigation (2026-08-24) — the 15 is NOT the Agent's" reads the source above
  and concludes the Agent advertises 1028, not 15. Something downstream computes
  15 from its own type support; changing what WE advertise does not reach it.
* **A missing advertisement is identical on every host.** 0741 reproduces 1 run
  in 13 on one host and 0 in many on three other environments. The same objection
  retired the Fast-DDS-version and agent-pairing hypotheses there.

So this phase stands on the 1024-byte cliff above, not on 0741. If it happens to
change 0741's behaviour that is information, not the goal — and anyone measuring
it should first check `pgrep -c -f add_two_ints_server`, because 54 orphaned
participants were found on one host on 2026-08-26 and every prior measurement was
taken with them live.

## W4 ran first, and the cliff is exactly where the Agent's constant puts it

The acceptance test was ordered ahead of the design work precisely so a wrong
premise would stop the phase cheaply. It did not stop it.

Measured with `test_xrce_e2e_integrity` (nano -> Agent -> nano, which crosses the
Agent's DDS type registration), varying only `PAYLOAD_SIZE`, on a bus cleared of
orphans first:

| payload bytes | delivered |
| --- | --- |
| 512 | yes |
| 1000 | yes |
| **1024** | **yes** |
| **1028** | **no — `received=0` after 20 s** |
| 1029 | no |
| 1032 | no |
| 1100 | no |
| 1500 | no |
| 2048 | no |

**The boundary sits between 1024 and 1028, which is `m_typeSize = 1024 + 4`
byte for byte.**

Two alternative explanations are excluded by the same numbers:

* **Not the MTU.** The posix transport runs at 4096, and delivery stops at
  ~1024 — a quarter of it.
* **Not fragmentation or a client-side publish limit.**
  `test_xrce_large_publish_sizes` already publishes successfully up to 12288
  bytes. Publishing works; DELIVERY is what stops. The sample leaves the client
  and never reaches the peer, which is 0757's signature — accepted, then
  discarded, with nothing reporting an error.

So the premise holds: every nano-ros XRCE topic carrying a message over ~1024
bytes is silently undeliverable today, and the phase's remaining work items are
worth doing.

**Reproduce:** set both `PAYLOAD_SIZE` values in `test_xrce_e2e_integrity` to
1028 and run it. Check `pgrep -c -f add_two_ints_server` first — 14 orphans were
live on this host at the start of this measurement and were killed before the
numbers above were taken.

**Note for W4 proper.** The measurement above modifies an existing test by hand;
it is evidence, not a landed gate. W4's deliverable is still a committed test
that fails without this phase's fix and passes with it, and it should assert
delivery at a size ABOVE 1028 rather than sweeping — the sweep was for finding
the boundary, and now the boundary is known.

## Design

The client offers three entity-creation profiles; the choice IS the design.

| profile | carries | cost | verdict |
| --- | --- | --- | --- |
| **BIN** (today) | topic name, type name, `uxrQoS_t {durability, reliability, history, depth}` | smallest submessage, no string building | Cannot express a size — checked at all three levels, including the QoS struct. This is the constraint the phase exists to escape. |
| **XML** | an arbitrary DDS topic/endpoint XML | builds a string at runtime; the CREATE submessage must fit the output stream MTU | The only profile that can state a size from the client. Already compiled in — `create_entities_xml` is in `nros-rmw-xrce-cffi/build.rs`'s source list. |
| **REF** | a profile NAME the Agent looks up in its own config | tiny submessage | Moves the number into a hand-maintained Agent config file — a second place to keep the truth, drifting from the type. That is the guess phase-380 removed; rejected on those grounds, not on cost. |

So: **XML, for the entities whose advertised size a peer reads.**

### Which entities, and why not all of them

The size a remote reader sizes itself from comes from the WRITER side of the
match. So the ones that matter are the ones we publish through:

* `publisher.c` — `uxr_buffer_create_topic_bin` + datawriter.
* `service.c` — `uxr_buffer_create_replier_bin` (the reply topic, and the
  request topic for a client).

`subscriber.c` should be left on BIN in the first instance. Our own receive
buffer is a local matter that phase-380 W4 already asserts at build time, and
every entity moved to XML costs MTU and code size on targets that have neither
to spare. Moving it later is cheap; moving it now buys nothing measurable.

### Getting the number to the C layer

The bound is a Rust `const` on the message type; entity creation is C. The ABI
already has a field for the subscription direction and none for the others:

* `rmw_subscription_options_t::rx_buffer_hint` (u32, "0 = unset") exists and
  today carries the LOCAL buffer size, set from `RX_BUF` in `spin.rs`. The XRCE
  backend ignores it entirely.
* `rmw_publisher_options_t` has `tx_express` and `_reserved[7]`.
* Services have no options struct in the create path at all.

Two candidate shapes, and W1 below should pick one with a measurement rather
than an opinion:

1. **Reuse/extend the options structs.** `_reserved[7]` on the publisher side
   can hold a `u32 max_serialized_size` without changing the struct's size, and
   the subscription side already has a field of the right shape. Services still
   need a route — either a new options param on the service-create slot (an ABI
   change, and phase-376 has just finished stabilising that ABI) or a lookup.
2. **A type-size registry, mirroring `register_type_descriptor`.** The Cyclone
   backend already has a seam where Rust hands the C side per-type information
   at registration time (`rmw_type_registry`). A size is exactly that shape,
   costs no ABI change, and serves publishers, subscriptions and services
   uniformly. Cost is a table and a lookup at entity creation.

(2) looks right for the same reason the descriptor seam looks right, but it
should be justified against the RAM it costs on the smallest target before being
adopted.

## Work items

**W1 — decide the transport of the number, by measurement.** Prototype both
shapes above far enough to compare: bytes of RAM per registered type, code size
delta on the smallest XRCE target, and whether services can be served without an
ABI change. Write the answer down with the numbers; do not pick on taste.

**Acceptance:** a recorded comparison, and a decision that names what it costs.

**W2 — the XML the Agent accepts.** Build the topic/endpoint XML carrying
`maxSerializedSize` (or the Agent's expected spelling — read
`TopicPubSubType`/`Topic` handling in `third-party/xrce/agent` rather than
guessing the tag) and prove the Agent creates a topic with the size we asked for.

**Acceptance:** an Agent-side observation — a log or a `ros2 topic info -v` on a
peer — showing a max size we chose and the Agent did not.

**W3 — MTU and the smallest target.** A CREATE submessage carrying XML must fit
the output stream. Measure the XML length for a realistic type against
`NROS_XRCE_CUSTOM_TRANSPORT_MTU` and the serial transport's framing, and state
the ceiling. If a long type name plus XML cannot fit the smallest supported MTU,
that is a finding this phase must publish, not paper over — a truncated CREATE
is worse than a wrong size.

**Acceptance:** a stated maximum type-name length at each supported MTU, and a
build-time or runtime refusal when it would be exceeded.

**W4 — the cliff test.** Publish a BOUNDED message larger than 1024 bytes over
XRCE to a stock ROS 2 subscriber and assert delivery. This is the acceptance for
the whole phase and it must FAIL before W1–W3 land, or the premise is wrong and
the phase should stop.

`packages/testing/nros-bench/large-msg-xrce` already publishes increasing
payload sizes and has a `fixtures.toml` row; extend or promote it rather than
writing a third large-message harness.

**Acceptance:** red before, green after, with the size that crosses the cliff
named in the test.

**W5 — the unbounded case, stated not solved.** An unbounded type has no bound
to advertise, so it keeps whatever the Agent defaults to. Say so in the same
place the knob is documented, so the next person does not read this phase as
covering everything. phase-380's own "What this does NOT fix" is the precedent.

## Risks

* **The premise is measurable and might be wrong.** If W4 shows a >1024-byte
  bounded message already arrives intact over XRCE, then something downstream is
  resizing and this phase has no problem to solve. Run W4 FIRST.
* **The ABI has just been stabilised.** phase-376 completed a five-wave parity
  campaign; adding a create-path parameter now should be avoided if the registry
  shape can serve instead.
* **XML costs the targets that can least afford it.** Every byte of submessage
  and every byte of string-building buffer lands on the smallest MCU builds. W3
  is not a formality.
* **Do not let this become 0741's fix by association.** It was found while
  investigating 0741 and does not close it. If it lands and 0741 still
  reproduces, that is the expected outcome.
