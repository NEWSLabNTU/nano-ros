---
id: 1368
title: "The zenoh reassembly ceiling is compared to nothing - an image whose
  derived receive bound exceeds ZPICO_FRAG_MAX_SIZE drops large samples with no
  build-time signal"
status: open
type: bug
area: rmw, zenoh, cmake, zephyr, sizing
severity: medium
found: 2026-09-18
related: [issue-0940, issue-0963, issue-1181, phase-40, phase-403]
---

## What the two numbers are

One half of this image's sizing is DERIVED and the other half is a flat
Kconfig default, and nothing subtracts them.

**Derived.** `NROS_SUBSCRIPTION_BUFFER_SIZE`, `NROS_SUBSCRIBER_BUFFER_SIZE`,
`ZPICO_SUBSCRIBER_LARGE_SIZE` and `ZPICO_MAX_LARGE_SUBSCRIBERS` all default to
the `-1` DERIVE sentinel and are resolved from the message-bound inventory
(`zephyr/cmake/nros_cargo_build.cmake:1121`, `:753`, `:774`, `:771`;
`cmake/NanoRosMessageBounds.cmake:651`). On the MR-CANHUBK344 safety island the
take buffer derives to 1496 B, set by `std_msgs/Float64MultiArray`, against
880 B for the largest type the image actually receives
(`zephyr/Kconfig:896-897`).

**Not derived.** `NROS_FRAG_MAX_SIZE` is a plain `int` with a literal default:

```
config NROS_FRAG_MAX_SIZE
    int "Maximum reassembled message size"
    default 2048
```

`zephyr/Kconfig:867-872`. It has no `-1` sentinel, no derivation, and it is
resolved with the non-derivable `_nros_resolve_knob`
(`zephyr/cmake/nros_cargo_build.cmake:714`), which forwards it verbatim to
`ZPICO_FRAG_MAX_SIZE=` in the zpico compile definitions
(`zephyr/cmake/nros_rmw_zenoh.cmake:314`) and to `#define Z_FRAG_MAX_SIZE`
in the generated zenoh-pico config header
(`packages/rmw/zenoh/nros-zpico-build/src/lib.rs:265`).

No line in the tree compares them. `grep -n FRAG_MAX_SIZE` over
`zephyr/cmake/`, `cmake/` and `scripts/` returns the resolve, the define and
the reader-registry row (`scripts/check/check-knob-single-reader.py:117`), and
nothing else.

## What the gap costs

The drop is inside the transport and it returns SUCCESS. In
`_z_transport_unicast_handle_frag`:

```c
// Check overflow
if ((_z_wbuf_len(dbuf) + msg->_payload.len) > Z_FRAG_MAX_SIZE) {
    *dbuf_state = _Z_DBUF_STATE_OVERFLOW;
}
...
// Drop message if it exceeds the fragmentation size
if (*dbuf_state == _Z_DBUF_STATE_OVERFLOW) {
    _Z_INFO("Fragment dropped because defragmentation buffer has overflown");
    _z_wbuf_clear(dbuf);
    *dbuf_state = _Z_DBUF_STATE_NULL;
    return _Z_RES_OK;
}
```

`packages/rmw/zenoh/zpico-sys/zenoh-pico/src/transport/unicast/rx.c:210-226`
(the multicast path repeats it at `src/transport/multicast/rx.c:300-316`).
`_Z_RES_OK` is a success. The `_Z_INFO` beside it is not a diagnostic either:
it expands to `(void)(0)` unless `ZENOH_LOG_INFO` or `Z_BUILD_LOG` is defined
(`zenoh-pico/include/zenoh-pico/utils/logging.h:95-102`), and neither name
appears anywhere in this tree outside the vendored library -- the only log
define the build passes is `ZENOH_DEBUG=0`
(`packages/rmw/zenoh/nros-zpico-build/src/runner.rs:2516`). So no callback
runs, no counter moves and no error crosses the C ABI: the sample is gone with
nothing recording that it arrived. The image looks like a subscription that
never fires. The
tree already knew this -- `docs/guides/embedded-tuning.md:114` says
"limits the largest message your node can receive. Messages exceeding this are
silently dropped", and
`docs/roadmap/archived/phase-40-large-message-support.md:65-66` says
"Reassembly overflow (payload > `Z_FRAG_MAX_SIZE`) is silently dropped by the
zenoh-pico defragmentation layer" -- and neither statement was attached to a
check.

## The configuration that trips it is in our own documentation

`docs/guides/embedded-tuning.md` publishes three tuning recipes, and each pairs
a frag ceiling with a subscriber buffer BY EYE:

| recipe | `ZPICO_FRAG_MAX_SIZE` | `NROS_SUBSCRIBER_BUFFER_SIZE` | line |
| --- | --- | --- | --- |
| Minimal (Cortex-M4) | 1400 | 512 | `:277` |
| Standard (Cortex-M7) | 4096 | 1024 | `:302` |
| Large (Cortex-R52) | 16384 | 4096 | `:327` |

The Minimal recipe's 1400 is below the island's derived take buffer of 1496.
Those two numbers were chosen in different files by different people and the
build joins them without comment. The Zephyr reference conf
(`docs/guides/embedded-tuning.md:493`) states `CONFIG_NROS_FRAG_MAX_SIZE=4096`
by hand and, twelve lines below, explains that the four SIZE knobs are left at
`-1` "on purpose" so the build works them out. The one knob that governs
whether the derived sizes can be delivered at all is the one nobody derives and
nobody checks.

Nothing is on fire on the reference island today: 1496 <= 2048, with 552 bytes
of margin. The defect is that the margin is unmeasured. A type added to the
linked closure, or an MTU-driven `ZPICO_FRAG_MAX_SIZE=1400`, crosses it in
silence.

## What was fixed here, and what was not

**Fixed: a CHECK.** `nros_resolve_knobs()` now compares the image's receive
provisioning against its receive ceiling and fails the configure naming both
numbers (`zephyr/cmake/nros_cargo_build.cmake:778`). The provisioning is the
resolved payload class -- `ZPICO_SUBSCRIBER_LARGE_SIZE` when the image routes
into the large class, `NROS_SUBSCRIBER_BUFFER_SIZE` otherwise -- because those
are the RECEIVE-side facts (`cmake/NanoRosMessageBounds.cmake:608-610`: "the
three payload-class knobs size the backend's staging pools for what the image
RECEIVES"), and because a knob left on rung 4 resolves to no variable at all,
which makes "no fact" skip the check instead of inventing a number for it.

The ceiling is `max(ZPICO_FRAG_MAX_SIZE, ZPICO_BATCH_UNICAST_SIZE)`, not
`ZPICO_FRAG_MAX_SIZE` alone. zenoh-pico announces `Z_BATCH_UNICAST_SIZE` as
this peer's batch size in its INIT
(`zenoh-pico/src/protocol/definitions/transport.c:144`) and reads into a zbuf
of that size (`src/transport/common/rx.c:39`), so a sample that fits one batch
arrives whole and never enters the defragmentation buffer. Taking the larger of
the two is the direction that cannot invent a failure. On the embedded defaults
it is the frag ceiling that binds: 2048 against a 1024 batch.

**Not fixed, and on purpose: a DERIVATION.** `NROS_FRAG_MAX_SIZE` still has no
`-1` sentinel. Deriving it from the message bounds would be wrong in a way the
other four are not:

* Fragmentation is a property of the TRANSPORT and of the LINK, not of the
  message. The batch size that decides whether a given sample is fragmented at
  all is negotiated with the peer, and the useful upper end of this knob is set
  by the path MTU -- which is why the Minimal recipe reaches for 1400 and not
  for a message size.
* The buffer is a heap allocation of the whole `Z_FRAG_MAX_SIZE` taken on the
  first fragment (`_z_wbuf_make`, `rx.c:201`), out of the same platform arena
  `NROS_ZEPHYR_HEAP_SIZE` bounds. A derivation would raise that demand on every
  image whose linked closure happens to hold one large type, including images
  that subscribe to none of them -- the same closure over-statement measured on
  the island (1496 against a real 880).
* The derived bound is an UPPER bound on the closure, and the failure direction
  of an over-stated frag ceiling is wasted arena rather than a dropped sample.
  Spending it silently is what the derivation would do.

So the build now states the disagreement and leaves the number to whoever owns
the board's RAM budget, which is the same division phase-412 chose for the
POSIX mutex floor (`nros_cargo_build.cmake:515`, "a floor with a measured
constant is honest about being a bound rather than a model").

## Remaining

* The check is Zephyr-side only, because `nros_resolve_knobs()` is. The native
  lane resolves the same pair through `nros-zpico-build`'s runner
  (`packages/rmw/zenoh/nros-zpico-build/src/runner.rs:885`) and gets no
  comparison.
* The provisioning number is the closure bound whenever the entity inventory
  does not narrow it to the subscribed set, so the check can refuse a build
  that would have worked. The escape is the one the message names: state
  `CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE` / `CONFIG_NROS_SUBSCRIBER_LARGE_SIZE`,
  which wins over the derivation. Narrowing it properly is issue 0963's
  subscribed-basis join, not this one.
* Nothing checks the SEND direction. `ZPICO_BATCH_UNICAST_SIZE` against the
  largest type the image PUBLISHES is the same subtraction, and
  `DEFAULT_TX_BUF` aliases the take buffer, so the fact is already derived.
