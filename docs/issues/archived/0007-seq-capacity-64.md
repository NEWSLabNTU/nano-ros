---
id: 7
title: Unbounded message sequences capped at 64 elements
status: resolved
type: enhancement
area: codegen
related: [rfc-0033, phase-229, issue-0008, issue-0021]
resolved_in: Phase 229
---

**Resolved by RFC-0033 / Phase 229.** The original limitation — a hardcoded
64-element sequence cap with no override, making large sensor messages
(`Image`, `PointCloud2`, `LaserScan`, `OccupancyGrid`) unusable on embedded —
is gone. Capacity is now per-field configurable via `nros-codegen.toml`
(`CapacityResolver`), in three storage modes:

- **`owned`** (all 3 langs) — `heapless::Vec<T, N>` with the resolved `N`; 64 is
  now only the fallback when neither config nor a `.msg` bound applies.
- **`heap`** (all 3 langs, 229.5) — `alloc`-backed growable containers
  (`nros_core::heap::{Vec,String}`, `nros::HeapSequence`/`HeapString`,
  rclc-style malloc'd C structs). Unbounded; large payloads carry no inline
  stack/struct cost. Makes large messages usable on any allocator target.
- **`borrowed`** (Rust, 229.6) — zero-copy `&'a [u8]` / `&'a str` slices into
  the CDR receive buffer, plus `nros_core::LeSliceView<'a, T>` for multi-byte
  numerics (alignment-agnostic). The only mode that fits large payloads on an
  **allocator-free** MCU. Emits `{Msg}View<'a>` + a `{Msg}Borrow` marker;
  subscribe via `node.create_subscription_borrowed::<{Msg}Borrow, _>()`. Runtime
  seam `670a62a4`, codegen `5097a7a7`, alignment guard `40e5c97e`, E2E
  `aeed3d4d`.

**Coverage at close:** large payloads are representable on every target —
allocator targets via `heap` (all 3 langs), allocator-free targets via
`borrowed` (Rust). The remaining gap is **borrowed zero-copy for C/C++**
(an alloc-free optimization, since C/C++ already have `heap`), tracked as
**[issue 0021](../0021-cpp-c-borrowed-views.md)**. The deeper single-copy
receive-path work is [issue 0008](../0008-two-copy-receive.md).
