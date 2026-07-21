---
id: 244
title: "Platform ABI surface asymmetry: PlatformSerial/PlatformIvc are Rust-trait-only (no C header mirror) unlike net/timer; zpico adds a second clock surface beside nros_platform_clock_ms"
status: open
type: tech-debt
severity: low
area: platform
related: [rfc-0034, issue-0239]
---

## Finding (RMW/platform API audit, 2026-07-21)

Two smaller shape inconsistencies in the platform layer:

1. **Serial / IVC are Rust-only.** `PlatformSerial`
   (`nros-platform-api/src/lib.rs:938`) and `PlatformIvc` (`:860`, Tegra
   IVC mailbox) exist as Rust traits with NO C-ABI mirror in the
   `nros_platform_*` headers, unlike clock/alloc/net/timer/threading which
   have both a C header symbol set AND a Rust trait. `PlatformLibc`
   (`:719`) is documentary-only (never dispatched; resolved at link from
   `nros-baremetal-common`). Result: a C backend cannot provide serial/IVC
   through the canonical ABI — asymmetric with the rest of the surface.
   Decide: mirror serial/IVC to C (consistency), or document why they are
   Rust-only (e.g. only ever consumed by Rust boards).

2. **zpico second clock surface.** `zpico-sys/src/platform_smoltcp.rs`
   adds `smoltcp_set_clock_ms` / `smoltcp_clock_now_ms` — an externally-fed
   clock beside the canonical `nros_platform_clock_ms`. Everything else in
   zpico is a thin adapter over `nros_platform_*`
   (`zpico/zpico-sys/c/zpico/platform_aliases.c` maps `_z_*`/`z_*` →
   `nros_platform_*`), so this is the one genuine parallel primitive.
   Confirm it is required (the bare-metal smoltcp stack needs an
   externally-driven tick that `nros_platform_clock_ms` can't provide) and
   note it, or fold it onto the canonical clock.

## Direction
Both are "decide + document or normalize", not urgent. Grouped because
they are the same class — the platform surface should be uniformly shaped
(C ABI + Rust trait) or its exceptions explicitly recorded, the way the
RMW audit expects for its vtable.
