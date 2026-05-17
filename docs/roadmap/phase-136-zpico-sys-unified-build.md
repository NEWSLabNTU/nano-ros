# Phase 136 — `zpico-sys` Unified Build Path on cc-rs + Platform Manifest

**Goal.** Eliminate the cc-rs ↔ CMake split in
`packages/zpico/zpico-sys/build.rs`. Move all zenoh-pico source
selection + per-platform defines into a declarative TOML manifest;
let one cc-rs invocation build every supported target. The CMake
dependency on `cmake = "0.1"` and the entire
`build_zenoh_pico_native` function (~600 LOC) are removed.

**Status.** Not started.

**Priority.** P2 — structural follow-up to Phase 134. Not blocking
CI; pays back permanently by removing the failure class that 134's
header-canonical contract has to defend against.

**Depends on.** Phase 134 (which makes `zenoh_config.h` canonical
and proves both paths agree on link flags). Without 134, this phase
risks chasing the same drift bug in a different shape during the
transition.

**Related.** Phase 128 (introduced the gate mismatch), Phase 133
(post-131 ci sweep), Phase 137 (potential follow-up: factor
`zpico-link-{lwip,smoltcp}` out of `zpico-sys` to mirror
`zpico-link-ivc`).

---

## Overview

### Why unify

Two compile paths drift. Phase 134 contains the drift via a single
canonical header, but the structural shape (cc-rs for embedded,
CMake-via-`build_zenoh_pico_native` for POSIX) keeps the surface for
future drift alive — every new flag, every new source file, every
new platform shim still has to be wired into both paths. Cost
compounds.

The Rust `*-sys` crate convention is cc-rs sole-driver
(libsqlite3-sys, libgit2-sys, ring, lz4-sys). zenoh-pico itself
ships a non-CMake build helper — `extra_script.py` for PlatformIO —
which proves the data model is portable. ~148 `.c` files total: well
within cc-rs reach.

### Why a manifest, not a build.rs constant table

`build.rs` already runs Rust at build time, so a `const` table would
compile. Pulling the data into TOML buys three things:

1. **Static review.** Reviewers diff a TOML hunk, not a Rust hunk
   with embedded literals.
2. **External tooling.** A future codegen step (verifier, CI gate)
   can read the same file.
3. **User override.** Downstream consumers can ship a custom
   `platforms.toml` for an out-of-tree board without forking
   `zpico-sys`.

The TOML lives at
`packages/zpico/zpico-sys/zenoh_platforms.toml`.

---

## Architecture

### Manifest schema

```toml
# zenoh_platforms.toml — canonical zenoh-pico build manifest.
# build.rs reads this once. Cargo features select the platform.
# Per-platform `link.*` policies mask LinkFeatures.

[platform.posix]
defines = ["ZENOH_LINUX"]
include = ["system/common", "system/unix"]
exclude = ["tests", "example"]
system_libs = ["pthread", "rt"]
mbedtls = "pkg-config"            # or "vendored" or "none"

[platform.zephyr]
defines = ["ZENOH_ZEPHYR"]
include = ["system/common", "system/zephyr"]

[platform.freertos-lwip]
defines = ["ZENOH_FREERTOS_LWIP"]
include = ["system/common", "system/freertos"]

[platform.nuttx]
defines = ["ZENOH_NUTTX", "ZENOH_LINUX"]  # nuttx reuses unix shim
include = ["system/common", "system/unix"]

[platform.threadx]
defines = ["ZENOH_GENERIC", "ZENOH_THREADX"]
include = ["system/common"]
# consumer ships the platform shim via zpico-platform-shim

[platform.generic]
defines = ["ZENOH_GENERIC"]
include = ["system/common"]

[platform.orin-spe]
inherits = "generic"
defines = ["ZENOH_ORIN_SPE"]
link.tcp = false
link.udp_unicast = false
link.udp_multicast = false
link.serial = false
link.tls = false
link.ws = false
link.bluetooth = false
link.raweth = false
link.ivc = "feature"             # gated by CARGO_FEATURE_LINK_IVC
link.custom = "feature"
```

`include` / `exclude` accept glob-style relative paths under
`zenoh-pico/src/`. `defines` are added unconditionally (cc-rs
`build.define(name, None)`). Non-link-feature defines like
`ZENOH_GENERIC` remain platform-data, not platform-logic.

### build.rs flow

```text
zenoh_platforms.toml  ──parse──►  PlatformManifest
                                       │
LinkFeatures::from_env() ──┐           │
                          apply        │
                  LinkPolicy::for(plat)│
                           ▼           │
                  resolved LinkFeatures│
                           │           │
                    write_header       │
                           ▼           │
                zenoh_config.h ◄──include flag
                           ▲           │
                  cc-rs.build ─────────┘
                           │
                  pkg-config (POSIX only)
                           ▼
                  libnros_rmw_zenoh.a
```

Single function. No platform-dispatched code branches in `build.rs`.
The data lives in the TOML.

### mbedTLS on POSIX

Three options the manifest can declare:

- `mbedtls = "pkg-config"` — `pkg-config` crate discovers system
  libs. Ubuntu's `libmbedtls-dev` ships no `.pc` files; build.rs
  synthesizes one in `$OUT_DIR/pkg-config/` and prepends to
  `PKG_CONFIG_PATH`. Same workaround Phase 117 already uses for the
  CMake path — port the snippet, drop the CMake half.
- `mbedtls = "vendored"` — depend on `mbedtls-sys` crate. Heavier
  build, no system requirement.
- `mbedtls = "none"` — TLS link feature off; no mbedTLS dep.

Selection per platform via the manifest, not Cargo features (the
user opts into TLS via `link-tls`; the platform decides where it
comes from).

### Source-list drift gate

Upstream zenoh-pico bumps can add new `.c` files under
`zenoh-pico/src/system/<plat>/`. Without a gate, the manifest
silently misses them and the build links against stale objects.

Mitigation: a build-time check inside `build.rs` (not a separate
test):

```rust
let actual = glob("zenoh-pico/src/system/<plat>/**/*.c");
let declared = manifest.resolved_sources();
assert_eq!(actual, declared,
    "zenoh-pico source list drift; update zenoh_platforms.toml");
```

Fails loud at `cargo build` time after every submodule bump. Forces
the manifest to stay in sync with upstream.

---

## Work Items

- [x] **136.1 — Manifest schema + reader.** (2026-05-18)
      Landed `zenoh_platforms.toml` with eight platforms (posix,
      zephyr, freertos-lwip, nuttx, threadx, bare-metal, generic,
      orin-spe). Added `serde` + `toml` build-deps to `zpico-sys`.
      Wrote `PlatformManifest::{load, parse, for_platform}` +
      `ResolvedPlatform` with `inherits`-chain merging (parent
      defines/include/exclude/system_libs unioned, child wins on
      mbedtls and per-key `link.*` overrides; cycle-detected).
      `build.rs` parses + resolves every declared platform at the
      top of `main()` so TOML drift surfaces as a build-script
      panic. The resolved data is not yet consumed by the cc-rs /
      cmake paths — 136.3 / 136.4 plug it in.
      **Files.** `packages/zpico/zpico-sys/zenoh_platforms.toml`,
      `packages/zpico/zpico-sys/build/manifest.rs`,
      `packages/zpico/zpico-sys/build.rs`,
      `packages/zpico/zpico-sys/Cargo.toml`.

- [x] **136.2 — `LinkPolicy` (from Phase 134).** (2026-05-18)
      `LinkFeatures` + `PolicyChoice` + `LinkPolicy` extracted from
      `build.rs` into `build/policy.rs` so the manifest layer can
      produce the same values in 136.4. Behaviour preserved:
      `LinkFeatures::from_env()`, `LinkFeatures::apply(&LinkPolicy)`,
      and the three constructors (`passthrough()`, `posix()`,
      `orin_spe()`) move verbatim. Renamed the manifest's enum from
      `LinkPolicy` → `LinkOverride` to avoid name collision; the
      manifest enum is the parser-side override hint, the policy
      struct is the resolved mask the cc-rs path consumes.
      **Files.** `packages/zpico/zpico-sys/build/policy.rs`,
      `packages/zpico/zpico-sys/build/manifest.rs`,
      `packages/zpico/zpico-sys/build.rs`.

- [x] **136.3 — Replace `build_zenoh_pico_native`.** (2026-05-18)
      CMake invocation deleted. POSIX zenoh-pico builds via cc-rs:
      `add_zenoh_pico_core_sources` + `system/unix/tls.c` +
      `ZENOH_GENERIC` / `ZENOH_LINUX` / `ZENOH_DEBUG=0` / buffer
      sizes. Version header writes to `OUT_DIR/zenoh-pico-version/`
      via the shared `generate_embedded_version_header`. cc-rs reads
      `zenoh-pico/src/` in-place; `copy_source_tree` deleted.
      `cmake = "0.1"` dropped from `Cargo.toml`. Manifest's
      `[platform.posix]` block has `mbedtls = "pkg-config"` set —
      it's not yet consumed by 136.3 (still passes the literal
      flag), but the data lives in one place for the next collapse.
      Verified by 8/11 nano2nano + 3/3 actions + 7/7 custom_msg
      pass (the 3 nano2nano fails are pre-existing missing rtic
      fixtures).

- [x] **136.4 — Collapse the per-RTOS cc-rs paths.** (2026-05-18)
      Schema option C (TOML + small DSL + shared `[arch.*]` table)
      implemented. Five per-RTOS functions + `build_zenoh_pico_native`
      deleted (~785 LOC); replaced by
      `build_zenoh_pico_unified(&ResolvedPlatform, &arch_table,
      &InterpContext, …)` (~250 LOC) driven entirely by
      `zenoh_platforms.toml`. Each `[platform.*]` block now carries
      `defines` / `defines_kv` / `defines_env` / `include` /
      `extra_sources` (with `if_env` + `with_define` modifiers) /
      `required_env` (with help + `validate_subdir`) /
      `include_paths` / `include_paths_conditional` (`when.target_match`
      / `target_not` / `if_env`) / `arch` / `compile` / `pic` /
      `link.*`. `[arch.*]` blocks (cortex-m3, cortex-m4f, cortex-a7,
      cortex-r5-softfp, riscv32imc, riscv32gc, riscv64gc) carry
      target-arch cflags + `needs_picolibc` / `needs_errno_override`
      / `needs_riscv_compiler` hooks. Interpolation tokens:
      `{nros}` / `{out}` / `{src}` / `{env:VAR}`.
      Verified by `cargo build -p zpico-sys` (default + posix +
      bare-metal-thumbv7m) + `test_talker_listener_communication`
      + `test_tls_talker_listener_communication` POSIX E2E pass.
      Embedded paths (FreeRTOS / NuttX / ThreadX) hit pre-existing
      symbol-conflict / serial-link errors on origin/main too —
      out of scope for 136.4 verification. See
      `docs/roadmap/phase-146-zenoh-pico-embedded-link-regressions.md`
      for the three documented symptoms + suggested fix paths.
      Net diff: ~793 LOC removed, ~600 LOC of TOML + 250 LOC of
      Rust consumer added.

- [x] **136.5 — mbedTLS via pkg-config.** (2026-05-18, landed with 136.3)
      `pkg-config = "0.3"` build-dep added. `build_zenoh_pico_native`
      calls `pkg_config::Config::probe("mbedtls")` which emits the
      include paths + `cargo:rustc-link-lib` triples
      (`mbedtls` / `mbedx509` / `mbedcrypto`) automatically. The
      `.pc`-synthesizer fallback (`generate_mbedtls_pc_files`) is
      preserved and prepended to `PKG_CONFIG_PATH` before the
      probe so Ubuntu's libmbedtls-dev (no `.pc`) resolves.
      Verified by `test_tls_talker_listener_communication` pass.

- [x] **136.6 — Source-list drift gate.** (2026-05-18 — partial)
      Build-script glob runs immediately after manifest resolution
      (still in `main()` prologue). For every platform, every
      `include` root in `zenoh_platforms.toml` must (a) resolve to
      an existing directory under `zenoh-pico/src/`, (b) contain
      `≥1 .c` file or sub-directory. Fires a build-script panic
      naming the manifest path + the offending include on drift.
      Verified by flipping `system/unix` → `system/nonexistent`:
      panic surfaces; restoring passes.
      Full set-equality vs the cc-rs source list (the version
      described in the phase doc above) lands with 136.4 once the
      per-RTOS functions collapse into a single manifest-driven
      path. The partial gate already catches the most common upstream
      bumps (renamed `system/<plat>/` dirs).
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [~] **136.7 — E2E tests.** (2026-05-18 — most gates already
      satisfied by the 136.3 / 136.5 + earlier work)
      - E2E.1 (build parity per platform): **not run** — needs the
        full embedded matrix that's on hold for the 137-140 refactor.
      - E2E.2 (Phase 134.5 symbol gate): **pass** —
        `scripts/check-zenoh-archive-symbols.sh` against the existing
        `build/install/lib/libnros_rmw_zenoh.a` reports
        `zenoh archive symbol parity: clean` (tcp/udp_unicast/
        udp_multicast: 8 wrappers each, all impls defined).
      - E2E.3 (drift gate fires on regression): **pass** — verified
        in 136.6 by flipping `system/unix` → `system/nonexistent`
        and observing the build-script panic name the offending key.
      - E2E.4 (POSIX native examples link clean): **pass** —
        `test_talker_listener_communication`,
        `test_tls_talker_listener_communication`, all 7
        `custom_msg::*` tests, all 3 `actions::*` tests pass.
      - E2E.5 (embedded smoke unchanged): **not run** — same hold
        reason as E2E.1.
      - E2E.6 (mbedTLS path): **pass** —
        `test_tls_talker_listener_communication` exercises the
        synthesized-`.pc` → `pkg_config::probe` →
        `cargo:rustc-link-lib=mbedtls / mbedx509 / mbedcrypto` chain
        end-to-end.
      - E2E.7 (`cmake` dep removed): **pass** —
        `cargo tree -p zpico-sys | grep cmake` returns no rows.
      - E2E.8 (build-time delta): **not measured** — informational.
      Full E2E sign-off needs E2E.1 + E2E.5 once the 137-140 build
      refactor lands and the embedded matrix run is unblocked.

- [x] **136.8 — Doc update.** (2026-05-18)
      `book/src/internals/zpico-build.md` lands the manifest
      reference: per-platform fields, per-arch fields,
      interpolation tokens, `when` matcher, `ExtraSource` /
      `RequiredEnv` shapes, mbedTLS source policy, source-drift
      gate, consumer flow, "adding a new platform" recipe.
      `book/src/SUMMARY.md` lists it under Internals.
      Cross-link from `book/src/concepts/platform-model.md` is
      a follow-up — the manifest is currently the source of
      truth for the platform-side knobs the platform-model doc
      references conceptually; explicit cross-link will land when
      that doc next gets a refresh.
      **Files.** `book/src/internals/zpico-build.md`,
      `book/src/SUMMARY.md`.

---

## Acceptance / E2E

- [ ] **E2E.1 — Build parity across every platform.** New
      `packages/testing/nros-tests/tests/zpico_build_matrix.rs`
      drives `cargo build -p nros-rmw-zenoh-staticlib --features
      platform-<P>,…` across `{posix, zephyr, freertos-lwip, nuttx,
      threadx, bare-metal, orin-spe}` with their default
      `link.*` policies. Each must produce an archive whose symbol
      list matches the matching pre-136 archive (run from a tag of
      the previous commit, diffed). Differences allowed only at the
      level of compiler-generated helper symbols; every
      `_z_f_link_*` and `_z_*_<transport>` symbol must be
      identical. Test FAILS on any divergence.

- [ ] **E2E.2 — Symbol gate from 134 still green.**
      `scripts/check-zenoh-archive-symbols.sh` from Phase 134.5
      runs against the unified path's output unchanged. Wrapper /
      impl pairs must still be both-defined or both-absent.

- [ ] **E2E.3 — Source-list drift gate fires.** Manually delete one
      entry from `zenoh_platforms.toml`'s `[platform.posix]
      include`; assert `cargo build -p nros-rmw-zenoh-staticlib
      --features platform-posix` fails with the documented drift
      error (test via subprocess capture in
      `tests/zpico_drift_gate.rs`). Restore the entry, assert
      passes. Guards the gate itself against regressions.

- [ ] **E2E.4 — POSIX native examples link clean.**
      `examples/native/c/talker`, `…/listener`,
      `examples/native/cpp/talker`, `…/listener` each build +
      execute under `just test-all` against a live zenohd. After
      136, the four examples produce zero linker errors and zero
      `U` symbols in their final binaries.

- [ ] **E2E.5 — Embedded smoke unchanged.**
      `just qemu test` + `just zephyr test` + `just freertos test`
      pre- and post-136 produce the same PASS / FAIL inventory. The
      structural refactor must not regress any platform's behaviour.

- [ ] **E2E.6 — mbedTLS path.** New
      `packages/testing/nros-tests/tests/zenoh_tls_link.rs` builds
      `nros-rmw-zenoh-staticlib` with `link-tls` on POSIX. Runs the
      resulting talker / listener against a TLS-secured zenohd.
      Tests the full pkg-config → synthesized `.pc` →
      `cargo:rustc-link-lib=mbedtls` chain end-to-end. FAILS if the
      synthesized `.pc` is missing on Ubuntu, FAILS if
      `cargo:rustc-link-lib` mis-orders the `mbedtls / mbedx509 /
      mbedcrypto` libs.

- [ ] **E2E.7 — `cmake` dep removed.** `cargo tree -p zpico-sys |
      grep cmake` returns no rows. Asserted in
      `tests/zpico_build_matrix.rs`'s setup phase. Guards against
      reintroduction.

- [ ] **E2E.8 — Build-time delta.** Microbench the wall-clock cost
      of `cargo build -p nros-rmw-zenoh-staticlib --release` on
      POSIX before and after 136. Document the delta (expected:
      cc-rs is ~30 % faster than CMake; net win). Not a hard gate
      — informational, in this doc's "Notes".

---

## Notes

- Upstream zenoh-pico ships `extra_script.py` as its
  PlatformIO/Arduino build helper. Its `SRC_FILTER` arrays are the
  data model this phase formalises into TOML. Upstream's own data
  proves the model.
- Embedded-LWIP / smoltcp glue currently lives inside the cc-rs
  build of `zpico-sys`. A follow-up Phase 137 can factor these out
  into `zpico-link-lwip` / `zpico-link-smoltcp` sub-crates,
  mirroring the existing `zpico-link-ivc` carve-out (Phase 131
  introduced that pattern). Not in scope here.
- The manifest is a contract with the user: out-of-tree boards can
  ship their own `[platform.<name>]` block in a downstream override
  TOML (loaded via `ZPICO_PLATFORMS_TOML` env). 136.1 only ships
  the in-tree platforms; the override hook can be a 136 follow-up.
- ESP-IDF / Zephyr integrations stay outside this phase. Those
  builds wrap zenoh-pico via the vendor's own component system
  (esp-idf-component, west module). `zpico-sys` is the Cargo-side
  builder for everything else.
