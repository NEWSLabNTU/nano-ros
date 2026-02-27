# Phase 60: std/alloc Feature Consistency

**Goal**: Ensure std/alloc features propagate correctly through all RMW backends and document C-level allocation behavior.

**Status**: In Progress (60.1–60.4 done)

**Priority**: Medium

**Depends on**: None

## Overview

An audit of std/alloc feature propagation revealed inconsistencies in the XRCE-DDS backend. While the Zenoh backend (`nros-rmw-zenoh` / `zpico-sys`) correctly defines and propagates `std`/`alloc` features through the entire crate chain, the XRCE backend (`nros-rmw-xrce` / `xrce-sys`) has no `std`/`alloc` features at all, breaking the feature chain.

Additionally, POSIX transport modules in `nros-rmw-xrce` use `std` types where `core` equivalents exist, and the crate's `no_std` attribute is coarsely gated on transport features rather than being unconditional.

Both C backends (zenoh-pico, Micro-XRCE-DDS) always require heap allocation at the C level (via platform-specific allocators like `malloc`, `k_malloc`, `pvPortMalloc`), independent of Rust's `alloc` feature. This is undocumented.

**Design principle**: std/alloc must never be implicitly implied by any feature. If a feature requires std, let the compile error surface naturally rather than silently enabling std.

### Comparison: Zenoh vs XRCE Feature Chains

**Zenoh (correct)**:
```
nros          std = [..., "nros-rmw-zenoh?/std"]
  └─ nros-node   std = [..., "nros-rmw-zenoh?/std"]
       └─ nros-rmw-zenoh  std = ["alloc", "zpico-sys/std", "nros-rmw/std", "log"]
            └─ zpico-sys       std = []  (gates `extern crate std` in lib.rs)
```

**XRCE (broken)**:
```
nros          std = [...]  ← nros-rmw-xrce?/std MISSING
  └─ nros-node   std = [...]  ← nros-rmw-xrce?/std MISSING
       └─ nros-rmw-xrce  ← NO std/alloc features defined
            └─ xrce-sys       std = []  (defined but never activated)
```

## Work Items

- [x] 60.1 — Add std/alloc features to nros-rmw-xrce
- [x] 60.2 — Propagate std/alloc to nros-rmw-xrce from nros and nros-node
- [x] 60.3 — Fix nros-rmw-xrce lib.rs no_std attribute
- [x] 60.4 — Rewrite posix_udp.rs to use libc
- [ ] 60.5 — Fix posix_serial.rs trivial std deps
- [ ] 60.6 — Document C-level allocation in std-alloc-requirements.md
- [ ] 60.7 — Clean up unused std features in zpico-sys and xrce-sys

### 60.1 — Add std/alloc features to nros-rmw-xrce

Add `std` and `alloc` features to `nros-rmw-xrce/Cargo.toml`, mirroring the pattern established by `nros-rmw-zenoh`.

```toml
std = ["alloc", "nros-rmw/std", "xrce-sys/std"]
alloc = ["nros-rmw/alloc"]
```

**Status**: Done

**Files**:
- `packages/xrce/nros-rmw-xrce/Cargo.toml` — add `std` and `alloc` feature entries

### 60.2 — Propagate std/alloc to nros-rmw-xrce from nros and nros-node

Add `nros-rmw-xrce?/std` and `nros-rmw-xrce?/alloc` to the `std`/`alloc` feature lists in `nros` and `nros-node`, matching how `nros-rmw-zenoh?/std` is already propagated.

**Status**: Done

**Files**:
- `packages/core/nros/Cargo.toml` — add `nros-rmw-xrce?/std` to `std`, `nros-rmw-xrce?/alloc` to `alloc`
- `packages/core/nros-node/Cargo.toml` — add `nros-rmw-xrce?/std` to `std`, `nros-rmw-xrce?/alloc` to `alloc`

### 60.3 — Fix nros-rmw-xrce lib.rs no_std attribute

The current gate:
```rust
#![cfg_attr(not(any(feature = "posix-udp", feature = "posix-serial")), no_std)]
```

This makes the entire crate `std`-dependent when any POSIX transport is enabled, even though `lib.rs` itself only uses `core::ffi` and `core::sync::atomic`.

Replace with unconditional `#![no_std]` plus feature-gated `extern crate`:
```rust
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;
```

The POSIX transport modules (`posix_udp`, `posix_serial`) are already feature-gated behind `#[cfg(feature = "posix-udp")]` / `#[cfg(feature = "posix-serial")]`, so they will naturally require `std` to compile. Since `posix-udp` implies `platform-posix` and POSIX platforms always use `std`, the compile error is the correct enforcement mechanism.

**Status**: Done

**Files**:
- `packages/xrce/nros-rmw-xrce/src/lib.rs` — replace `cfg_attr` with unconditional `#![no_std]` + feature-gated extern crates

### 60.4 — Rewrite posix_udp.rs to use libc

Replace `std` dependencies with `core` or `libc` equivalents where possible:

| Current | Replacement |
|---------|-------------|
| `std::ffi::c_int` | `core::ffi::c_int` |
| `std::time::Duration` | `core::time::Duration` |
| `std::net::UdpSocket` | `libc` syscalls (`socket`, `bind`, `sendto`, `recvfrom`, `setsockopt`) |
| `std::io::ErrorKind` | `libc` errno constants (`EWOULDBLOCK`, `EAGAIN`, `ETIMEDOUT`) |
| `eprintln!()` | Gate behind `#[cfg(feature = "std")]` or remove |

This is the largest work item. The `UdpSocket` replacement requires writing raw POSIX socket code using `libc`, similar to how `posix_serial.rs` already uses `libc` for serial port operations.

**Status**: Done

**Files**:
- `packages/xrce/nros-rmw-xrce/src/posix_udp.rs` — rewrite to use `libc` syscalls
- `packages/xrce/nros-rmw-xrce/Cargo.toml` — add `dep:libc` to `posix-udp` feature (if not already present)

### 60.5 — Fix posix_serial.rs trivial std deps

Two trivial fixes:
- `use std::ffi::c_int` → `use core::ffi::c_int`
- `eprintln!()` calls: gate behind `#[cfg(feature = "std")]` or remove

The rest of `posix_serial.rs` already uses `libc` directly.

**Status**: Pending

**Files**:
- `packages/xrce/nros-rmw-xrce/src/posix_serial.rs` — fix imports and gated prints

### 60.6 — Document C-level allocation in std-alloc-requirements.md

Add a section to the existing std-alloc reference document explaining that:

1. Both C backends (zenoh-pico, Micro-XRCE-DDS) perform heap allocation at the C level using platform-specific allocators (`malloc`, `k_malloc`, `pvPortMalloc`, `tx_byte_allocate`)
2. This is independent of Rust's `alloc` feature — disabling `alloc` in Rust does not eliminate heap allocation from the C transport layer
3. The `alloc` feature controls only Rust-side heap usage (e.g., `Box`, `Vec`, `String`)
4. Add the missing XRCE and Zenoh backend crates to the std/alloc requirements tables

**Status**: Pending

**Files**:
- `docs/reference/std-alloc-requirements.md` — add C-level allocation section and backend crate tables

### 60.7 — Clean up unused std features in zpico-sys and xrce-sys

Both `-sys` crates define `std = []` (empty feature). Evaluate each:

- **`zpico-sys`**: The `std` feature is used — `lib.rs` gates `extern crate std` on it. No change needed.
- **`xrce-sys`**: The `std` feature is defined but `lib.rs` is unconditionally `#![no_std]` with no `extern crate std` anywhere. Either:
  - Add `#[cfg(feature = "std")] extern crate std;` to `xrce-sys/src/lib.rs` for consistency, or
  - Remove the `std = []` feature if it serves no purpose

**Status**: Pending

**Files**:
- `packages/xrce/xrce-sys/Cargo.toml` — potentially remove `std = []`
- `packages/xrce/xrce-sys/src/lib.rs` — potentially add `extern crate std` gate

## Acceptance Criteria

- [ ] `nros-rmw-xrce` has `std` and `alloc` features that forward to `nros-rmw` and `xrce-sys`
- [ ] `nros` and `nros-node` forward `std`/`alloc` to `nros-rmw-xrce?/std` / `nros-rmw-xrce?/alloc`
- [ ] `nros-rmw-xrce/src/lib.rs` uses unconditional `#![no_std]`
- [ ] `posix_udp.rs` compiles without `std` (uses `libc` + `core` only)
- [ ] `posix_serial.rs` has no `std::` imports (uses `core::` + `libc` only)
- [ ] `cargo build -p nros-rmw-xrce --no-default-features` succeeds without `std`
- [ ] `cargo build -p nros-rmw-xrce --no-default-features --features posix-udp` succeeds (std provided by libc)
- [ ] `docs/reference/std-alloc-requirements.md` documents C-level allocation behavior and includes all backend crates
- [ ] `just quality` passes

## Notes

- The POSIX transport modules (`posix_udp`, `posix_serial`) inherently need a POSIX environment, but they should use `libc` directly rather than going through `std`. This keeps the crate `no_std`-clean while still supporting POSIX transports.
- `posix_serial.rs` already follows this pattern (uses `libc` for termios, open, read, write). `posix_udp.rs` is the outlier using `std::net::UdpSocket`.
- After 60.4, enabling `posix-udp` without `std` will compile successfully because the module uses `libc` directly. The `std` feature becomes purely about Rust-side standard library access (error formatting, stdio, etc.), not about transport functionality.
