---
id: 2177
title: "The mps2-an385-zephyr image's network is the reader's to state, and nobody has measured the chapter's advice"
status: open
type: test-gap
severity: low
area: [zephyr, docs, testing]
related: [phase-517]
found: 2026-10-31
---

**Home: [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W9's **Left**.

## What

`integration-zephyr.md` "Run on QEMU (mps2-an385)" has the reader build an
`[image.*] board = "mps2-an385-zephyr"` and boot it with
`west build -t run`. The chapter then says to state the image's address
(`10.0.2.15`, gateway `10.0.2.2`) and `locator = "tcp/10.0.2.2:<port>"`
themselves. The in-tree fixture lanes get the same network from
`packages/boards/nros-board-mps2-an385-zephyr/prj.conf`, which an installed
user's image does not read. The advice is written, not measured: no probe
track builds or boots this board, and the block carries no probe tags.

## Fix direction

* Have the board package supply the image's QEMU network the way it does for
  the fixture lanes (an RFC-0109 board network, like the NuttX and FreeRTOS
  QEMU boards), so the chapter has nothing for the reader to state.
* Then make the chapter's block a probe variant on the `zephyr` track, and
  boot it.
