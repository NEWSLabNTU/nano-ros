# Audit findings — 2026-07-16

- Depth: **quick** (first run under the `/audit` skill; also its validation run)
- Categories: all (A–J); E/G/B via detection greps + confirm reader, C/I/F/H/J
  via one reader each
- Baseline: none (first findings log)
- Commit: `4853ab703` (+ upstream `main` of the same day)

Every finding below was read and confirmed by an agent, not just grepped.
Classification: all **new** (no baseline).

## P2

- J1 · `examples/qemu-riscv64-threadx/rust/*/src/cyclonedds_app.c` · P2 ·
  hand-written Cyclone descriptor-registration TU (strong override of
  `nros_rmw_cyclonedds_register_app_descriptors`) shipped in all 6 copy-out
  rust examples; plus the same family's `#[unsafe(no_mangle)] app_main` shim
  (`talker/src/lib.rs:83`), full CMake wiring (`talker/CMakeLists.txt:41`),
  and `extern crate … as _` link anchors · framework should own all four; the
  #195 `.init_array` walk likely lets the generated ctor TUs replace
  `cyclonedds_app.c` outright → **issue #205**
- C1/I1/I3 · `packages/core/nros-cpp/include/nros/node.hpp:686-760` · P2 ·
  `ROS_DOMAIN_ID`/`NROS_LOCATOR` env-overlay is business logic living only in
  the C++ header (C users get none of it), duplicated verbatim across two
  `init()` overloads, with the 232 domain-max inlined and malformed input
  silently collapsing to domain 0 · lift into the Rust core / one FFI helper;
  error on bad parse → **issue #206**
- I3 · `packages/zpico/nros-zpico-build/src/runner.rs:956-980` · P2 ·
  `size_probe` compile failure warn-and-continues with hardcoded
  `SOCKET_SIZE=16`/`ENDPOINT_SIZE=8` — a guessed pass-by-value ABI for
  `_z_sys_net_socket_t` (the code's own comment calls it a foot-gun) ·
  hard-fail on cross targets → **issue #207**
- F3/H1 · `setup.bash` + `justfile:2235,2270` + 3 book pages · P2 · stale
  second activation file diverges from the activate.sh SSoT (`NROS_ROOT` vs
  `NROS_REPO_DIR`); `just setup` still tells users to source it; zephyr /
  contributing / build-commands book pages still reference it · retire +
  repoint → **issue #208**
- F3/H3 · `book/src/getting-started/installation.md:182`,
  `book/src/reference/cli.md:64` · P2 · book advertises board id `esp32`
  which `nros-sdk-index.toml` does not define (only `qemu-esp32-baremetal`) →
  `nros setup esp32` fails; and top-level verb `nros init` has no CLI-reference
  section · → **issue #209**
- H1 · `packages/cli/CLAUDE.md:1` · P2 · entire file is the retired
  "colcon-cargo-ros2" guide — describes a different project · rewrite for the
  in-tree nros CLI → **issue #210**
- J1 · `examples/zephyr/rust/*/build.rs` + every workspace `zephyr_entry`
  `build.rs` · P2 · per-example Kconfig→rustc-env locator/domain bake
  (documented "known-issue #17" workaround) copy-pasted across all zephyr rust
  examples + workspace entries · a shared build-helper crate → **issue #211**
- J1 · `examples/workspaces/ws-custom-msg-c/src/reading_talker_pkg/src/ReadingTalker.c:42`
  (+ mixed/cpp variants) · P2 · hand-rolled CDR (manual encapsulation header,
  fixed byte offsets, hand-typed DDS type-name string) because codegen emits no
  C typesupport for workspace custom messages · → **issue #212**

## P3 (report-only, not filed)

- C1 · `packages/core/nros-cpp/include/nros/lifecycle.hpp:130-148` ·
  `LifecycleNode::autostart` re-implements the REP-2002 target→transition map
  instead of forwarding to `nros_cpp_lifecycle_autostart` (justified by
  callback-binding order; policy now in two places).
- J1 · `examples/native/rust/lifecycle-node/src/main.rs:44` · five
  `unsafe extern "C"` lifecycle callbacks + raw slot registration in a RUST
  example ("exercises the FFI surface") — points at a missing safe Rust
  lifecycle-callback API; kept P3 pending a design decision (borderline P2).
- J1 · `examples/qemu-arm-baremetal/rust/talker/src/main.rs:3`,
  `examples/stm32f4/rust/talker-rtic/src/main.rs:24` · panic-handler /
  defmt-transport selection lives in example bodies while the threadx board
  crate owns its `#[panic_handler]` — inconsistent convention (app-level
  choice is defensible; flag only).
- G2 · `examples/px4/cpp/uorb/nros-register-check/CMakeLists.txt:21` ·
  in-repo validation module uses fixed sibling offsets; could accept
  `-DNANO_ROS_ROOT` for symmetry with the threadx examples.

## Confirmed clean / false positives

- **A2** — all 22 `STREQUAL ""` grep hits guarded (`DEFINED`-first or
  cache-var always set). **E1** — every `Command::new(cargo|cmake|west|idf.py)`
  hit is a `--version` prerequisite probe, a configure-only negative test, a
  `cargo tree` graph read, or the sanctioned diagnostic-verbatim compile;
  fixture `build_*` helpers all resolve prebuilt + error toward
  `just build-test-fixtures`. **E3** — no phase-numbered test names. **G2** —
  walk-ups are the sanctioned 3-tier resolver fallback / documented Pattern A.
  **B7** (sampled) — hits are `cargo:` build-script lines or gated.
- **C4 (RFC-0049)** — adoption clean: schema `deny_unknown_fields`, tri-state
  env front-end, `KnobSource` provenance, capability downgrade+warn;
  `zenoh_platforms.toml` genuinely retired; remaining raw env reads are the
  documented tenant-by-tenant migration.
- **H2** — sampled cross-links (CLAUDE.md table, issues README, design README)
  all resolve.

## Coverage

Read-confirmed surfaces per category are listed in each reader's coverage
note (session 2026-07-16). Not covered this run: C2 layer-map direction
sweep, C3 generated-dir hand-edit scan, deep B1/B3/B5/B6 code reads
(grep-sampled only), esp32/px4/bridge examples for J1, per-platform book
pages beyond installation.md, exhaustive book link crawl, D (codegen template
drift), runner.rs lines 1162+. Recommend `/audit deep B,D` as the next
targeted run.
