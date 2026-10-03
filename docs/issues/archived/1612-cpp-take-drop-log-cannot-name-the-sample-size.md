---
id: 1612
title: "A BufferTooSmall drop on the C++ take path cannot name the sample's size:
  the RMW take ABI returns bytes-or-error with no required-length out-param"
status: resolved
type: enhancement
area: [rmw, cpp, diagnostics]
severity: low
found: 2026-10-01
related: [1425, 0757, phase-460, 1632]
---

## What

Issue 1425's acceptance asked for the first too-small drop on a C++
subscription to log "topic and both sizes". phase-460 W7 (`f74ebdade`) counts
the drop, pushes the total into the boot record and logs the BUFFER size, and
recorded why it stops there: the RMW C ABI's take returns "non-negative = bytes
produced, negative = error code", so `TransportError::BufferTooSmall` carries no
length, and no layer below the C++ wrapper ever learns how large the dropped
sample was. `nros-node`'s arena dispatch (`arena.rs`, issue 0757) has the same
gap for the same reason. The topic is absent for a different reason: it lives on
the C++ `Subscription<M>`, not in the storage the take path is reached from.

1425 was closed on the faults it named (exhaustion and the drop both reach the
record); this is the part that needs an ABI change.

## What would fix it

A required-length out-param (or a negative return that encodes it) on the RMW
take slot, filled by each backend that knows the wire size before it copies --
zenoh-pico's sample length, XRCE's payload length, Cyclone's serdata size --
then carried by `TransportError::BufferTooSmall { needed }`. A header change,
so RFC-0054 applies: regenerate the bindings, and every backend's slot moves in
the same change.

## Resolution (2026-10-02)

**Fixed for the subscription `take` on every backend that can know the size.**

**ABI, additive (RFC-0054).** No new slot, no layout change: on
`NROS_RMW_RET_BUFFER_TOO_SMALL` from `take`, `rmw_mut_byte_span_t.len` — the
field that was "undefined on failure" — now carries the size the sample NEEDED,
or `NROS_RMW_TAKE_LEN_UNKNOWN` (`0`) when the backend cannot know. Zero is safe
as the sentinel because a zero-byte sample fits every buffer, and the CALLER
pre-sets it, so a backend written before the rule (which never touches `len` on
failure) reads as "unknown" rather than as whatever was on the stack. Headers:
`rmw_entity.h` (the rule + the macro), `rmw_ret.h`, `rmw_vtable.h`; bindings
regenerated. `check-abi-bindings`, `check-rmw-abi-shape`,
`check-rmw-api-parity` and `check-ffi-struct-mirrors` all green — no new symbol
to classify.

**Not `TransportError::BufferTooSmall { needed }`**, which this issue
suggested. That variant is returned by a dozen non-take paths (arena
exhaustion, short reply/request buffers, the streamed-publish cap), each of
which would have had to invent a size. The size is a query beside the error
instead: `nros_rmw::Subscription::refused_sample_len() -> Option<usize>`,
default `None`.

| Layer | Where the size comes from |
| --- | --- |
| zenoh | the ring's stored length; one `refuse_oversized_head` helper now does every refusal (the three single takes AND the batch park), so none can drop and forget |
| XRCE | the staged entry length (`subscriber.c`) |
| Cyclone | `ddsi_serdata_size`, also carried through the `take_sequence` park (`pending_too_small_len`) |
| uORB | `meta->o_size` |
| cffi Rust adapter | the Rust backend's `refused_sample_len()` written into `len` |
| `CffiSubscription` | reads `len`; a value that would have FIT is a contract breach and reads as unknown; a native batch refusal clears it (no span) |
| nros-cpp drop log | `"<needed>-byte sample, <cap>-byte buffer"`, `?` when unknown; `MessageTooLarge` (backend staging, refused before any take) always `?` |

**Measured (before → after).** Before: the line said only
`sample too big for the 16-byte buffer`. After: the nros-cpp cell asserts
`1000-byte sample` and `16-byte buffer` in the same line, and that the line
still ends `(issue 1425).` — `nros_log`'s 256-byte all-or-nothing push would
cut the tail if it grew too long. Per backend:

- **cffi** `tests/take_sequence.rs::a_refused_take_reports_the_size_the_sample_needed`
  — size written → `Some(500)`; legacy backend → `None`; fitting `len` → `None`;
  and the stub asserts the caller pre-set UNKNOWN.
- **cffi adapter** `tests/rust_adapter.rs` — a Rust backend refusing 300 bytes
  reaches the C span as `len == 300`.
- **zenoh** `sub_buf_caller_too_small` — 512 bytes into 256 records `Some(512)`.
- **XRCE** new ctest `nros_rmw_xrce_take_refused_len`: 300-byte sample, 150-byte
  buffer, `len` 300 (`just check rmw-xrce`, 3/3).
- **Cyclone** `take_sequence_pending_status`, both roads: `80-byte sample refused
  by a 15-byte buffer, len reported 80` directly and through the batch park.
  Negative control: removing the parked write fails road 1 with `len 0`
  (`just check rmw-cyclonedds`, 33/33).
- **uORB** `register_smoke`: a 4-byte take reports `o_size`
  (`just check rmw-uorb`).

**NOT measured.** No embedded image was run with an oversized sample — the
backends' C code is the same TUs the host lanes compile, but no RTOS image
printed the new line. The Rust arena's drop log (`nros-node` `arena.rs`, issue
0757) and the service `take_request`/`take_response` paths still do not name
the size: filed as [issue 1632](1632-arena-drop-log-and-service-takes-do-not-name-the-refused-size.md).
The topic is still not on the line (unchanged).

Sweep:
`grep -rn 'BUFFER_TOO_SMALL' packages/rmw --include='*.c' --include='*.cpp'` and
`grep -rn 'impl.*Subscription for' --include='*.rs' packages`.
