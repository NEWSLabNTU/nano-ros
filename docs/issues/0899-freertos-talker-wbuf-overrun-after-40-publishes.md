---
id: 899
title: "The FreeRTOS C talker dies mid-run inside zenoh-pico's write buffer —
  two different asserts, both after tens of successful publishes"
status: open
type: bug
area: rmw, boards
related: [issue-0877, issue-0135]
---

## Symptom

The `mps2-an385` FreeRTOS C talker publishes correctly for tens of messages and
then aborts inside zenoh-pico. TWO different assertions, from two runs of the
SAME binary:

    Publishing: 'Hello World: 40'
    assertion "i < _z_iosli_svec_len(&wbf->_ioss)" failed:
      zenoh-pico/src/protocol/iobuf.c, line 374, function: _z_wbuf_put

    Publishing: 'Hello World: 19'
    FreeRTOS ASSERT FAILED: third-party/freertos/kernel/queue.c:1673

Delivery WORKS up to that point — a paired listener received all 40.

## Reproduction (reliable, ~1 minute)

Both guests, host router, the harness's own QEMU arguments:

    ZENOH_CONFIG_OVERRIDE='scouting/multicast/enabled=false;listen/endpoints=["tcp/0.0.0.0:7900"]' \
      "$(ros2 pkg prefix rmw_zenoh_cpp)/lib/rmw_zenoh_cpp/rmw_zenohd" &
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -icount shift=auto \
      -semihosting-config enable=on,target=native \
      -nic user,model=lan9118,net=192.0.3.0/24,host=192.0.3.1 \
      -kernel examples/qemu-arm-freertos/c/listener/build-zenoh/c_listener &
    sleep 22
    qemu-system-arm … -kernel examples/qemu-arm-freertos/c/talker/build-zenoh/c_talker

The talker ALONE, with no listener, ran 67 publishes without asserting — so the
fault needs the traffic a real peer generates, not just publishing.

## What the two asserts have in common

* `iobuf.c:374` is `_z_wbuf_put(wbf, b, pos)` walking the io-slice list for
  `pos`. It fires when `pos` exceeds the total capacity of every slice — a
  BACKFILL past the end of the write buffer, i.e. wrong position arithmetic or a
  buffer that shrank/was reused underneath.
* `queue.c:1673` is `configASSERT(pxQueue->uxItemSize == 0)` inside
  `xQueueSemaphoreTake` — a handle taken as a SEMAPHORE that is actually a
  QUEUE.

Neither is a clean out-of-resources report; both are the shape of memory being
used as something it is not. Two distinct bad-state assertions from one binary
points at corruption rather than at either subsystem's own logic.

## Checked and ruled out

* **Buffer sizing.** `Z_FRAG_MAX_SIZE 2048` / `Z_BATCH_UNICAST_SIZE 1500`, and
  the payload is `Hello World: N`. The failure is accumulation over tens of
  messages, not one oversized write.
* **The condvar ABI mirror.** `nros_freertos_condvar_t` mirrors zenoh-pico's
  `_z_condvar_t` and the offsets are `_Static_assert`ed. On THIS board
  `configSUPPORT_STATIC_ALLOCATION` is 0, so zenoh-pico's struct is exactly
  `{mutex, sem, waiters}` and the mirror matches.
* **Generated-config divergence.** Five copies of `zenoh_generic_config.h` exist
  in the leaf's cargo dir with three distinct hashes, differing by
  `#define Z_FEATURE_LINK_CAN 0`. That define is emitted by OUR generator and
  referenced NOWHERE in vendored zenoh-pico, so it is inert — these are stale
  build-script outputs from different times, not the issue-0135 live mismatch.

## A claimed "adjacent defect" that was NOT one — retracted

An earlier revision of this issue claimed `nros_freertos_condvar_t` was a latent
size mismatch on `nros-board-freertos-posix`, because zenoh-pico's
`_z_condvar_t` gains two `StaticSemaphore_t` under
`configSUPPORT_STATIC_ALLOCATION == 1` while the mirror does not.

That is wrong, and the reasoning that made it look right is worth recording.
The mirror comment says it pins field OFFSETS and not total SIZE, which reads
like an oversight — a trailing conditional field is exactly what slips through
an offsets-only check. So I "fixed" it by adding the two buffers.

The build refused, and it was right to: an existing assert,
`NROS_PLATFORM_CONDVAR_STORAGE_SIZE >= sizeof(nros_freertos_condvar_t)`, failed
because the honest-looking struct no longer fit the 256-byte opaque storage the
platform ABI reserves.

The direction of ownership is what settles it. `zpico-sys/c/zpico/platform_aliases.c`
REDIRECTS zenoh-pico's condvar API at ours:

    int8_t _z_condvar_init(void *cv) { return nros_platform_condvar_init(cv); }

so zenoh-pico never interprets those bytes — it allocates opaque `[u8; N]`
storage (its own header comment says the threading primitives are opaque
precisely because they "do not match `nros_platform_task_t` shapes 1:1") and
calls us. The storage only ever holds OUR three fields. Pinning offsets and not
size is therefore correct, and the tail of zenoh-pico's struct is irrelevant
because nothing on this path ever writes it.

Reverted. Recorded because the mistake is re-derivable: the comment looks like
an admission of a gap, and it is actually a statement of scope.

## Investigated: a real zenoh-pico bug found, and it is NOT the cause

`_z_wbuf_reset` removes while iterating:

    for (size_t i = 0; i < _z_iosli_svec_len(&wbf->_ioss); i++) {
        if (!ios->_is_alloc) { _z_iosli_svec_remove(&wbf->_ioss, i, false); }
        else                 { _z_iosli_reset(ios); }
    }

`_z_svec_remove` shifts the tail down and decrements `_len`, so `i++` SKIPS the
element that moved into the freed slot. Borrowed (non-alloc) slices therefore
survive a reset while the buffer keeps counting their capacity.

It is reachable, and precisely: `_z_wbuf_wrap_bytes` appends TWO ADJACENT
non-alloc slices (the wrapped payload, then the previous slice's remaining
space) and `_z_buf_encode` takes that path for any payload over `Z_ZID_LENGTH`
on an expandable wbuf — every publish here. Two adjacent removals is exactly the
case the skip mishandles. Present at our pin AND on the fork's `main`, so it is
an unfixed upstream defect.

**It does not fix this issue.** With the corrected loop compiled in and the
example RELINKED, the assert still fires at 40 publishes, twice:

    run 1: publishes=40 delivered=40 asserts=1
    run 2: publishes=40 delivered=40 asserts=1

One earlier run showed 61 publishes and no assert, which looked like success —
but it had `delivered=0`, so the peer never attached and the failing path was
never exercised. An outlier, not evidence. Recorded because it was momentarily
convincing.

## The measurement was nearly wrong for a second reason — issue 0902

The first attempt at the above tested a binary that did not contain the fix:
`zpico-sys` lists SEVEN zenoh-pico files in `rerun-if-changed` and `iobuf.c` is
not among them, so editing it recompiles nothing. And even once `zpico-sys`
rebuilds, the C example is not relinked (issue 0475's class). Getting a
zenoh-pico edit into an image currently needs a touch of a watched file AND a
touch of the leaf's `main.c`.

Anyone continuing this issue must do that first, or they will measure the old
binary — as I did.

## Acceptance

* The talker survives a sustained run with a peer attached.
* Whatever is corrupted is named, rather than surfacing as two unrelated
  assertions.
