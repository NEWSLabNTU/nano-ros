---
id: 2575
title: "zenoh-pico `TX_SPLIT_LOCK`: a fragmented send writes the link without the link lock, so its frames and a detached batch send's are unordered"
status: open
type: bug
area: [rmw, zenoh, zephyr]
severity: low
found: 2026-10-11
related: [rfc-0110, 2001, 0213]
---

GitHub: NEWSLabNTU/nano-ros#1951

**Standalone.** No active phase owns the zenoh-pico TX path; the patch goes on
the zenoh-pico fork's patch line (`packages/rmw/zenoh/zpico-sys/zenoh-pico`).
RFC-0110 is the executor-driving design the split lock belongs to.

## What happens

With `Z_FEATURE_TX_SPLIT_LOCK=1` (Zephyr `CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y`,
used by the safety island's S32K344 image together with `TX_BATCH`), two
senders can write the same link at once:

- the fragment path, `_z_transport_tx_send_fragment_inner`
  (`src/transport/common/tx.c:98` and `:104`), calls `_z_link_send_wbuf`
  holding `_mutex_tx` but NOT the link mutex (`_z_transport_link_tx_lock`);
- a detached batch send (`tx.c` ~400-460, the overflow steal / flush of
  `_wbuf_spare`) holds only the link mutex.

`_z_transport_tx_flush_buffer` documents the intent ("one link-mutex hold
covers both, so nothing interleaves between them"), but the fragment path is
outside it. Still the shape of the pinned submodule on 2026-11-15.

Found while reviewing the Zephyr serial TX path for phase-474 I12 (PR #1946).

## Consequence

- Before PR #1946, the Zephyr serial send streamed a frame into a 256 B ring
  and could sleep mid-frame, so two senders could interleave BYTES of two COBS
  frames (two corrupt frames).
- After it, each serial frame goes into the TX ring under a per-link
  `k_mutex`, so a frame is atomic, but the ORDER of frames between the two
  paths is undefined. On a reliable channel the receiver expects sequence
  numbers in order; a fragment overtaking an older parked batch (or the
  reverse) can cost a message or a retransmit, depending on the peer.
- Other stream links (TCP) depend on the socket's own per-write atomicity; the
  ordering question is the same.

Only messages larger than one batch take the fragment path, so a link that
never fragments outbound is not affected in practice. The island's outbound
messages are small; not observed on the board.

## Suggested fix

Take the link mutex around the fragment loop's `_z_link_send_wbuf` calls,
after draining any pending spare as `_z_transport_tx_flush_buffer` does, so
every link write under split lock goes through the same lock and parked
batches (older SNs) always precede the fragments.

## How to verify

A host test with split lock on: one publisher overflows a batch (parks a
spare) while another sends a message larger than the batch; assert the link
sees the spare's frames before the fragments, with no interleaving.
