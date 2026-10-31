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

## Re-read against main, 2026-10-31

`nros-board-mps2-an385-zephyr` is a **Scaffold — not supported** row in
`book/src/reference/board-support-tiers.md`: a C-only board package
(`nros-board.toml`, `prj.conf`, `package.xml`, no crate), tier unassigned,
built by no fixture lane under that name. The chapter's "Run on QEMU
(mps2-an385)" section therefore walks an installed user onto a scaffold
board and tells them to state its network by hand. The supported Cortex-M
Zephyr path is `nros-board-zephyr`'s `ZephyrQemuCortexM` descriptor
(**Tier 2**: C, C++ and Rust pubsub at Runtime), so the first question is
which board the chapter should name, and only then how that board supplies
its network.

## Fix direction

* Decide the board the chapter names. If it stays `mps2-an385-zephyr`, the
  board has to leave Scaffold first: supply the image's QEMU network the way
  the NuttX and FreeRTOS QEMU boards do (an RFC-0109 board network), so the
  chapter has nothing for the reader to state. If the chapter moves to the
  Tier 2 descriptor, the network question is asked of that board.
* Until then, mark the section as unmeasured on the installed track, in the
  chapter, so a reader knows the `10.0.2.15` / `10.0.2.2` advice is written
  and not replayed.
* Then make the block a probe variant on the `zephyr` track, and boot it.
