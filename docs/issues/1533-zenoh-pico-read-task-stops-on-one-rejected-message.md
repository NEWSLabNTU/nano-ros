---
id: 1533
title: "zenoh-pico's read task stops for good when the session layer rejects one received message"
status: open
area: zenoh, zephyr, serial
severity: high
phases: []
rfcs: []
related: [0852, 0821, 1534]
---

# zenoh-pico's read task stops for good when the session layer rejects one received message

Found by the safety island (simple-autoware-safety-island, phase8-W2) on an
MR-CANHUBK344 (S32K344) talking to `rmw_zenohd` over the DCD-LZ UART. Its
brief B had recorded the symptom without a cause: host samples stopped
reaching the board about a second after the first spin, and "the router
expired the session about 21 s after it opened, in 3 of 3 runs with host
traffic".

## The mechanism

`_z_unicast_handle_frame` walks the network messages of a FRAME and returned
the first error `_z_handle_network_message` gave it. `_z_unicast_process_messages`
returned it too, and `_zp_unicast_read_task` answered any error with
`_read_task_running = false` and returned: the read thread exited. Nothing closed
the session. The lease task went on sending keepalives and data, so the router
saw a live peer, while the board received nothing until its OWN lease expired
two periods later (`_z_unicast_lease_task`, 2 x 10 s), when it sent CLOSE and
reconnected. That is the 21 s: 20 s of lease plus the lease task's phase.

The session layer returns errors that are verdicts on ONE message:

- `_Z_ERR_KEYEXPR_UNKNOWN`: a declaration or a sample names a key id whose
  `D_KEYEXPR` arrived in a frame the link lost;
- `_Z_ERR_ENTITY_UNKNOWN` / `_Z_ERR_MESSAGE_ZENOH_DECLARATION_UNKNOWN`: an
  undeclaration for an id this session never saw;
- `_Z_ERR_SYSTEM_OUT_OF_MEMORY` for one sample.

Upstream eclipse-zenoh/zenoh-pico has the same frame handler; its read task
closes the session on the error instead of going silent, which is visible but
still drops the link for one message.

## Measured

S32K344 at 115200 baud, a socat tap on the wire, and SWD reads of the
transport while it runs. Both triggers, each on the island image with nothing
else changed:

1. Three `ros2 topic pub` nodes leave. `rmw_zenohd` sends `U_TOKEN` for each
   token twice: once by the id it declared to the board (17-28), once by ids it
   never declared to it (29-38, each carrying the key in the wire-expr
   extension). The board removed four tokens from its graph cache (15 -> 11),
   hit `U_TOKEN 29`, and `_read_task_running` read 0 from then on; 181 bytes sat
   in the RX ring unread and every subscription's ring tail froze.
2. The board joins while host nodes exist. The router answers the liveliness
   subscriber's interest with 2.5 KB of `D_KEYEXPR` + `D_TOKEN` in three
   back-to-back frames; the RX ring (1 KiB) overflowed while the read task was
   starved (issue 1534), a frame was lost, and the next `D_TOKEN` named a key id
   declared in it. The read task stopped 1-2 s after the first spin; the board
   closed the session 21.18 s after it opened (router log, board CLOSE on the
   tap). This is brief B's run shape exactly: 3 samples, then none.

## Fix (prepared on the fork, not yet pinned)

The change below is commit `52f60b79` on branch
`fix/serial-reader-survives-declare-errors` of the zenoh-pico fork, based on the
fork's `nano-ros` branch (tree-equal to the pinned `dd071b8d`). It is committed
locally and NOT pushed: CLAUDE.md leaves fork pushes to the maintainer. To land
it: push that branch to `jerry73204/zenoh-pico`, then move the pin in
`packages/rmw/zenoh/zpico-sys/zenoh-pico` to it and mark this issue resolved.
It compiles clean on the Linux build of zenoh-pico (no warnings); the Zephyr
serial counters have not been built on a board yet.

`_z_handle_network_message_in_frame` (transport/common/rx.c) wraps the call for
all four sites (unicast and multicast, frame and defragmented fragment): an
error other than connection- or session-closed rejects that message, is counted
in `_z_rx_rejections` (count, last error; readable over a debugger), and the
batch continues. Decode errors stay fatal in the callers, because after one the
position in the batch is unknown. The fragment paths already discarded the
handler's result; the frame paths now match them.

With it, a lost frame costs the messages in it and nothing more, and a
duplicate undeclaration costs a counter increment.

The same change adds `_z_zephyr_serial_stats` to the Zephyr serial link (bytes
and frames each way, bad frames, overruns, RX ring overflows and high water,
and the cycles spent in the busy-wait transmit), and nano-ros gains
`CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES` for the ring that was a bare
`#ifndef`.

Mitigation that needs no firmware, for a router in front of a serial client:
an ACL denying `liveliness_token` on egress to `link_protocols: ["serial"]`
keeps both the history burst and the undeclarations off the link (measured:
zero `D_TOKEN`/`U_TOKEN` toward the board, the host still lists the board's
nodes, and the read task survived a transient `ros2 node list`).
