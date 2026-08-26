---
id: 817
title: "Sixteen allocation sites in the Zephyr port called `k_malloc` directly
  instead of the platform funnel"
status: resolved
type: bug
area: platform
resolved_in: 5de4326b3
related: [issue-0811, issue-0816, phase-391]
---

## What was wrong

RFC-0034 D6 gives nano-ros one allocation funnel, `nros_platform_alloc`.
Sixteen sites in `packages/platform/nros-platform-zephyr/src/` did not use it:

| file | sites |
| --- | --- |
| `net.c` | the bound local sockaddr and its `zsock_addrinfo`, plus three frees |
| `timer.c` | the timer handle and its free |
| `platform.c` | the `k_mutex`, `k_condvar` and task control blocks, plus their frees |

On Zephyr both routes reach `_system_heap`, so this looked cosmetic.

## Why it was not cosmetic

The funnel is what lets the arena's ALGORITHM be replaced — a constant-time
allocator for the real-time tier — by editing one function instead of hunting
every allocation site in the tree. A direct `k_malloc` keeps allocating from
the kernel heap after the funnel has moved, silently splitting the single arena
D6 exists to keep whole.

## Resolution

All sixteen routed through `nros_platform_alloc` / `nros_platform_dealloc`. The
backend itself is deliberately unchanged: `nros_platform_alloc` / `_realloc` /
`_dealloc` in `platform.c` still call `k_malloc` / `k_free`, and are the one
place that should.

Verified on mr_canhubk3/s32k344 by disassembly: `z_malloc` tail-calls
(`b.w`, not `bl`) into `nros_platform_alloc`, so zenoh-pico's 42 allocation
sites and the Rust `#[global_allocator]` share one funnel with no
direct-to-kernel exceptions. Image size is unchanged because `net.c` and
`timer.c` are garbage-collected in the serial-transport configuration; the fix
is latent there and load-bearing for TCP/UDP builds.

## What it left behind

- A source grep found these; it cannot see vendored C. The durable check is a
  link-time symbol gate — [issue 0816](../0816-no-alloc-claimed-but-unenforced.md).
- `net.c` still frees `ep->iptcp` with `zsock_freeaddrinfo` regardless of which
  allocator produced it — [issue 0811](../0811-zephyr-net-iptcp-allocator-provenance-mismatch.md).
  Benign while both routes bottom out in `k_malloc`; not benign after the
  funnel moves.
