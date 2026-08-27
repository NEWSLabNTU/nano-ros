---
id: 832
title: "`nros_platform_alloc` is DEFINED but UNREFERENCED in the cyclonedds and
  xrce native images — the vendor allocators bypass the funnel"
status: open
type: bug
area: platform, rmw
related: [phase-391, issue-0817]
---

## What was measured

Three fully-linked native executables, same example, one per backend
(`examples/native/c/talker/build-<rmw>/c_talker`, x86-64, built 2026-08-27):

| backend | `nros_platform_alloc` | inbound edges | what allocates instead |
| --- | --- | --- | --- |
| zenoh | `T` (defined) | **1** — `z_malloc` | — (funnel reached) |
| cyclonedds | `T` (defined) | **0** | `ddsrt_malloc`, `ddsrt_malloc_s`, `ddsi_config_init`, `network_interface_find_or_append` |
| xrce | `T` (defined) | **0** | `get_ip_from_iface` |

All four bypassing functions tail-call `malloc@plt` directly.

The zenoh chain, from disassembly:

```
z_malloc:             jmp  nros_platform_alloc
nros_platform_alloc:  test %rdi,%rdi ; je ... ; jmp malloc@plt
```

## The wording that needs correcting

This was first reported as the funnel being "undefined in the cyclonedds and
xrce images". It is not. It is **defined in all three** — the platform layer IS
linked and exports it, and it forwards to libc `malloc` — and **unreferenced in
two of them**. The distinction is the whole finding: "the platform layer is not
linked on hosted images" is a hypothesis this measurement DISPROVES, so the
bypass cannot be explained away as expected-for-hosted.

The bypass is in VENDOR code. Cyclone's `ddsrt` and XRCE's transport call libc
directly; neither was ever routed through the funnel. Issue 0817 fixed sixteen
such sites in the Zephyr port; these are the native equivalents and were not in
that sweep.

## Why this matters to phase 391 W4

The tier model promises `unified` = "allocation symbols only inside
`nros_platform_*` backend objects". On these two images that is already false,
and — the part that bites — **a gate keying on the funnel symbol being PRESENT
would pass all three**, because the symbol is present in all three. It has to
key on the vendor allocators being ABSENT, or on the funnel having inbound
edges, or it is precisely the campaign's named trap: a gate that passes because
nothing exercises it.

## Not established

* **The embedded/ARM case.** The Zephyr images in this tree
  (`zephyr-workspace/build-*`) are native_sim x86-64 RELOCATABLE objects, where
  call-site analysis is unreliable because relocations are unresolved. There is
  no ARM cyclonedds or xrce image here to check, so whether the bypass survives
  on a target without libc `malloc` is UNKNOWN and should not be assumed either
  way.
* **Whether routing them through the funnel changes behaviour on hosted
  images.** It would not today — `nros_platform_alloc` forwards to `malloc`
  there — so the fix is about the tier promise and the embedded case, not about
  native behaviour.

## Method note

Two false readings were produced and discarded while measuring this, both worth
repeating for the next person:

* Grepping disassembly for `call <nros_platform_alloc>` reports ZERO on the
  zenoh image, which reads as "bypassed everywhere". The funnel is reached by a
  TAIL-CALL (`jmp`), which the phase doc itself notes. Count both.
* `nm` on a Zephyr ELF here returns 0 symbols — the file is read but not
  understood. Use `llvm-nm` (rustup's llvm-tools) and check the symbol COUNT
  before believing any absence.
