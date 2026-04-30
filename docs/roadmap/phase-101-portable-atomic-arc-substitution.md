# Phase 101 — `portable-atomic-util::Arc` substitution for CAS-poor targets

**Goal:** unblock dust-dds (and any future stdlib-`Arc`-using crate) on RISC-V `imc`
targets — primarily ESP32-C3 (`riscv32imc`) — by substituting `alloc::sync::Arc` /
`Weak` with the `portable-atomic-util` polyfill at the dependency level. Closes the
last open Phase 97 slice (`97.4.esp32-qemu`) without forking the regex stack.

**Status:** Not Started.
**Priority:** Low — esp32-qemu DDS is bonus coverage; 6 of 7 Phase 97 slices already
green. Drives forward only if (a) ESP32-C3 DDS becomes a user request, or (b) we
adopt another `riscv32imc`-class target where the same `Arc` gating bites.
**Depends on:** Phase 97 tracing-optional fork (`jerry73204/dust-dds` branch
`nano-ros/phase-97-tracing-optional`, commit `8dd8f542`) — already pushed.

## Background

`alloc::sync::Arc` and `alloc::sync::Weak` are gated in stdlib behind
`#[cfg(target_has_atomic = "ptr")]`. On `riscv32imc` (no RISC-V `A` extension,
no native pointer-CAS) the predicate evaluates false at toolchain build time and
the `alloc::sync` module **does not exist**. This is a target query, not a Cargo
feature flag — `-Z build-std`, libcall stubs, and `portable-atomic` cfg knobs
cannot re-expose it. The only fix is **source-level type substitution**.

This bites three places in our stack:

1. **dust-dds** — `Arc<[u8]>` for RTPS submessage buffers
   (`rtps_messages/submessage_elements.rs`), `Arc<Mutex<HandleInner>>` in
   `std_runtime/timer.rs`. Direct blocker for Phase 97.4.esp32-qemu.
2. **`regex` 1.x** (transitively `regex-automata`) — pulled in by dust-dds for
   partition-QoS fnmatch matching. `regex-automata` uses `alloc::sync::Arc`
   *and* native `compare_exchange` on `AtomicPtr` / `AtomicBool`.
3. **Any future no_std embedded crate that pulls a stdlib-`Arc`-using dep.**

`portable-atomic-util` (sibling of the well-known `portable-atomic` crate)
provides `Arc` / `Weak` clones backed by `portable-atomic` atomics. On targets
with native CAS the `portable-atomic` crate forwards to `core::sync::atomic`,
giving zero overhead vs. stdlib `Arc`. On CAS-poor targets it polyfills via
either critical-section (preferred — we already have `critical-section = "1.2"`
wired through `nros-platform-*`) or single-core IRQ-disable
(`--cfg=portable_atomic_unsafe_assume_single_core`).

## Design

### Why upstream-style substitution beats a project-local shim

Two ways to substitute:

**Option A (project-local shim):** `nros-platform-api::sync` re-exports the
right `Arc` per target; dust-dds patched to import from there. Couples dust-dds
to nros — ugly, blocks upstreaming our fork.

**Option B (upstream-friendly):** dust-dds patched to import
`portable_atomic_util::Arc` directly. New direct dep on `portable-atomic-util`,
gated behind a Cargo feature so the std/POSIX path stays unchanged. PR-able
back to upstream `s2e-systems/dust-dds` as an embedded-friendliness improvement.

**This phase ships Option B.** `nros-platform-api::sync` is *not* introduced —
no precedent set for nano-ros types leaking into third-party deps.

### Patch shape (dust-dds side)

```toml
# packages/dds/dust-dds/dds/Cargo.toml
[dependencies]
portable-atomic-util = { version = "0.2", default-features = false, features = ["alloc"], optional = true }
portable-atomic = { version = "1", default-features = false, optional = true }

[features]
default = ["dcps", "rtps", "rtps_udp_transport", "std", "tracing"]
# Existing features unchanged. New:
portable-atomic = ["dep:portable-atomic-util", "dep:portable-atomic"]
```

```rust
// packages/dds/dust-dds/dds/src/lib.rs (additions)

// Phase 101 — `Arc` / `Weak` substitution for CAS-poor targets
// (e.g. `riscv32imc`). When the `portable-atomic` feature is on, route
// through `portable-atomic-util` so stdlib's `target_has_atomic = "ptr"`
// gating doesn't bite. Off by default — std/POSIX path stays on stdlib
// types with zero overhead.

#[cfg(feature = "portable-atomic")]
#[doc(hidden)]
pub use portable_atomic_util::{Arc, Weak};

#[cfg(not(feature = "portable-atomic"))]
#[doc(hidden)]
pub use alloc::sync::{Arc, Weak};
```

```rust
// All ~10 dust-dds source-level Arc/Weak imports change from:
use alloc::sync::Arc;
// to:
use crate::Arc;
```

That's it on the dust-dds side. ABI-internal — no FFI surface passes `Arc`
opaquely (audit confirms RTPS buffer Arcs all internal to the crate).

### Patch shape (consumer / nros-rmw-dds side)

```toml
# packages/dds/nros-rmw-dds/Cargo.toml
[features]
# New: route through portable-atomic-util on CAS-poor targets.
portable-atomic = ["dust_dds/portable-atomic"]

# `nostd-runtime` (and downstream `platform-bare-metal` etc.) gain
# `portable-atomic` automatically when the consuming board crate is
# `riscv32imc`-class. Cleanest: don't auto-imply — let the board crate
# turn it on explicitly:
nostd-runtime = ["dust_dds/dcps", "dust_dds/rtps", ...]  # unchanged
```

```toml
# packages/boards/nros-board-esp32-qemu/Cargo.toml
nros = { ..., features = [..., "platform-bare-metal", "rmw-dds-portable-atomic"] }
# where nros's `rmw-dds-portable-atomic` forwards to
# `nros-rmw-dds?/portable-atomic`.
```

### `regex` problem — separate fix

Substituting `Arc` in dust-dds doesn't help `regex-automata` (transitive dep,
not under our control without vendoring). Two paths:

1. **Drop the dep.** Replace dust-dds's 4 `Regex::new(&fnmatch_to_regex(n))` call
   sites + the `fnmatch_to_regex` helper with a direct fnmatch matcher
   (`*`, `?`, `[abc]`, `[!abc]`, `\\X`, literal). ~80 lines, no external dep.
2. **Make it optional.** `regex` becomes a Cargo feature; partition-QoS
   wildcard matching degrades to literal-only when off. Less complete but
   smaller patch.

**Pick path 1.** Fnmatch is well-specified, partition-QoS already DDS-mandated
to support it, and dropping the regex dep removes the transitive
`regex-syntax` + `regex-automata` + `aho-corasick` chain (~3 k LoC, several
build-time seconds).

## Work Items

- [x] **101.1 — Audit `Arc`/`Weak` use sites in dust-dds.**
      **Result:** safe to substitute. 17 dust-dds source files import
      `alloc::sync::{Arc, Weak}` across ~82 reference sites. Zero
      `extern "C"`, `#[repr(C)]`, or `#[no_mangle]` crossings — every
      `Arc` lives entirely inside Rust code. Two Rust trait-boundary
      sites surface `Arc<[u8]>` to consumers and need the consumer to
      pick the same `Arc` flavour:
      * `transport/interface.rs:26` —
        `MpscSender<Arc<[u8]>>` parameter on
        `TransportParticipantFactory::create_participant`.
      * `transport/types.rs:348` — `pub data_value: Arc<[u8]>` on
        `CacheChange`.
      Both consumed by `nros-rmw-dds` (15+ `use alloc::sync::Arc`
      sites — `runtime.rs`, `transport_nros.rs`, `session.rs`,
      `publisher.rs`, `subscriber.rs`, `waker_cell.rs`). Construction
      sites: `transport_nros.rs:403,433` (`sender.send(Arc::from(...))`).
      Plan: re-export pattern (Option B) inside dust-dds's `lib.rs`,
      then have `nros-rmw-dds` import via `dust_dds::Arc` when the
      `portable-atomic` feature is on. ABI-incompatible flavours never
      meet because the boundary type is owned by dust-dds.
      **Files:** `packages/dds/dust-dds/dds/src/**/*.rs` (read-only audit).

- [x] **101.2 — Replace `regex` with hand-rolled fnmatch in dust-dds.**
      Done — `regex` dep dropped from
      `packages/dds/dust-dds/dds/Cargo.toml`. New `fnmatch_match(p, c)
      -> bool` recursive matcher in `dcps/domain_participant.rs`
      (zero-alloc, supports `*`, `?`, `[abc]`, `[a-z]` ranges,
      `[!abc]` negation, `\\X` escape, literals). Replaced 4 partition
      QoS call sites in `is_compatible_subscription` /
      `is_compatible_publication`. 7 unit tests cover literal /
      star / question / class / range / escape / DDS Partition QoS
      examples. Full `cargo test -p dust_dds --lib`: 143 passed.

- [x] **101.3 — Add `portable-atomic` feature to dust-dds.**
      Done — new optional `portable-atomic-util` + `portable-atomic`
      deps + `portable-atomic` Cargo feature in `dust_dds`. New
      `pub mod sync` in `dust_dds/lib.rs` re-exports `Arc` / `Weak`
      from `portable_atomic_util` when feature is on,
      `alloc::sync` otherwise. All ~13 in-crate
      `use alloc::{..., sync::Arc, ...}` sites rewritten to drop
      `sync::Arc` and add `use crate::sync::Arc;`. Two
      `Arc::new([])` sites rewritten to `Arc::from(&[][..])`
      (`portable_atomic_util::Arc::new([])` returns
      `Arc<[T; 0]>` and does not unsize-coerce to `Arc<[T]>` —
      stdlib unsize-coercion is built-in only for stdlib smart
      pointers). Default build, `--features portable-atomic`
      build, and full `cargo test -p dust_dds --lib` (143/143)
      all pass.

- [x] **101.4 — Forward feature through `nros-rmw-dds`.**
      Done — `nros-rmw-dds` Cargo feature `portable-atomic`
      forwards to `dust_dds/portable-atomic`. `nros` umbrella
      Cargo feature `rmw-dds-portable-atomic` forwards to
      `nros-rmw-dds?/portable-atomic`. New `pub(crate) mod sync`
      in `nros-rmw-dds/src/lib.rs` re-exports
      `dust_dds::sync::{Arc, Weak}` so the crate stays in lockstep
      with dust-dds's flavour choice (avoids the
      `MpscSender<Arc<[u8]>>` /
      `CacheChange::data_value: Arc<[u8]>` ABI mismatch at the
      dust-dds boundary). All ~15
      `use alloc::sync::Arc;` sites rewritten to
      `use crate::sync::Arc;`. Verified:
      `cargo build -p nros-rmw-dds --no-default-features --features
      platform-zephyr,nostd-runtime,ros-humble,portable-atomic
      --target thumbv7m-none-eabi` builds clean, and the std/POSIX
      path
      `cargo build -p nros-rmw-dds --no-default-features --features
      std,platform-posix,ros-humble` also still works.

- [x] **101.5 — Wire ESP32-QEMU example crates to enable the feature.**
      Done — `nros = [..., "rmw-dds-portable-atomic"]` added to both
      `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/Cargo.toml`.
      Also dropped `global-allocator` from the same Cargo's
      `nros-platform = [...]` features — `esp-alloc` (pulled
      transitively via `esp-hal`) already provides
      `#[global_allocator]`, and enabling both produces a
      `the #[global_allocator] in nros_platform conflicts with
      global allocator in: esp_alloc` link error.

      **Decision:** the board crate's `dds-heap` feature stays as a
      pure heap-budget knob (forwards
      `nros-platform-esp32-qemu/dds-heap` only). Forwarding
      `nros/rmw-dds-portable-atomic` from the board crate would
      require the board to depend on the `nros` umbrella, which it
      currently doesn't (and shouldn't — the board is consumed by
      `nros`, not the other way round). Letting the example set the
      feature directly keeps the dependency graph one-way.

      Verified: both `cargo build -p esp32-qemu-dds-talker --release`
      and `cargo build -p esp32-qemu-dds-listener --release` build
      clean for `riscv32imc-unknown-none-elf`.
      **Files:**
      `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/Cargo.toml`.

- [ ] **101.6 — Push fork branch + bump submodule pointer.**
      Push the `nano-ros/phase-101-portable-atomic` branch on
      `jerry73204/dust-dds`. Update root submodule pointer. Open upstream PR
      against `s2e-systems/dust-dds` (Option B is upstream-friendly).
      **Files:** `packages/dds/dust-dds` (submodule).

- [ ] **101.7 — ESP32-QEMU DDS pubsub E2E.**
      Two-instance `nros-tests` fixture (mirror of
      `tests/baremetal_qemu_dds.rs`). Acceptance: ≥80 % message delivery in a
      15 s window. Update Phase 97 doc: 97.4.esp32-qemu `[ ]` → `[x]`. Move
      Phase 97 to `archived/`.
      **Files:** `packages/testing/nros-tests/tests/esp32_qemu_dds.rs`,
      `.config/nextest.toml`, `docs/roadmap/phase-97-dds-per-platform-examples.md`.

## Acceptance Criteria

- [ ] `cargo build -p dust_dds` (default features) builds clean — zero overhead
      vs. pre-Phase-101.
- [ ] `cargo build -p dust_dds --no-default-features --features
      dcps,rtps,portable-atomic` builds clean on a `riscv32imc` target.
- [ ] `cargo build -p esp32-qemu-dds-talker --release` succeeds.
- [ ] `cargo build -p esp32-qemu-dds-listener --release` succeeds.
- [ ] Two-instance ESP32-QEMU talker↔listener E2E achieves ≥80 % delivery.
- [ ] No regression in any existing 97.4 slice (run full nextest suite).
- [ ] Upstream PR open against `s2e-systems/dust-dds` (link in this doc).

## Notes

- `portable-atomic-util` is from the same author as `portable-atomic` (taiki-e),
  same maintenance/release cadence, well-vetted in the Embassy ecosystem. Not
  experimental.
- `portable-atomic-util::Arc` ABI-incompatible with `alloc::sync::Arc` — the
  internal layout differs (counter precedes data, not surrounds it). Anywhere
  `Arc` crosses an FFI boundary, both flavors must agree. Audit (101.1) covers
  this. If a boundary surfaces, the workaround is `Box<dyn Trait>` indirection.
- `critical-section` impl on ESP32-C3 already provided by `esp-hal` (single-core
  RISC-V via `portable-atomic/unsafe-assume-single-core` cfg). No additional
  platform work needed.
- Once Option B lands upstream, the `nros-platform-api::sync` shim discussed in
  the original Option A becomes unnecessary and gets dropped from the design.
- This phase is deliberately scoped to dust-dds. If a *second* third-party
  crate hits the same gating, file a sibling phase with the same playbook —
  do not retrofit a project-wide `nros-platform-api::sync` until two consumers
  exist (rule of three).
