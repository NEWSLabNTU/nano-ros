# Phase 133 — Post-Phase-131 CI Sweep

**Goal.** Chronicle every drift / latent bug surfaced by the first
clean `just ci` run after Phase 131 landed on `main`. Acts as an
index over the per-issue fix commits + the deferred follow-up phases
that close the larger gaps.

**Status.** 4 of 6 items landed. 2 deferred to dedicated phase docs.

**Priority.** P2 — bookkeeping. Each line item is small (or
delegated). Recorded here so future "why does CI complain about X"
investigations land on a single page.

**Depends on.** Phase 131 (the trigger).

---

## Overview

`just ci` had not been run end-to-end on this machine between phases
127 → 131. The first run after Phase 131 surfaced six independent
issues; four were trivial-but-real drift in files Phase 131 also
touched (so easy to mis-attribute), and two were latent bugs in
earlier phases that only fire under specific test ordering. None
trace back to Phase 131's own work — but Phase 131 unmasked them
by changing the workspace shape enough to force a fresh full build.

Recording each here keeps the per-fix commits searchable and
prevents the same root-causes recurring under a new label later.

---

## Items

### 133.1 — `cargo +nightly fmt` drift across phases 127–130
**Status.** Landed (`808ab59`).
**Trigger.** `just check` → `just check-workspace` → `cargo fmt --check`.
**Files.** 10 files in `packages/core/nros-{node,platform-api,platform-cffi}/`, `packages/dds/nros-rmw-dds/src/session.rs`, `packages/testing/nros-tests/src/{qemu,fixtures/binaries/mod}.rs`, `packages/xrce/nros-rmw-xrce-cffi/build.rs`.
**Why.** `rustfmt.toml` enables nightly-only options (`imports_granularity`, `format_code_in_doc_comments`). Stable `cargo fmt` silently skips them; CI uses nightly and flags the diff. Several earlier-phase commits formatted with stable.
**Fix.** `cargo +nightly fmt` sweep.

### 133.2 — Phase 128 left dead per-backend `rmw-*-cffi` feature refs
**Status.** Landed (`97da37c`).
**Trigger.** `just check-workspace` → `cargo clippy --workspace --no-default-features --exclude …` failed with: `nros-node does not have feature 'rmw-dds-cffi'` / `'rmw-zenoh-cffi'`.
**Files.** `packages/testing/nros-tests/Cargo.toml` `trigger-test`, `multi-rmw-bridge` features.
**Why.** Phase 128.C.3 removed per-backend feature flags from `nros-node/Cargo.toml` in favour of the umbrella `rmw-cffi` + walker (`nros_rmw_cffi_walk_init_section`). `nros-tests/Cargo.toml` still listed `nros-node/rmw-zenoh-cffi` and `nros-node/rmw-dds-cffi`. Surfaced only when something forced the workspace manifest to re-resolve, which Phase 131 did.
**Fix.** Replace with `nros-node/rmw-cffi`; drop the dds-specific ref entirely (the `dep:nros-rmw-dds` line is enough).

### 133.3 — Phase 130 wake primitive: header declared, no export macro
**Status.** Landed (`585616d`).
**Trigger.** `just check-platform-abi-mirror` (Phase 121.4.b drift gate) reported 7 wake symbols missing from `nros_platform_export_*!` macro emission.
**Files.** `packages/core/nros-platform-cffi/src/lib.rs` — extends `nros_platform_export_threading!`.
**Why.** Phase 130 added `nros_platform_wake_{init,drop,wait_ms,signal,signal_from_isr,storage_size,storage_align}` to `platform.h` and to the `unsafe extern "C"` block, but never plumbed them into a `nros_platform_export_*!` macro. Result: header declared the symbols, but no platform crate could supply a `pub extern "C" fn` definition. ABI drift gate caught it.
**Fix.** Extend `nros_platform_export_threading!` with the 7 wake fns delegating to `PlatformThreading::wake_*`. Same macro since the wake methods live on the same trait.

### 133.4 — clang-format drift in nros-cpp action headers
**Status.** Landed (`5a83158`).
**Trigger.** `just check-cpp` reported 11 `-Wclang-format-violations` in `action_{client,server}.hpp`, `polling_action_{client,server}.hpp`.
**Files.** the 4 hpp files above.
**Why.** `reinterpret_cast<uint8_t(*)[16]>` lacks a space before the parens per the project's `.clang-format`. Pre-existing drift; never caught locally between phases 127 → 131.
**Fix.** `clang-format -i` sweep.

### 133.5 — zpico-sys build.rs race on `c/include/zpico.h` regeneration → **Phase 134**
**Status.** Landed (`d41cf9c`); root cause documented + follow-up tracked in [phase-134-zenoh-pico-udp-multicast-gate.md](phase-134-zenoh-pico-udp-multicast-gate.md) (separate concern, same `zpico-sys` crate).
**Trigger.** `just check` parallel `cargo check` fan-out failed with `unknown type name 'zpico_ring_desc_t'` etc.
**Files.** `packages/zpico/zpico-sys/build.rs::generate_header`.
**Why.** `std::fs::write(&output_file, processed)` to source-tree `c/include/zpico.h` from N parallel cargo invocations (one per example target-dir) interleaved bytes when concurrent writers raced. Parallel cc readers picked up a truncated header.
**Fix.** Same-content skip + write-to-temp + atomic `rename(2)` into place. POSIX-atomic; concurrent readers see either old-full or new-full, never partial.

### 133.6 — Phase 128 incomplete UDP multicast feature gate → **Phase 134**
**Status.** Not started. Deferred to [phase-134-zenoh-pico-udp-multicast-gate.md](phase-134-zenoh-pico-udp-multicast-gate.md).
**Trigger.** Test-all C-link of every native_api / rmw_interop / c_xrce_api example fails: `/usr/bin/ld: …libnros_rmw_zenoh.a(udp.c.o): in function '_z_f_link_open_udp_multicast': undefined reference to '_z_read_udp_multicast' / '_z_read_exact_udp_multicast'`.
**Why.** Phase 128 deleted "inert link-tcp / udp-unicast" features but the cleanup left two zenoh-pico source files compiled with mismatched `Z_FEATURE_LINK_UDP_MULTICAST` flags inside the same archive: `src/link/multicast/udp.c` keeps the link-wrappers (built with `=1`); `src/system/unix/network.c` no longer emits the underlying transport fns (built with `=0`). Archive ships wrappers with no underlying impl → linker fails at any consumer of `libnros_rmw_zenoh.a`.
**Impact.** Blocks ~86 of the 148 post-131 ci failures.

### 133.7 — `test-all` install-ordering bug → **Phase 135**
**Status.** Not started. Deferred to [phase-135-test-all-install-ordering.md](phase-135-test-all-install-ordering.md).
**Trigger.** First-run fresh `just ci` on a checkout that has never run `just install-local`: native_api / dds_api / rmw_interop / xrce tests panic with `cmake configure failed … Could not find a package configuration file provided by "NrosPlatformPosix"`. Subsequent runs pass (the late `_test-c-codegen` recipe inside test-all calls `just install-local` and populates the install).
**Why.** `test-all` depends only on `build-zenohd`. Tests that build C / C++ examples via cmake assume `build/install/lib/cmake/NanoRos/…` is populated, which only happens after `just install-local` runs.
**Impact.** Cosmetic on warm machines; broken first-run CI on fresh clones.

---

## Acceptance

- [x] 133.1–133.4 committed on `main`.
- [x] 133.5 committed on `main`.
- [x] phase-134 + phase-135 stubs exist with reproducible trigger + a one-line theory of the fix.
- [x] This index links every fix commit + every deferred phase.

---

## Notes

- The same iteration that surfaced these also re-verified Phase 131's
  own scope — every Phase 131 sub-group commit builds cleanly under
  cargo metadata + cargo check, both before and after these fixes.
  Phase 131 did not introduce any of the bugs listed here.
- The XRCE-agent / ROS-2-humble / cross-toolchain `[SKIPPED]` panics
  (~62 of 148) are not bugs — `nros_tests::skip!` panics with
  `[SKIPPED]` per CLAUDE.md "Tests must fail on unmet preconditions"
  rule. They surface as nextest FAILs on machines missing the env;
  intended behaviour.
