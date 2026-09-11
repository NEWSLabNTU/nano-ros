---
id: 902
title: "action goals complete between 20 % and 90 % of the time on the same build,
  with no session expiry and no fault to explain the difference"
status: open
type: bug
area: rmw
related: [issue-0912, issue-0882, issue-0879, issue-0852, phase-444, phase-455]
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

## Status 2026-09-11 — both leak arms fixed, the symptom never re-measured

Checked against the code on `main`, not against the commit messages.

**Fixed: the reply-slot leak, both arms.** Every release now goes through ONE
helper, `_zpico_release_reply_slot` (`zpico.c:4083`):

* declined query: `1a032a10b`. `zpico_queryable_take_reply_seq` clears the
  seq, and `query_handler` releases any clone whose seq is still set when the
  callback returns (`zpico.c:990`). This covers the empty-payload probe and the
  ring-full drop.
* failed reply: `b56e3d50a`. Every error return in `zpico_query_reply`
  releases the slot, and so does the success path (`zpico.c:4112`, `:4119`,
  `:4137`).

**What remains, and why this stays open:**

1. **The symptom was never re-measured.** No run after the fixes records a goal
   completion rate, so nobody knows whether the leak was the whole cause or
   only part of it. The 09-04 section's "second candidate" (a per-reply
   transport failure) does not explain the idle soak, but nothing has ruled it
   out either. `phase-444-rmw-fix-up.md` W2 carries this as its acceptance: the
   completion rate on a freshly built image, over enough runs to separate it
   from 20–90 %. That run needs a router and a live peer.
2. **The two free checks were never recorded.** One: did any goal ever succeed
   after an earlier failure in the same boot? Two: is there a ~15 B
   `REPLY_FINAL` between the 82 B query and the 78 B status? Both read data
   that already exists. They would confirm the MECHANISM on the pre-fix
   captures, independent of item 1.
3. **No regression test.** Both fix commits say why: reaching either arm needs
   a query delivered to a queryable, which needs a router. No test under
   `packages/testing` references the reply-slot table.
4. **The latent desync is unchanged.** `shim/service.rs:402` builds
   `buffer_index` from the Rust counter, and `:246` passes it to
   `zpico_queryable_take_reply_seq` as the C queryable handle. The comment at
   `:241` asserts they are equal, and nothing enforces that. Still unreachable
   in this image; still one dropped server away.
5. **A correction to the 09-04 section.** `ZPICO_MAX_PENDING_REPLIES` is
   `#ifndef`-guarded (`zpico.c:265`), so a raw `-D` does override it. No
   Kconfig, cmake or env knob sets it, which is what the leak analysis relied
   on.

Also done here: `related:` gains issue 0912, as the 09-04 section asked,
together with phase-444.

**What would close it:** item 1's measurement at or near 10/10 on a fresh image
closes the issue. A residual failure rate reopens the second candidate, and
items 2 and 3 are what would make that residual diagnosable.

## Status 2026-09-12 — the failure has an observable now (phase-455 W1)

Item 3 above said there is no regression test, and gave the reason: reaching
either leak arm needs a query delivered to a queryable, which needs a router.
Item 1 said the symptom was never re-measured. Those two are the same problem
seen from opposite ends — the only evidence available was a RATE, and a rate
answers "did results arrive", never "did a slot run out".

**The image says it now.** `query_handler`'s reply-slot allocation is
`zpico_reply_slot_pick`, which COUNTS a refusal and latches the transition
into exhaustion:

* `zpico_reply_slot_stats(session, handle, *refusals, *capacity)` — slots
  currently held as the return value, cumulative refusals beside it, shaped
  after `zpico_graph_entry_count`'s `out_dropped`. **Zero refusals is a
  statement** — "this server has never run out" — which `last_reply_seq == -1`
  could not make, because that value also means "no query pending". That
  conflation is the whole of this issue.
* `zpico_reply_slot_take_announcement(session, handle)` — take-and-clear, so
  the saturation is reported ONCE per transition rather than once per spin
  (phase-444 W6's correction, on the Cyclone parameter services).
* `send_response` no longer folds a negative seq into
  `TransportError::ServiceReplyFailed`. Nothing was attempted, so "the reply
  failed" was false; it returns a `Backend` diagnostic naming the knob, and
  `nros_log` emits one line at the transition. The log is on the RUST side
  deliberately: `printk` is a no-op under `ZPICO_SMOLTCP`/`ZPICO_SERIAL`, the
  bare-metal serial board this issue was measured on, so a C-side print would
  have reached nothing on the one target where the defect was found.

The four swallow sites (`action_core.rs:695`, `:706-710`, `arena.rs:1925`,
`nros-cpp/src/action.rs:529-532`) are unchanged and still recorded here.

**Item 5 re-measured rather than re-read.** `ZPICO_MAX_PENDING_REPLIES` is
still `#ifndef`-guarded and a raw `-D` still overrides it:
`CFLAGS="-DZPICO_MAX_PENDING_REPLIES=0" cargo build -p zpico-sys` fails on the
file's own `#error "ZPICO_MAX_PENDING_REPLIES must be >= 1"` (zpico.c:383),
with the flag visible last on the cc-rs command line. Still no Kconfig, cmake
or env producer sets it. The regression test does not rely on it — the pure
pick takes `cap` as a parameter, so any capacity is drivable without a rebuild.

**The tool this issue cites was never in version control.**
`experiments/serial-interop/serial-tap.py` appears in no commit in full
history, under no other name, and in no checkout on this disk; the captured
dumps are likewise absent. So the "two free checks, before any hardware" in the
09-04 section read data that nobody now has, and item 2 above cannot be
discharged by anyone but the person who ran the tap. Recorded here rather than
left for the next reader to search for.

**What still stands.** Item 1's measurement — the completion rate on a fresh
image — is phase-455 W2. Item 4, the `buffer_index`/queryable-handle desync,
is unchanged.
