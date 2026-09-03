---
id: 902
title: "action goals complete between 20 % and 90 % of the time on the same build,
  with no session expiry and no fault to explain the difference"
status: open
type: bug
area: rmw
related: [issue-0882, issue-0879, issue-0852]
---

## Measurement

Same image, same board, same router config, `order: 6`, direct serial link:

| run | goals completing |
| --- | ---: |
| after the 0882 allocator fix | 9/10 |
| after the 0879 INIT fix | 6/10 |
| immediately after, unchanged build | 8/10 |
| after a 100 s idle soak | 2/5 |

Nothing distinguishes these runs but time. Split across two fixes it looked like
one had regressed the other; a repeat run inside the same build gave 6/10 then
8/10, so the spread is the system, not the change.

## What it is NOT

Both of the obvious explanations are excluded by direct measurement, not by
argument:

- **Not a session expiry.** Zero `Closing session because it has expired`
  messages across a 160 s session that included five goals
  ([issue 0839](archived/0839-action-image-session-expires-every-20s.md) is
  resolved on exactly this evidence).
- **Not a crash.** Zero faults; the board is alive and answering afterwards.
- **Not discovery.** `ros2 node list` and `ros2 action list` resolve before and
  after, including after 160 s of idling.

So the session stays up, the board stays alive, and a goal still fails to
complete. The failures observed earlier had a consistent shape worth
re-checking: the goal is **accepted** and the result never arrives.

## Why this matters more than the raw number

A 20–90 % spread with no observable cause is worse than a hard failure. It is
not measurable as a regression gate, and any future change to this path will be
evaluated against noise wide enough to hide it — which has already happened once
in this campaign, when 6/10 was briefly read as a regression from 9/10.

## Where to start

The instrumentation for this already exists and is proven on this board:

- the socat tap (`experiments/serial-interop/serial-tap.py`) shows whether the
  `get_result` query and its reply reach the wire, and in which direction the
  exchange stops. It does not halt the core.
- RTT shows whether the application layer saw the query — but attaching it
  perturbs the link ([issue 0913](0913-the-debugger-is-not-a-passive-instrument.md)),
  so use it after the fact, not during.

Capture one *failing* goal on the tap and establish whether the reply is never
sent or never arrives. That is one experiment and it splits the problem in half.


## First experiment done — the reply is never SENT, and the failure is board-side

Two adjacent goals on one run, captured through the socat tap (which does not
halt the core), one failing and one succeeding, 3821 and 3826 bytes of wire
traffic respectively — near-identical volume.

| | succeeding | failing |
| --- | --- | --- |
| `send_goal` query in | `router->board len=92` | `router->board len=92` |
| board's accept reply | `board->router len=187` … `_action/send_` | same |
| next | `router->board len=82` | `router->board len=82` |
| next | `board->router len=78` | `board->router len=78` |
| **then** | **`board->router len=209`** | **`board->router len=51`, then keepalives only** |

The payloads:

```
OK   len=209: 10/fibonacci/_action/get_result/example_interfaces::action::dds_::
              Fibonacci_GetResult_/TypeHashNotSupported  <result payload>
FAIL len=51:  %...!..C!.................I.%...i..h4XK.O..........   (no key at all)
```

**Two conclusions, both firm:**

1. **The `get_result` query REACHES the board.** The `router->board len=82`
   frame is present in both runs, and the board answers it in both — differently.
   So nothing is lost on the way in.

2. **The result reply is never sent.** In the success the board emits a keyed
   209-byte frame carrying the `get_result` keyexpr and the payload. In the
   failure that frame never appears; a 51-byte frame with no keyexpr goes out
   instead, and the link then carries only keepalives.

So this is **not a transport defect**. The link delivers the query, the board
receives it, and the board's own action/`get_result` path fails to produce the
reply. That halves the problem exactly as intended and moves it off the wire and
into the RMW/executor side.

## Next

Identify the 51-byte frame. It shares its leading bytes with the `len=78` frame
that precedes it in both runs, so it is likely a short protocol message — a
zenoh `Err` reply or a final-marker with no payload — rather than a malformed
result. Decoding it names what the board thinks it is answering with.

That is a decoder change, not a hardware run: the bytes are already captured in
the tap dumps.

---

# 2026-09-04 — a mechanism that explains the idle soak, and a misattribution

## The measurement table credits the wrong issue

The row "after the 0882 allocator fix — 9/10" is
[#0912](archived/0912-transport-failure-teardown-crashes.md), not 0882. **0882
is the NuttX cmake carrier bug and contains no allocator fix**; 0912 is the
`k_free` on a TLSF block -> `z_free` one, on this board, and its final table
reads 9/10. 0912 is also the direct predecessor — it established that "the
remaining 1 in 10 is a transport failure the board survives" — and it is missing
from `related:`. Fix both.

## The 51-byte frame is almost certainly the FINAL status publish, not an error

The "Next" section guesses a zenoh `Err` reply or a final-marker. **A zenoh
`Err` reply is impossible**: `z_query_reply_err` has **zero** call sites in
`zpico.c`. The shim can emit an OK reply or nothing.

The arithmetic points at `publish_status_array()` with an empty goal list:

* the status array serialises only `active_goals` — `write_u32(len)` then one
  `GoalStatusStamped` each (`action_core.rs:1067-1086`);
* one `GoalStatusStamped` = 16 (uuid) + 4 + 4 (stamp) + 1 (status) = **25 B**
  (`nros-core/src/action.rs:263-269, 367-374, 404-409`);
* so 1 goal -> 33 B payload, 0 goals -> 8 B. **Delta 25.** Observed delta is
  78 - 51 = **27**, i.e. 25 + framing;
* status goes out on a **declared** (numeric) keyexpr, so no key string appears
  — matching "no key at all" — and it shares its leading bytes with the 78-byte
  frame, which is what was observed. A `REPLY_FINAL` would not share a prefix
  with a publication and is ~15 B on this link, not 51.

**If that reading holds the conclusion changes materially:** in the failing run
the board **executed the goal and ran `complete_goal_raw` to its last line**.
The missing 209 sits between the two status publishes — in the deferred-reply
flush at `action_core.rs:684-700`. This is not "the board never got there"; it
is "the reply was attempted or skipped, and the failure was discarded".

Checkable for free in the captures already taken: decode the 78 and 51 as zenoh
Push messages and compare resource ids and payload element counts.

## Leading candidate: a reply-slot leak, and it is the only thing that explains the idle soak

`ZPICO_MAX_PENDING_REPLIES` is **4**, hardcoded at `zpico.c:256-257` with **no
Kconfig knob, no `-D`, no env** — verified by grep across `*.rs`, `Kconfig` and
`*.cmake`. The slot is reclaimed **only** after a fully successful reply
(`zpico.c:3905-3906`); every error return above it leaves `stored_query_valid`
set.

And the query is cloned into a slot **before** the user callback runs
(`zpico.c:875-885`), while the Rust callback drops empty-payload queries
**after** that, at `shim/service.rs:225-227`:

```rust
if payload.is_null() || payload_len == 0 {
    return;
}
```

Its own comment says what those are: "liveliness probes that zenoh-pico
delivers through the same queryable callback as real service requests". That
filter predates the Phase-237 clone, so the clone was added **underneath** an
unconditional early return. **Every background discovery or liveliness probe
delivered to the queryable permanently consumes one of four slots.** The
ring-full drop at `:234-236` is the same shape.

Once four are gone, `reply_seq` is `-1` forever: `send_response(-1)` ->
`zpico.c:3848-3850` -> `ZPICO_ERR_INVALID`, swallowed. **Goal accepted,
executed, status published, no result, no error anywhere.**

Why the silence is structural — the error is discarded at four layers:
`action_core.rs:695` (`delivered_any |= sent.is_ok()`, after the entry was
already `swap_remove`d at `:688`, so a failed send strands the requester and
returns nothing), `:706-710` (`Ok` as long as the result reached the slab),
`arena.rs:1925` (`if let Ok(Some(_))`), and `nros-cpp/src/action.rs:529-532`,
which prints **"Goal succeeded"** for a goal whose result was never sent. The
one site that would explain it is `log::error!` under `#[cfg(feature = "std")]`
— dead on this image.

**This reframes the issue's central claim.** If the mechanism is right, the
20-90 % spread is **not noise; it is a deterministic countdown**, consumed by
elapsed time with a peer present rather than by goal traffic — which is exactly
why 100 s of idling made it worse (2/5). And the failing state is **absorbing**:
once a boot starts failing it can never recover.

## Two free checks, before any hardware

1. **In every existing run log: did any goal succeed AFTER an earlier goal
   failed in the same boot?** One recovery falsifies the leak outright. A
   contiguous failing tail confirms it to first order.
2. **In the tap dumps already captured:** a `reply_seq == -1` query is never
   cloned, so `_z_query_clear` fires when the handler returns and emits a
   `REPLY_FINAL` (`zenoh-pico/src/net/query.c:54-60`) — a ~15 B board->router
   frame between the 82 B query and the 78 B status, easily binned as a
   keepalive. Present => the leak. Absent anywhere in the run => a per-reply
   transport failure instead.

## Ruled out with code, not argument

* **`ZPICO_MAX_QUERYABLES` (issue 0460).** Fails at `ZenohServiceServer::new`,
  at boot, hard. The action would never appear in `ros2 action list`, which was
  measured as resolving. Cannot be intermittent.
* **`ZPICO_MAX_PENDING_GETS` / `ZPICO_GET_REPLY_BUF_SIZE`.** These size the
  `zpico_get*` **client** path. The board is the action **server** and issues no
  queries for this flow.
* **`z_query_clone` failing under heap pressure.** It is a refcount increment
  (`refcount.h:70-72`), not an allocation — so `reply_seq == -1` means "the
  table was full" and nothing else. That is what makes this a capacity story
  rather than a memory story.
* **`ffi_guard` masking interrupts across the reply.** `critical_section::with`
  only under the `ffi-sync` feature, which only the RTIC bare-metal examples
  enable. No-op here.

## Second candidate, if the free checks refute the leak

A per-reply failure inside zenoh-pico, reaching the same discarded `Err`:
`_z_send_n_msg` with `CONGESTION_CONTROL_BLOCK` returning
`_Z_ERR_TRANSPORT_TX_FAILED` (`net/primitives.c:465-467`), `z_bytes_copy_from_buf`
OOM (`zpico.c:3861/3878`), or `_Z_ERR_KEYEXPR_NOT_MATCH` (`:438-440`) via a real
design weakness: `ServiceBuffer.keyexpr` is a **single shared copy overwritten
by every query on the queryable** (`shim/service.rs:204-218`, before the
empty-payload return), while `try_recv_request` copies it into `reply_keyexpr`
at **dequeue** time (`:503-512`). A foreign-keyexpr query landing between
enqueue and dequeue makes the server reply with the wrong key. The comment
"constant per server" is the assumption a wildcard or probe query breaks.

**It does not explain the idle soak** — nothing about 100 s of idling raises TX
failure or OOM probability — and that should be said plainly rather than glossed.

## Latent, file separately if confirmed

`shim/service.rs:246` takes the reply seq indexed by the **Rust** service-buffer
counter, while `send_response` (`:539`) passes the **C queryable handle**
(`zpico.c:2805-2814`, first free slot). They coincide only while every server is
created in order and none is destroyed — `NEXT_SERVICE_BUFFER_INDEX` never
decrements, but `Queryable::drop` (`zpico.rs:218`) frees the C slot for reuse.
One dropped service server desynchronises the two permanently. Not reachable in
this image; one lifecycle transition away.
