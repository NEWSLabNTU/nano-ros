# Phase 177 - Build-issue catalog (post-refactor sweep)

**Goal.** Index the build issues surfaced by a clean `just setup` +
`just build-all` sweep on `main` (2026-05-20/21, after the Phase
118 example-collapse + Phase 169 dust-dds retirement + the
parallel-merge of Phase 173 platform-entry work). Each row points at
the owning phase; environmental/host-only items that have no other home
are tracked here directly.

**Status.** Index, not implementation — same shape as Phase 160's
test-failure catalog. Most refactor-fallout was fixed during the sweep
(see "Fixed" below); the rows here are what remains.

**Priority.** P2. 177.1 (the sole `build-all` blocker) is **fixed**; the
rest are deferred-by-design or host-environment.

## Open issues

### 177.2 — cyclonedds-zephyr feature gaps → **Phase 171.0.a / .b / .c**

- C-service request delivery (C client→server) — 171.0.a.
- Actions all languages (IDL `.action` converter unbuilt) — 171.0.b.
- aemv8r FVP reference re-verify — 171.0.c.

### 177.3 — Cyclone for pure-cargo Rust examples → **Phase 175**

`nros_rmw_cyclonedds_register` lives only in the C++/CMake build, so
`cargo build --features rmw-cyclonedds` of a native/freertos/threadx
Rust example can't link it. Fixture matrices are zenoh-only; the
feature stays defined-but-unbuilt. **Owned by Phase 175.** **175.A
build path landed 2026-05-21** — `examples/native/rust/{talker,listener}/CMakeLists.txt`
link Cyclone via CMake/Corrosion. Native two-process user data is fixed
too: hosted `session_drive_io(timeout_ms)` now sleeps for poll-only
executor pacing, so talker timer publishes and listener receives on
loopback. 175.B (embedded ddsrt port) still deferred. Decision
2026-05-21: keep Cyclone targeted at bare metal (don't delete the
embedded cells) — see Phase 171.B.

## Fixed during the sweep (2026-05-20/21 — no longer issues)

- **177.4** esp_idf setup git-ref corruption — root cause was the
  `fetch origin v5.3:v5.3` refspec in `scripts/esp_idf/setup.sh` writing
  the annotated `v5.3` *tag* into `refs/heads/v5.3` (a branch) →
  `non-commit object` error. Fixed in `6be211ee4` (`fetch --depth 1
  --tags origin <ref>` + `checkout <ref>`). The existing workspace was
  not fundamentally corrupted (the bad write just failed); verified the
  fixed fetch+checkout brings `esp-idf-workspace/esp-idf` to `v5.3`
  cleanly — no destructive re-clone needed.
- **177.1** cyclonedds-zephyr `nsos_adapt.c` duplicate `case
  NSOS_MID_IPPROTO_IP:` — `native-sim-ipproto-ip-patch.sh` (Phase 11W)
  already adds a complete IPPROTO_IP case (all IP_* multicast/membership
  optnames + getsockopt) to `nsos_adapt_setsockopt`; the redundant
  `nsos-adapt-ipproto-ip-patch.sh` (11W.12) added a second label →
  `duplicate case value`. Fixed: 11W.12 now skips when the case is
  already present (it always is — runs after native-sim). The 54
  cyclonedds-zephyr fixtures no longer hit the duplicate. Was the sole
  `build-all` blocker.

- qemu `build-zenoh-pico.sh`: missing `nros-platform-cffi/include` +
  `c/zpico` include paths (Phase 154 `<nros/platform_net.h>`).
- `justfile build-workspace`: exclude `nros-rmw-xrce-cffi-staticlib`
  (no_std staticlib) + nros-c/cpp/staticlibs on the `nextest --no-run`
  line (Phase 88 `nros_platform_log_write` link).
- `nros/src/lib.rs`: gate the `sched_context` re-export on `rmw-cffi`
  (matches the `has_rmw` module gate in nros-node).
- `nros-c` / `nros-cpp` `build.rs`: add the picolibc `-isystem` for
  riscv64-none `cc::Build` (Corrosion didn't forward the toolchain's).
- Stale pre-collapse `rust/{zenoh,dds}/<ex>` fixture paths dropped from
  the native/freertos/threadx/nuttx recipes (Phase 118 collapse).
- dust-dds → `nros-rmw-cyclonedds-sys` rust example migration (Phase
  171.B.2); bare-metal fixture matrices reverted to zenoh-only.
- Unified jobserver `gmake`→make-4.4 alias (stray make 4.3 choked on
  the inherited fifo `--jobserver-auth`) — Phase 176.
- **177.5** NuttX/ESP32 `-Z build-std` e2e (`fixture_workspace_builds_
  generated_{nuttx,esp32}_package`): verified green with the pinned
  `nightly-2026-04-11` + `rust-src` installed. Added a
  `build_std_nightly_skip()` precondition guard (reads the channel from
  `tools/rust-toolchain.toml`) so both skip cleanly with the exact
  `rustup` remedy when the toolchain is absent, instead of failing
  partway through with an opaque `can't find crate for 'core'`. Host
  remedy unchanged: `rustup toolchain install nightly-2026-04-11 &&
  rustup component add rust-src --toolchain nightly-2026-04-11`.

## Notes

- This is an INDEX. 177.1 (the sole build-all blocker), 177.4 (esp_idf
  git-ref corruption), and 177.5 (build-std e2e clean-skip guard) are
  **fixed**, so `just build-all` is green end-to-end. Archive this doc
  once 177.2 / 177.3 migrate to their owning phases (171.0 / 175).
- The sweep also validated the Phase 176 unified jobserver
  (`build-all-jobserver`) end-to-end — not a build issue, recorded in
  Phase 176.
