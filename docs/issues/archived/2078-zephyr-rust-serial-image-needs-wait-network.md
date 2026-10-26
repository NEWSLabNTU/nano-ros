---
id: 2078
title: "A serial-only Zephyr Rust image does not link: the entry calls nros_platform_zephyr_wait_network, built only with CONFIG_NET_SOCKETS"
status: resolved
type: bug
area: [zephyr, build]
severity: medium
found: 2026-10-26
related: [phase-512, 2065]
resolved: 2026-10-26
resolved_in: "PR #2692 (2026-10-26)"
---

## Measured

The phase-512 H9 image: `realtime-rust` `derived_bringup` for
`mr_canhubk3/s32k344` with the zenoh serial locator and no IP stack, on
nano-ros 9f2d7b4bb3. The link failed with:

```
undefined reference to `nros_platform_zephyr_wait_network'
```

The Rust Zephyr entry calls `nros_platform_zephyr_wait_network`
unconditionally. `net_wait.c`, which defines it, is compiled only with
`CONFIG_NET_SOCKETS`. A C entry on the same board links, because the C
examples guard the call.

It was worked around locally, not committed, with a stub returning 0 under
`!CONFIG_NET_SOCKETS`. The diff is in the batch archive
(`rust-frame/local.diff`).

## Resolution

The symbol is defined in every Zephyr image. Without `CONFIG_NET_SOCKETS`
`zephyr/CMakeLists.txt` compiles
`packages/platform/nros-platform-zephyr/src/net_wait_none.c`, which returns 0 at
once: with no IP stack there is nothing to wait for. `net_wait.c` stays
sockets-only. The C header (`nros/platform_zephyr.h`) and the C++ guide say so,
so a C/C++ caller needs no guard either. The ABI mirror is unchanged (the
symbol stays on issue 1208's out-of-surface list).

Verified on the MR-CANHUBK344 on 2026-10-26: the batch-A `realtime-rust`
`derived_bringup` serial image (no IP stack) links with no stub and no
boot-config fragment, boots, and its nodes and topics show in `ros2 node list` /
`ros2 topic list` through the serial gateway.

Held by a new west fixture row, `build-cortex-m-rust-talker-serial-zenoh`
(`examples/zephyr/rust/talker` on mps2/an385 with
`cmake/zephyr/mps2-an385-serial.conf`), whose build fails with "undefined
reference to `nros_platform_zephyr_wait_network'" without the fix, and by
`tests/zephyr_link_layout.rs::serial_only_rust_image_links_without_ip_stack`,
which also requires the image to carry no IP stack. The row lists the board
tail itself so the serial delta merges after it;
`scripts/build/zephyr-fixture-leaves.sh` does not append a tail a row already
names.

## Closes when

A Zephyr Rust image with no IP stack links and boots: the symbol is always
defined (a no-op without sockets), or the Rust entry calls it only when the
image has a network interface. A serial-only Rust Zephyr build (a native_sim
or QEMU fixture is enough) holds it.
