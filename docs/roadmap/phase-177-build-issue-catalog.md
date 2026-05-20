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

**Priority.** P2. One row (177.1) is the sole blocker to a green
`build-all`; the rest are deferred-by-design or host-environment.

## Open issues

### 177.1 — cyclonedds-zephyr `nsos_adapt.c` duplicate case → **Phase 171.0.d**

The only thing failing `just build-all` on `main`. `nsos-adapt-ipproto-ip-patch.sh`
(Phase 11W.12) adds a second `case NSOS_MID_IPPROTO_IP:` to the
`nsos_adapt_setsockopt` switch that an earlier NSOS patch already
populated → `error: duplicate case value` → all 54 cyclonedds-zephyr
fixtures fail. The 7 cargo platforms + zenoh/xrce-zephyr build clean.
**Owned by Phase 171.0.d** (fix: merge into the existing case, don't
emit a second label).

### 177.2 — cyclonedds-zephyr feature gaps → **Phase 171.0.a / .b / .c**

- C-service request delivery (C client→server) — 171.0.a.
- Actions all languages (IDL `.action` converter unbuilt) — 171.0.b.
- aemv8r FVP reference re-verify — 171.0.c.

### 177.3 — Cyclone for pure-cargo Rust examples → **Phase 175**

`nros_rmw_cyclonedds_register` lives only in the C++/CMake build, so
`cargo build --features rmw-cyclonedds` of a native/freertos/threadx
Rust example can't link it. Fixture matrices are zenoh-only; the
feature stays defined-but-unbuilt. **Owned by Phase 175** (native
CMake/Corrosion glue + a ddsrt RTOS port for embedded). Decision
2026-05-21: keep Cyclone targeted at bare metal (don't delete the
embedded cells) — see Phase 171.B.

### 177.4 — esp_idf setup git-ref corruption (host environment)

`just esp_idf setup` fails: `cannot update ref 'refs/heads/v5.3':
trying to write non-commit object … to branch` in
`esp-idf-workspace/esp-idf`. A corrupted local clone, not a code issue.
esp_idf is `extended`-tier and NOT exercised by `just ci` / `test-all`,
so it doesn't gate the default build. Remedy: re-clone the esp-idf
workspace (`rm -rf esp-idf-workspace && just esp_idf setup`).

### 177.5 — NuttX C/C++ generated-package e2e needs pinned nightly (host)

`fixture_workspace_builds_generated_nuttx_package` (codegen
orchestration e2e) builds `armv7a-nuttx-eabihf` via `-Z build-std`,
which needs the pinned `nightly-2026-04-11` + `rust-src` (matches the
in-tree libc fork). Skips/fails if that toolchain isn't installed.
Remedy: `rustup toolchain install nightly-2026-04-11` (the generated
package's `rust-toolchain.toml` pins it). Host-only.

## Fixed during the sweep (2026-05-20/21 — no longer issues)

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

## Notes

- This is an INDEX. When 177.1 (the build-all blocker) closes, `just
  build-all` is green end-to-end; strike its row. Archive this doc once
  all rows resolve or migrate to their owning phases.
- The sweep also validated the Phase 176 unified jobserver
  (`build-all-jobserver`) end-to-end — not a build issue, recorded in
  Phase 176.
