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

## SUPERSEDED — the gdb backtrace, and the ratchet reading it produced

The backtrace below is sound and still useful. The conclusion drawn from it —
that the capacity ratchet is what empties the buffer — is not; see "Retractions"
at the end. Kept because the frames are the evidence that led to the real cause.


`gdb-multiarch` against the QEMU guest (`-s -S`), breaking on `__assert_func`:

    #1  _z_wbuf_put
    #2  __unsafe_z_prepare_wbuf
    #3  _z_transport_tx_send_n_msg
    #4  _z_write
    #5  _z_publisher_put_impl
    #6  zpico_publish_with_attachment

It fires on `__unsafe_z_prepare_wbuf`'s OWN FIRST WRITE — the stream length
prefix, `_z_wbuf_put(buf, 0, 0)` — immediately after `_z_wbuf_reset`. The buffer
comes out of reset with no capacity at position 0.

Why: `_z_wbuf_wrap_bytes` does

    ios->_capacity = ios->_w_pos;   // "Block writing on this ioslice"

and `_z_iosli_reset` clears only the positions:

    static inline void _z_iosli_reset(_z_iosli_t *ios) {
        ios->_r_pos = 0;
        ios->_w_pos = 0;            // _capacity NOT restored
    }

So the truncation is PERMANENT and ratchets the owned slice down on every
wrap-path publish, until nothing is writable. `_expansion_step` holds the true
slice size (`_z_wbuf_make` sets it to the requested capacity and makes the first
slice exactly that big), so it is recoverable.

## SUPERSEDED — the patch tried against that reading


In `_z_wbuf_reset`, restore an owned slice's capacity from `_expansion_step`,
and stop skipping elements after a removal (the `i++`-past-a-shift bug already
described above).

Measured, three runs each, same reproduction:

    before:  publishes 40, 40, 40   (assert every time, deterministic)
    after:   publishes 19, 60, 67   (one run reached 67 with NO assert)

So the capacity ratchet is A cause — the failure stops being deterministic — and
it is NOT the only one. Stated plainly rather than shipped as a fix.

## The remaining fault looked like use-after-free — and it is

A later gdb run tripped the FREERTOS assert rather than zenoh-pico's. The
`0xdeadbeef` read off an argument register there turned out to be the debugger's
(see "Retractions"), but the instinct it prompted was right: the next section
proves a use-after-free, and it accounts for BOTH assertions.

## Tooling notes for whoever continues

* `arm-none-eabi-gdb` on this host is BROKEN — `libncursesw.so.5` missing. Use
  `gdb-multiarch` with `set architecture arm`.
* zenoh-pico is compiled without DWARF, so fields cannot be evaluated by name;
  the backtrace above came from symbol names plus AAPCS argument registers
  (`r0`, `r1`, `r2`).
* Start the talker with `-s -S` and attach; the listener and router run normally.
* The stale-binary trap here was [issue 0911](0911-zpico-sys-no-rebuild-edge-on-zenoh-pico-sources.md)
  (renumbered from 0902), and it is **fixed**: `nros-zpico-build` now watches the
  `zenoh-pico/src` and `include` trees, so an edit to any compiled source
  re-runs the build script. The old workaround -- touch a WATCHED file such as
  `src/net/primitives.c` AND the leaf's own `main.c` -- is no longer needed for
  the archive. The example RELINK edge (issue 0475's class) is still open on the
  cargo path, so on that path touching the leaf's `main.c` may still be
  required.

## ROOT CAUSE: the lease task frees the transport under the publishing task

Both assertions are one use-after-free, and the packet capture names its trigger.

**What the image does, every twenty seconds.** Probes on `_z_mutex_init` /
`_z_mutex_drop` (zenoh-pico's own FreeRTOS `system.c` — NOT `platform_aliases.c`;
that arm is for ThreadX) print the transport's three mutexes being created and
destroyed over and over on a talker that is doing nothing but publishing at 1 Hz:

    !!! ZMUTEX-INIT slot=0x200e7124 h=0x200e73a8 from=_z_unicast_transport_create_inner task=app
    ...  Publishing: 'Hello World: 19'
    !!! ZMUTEX-DROP slot=0x200e7124 h=0x200e73a8 from=_z_common_transport_clear  task=zpico_lease

Created by `app`. Destroyed by **`zpico_lease`**. `_z_common_transport_clear`
runs `vSemaphoreDelete(_mutex_tx)` and `_z_wbuf_clear(&_wbuf)` with nothing
holding off a publisher that is inside the same transport at that moment. The
window is one publish wide and it opens every lease period.

**Which is exactly what the two asserts are.** They are the two ways to lose
that race, one per freed object:

* `_z_wbuf_clear` leaves `_ioss` empty, so the next
  `__unsafe_z_prepare_wbuf` → `_z_wbuf_put(buf, 0, 0)` walks a zero-length
  slice list and trips `iobuf.c:374`, `assert(i < _z_iosli_svec_len(...))` —
  the originally reported failure.
* `vSemaphoreDelete` frees the queue, so the next `_z_mutex_lock` reaches
  `xQueueTakeMutexRecursive` on freed memory and trips
  `queue.c`'s `configASSERT(pxQueue->uxItemSize == 0)`:

      !!! SEMTAKE-BAD q=0x200e73a8 itemSize=537821688 len=1 waiting=33 task=app
                     ret=xQueueTakeMutexRecursive

  `q=0x200e73a8` is `_mutex_tx`'s handle from the FIRST init above — the one
  `zpico_lease` had already deleted. `itemSize=537821688` is `0x200E7378`, a
  heap pointer 0x30 below the block: free-list bookkeeping written over the
  Queue_t.

One writer, two victims, and the address identity ties the crash to the drop.

**Why the lease task tears anything down.** A probe on the expiry branch says
so directly, and it fires on schedule:

    !!! LEASE-EXPIRED nothing received in 10000ms

`_zp_unicast_lease_task` closes a client session when `_received` was false for
a whole lease period. The read task marks it on every RX batch — and it runs
exactly ONCE per session, at open, then blocks for the full ten seconds.

**Because the router really does go silent.** `tshark` on the loopback side of
the slirp forward, one 45 s run, filtering on payload only:

    router -> guest, tcp.len>0:   0.004 s  76 B
                                  0.006 s   8 B
                                  0.011 s  12 B
                                 19.522 s  76 B   <- a NEW handshake
                                 19.522 s   8 B
                                 19.523 s  12 B
                                 40.086 s  76 B   <- and another
    guest -> router, tcp.len>0:  40 frames (the 1 Hz publishes)

Three frames in the first twelve milliseconds, then nothing until the session
has already been declared dead and rebuilt. `rmw_zenohd` sends this session no
periodic KeepAlive at all, and zenoh-pico's client lease expires on silence.

## This is TWO defects, and only the second one is FreeRTOS's

**A — the reconnect churn is platform-independent.** The same capture against
the NATIVE Linux `c_talker`, same router, same port:

    0.000 s 76 B / 0.000 s 8 B / 0.000 s 12 B / 20.075 s 76 B ...

Identical: silence, expiry, fresh handshake at ~20 s. Every zenoh-pico client
we ship drops and rebuilds its session every twenty seconds against the ROS
router. Native survives it (no assert fires), so it has been invisible — but a
session that dies on a timer is a plausible cause of interop flakiness far
beyond this issue, and it should be filed and chased on its own.

**B — the teardown race is the crash.** Defect A only supplies a trigger;
`_zp_unicast_failed` freeing a live transport from the lease task is unsafe
whatever caused the lease to lapse (a real link drop does it too). Fixing A
alone would make this crash rare instead of fixed.

## Direction

For B, in order of preference: have `_zp_unicast_failed` take `_mutex_tx` (and
`_mutex_rx`) for the teardown so a publisher cannot be inside the transport, or
defer the clear to the task that owns the session rather than doing it on the
lease task. Note the FreeRTOS mutex is RECURSIVE by construction
(`nros_platform_mutex_init` says so deliberately, and zenoh-pico's own FreeRTOS
`_z_mutex_init` uses `xSemaphoreCreateRecursiveMutex` too), so a lock taken for
teardown will not deadlock against an outer hold in the same task — and equally,
it will not protect against one.

For A, establish first whether the router is expected to keepalive here or
whether zenoh-pico must keepalive to keep its OWN lease refreshed; the talker
never sends one because `_transmitted` is true every second, which suppresses
it. That is a protocol question, not a port question.

## Retractions

Three earlier readings in this issue were wrong and are withdrawn:

* **The lwIP concurrency theory.** `lan9118_lwip.c:502` calls
  `netif->input(p, netif)`, and the board registers `tcpip_input` as that
  callback (`board_mps2.c:101`) — frames ARE marshalled into `tcpip_thread`.
  The driver header's example still names `ethernet_input`, which is what the
  claim was read off; the header comment is stale, the code is correct.
* **The `0xdeadbeef` "poison".** It came from a gdb script dereferencing `wbf`
  in a frame with no DWARF, i.e. from the debugger, not the image. Nothing in
  the firmware writes that pattern.
* **The `_z_wbuf_reset` capacity ratchet as the cause.** The two defects in
  that function are real — the removal loop still does `i++` after
  `_z_iosli_svec_remove` shifts the vector down, so one of the two adjacent
  non-alloc slices `_z_wbuf_wrap_bytes` appends survives every reset; and
  `_z_iosli_reset` never restores the `_capacity` that `wrap_bytes` truncated.
  But `_z_buf_encode` only calls `wrap_bytes` when `_expansion_step != 0`, and
  the transport `_wbuf` is built `_z_wbuf_make(mtu, false)` — non-expandable.
  It never holds a non-alloc slice, so neither defect can reach it. The
  40/40/40 → 19/60/67 shift attributed to "fixing" this was run-to-run
  variance. Worth reporting upstream on its own merits; not this bug.

## Acceptance

* The talker survives a sustained run with a peer attached.
* Whatever is corrupted is named, rather than surfacing as two unrelated
  assertions.
