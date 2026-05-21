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

**Priority.** P2. 177.1 (the sole `build-all` blocker), 177.4, and 177.5
are **fixed**; the remaining rows (177.2 / 177.3) are deferred-by-design
to their owning phases (171.0 / 175).

## Open issues

### 177.2 — remaining Cyclone Zephyr action gaps → **Phase 171.0.b / .c**

- C-service request delivery (C client→server) is no longer an open
  catalog item: Phase 171.0.a found the RELIABLE+VOLATILE request
  match race and gates/buffers service requests before first write.
- Native actions are no longer an open catalog item: C, Rust, and C++
  same-language Cyclone DDS action E2E are runtime-verified, and the
  C++ `get_result` framing bug is fixed. Remaining 171.0.b action work
  is Zephyr Cyclone DDS actions plus cross-implementation framing /
  feedback polish.
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

## Follow-up sweep failures (2026-05-21)

These were found while rerunning `just ci`, `just build-all`, and the
standard `test-all` tail after later remote changes. They are grouped by
failure mode so follow-up phases can claim them without re-reading the
full logs.

### Quality/check fallout — fixed in Phase 177 follow-up

- `just ci/build-all` is not a recipe path; the actionable split is
  `just ci` for quality/test orchestration and `just build-all` for the
  build matrix.
- Clippy rejected a doc-comment lazy continuation in
  `nros-rmw-cyclonedds-sys`.
- Stale generated `examples/**/build*` directories made example checks
  recurse into nested Corrosion workspaces.
- `nros-c` library tests linked without platform log symbols
  (`nros_platform_log_write`, `nros_platform_log_flush`).

### Build-all platform blockers — fixed in Phase 177 follow-up

- NuttX C/C++ fixtures failed opaque-storage asserts because the target
  size probe returned no usable constants for the custom target. The
  C/C++ build scripts now use committed NuttX fallback sizes when the
  probe returns empty/zero sizes.
- Zephyr fixtures failed in this sandbox because the sibling Zephyr
  workspace and Zephyr toolchain cache paths were read-only. The Zephyr
  recipe now uses writable repo-local build/cache roots when needed.
- Zephyr native_sim builds failed through Zephyr's built-in `ccache`
  wrapper writing under read-only `/run/user/.../ccache-tmp`. The
  recipe now disables Zephyr's built-in ccache path (`USE_CCACHE=0`)
  while preserving the repo-controlled `sccache` compiler launcher.
- Zephyr CycloneDDS fixture builds also needed small compatibility
  fixes: `steady_clock::time_point`, `THREAD_CUSTOM_DATA`, a weak
  `nsos_adapt_getifaddrs` fallback, and a non-fatal Cortex-R Rust patch
  when upstream Kconfig is not writable.

### Test-all environment/setup gaps — still open or host-local

- PX4 tests need a valid `PX4_AUTOPILOT_DIR` (or a fixture path that
  points at the checked-out PX4 submodule/workspace).
- ESP-IDF and PlatformIO groups require host tools (`idf.py`, `pio`) not
  present in the minimal sweep environment.
- Several runtime/bridge groups failed because required binaries or
  fixtures had not been prebuilt for the full matrix before `test-all`.

### Test-all runtime/E2E gaps — still open

- The full `just ci` quality/build stages completed, then the `test-all`
  runtime layer reported 957 tests run: 876 passed, 79 failed, 2 timed
  out, and 9 skipped. The harness summarized this as 29 real failures
  out of 81 total failures/timeouts.
- Failures clustered in runtime E2E groups such as nano2nano, bridge,
  and Zephyr/service orchestration. These need focused reruns with the
  relevant fixtures and services prebuilt rather than another broad
  root-level sweep.

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
- **177.5** NuttX/ESP32 `-Z build-std` e2e
  (`fixture_workspace_builds_generated_{nuttx,esp32}_package`):
  verified green with the pinned `nightly-2026-04-11` + `rust-src`
  installed. Added a
  `build_std_nightly_skip()` precondition guard (reads the channel from
  `tools/rust-toolchain.toml`) so both skip cleanly with the exact
  `rustup` remedy when the toolchain is absent, instead of failing
  partway through with an opaque `can't find crate for 'core'`. Host
  remedy unchanged: `rustup toolchain install nightly-2026-04-11 &&
  rustup component add rust-src --toolchain nightly-2026-04-11`.

## Notes

- This is an INDEX. 177.1, 177.4, and 177.5 are **fixed**, but the
  2026-05-21 follow-up sweep found additional environment-sensitive
  build/test issues recorded above. Archive this doc only after 177.2 /
  177.3 migrate to their owning phases (171.0 / 175) and the follow-up
  runtime/setup groups have owners.
- The sweep also validated the Phase 176 unified jobserver
  (`build-all-jobserver`) end-to-end — not a build issue, recorded in
  Phase 176.
