---
id: 2116
title: "nros-board-esp32-qemu does not build with its `serial` transport"
status: resolved
type: bug
area: [boards, esp32, build]
severity: medium
found: 2026-10-27
related: [issue-2047]
resolved: 2026-10-28
---

## What

`nros-board-esp32-qemu` offers two mutually exclusive transports, `ethernet`
(default) and `serial`. With `--no-default-features --features serial` it did
not compile:

```
error[E0609]: no field `mac_addr` on type `&config::Config`
  --> src/node.rs:27:24
error[E0609]: no field `ip` on type `&config::Config`
```

With `rmw-zenoh` on too, `board_entry.rs` failed the same way on `ip`,
`prefix`, `gateway` and `mac_addr`, and `-D warnings` then refused an unused
`DEFAULT_LOCATOR`.

## Why

`1afa8e6b6d` ("stabilize qemu zenoh endpoints") added
`network_identity_seed`, which folds the MAC and IP into the RNG seed, with no
`cfg(feature = "ethernet")`; the deploy overlay in `board_entry.rs` and the
router default read the same ethernet-only facts. Nothing noticed because
**nothing builds the `serial` variant**: every example and fixture takes the
default feature set, which carries `ethernet`, and the crate is in the root
`exclude` list. The same unlinted build also hid a
`clippy::manual_is_multiple_of` in `network.rs` on the ethernet side.

Found while migrating the board for issue 2047.

## Resolution

- The seed has a serial arm. A serial image has no MAC or IP, and one UART is
  one peer, so the hardware RNG alone seeds it.
- The overlay's interface block and `DEFAULT_LOCATOR` are gated to `ethernet`.
- `network.rs` uses `is_multiple_of`.

The class is closed by a gate, `check-board-transport-variants` on the build
lane. It derives every board crate offering both transports
(`nros-board-esp32-qemu`, `nros-board-mps2-an385`), reads each one's triple
from its `nros-board.toml`, and runs clippy `-D warnings` on the default
feature set with each transport in turn. That is 4 variants, ~37 s cold, in a
target dir per board (issue 0616). Mutation-checked: with main's `node.rs`
restored, the gate fails naming `nros-board-esp32-qemu [rmw-zenoh,serial]`.
