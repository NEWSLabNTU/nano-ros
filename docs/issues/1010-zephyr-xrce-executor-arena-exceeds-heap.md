---
id: 1010
title: "Every zephyr XRCE example dies at boot allocating the XRCE session
  struct — 81% of it is `subscriber_slots` at ring depth 32"
status: open
type: bug
area: zephyr, platform, examples
severity: high
found: 2026-09-03
related: [issue-0968, issue-1003]
---

## Measured

All nine `example_e2e` zephyr XRCE cells, on freshly built images, all three
languages:

```
Summary [290.668s] 9 tests run: 0 passed, 9 failed, 45 skipped
```

Zero skips — every one of the nine actually ran. 72 copies of one message:

```
nros: HEAP EXHAUSTED: request 329648 bytes, arena 66048 bytes
      (raise CONFIG_NROS_ZEPHYR_HEAP_SIZE / NROS_ZEPHYR_HEAP_SIZE)
```

By workload: pubsub 329648, service/action 423808 and 427952. Arena 66048
throughout (65536 + 512 of allocator overhead).

## Why it can never work

From the built images' own `autoconf.h` (`build-rust-talker-xrce`):

```
#define CONFIG_NROS_ZEPHYR_HEAP_SIZE 65536
#define CONFIG_NROS_EXECUTOR_ARENA_SIZE 0
```

`0` means derive, and `zephyr/Kconfig` says what the derive does:

> budgets EVERY callback slot at action-client size (~18 KiB) -- correct, but
> an order of magnitude over for a pub/sub-only image

So the executor arena derives to ~322-418 KiB and is requested as a **single**
allocation (`nros_platform_alloc` -> `nros_zephyr_heap_alloc`, one call, the
size printed above) from an arena of 64 KiB. This is not fragmentation, not a
leak, and not load-dependent: one block strictly larger than the whole arena
fails 100% of the time, on every host, forever.

Both knobs are individually defensible and jointly impossible:

* `NROS_ZEPHYR_HEAP_SIZE` default 65536 — a sane default for a small image.
* `NROS_EXECUTOR_ARENA_SIZE` default 0 — a deliberately conservative derive.

**Nothing compares them**, and no zephyr XRCE example sets either. The only
mention across the whole example tree is a COMMENT in
`examples/zephyr/c/talker/prj-xrce.conf` explaining the knob exists.

## What this does and does not close

This is the boot failure for the whole zephyr XRCE cluster in issue 0968 — the
`action` sub-signature there (`Transport(BadAlloc)`, `rc=-6`, kernel panic) is
this allocation failing and the caller reacting to NULL.

**It is not yet proven that fixing it makes the tests pass.** Raising the heap
and re-running is the next step; other failures may sit behind this one. The
established fact is narrower and stronger: these images cannot boot as
configured, so no runtime result from them has ever been evidence about
anything else.

That also means 0968's earlier `pubsub`/`service` "boots, then no delivery"
reading needs re-examination against this, rather than being carried forward.

## Suggested shape of a fix

Sizing the examples is the immediate unblock, but the durable fix is the
comparison nothing does today: the derived executor arena is known at BUILD
time and the heap arena is a Kconfig integer, so an image whose arena cannot
hold its executor should fail to LINK, not at boot. `NROS_EXECUTOR_ARENA_SIZE`'s
own help already warns "too small fails at runtime, not at link" about the
other direction; this is the same complaint one level up.

## Acceptance

* [ ] A zephyr XRCE image's derived executor arena fits its heap arena, or the
      build fails naming both numbers.
* [ ] The nine `example_e2e` XRCE cells boot.
* [ ] 0968's zephyr section is re-measured against images that boot.

## CORRECTION 2026-09-04 — the executor arena is NOT the allocation that fails

This issue's title and diagnosis are wrong, and the fix they imply would have
broken three passing cells. Both are retracted here, with the measurements.

### What this issue claimed

That `Executor::open` asks for the DERIVED EXECUTOR ARENA as one block, that the
block is ~6x the `NROS_ZEPHYR_HEAP_SIZE` arena it comes from, and that the fix is
a build-time comparison of the two — "an image whose arena cannot hold its
executor should fail to LINK, not at boot".

I implemented exactly that: a check in `nros-node`'s build script comparing the
derived `ARENA_SIZE` against `CONFIG_NROS_ZEPHYR_HEAP_SIZE`, panicking with both
numbers. It works — it fires end to end on a real build. It is also wrong, and
it is not committed.

### Measured, and it refutes the premise twice

`build-cpp-talker-zenoh` and `build-cpp-talker-xrce` carry BYTE-IDENTICAL
executor and heap knobs (`CONFIG_NROS_EXECUTOR_MAX_CBS=16`,
`CONFIG_NROS_ZEPHYR_HEAP_SIZE=65536`, `CONFIG_NROS_EXECUTOR_ARENA_SIZE=0`), and
running the derivation against each `.config`:

| image | derived `ARENA_SIZE` | heap | runtime |
| --- | --- | --- | --- |
| zenoh cpp talker | **74240** | 65536 | **passes** (`case_0{1,2,3}_zenoh_*_pubsub_e2e`, 3/3) |
| xrce cpp talker | **74240** | 65536 | dies: `HEAP EXHAUSTED: request 329648` |

Two independent refutations:

1. **`arena > heap` is not fatal.** 74240 exceeds 65536 on the zenoh image, which
   passes. The comparison predicts nothing, and a gate built on it fails working
   images — which is what mine would have done to those three cells.
2. **The arena cannot explain the failure.** It is the SAME size for both RMWs,
   and only XRCE dies. 329648 is not 74240, so the block that exhausts the heap
   is not the arena.

### What is actually true

The fatal allocation is **RMW-specific and still unidentified**. The two configs
differ only in RMW knobs (`NROS_BATCH_UNICAST_SIZE`, `NROS_FRAG_MAX_SIZE`,
`NROS_GRAPH_CACHE_SIZE`, … on the zenoh side; the XRCE set on the other), and no
single knob is anywhere near 329648 — so it is a composite the XRCE path
requests during `Executor::open`.

The workload dependence recorded above still holds and is now the useful clue:
329648 for pubsub, 423808/427952 for service and action. Whatever it is scales
with entity count on the XRCE side.

### Where to start, and where NOT to

Start at what the XRCE transport allocates during `Executor::open` — not at the
arena derivation, and not at `NROS_EXECUTOR_*`. The RMW is the only differing
variable in a controlled A/B.

Do not re-implement the arena-vs-heap gate. It is written, it works, and it is
wrong; this section exists so the next person does not spend the afternoon
rediscovering that.

### Method note

The gate looked right, fired correctly on a synthetic config, and produced a
clean error message. What killed it was asking whether it would fail images that
currently PASS — one command, before committing anything. The three zenoh cells
answered it.

(Also worth knowing for the next gate: a `#[cfg(test)]` module inside `build.rs`
is never compiled by `cargo test`. The tests I wrote there would have been dead
code; they had to move to `nros-zephyr-build` to run at all.)

## Revised acceptance

* [ ] Identify the XRCE-side allocation that requests ~330-428 KB, by workload.
* [ ] The nine zephyr XRCE cells boot.
* [ ] ~~A link-time gate comparing executor arena to heap~~ — refuted above.

## 2026-09-04 — IDENTIFIED: the allocation is the XRCE session struct's subscriber ring pool

The correction above left one question: what asks for ~330-428 KB. It is
`xrce_session_state`, and one member dominates it.

### Measured, not estimated

`sizeof` on the real declarations from `nros-rmw-xrce/src/internal.h`, with the
defines the zephyr XRCE images actually build with
(`XRCE_BUFFER_SIZE=1024`, `XRCE_SUBSCRIBER_RING_DEPTH=32`,
`XRCE_MAX_SUBSCRIBERS=8`):

| member | bytes |
| --- | --- |
| `xrce_subscriber_ring_entry` (`data[1024]` + `len` + `overflow`) | 1,040 |
| `xrce_subscriber_slot` (32 entries + indices) | 33,288 |
| `subscriber_slots[8]` | **266,304** |

Against the observed `request 329648`, that is **81 %** of the allocation; the
remaining ~63 KB is the service server/client pools (4 + 4 slots carrying their
own `data[XRCE_BUFFER_SIZE]`), the stream buffers, and `uxrSession`.

The workload scaling on file follows from the same structure: pubsub 329,648,
service and action 423,808 / 427,952 — more service slots, same ring pool.

### Why the number is what it is

`XRCE_SUBSCRIBER_RING_DEPTH` defaults to 32, and `internal.h` says why:

> Phase 160.H.1 — depth 32 (was bumped 4 -> 16 upstream first; raised to 32
> after re-testing 100Hz burst behaviour).

That is a HOST-THROUGHPUT tuning value. It sits in a struct allocated from a
64 KiB embedded heap, and no zephyr example overrides it. `nros-rmw-xrce-cffi/
build.rs` already documents the lever in as many words — "combined with
`STREAM_HISTORY=4` + `NROS_XRCE_CUSTOM_TRANSPORT_MTU=512` the session struct
drops from ~390 KB to ~10-20 KB" — for a configuration far smaller than what
these images use (they keep `MAX_SUBSCRIBERS=8`, `MAX_SERVICE_SERVERS=4`,
`MAX_SERVICE_CLIENTS=4`, `BUFFER_SIZE=1024`).

### Two traps for whoever takes the fix

* **Ring depth alone is NOT enough.** At depth 4 the pool falls to 33,344 —
  measured the same way — but the whole session still lands near 96 KB, above
  the 64 KiB heap. A fix needs the ring depth AND one of `MAX_SUBSCRIBERS` /
  `XRCE_BUFFER_SIZE`, or a larger heap. Changing one knob and re-running is how
  this looks fixed and is not.
* **`NROS_XRCE_SUBSCRIBER_RING_DEPTH` is not a Kconfig symbol.** `knob()` reads
  the ENVIRONMENT first and falls back to `CONFIG_<name>` in `$DOTCONFIG`; since
  Kconfig does not define this one, a `CONFIG_NROS_XRCE_SUBSCRIBER_RING_DEPTH=4`
  line in a `.conf` is dropped as an unknown symbol and reaches nothing. Setting
  it needs an env export (or a new Kconfig entry). This is the same shape as
  issue 0876 — a conf line that changes nothing and looks like it should.

### What is still NOT known

Whether shrinking the session to fit is SUFFICIENT for these cells to pass, or
only necessary. The nine cells fail at `Executor::open`; nothing downstream of
that has ever run, so no claim is made about what happens once they boot.

## Revised acceptance

* [x] Identify the XRCE-side allocation. It is `xrce_session_state`, dominated
      by `subscriber_slots[MAX_SUBSCRIBERS]` at `RING_DEPTH x (BUFFER_SIZE+16)`.
* [ ] Size it (or the heap) so the session fits, remembering ring depth alone
      does not.
* [ ] The nine zephyr XRCE cells boot.
