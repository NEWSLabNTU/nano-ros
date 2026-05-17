# Phase 134 — zenoh-pico Build-Flag Canonical Header

**Goal.** Eliminate the linker-time class where `libnros_rmw_zenoh.a`
ships `_z_f_link_*` wrappers compiled under one `Z_FEATURE_LINK_*`
value while the underlying transport impls compile under a different
value, leaving the archive internally inconsistent. The root fix is
structural: make `zenoh_config.h` the **single source of truth** for
every `Z_FEATURE_LINK_*` flag and stop overriding it from `build.rs`
via `.define(…)` calls. After the change there is no path through
`build.rs` where two compile units can disagree about a flag because
both paths read the same header.

This phase is the minimal, ship-fast fix. The larger "unify the two
build paths and eliminate CMake from `zpico-sys`" effort is deferred
to Phase 136.

**Status.** Not started.

**Priority.** P1 — blocks ~86 of 148 post-Phase-131 ci failures
(`native_api` 58, `rmw_interop` 40, `c_xrce_api` 10) all caused by
this one half-defined-archive class.

**Depends on.** Phase 128 (which introduced the gate mismatch by
deleting "inert link-tcp / udp-unicast" but leaving the multicast
side half-wired across the cc-rs / CMake split).

**Related.** Phase 133 (post-131 ci sweep — same failure inventory),
Phase 131 (forced clean install that surfaced the latent bug),
Phase 136 (structural unify of the two build paths — supersedes the
need for 134's `.define()` deletions but does not replace 134's
header-as-source-of-truth contract).

---

## Overview

`packages/zpico/zpico-sys/build.rs` has two compile paths that build
zenoh-pico for the C shim layer:

1. **cc-rs path** (`build_c_shim`, `build_zenoh_pico_threadx`,
   `build_zenoh_pico_freertos`, …) — used for every embedded target
   (threadx, freertos, nuttx, bare-metal, esp-idf, Orin SPE).
2. **CMake path** (`build_zenoh_pico_native`) — used for the POSIX
   path. Delegates to upstream `zenoh-pico/CMakeLists.txt`.

Today every flag has up to four expressions: a `LinkFeatures` field,
a per-path `build.define(…)` literal in cc-rs, a separate
`cmake_cfg.define(…)` literal in CMake, and a `#define …` line
written into `zenoh_config.h` by `generate_config_header`. Drift
between any two of those four is silent until the linker fails.

After this phase:

- `zenoh_config.h` is canonical. Every `Z_FEATURE_LINK_*` flag
  appears there once, derived from `LinkFeatures`.
- Both compile paths force-include the header (`-include
  <out_dir>/zenoh_config.h`).
- Every `build.define("Z_FEATURE_LINK_*", …)` and every
  `cmake_cfg.define("Z_FEATURE_LINK_*", …)` in `build.rs` is
  deleted.
- Platform-invariant overrides (e.g. SPE has no Ethernet, so TCP /
  UDP must be 0 regardless of `LinkFeatures`) live in a per-platform
  policy table, not as inline literals.

---

## Architecture

### A. Where the flags get set today

```
build.rs::generate_config_header   →  zenoh_config.h  (declarative)
build.rs::build_c_shim             →  build.define(...)  (literals)
build.rs::build_zenoh_pico_threadx →  build.define(...)  (literals)
build.rs::build_zenoh_pico_freertos →  build.define(...) (literals)
build.rs::build_zenoh_pico_native  →  cmake_cfg.define(...) (literals + reliance on CMake default)
build.rs Orin-SPE block (line ~1925) → build.define(...) (literals)
```

Four sources of truth + one CMake-default fallthrough = guaranteed
divergence given enough changes. Phase 128 was the change.

### B. Single source of truth

```
LinkFeatures + per-platform LinkPolicy  →  zenoh_config.h  →  every compile unit
```

`build.rs` writes the header once. Both paths `-include` it.
Per-path `build.define(…)` / `cmake_cfg.define(…)` calls for
`Z_FEATURE_LINK_*` are deleted entirely. Other defines unrelated to
link features (`ZENOH_GENERIC`, `ZENOH_LINUX`, `ZPICO_SMOLTCP`,
`Z_FEATURE_MULTI_THREAD`, …) stay where they are — they are not the
bug class. The CMake path keeps the few non-`Z_FEATURE_LINK_*`
defines it needs to drive upstream's CMake (`BUILD_SHARED_LIBS=OFF`,
buffer sizes, …) untouched.

For platform invariants (Orin SPE has no Ethernet; bare-metal
serial-only boards have no IVC) introduce a `LinkPolicy` struct that
masks `LinkFeatures` before `generate_config_header` writes the
header. Policies are platform-specific data, not inline literals
scattered through `build.rs`. The Orin SPE block becomes:

```rust
// Before: ten literal build.define("Z_FEATURE_LINK_*", "0") calls
// After:
let link = LinkFeatures::from_env().apply(LinkPolicy::orin_spe());
generate_config_header(&out_dir, &link, &buf_config);
// remaining ZENOH_GENERIC / ZENOH_ORIN_SPE / Z_FEATURE_MULTI_THREAD
// defines stay — they are not link-feature gates.
```

---

## Work Items

- [ ] **134.1 — Audit `Z_FEATURE_LINK_*` sites.**
      Walk `build.rs` and produce a table: caller path → flag-source
      variable → which compile unit sees the value. Confirm the
      disagreement on `Z_FEATURE_LINK_UDP_MULTICAST` and check the
      same shape for `_TCP`, `_UDP_UNICAST`, `_SERIAL`, `_WS`,
      `_BLUETOOTH`, `_TLS`, `_IVC`, `_CUSTOM`. Land the table in this
      doc under "Notes" so future readers see the pre-fix state.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] **134.2 — Introduce `LinkPolicy`.**
      Add `struct LinkPolicy { tcp: bool, udp_unicast: bool,
      udp_multicast: bool, serial: bool, ws: bool, bluetooth: bool,
      tls: bool, ivc: PolicyChoice, custom: PolicyChoice }`. `PolicyChoice`
      is `Force(bool)` or `FollowCargoFeature(&'static str)`. Constructors:
      `LinkPolicy::posix()`, `::orin_spe()`, `::bare_metal_serial()`,
      `::default()`. `LinkFeatures::apply(self, policy) -> LinkFeatures`
      masks per the policy.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] **134.3 — Force-include `zenoh_config.h` on both paths.**
      Add `-include <out_dir>/zenoh_config.h` to every cc-rs
      `build.flag(…)` call. For the CMake path, pass
      `-DCMAKE_C_FLAGS=-include <out_dir>/zenoh_config.h` via
      `cmake_cfg.cflag(…)` (or equivalent) so upstream's CMake-driven
      build also picks up the header before its own defaults fire.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] **134.4 — Delete every `Z_FEATURE_LINK_*` literal.**
      Remove all `build.define("Z_FEATURE_LINK_*", …)` calls from
      `build_c_shim`, `build_zenoh_pico_threadx`,
      `build_zenoh_pico_freertos`, the Orin-SPE block, and every
      other cc-rs site. Remove all
      `cmake_cfg.define("Z_FEATURE_LINK_*", …)` calls from
      `build_zenoh_pico_native`. Per-platform invariants flow through
      the new `LinkPolicy` instead.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] **134.5 — Build-time archive invariant check.**
      Add `scripts/check-zenoh-archive-symbols.sh` that runs `nm
      build/install/lib/libnros_rmw_zenoh.a` and asserts: for every
      `_z_f_link_*_udp_multicast` (and `_tcp` / `_udp_unicast` /
      `_serial` / …) wrapper symbol defined, the matching
      `_z_*_udp_multicast` impl is also defined (no `U`). Wire into
      `just check-zenoh-archive` and call it from `just doctor` +
      from CI. Catches the regression class permanently.
      **Files.** `scripts/check-zenoh-archive-symbols.sh`, `justfile`.

- [ ] **134.6 — E2E tests.** See "Acceptance / E2E" below.

---

## Acceptance / E2E

The header-canonical contract has to hold end-to-end, not only at
the `nm` level. Land all of:

- [ ] **E2E.1 — Symbol parity gate.** New
      `packages/testing/nros-tests/tests/zenoh_archive_symbols.rs`
      runs `check-zenoh-archive-symbols.sh` over the install tree
      produced by `just build`. Asserts no `U` rows for any
      `_z_*_udp_multicast` / `_z_*_tcp` / `_z_*_udp_unicast` /
      `_z_*_serial` symbol whose wrapper is `T`. Test FAILS on any
      `U/T` mismatch, never silently skips. Runs in `just ci`.

- [ ] **E2E.2 — Native C link smoke.** `examples/native/c/` and
      `examples/native/cpp/` each ship one talker / listener pair.
      Run via `just test-all`. After 134, the link errors
      (`undefined reference to '_z_read_udp_multicast'` and the
      `_z_read_exact_udp_multicast` partner) must be gone. Expected
      drop in CI: ~58 `native_api` + 40 `rmw_interop` + 10
      `c_xrce_api` = ~108 fails → PASS.

- [ ] **E2E.3 — Flag-drift property test.** New
      `packages/testing/nros-tests/tests/zenoh_flag_consistency.rs`
      programmatically builds `nros-rmw-zenoh-staticlib` with
      `LinkFeatures` toggled across the cross-product (TCP on/off ×
      UDP-multicast on/off × IVC on/off × CUSTOM on/off — 16
      combinations). For each: parse the generated
      `zenoh_config.h`, dump the archive's defined `_z_f_link_*` and
      `_z_*_<transport>` symbols via `nm`, assert the **header
      value** matches the **archive presence** (header=1 ⇔ both
      wrapper and impl present; header=0 ⇔ both absent). Cycle time
      ~5 min, gated behind a `link-flag-matrix` feature so it only
      runs in `just test-all`.

- [ ] **E2E.4 — POSIX vs embedded parity.** `packages/testing/
      nros-tests/tests/zenoh_header_parity.rs` builds the staticlib
      for two targets in one run (`x86_64-unknown-linux-gnu` POSIX
      via CMake path; `thumbv7m-none-eabi` bare-metal via cc-rs
      path). For each, dumps the generated `zenoh_config.h` and
      asserts the `Z_FEATURE_LINK_*` values match the `LinkFeatures`
      they were built with. Closes the cc-rs ↔ CMake divergence loop
      that Phase 128 left open.

- [ ] **E2E.5 — `just ci` post-134.** Re-run `just ci`; FAIL count
      must drop by ≥85. Document the resulting count in this doc
      before archive.

---

## Notes

- The `Z_FEATURE_LINK_UDP_MULTICAST` mismatch is the smoking gun, but
  every link feature has the same shape. The audit (134.1) catches
  any sibling already festering.
- Do **not** "fix" by setting `Z_FEATURE_LINK_UDP_MULTICAST=0`
  everywhere without checking whether the runtime needs multicast.
  rmw_zenoh discovery uses multicast on LAN by default. Per-platform
  `LinkPolicy` is the correct shape.
- Phase 131's parallel-build pressure did not introduce the bug — it
  surfaced because Phase 131 forced a clean install where the bug
  had previously been masked by stale archives with both halves
  defined.
- This phase intentionally keeps the two build paths split. Phase
  136 unifies them. 134 makes the canonical-header contract hold
  even with the split present, so 136 can be a pure refactor
  afterward.
