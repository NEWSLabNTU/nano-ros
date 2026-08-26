# Phase 384 — XRCE silently drops every message over 1024 bytes

**Status (2026-08-26). W4 RUN FIRST — the DEFECT is confirmed and its mechanism
traced to the line. The phase's ORIGINAL DESIGN IS REFUTED: the Agent ignores any
size a client sends. Redesigned below; W1-W3 as first drafted are withdrawn.**

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

## The mechanism, traced (2026-08-26)

The measurement above is explained exactly by the Agent's own serializer:

```cpp
// third-party/xrce/agent/src/cpp/types/TopicPubSubType.cpp
TopicPubSubType::TopicPubSubType(bool with_key) {
    m_typeSize = 1024 + 4 /*encapsulation*/;      // the ONLY assignment, anywhere
}

bool TopicPubSubType::serialize(void *data, rtps::SerializedPayload_t *payload) {
    bool rv = false;
    ...
    if (buffer->size() <= (payload->max_size - 4)) { memcpy(...); rv = true; }
    return rv;                                     // else: false, silently
}
```

`payload->max_size` comes from `m_typeSize`, so the predicate is
`size <= 1024` — and the measurement is 1024 delivered, 1028 not. Source and
experiment agree to the byte.

When `serialize` returns false the DDS write is dropped. The XRCE client is
never told: `WRITE_DATA` is acknowledged by the XRCE reliability layer long
before the DDS publish is attempted, so from the application's side the publish
succeeded. That is issue 0757's signature and why nothing has ever reported it.

## Why this phase's original design is REFUTED

The first draft proposed moving entity creation from the BIN profile to XML so
the client could state the type's size. **The Agent would ignore it.**

* `m_typeSize` is assigned in exactly ONE place — the constructor above — and
  never from topic attributes, XML, or anything else. Verified by grepping the
  whole Agent tree for the symbol: one hit.
* Both construction sites are `TopicPubSubType{false}`
  (`FastDDSMiddleware.cpp:180`, `FastEntities.cpp:186`). The XML path sets the
  type's NAME from the attrs and nothing else.

So BIN, XML and REF are equivalent for this purpose. The client cannot influence
the ceiling by any profile, and the "which profile" question the first draft
called "the design" was the wrong question.

Recorded rather than deleted because the draft was written from the client-side
API alone, and reading the Agent's serializer — twenty minutes — would have
saved the whole design.

## The ceiling is NOT unique to XRCE — but the SILENCE is (2026-08-26)

An earlier revision of this phase asserted the 1024 limit was "the Agent's
alone". Measured, it is not, and the correction changes what Track A is for.

| run | result |
| --- | --- |
| zenoh, 2048-byte payload, default 1024 rx buffer | FAIL — `overflow_drops=20` |
| zenoh, 4096-byte payload, LARGE rx buffer fixture | **PASS** |
| XRCE, 2048-byte payload | FAIL — `received=0`, no drops counted |

Three differences, and each one matters:

* **Cause.** Zenoh stops at OUR `NROS_SUBSCRIPTION_BUFFER_SIZE` (1024 by
  default). XRCE stops at the AGENT's `m_typeSize`, traced above.
* **Configurability.** Zenoh's ceiling is raisable and raising it works — 4096
  delivers. XRCE's is not reachable from this repo at all, so the knob an
  integrator would try first, and which `report_dropped_take` explicitly tells
  them to raise, cannot help them. That is worth saying out loud in the book.
* **Observability.** Zenoh COUNTS the loss (`overflow_drops=20`) — that is
  issue 0757's fix working. XRCE reports nothing: the sample is acknowledged by
  the XRCE reliability layer and dropped inside the Agent, so no counter on
  either side of our code ever sees it.

So Track A is not "XRCE is uniquely limited". It is **"XRCE is the one that
loses data without saying so, and its limit is the one the user cannot raise"**.
That is a narrower claim and the one the evidence supports.

## Redesign

The ceiling is in the Agent, so a real fix is Agent-side, and the Agent is
**upstream eProsima**: `.gitmodules` records no `branch =`, and its HEAD is a
clean `Release v2.4.3` with no carried patches. Changing it means creating a
fork and a patch line under the rules in CLAUDE.md — a structural step this
phase should not take casually.

Two tracks, and the first does not depend on the second:

**Track A — make it LOUD, in this repo, now.** phase-380 computes a type's max
serialized size at compile time. The Agent's ceiling is a known constant. So an
image that publishes or subscribes a BOUNDED type larger than 1024 bytes over
XRCE can fail the BUILD, exactly as phase-380 W4 does for the receive buffer.
That converts silent data loss into a compile error without touching the Agent,
and it is honest about the limit rather than pretending the transport is
size-agnostic. It does not raise the ceiling.

**Track B — raise the ceiling, upstream or in a fork.** `TopicPubSubType` needs
to take its size from the topic it is registered for. That is a small patch —
a constructor parameter and a call site — but it lands in a vendored third-party
Agent, so it means: fork under NEWSLabNTU, patch line named in `.gitmodules`, and
ideally a PR to eProsima so the fork can retire. Only once B exists does the
client-side advertising work (the original W1-W3) become useful, because only
then is there something listening.

Sequencing matters: **A first.** It is small, local, testable here, and it stops
the silent loss. B is worth doing and is a different kind of commitment.

## Work items

**W1 — the build-time refusal (Track A).** An XRCE image that publishes or
subscribes a bounded type whose `MAX_SERIALIZED_SIZE` exceeds the Agent's
ceiling fails the build, naming the ceiling and the type. Reuse
`nros_serdes::size::bound_fits` and the pattern phase-380 W4 established; the
constant belongs beside the XRCE backend, not in the generic core, because it is
that transport's limit and not a property of messages.

**Acceptance:** a build that fails for a >1024-byte bounded type over XRCE,
naming the ceiling and the type.

NOT "and passes over another backend" — that was in an earlier draft of this
item and it is false; see the comparison below.

**W2 — the regression test.** Commit the measurement above as a test: publish
above the ceiling and assert delivery is refused loudly rather than silently
dropped. It must fail on today's tree. `nros-bench/large-msg-xrce` and
`test_xrce_e2e_integrity` already exist; extend rather than add a third harness,
and assert at a size just past the boundary rather than sweeping — the sweep
found the boundary and it is recorded above.

**W3 — say it where users choose a message (Track A).** The 1024-byte XRCE
ceiling belongs in the book beside the transport's other limits, with the
measurement that establishes it. An integrator choosing a message size for an
XRCE deployment currently has no way to learn this except by losing data.

**W4 — DONE.** The premise measurement, above.

**W5 — decide on the fork (Track B).** A written decision, not code: do we fork
the Agent to make `TopicPubSubType` size-aware, upstream it, or accept the
ceiling and live with W1's refusal? The cost is a fork and a patch line; the
benefit is large messages over XRCE at all. This is a maintainer call and should
be recorded as one.

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
