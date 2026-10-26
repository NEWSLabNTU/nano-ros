---
id: 2078
title: "A serial-only Zephyr Rust image does not link: the entry calls nros_platform_zephyr_wait_network, built only with CONFIG_NET_SOCKETS"
status: open
type: bug
area: [zephyr, build]
severity: medium
found: 2026-10-26
related: [phase-512, 2065]
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

## Closes when

A Zephyr Rust image with no IP stack links and boots: the symbol is always
defined (a no-op without sockets), or the Rust entry calls it only when the
image has a network interface. A serial-only Rust Zephyr build (a native_sim
or QEMU fixture is enough) holds it.
