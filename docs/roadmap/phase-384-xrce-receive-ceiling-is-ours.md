# Phase 384 — XRCE's 1024-byte receive ceiling is OURS, and it was reported under the wrong name

**Status (2026-08-26). The phase's premise was WRONG TWICE and is now measured to
the byte. The Agent is not the ceiling; our own `XRCE_BUFFER_SIZE` is, it is
already raisable, and the failure was never silent — it was reported as
`DeserializationError` by a code path that does no deserialization. That
mislabelling is fixed here. What remains is documentation and a regression test,
plus a genuinely silent failure one ceiling up, filed as issue 0819.**

## What actually happens

`packages/rmw/xrce/nros-rmw-xrce/src/internal.h:69`:

```c
#ifndef XRCE_BUFFER_SIZE
#define XRCE_BUFFER_SIZE 1024
```

and `subscriber.c:68`, in the topic callback:

```c
if (len + XRCE_CDR_HEADER_LEN > XRCE_BUFFER_SIZE) {
    entry->overflow = true;
    entry->len = 0;
}
```

An oversized sample is accepted off the wire, marked `overflow`, and the next
take returns `NROS_RMW_RET_MESSAGE_TOO_LARGE`. Nothing is dropped quietly and
nothing about the Agent is involved.

The ring entry is a fixed `uint8_t data[XRCE_BUFFER_SIZE]`, so the caller's own
`RX_BUF` does not reach it: the stress binary asks for
`create_subscription_sized::<Int32, 16384>` and still stops at 1024, because the
16384 buffer is the destination of a copy that never happens.

## The measurement

`packages/testing/nros-bench/stress-xrce`, nano -> Agent -> nano, varying only
`PAYLOAD_SIZE` (which INCLUDES the 4-byte CDR header the publish side strips):

| payload bytes | `XRCE_BUFFER_SIZE` = 1024 | = 8192 |
| --- | --- | --- |
| 1024 | delivered, 10/10 valid | delivered |
| 1025 | `MessageTooLarge` ×10 | delivered |
| 1100 | `MessageTooLarge` ×10 | delivered |
| 2048 | `MessageTooLarge` ×10 | **10/10 valid** |
| 3584 | — | 10/10 valid |
| 4096 | — | **received, 0/10 valid** — issue 0819 |

The predicate `body + 4 > 1024` with `body = PAYLOAD_SIZE - 4` is exactly
`PAYLOAD_SIZE > 1024`, and the boundary is exactly there. Source and experiment
agree to the byte, and raising `NROS_XRCE_BUFFER_SIZE` moves the boundary,
which the Agent hypothesis cannot explain.

## Why it looked silent: an error label that erased its own cause

The listener reported `Receive error: Transport(DeserializationError)` for every
oversized sample. That is what made this look like a mysterious drop rather than
a configuration ceiling — the backend's `MessageTooLarge` was being thrown away
one layer up, in `nros-node`:

```rust
// packages/core/nros-node/src/executor/handles.rs — before
pub fn try_recv_raw(&mut self) -> Result<Option<usize>, NodeError> {
    self.handle
        .try_recv_raw(&mut self.buffer)
        .map_err(|_| NodeError::Transport(TransportError::DeserializationError))
}
```

`try_recv_raw` does not deserialize anything. `map_err(|_| …)` discarded a
perfectly good `TransportError` and substituted a name for a step this function
never performs.

**Fixed as a class, not at the reported site.** Of 36 `map_err(|_|
…DeserializationError)` in `nros-node`, 28 wrap a real `deserialize()` /
`CdrReader` call and are correctly named. Six wrap a TRANSPORT call and were
misnaming its error; all six now propagate it with `map_err(NodeError::Transport)`:

* `executor/handles.rs` — `Subscription::try_recv` (the pre-deserialize take),
  `Subscription::try_recv_raw`, `RawSubscription::try_recv_raw`,
  `try_recv_raw_with_attachment`, `try_recv_validated`
* `executor/action_core.rs` — `try_recv_feedback_raw`

Sweep that finds them:

```
grep -rn "map_err(|_| .*TransportError::DeserializationError" packages/core/nros-node/src/
```

then keep the ones whose wrapped expression is a `deserialize` / `CdrReader`
call and fix the ones whose expression is a transport call.

## What this refutes

Two earlier drafts of this phase, both recorded rather than deleted because each
was written from a source read without a measurement:

* **"The Agent hardcodes `m_typeSize = 1024 + 4` and silently drops anything
  larger."** The Agent does have that constant, and its `serialize` predicate
  does work out to `body <= 1024`. It is simply not what stops these samples:
  with the ring raised, 2048-byte payloads cross the same Agent intact. The
  constant was a real thing in the right size range that had nothing to do with
  the symptom — and both the boundary sweep and the mechanism trace "confirmed"
  it, because `1024` fits both stories.
* **"Track B — fork the Agent so `TopicPubSubType` takes its size from the
  topic."** Unnecessary. No fork, no patch line, no upstream PR.

The tell that was available the whole time and not used: the samples were
ARRIVING. Ten receive errors per run means ten samples reached our subscriber.
A drop inside the Agent produces zero of anything.

## Work items

**W1 — DONE.** The six mislabelled transport errors now propagate. This is the
fix that turns the symptom from "mysterious loss" into
`Transport(MessageTooLarge)`, which names its own remedy.

**W2 — the regression test.** Assert BOTH halves at the boundary: that a payload
over `XRCE_BUFFER_SIZE` fails with `MessageTooLarge` (not
`DeserializationError`, which is what would regress), and that the same payload
is delivered intact when the fixture raises `NROS_XRCE_BUFFER_SIZE`. The
large-buffer variant should be a `fixtures.toml` row with
`env = { NROS_XRCE_BUFFER_SIZE = "8192" }`, exactly as the stress-zenoh
large-buffer row already does for `ZPICO_SUBSCRIBER_BUFFER_SIZE` — the axis and
its precedent both exist.

**W3 — document the knob.** `NROS_XRCE_BUFFER_SIZE` is real, is validated in
`nros-rmw-xrce-cffi/build.rs`, and is findable today only by reading that build
script. It belongs in the book beside `ZPICO_SUBSCRIBER_BUFFER_SIZE`, with the
static cost stated (`XRCE_SUBSCRIBER_RING_DEPTH × XRCE_BUFFER_SIZE` per
subscriber, which is why the default is small) and with the 0819 ceiling named
as the point past which raising it stops helping.

**W4 — DONE, and it is what corrected the phase.** The premise sweep, above.

**W5 — WITHDRAWN.** The fork decision has no subject.

## Risks

* **Do not let this become 0741's fix by association.** It was found while
  investigating 0741 and does not close it. 0741 is a 28-byte reply into a
  15-byte history — three orders of magnitude below any ceiling here.
* **Raising the default is not the fix.** `XRCE_BUFFER_SIZE` multiplies by ring
  depth and subscriber count into static RAM on the smallest targets. The
  default staying small is correct; being undocumented is not.
* **W2 must fail on the pre-W1 tree** for the right reason — assert the error is
  `MessageTooLarge`, because `DeserializationError` is precisely the regression.
