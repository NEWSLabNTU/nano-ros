# Phase 208.acc.5 — Multi-agent strict-follow re-audit (Batches 1–6, all tutorials)

**Acceptance bar:** `a strict-follow re-audit of any tutorial produces 0
BLOCKERS in the report.`

**Final result: MET — 13 / 13 audited tutorials return 0 BLOCKERS** after
the per-batch fix commits below. The first run on each batch surfaced 8
real BLOCKERs across freertos, bare-metal, integration-zephyr, and px4;
each landed a fix commit and re-runs against the same step list now pass.

## Per-batch summary

| Batch | Tutorial | First-run BLOCKERs | Fix commit | Final |
|---|---|---|---|---|
| 1 | installation | 0 | — | ✅ |
| 1 | first-node-rust | 0 | — | ✅ |
| 1 | first-node-c | 0 | — | ✅ |
| 1 | first-node-cpp | 0 | — | ✅ |
| 1 | troubleshoot-first-10-min | 0 | — | ✅ |
| 2 | threadx | 0 | — | ✅ |
| 2 | freertos | 1 (`just freertos zenohd` hardcoded path) | `89f69d911` | ✅ |
| 2 | bare-metal | 4 (namespace + path + workspace + codegen) | `89f69d911` + `phase-208-followups.md` | ✅ |
| 3 | integration-nuttx | 0 (FRICTION-class port + env-var fix) | `2bb0dfdcc` | ✅ |
| 4 | integration-zephyr | 2 (west.yml missing `revision: main`, parent didn't pin Zephyr) | `5e24268d1` | ✅ |
| 5 | esp32 | 0 (4 FRICTION items fixed for clarity) | `3b17fcc66` | ✅ |
| 5 | integration-esp-idf | 0 | — | ✅ |
| 6 | px4 | 1 (`EXTERNAL_MODULES_LOCATION` missing `/nano-ros`) | `53ef20a53` | ✅ |

## Batch-by-batch fix detail (only items with code/doc changes)

### Batch 2 — Recipes pointed at retired `build/zenohd/` path

All `just <plat> zenohd` recipes (qemu-baremetal / esp32 / freertos /
threadx-riscv64 / nuttx / native / zephyr / threadx-linux):
`build/zenohd/zenohd` → plain `zenohd` (D.2 shim resolves it).
`bare-metal.md`: `just qemu-baremetal` → `just qemu` (the justfile maps
`mod qemu 'just/qemu-baremetal.just'`).

### Batch 3 — NuttX

Port mismatch — doc cited `examples/qemu-arm-nuttx/c/talker/nros.toml`
(7552) but `just nuttx zenohd` binds 7452 per the CLAUDE.md platform
table. Switched citation to the Rust variant (7452, matches the
recipe) + added a note on the per-language port pattern (Rust 7452,
C 7552, C++ 7652). `$NUTTX_APPS` → `$NUTTX_APPS_DIR` (9 occurrences;
matches `.envrc`).

### Batch 4 — Zephyr

`west.yml` snippet (in the doc + the in-tree `zephyr/west.yml` comment
block) lacked `revision: main`. `west` defaults to `master`; the repo
has `main`; `west update` failed verbatim with `couldn't find remote
ref master`. Added `revision: main`.

The doc told users Zephyr must be in the parent manifest but didn't
show how. Added explicit `zephyr` remote + project entry
(`url-base: https://github.com/zephyrproject-rtos`, `revision: v3.7.0`,
`import: true`).

Added `west init -l .` step at the top of Build for fresh manifest-only
workspaces.

### Batch 5 — ESP

`esp32.md`: rewrote the Setup paragraph to drop the "prebuilt esp-hal
toolchain" overstatement (esp-hal is a Cargo dep; the only manual step
is `rustup target add riscv32imc-unknown-none-elf`). `just esp32 build`
comment fixed (it builds real-hw fixtures; `build-qemu` is the QEMU
one). Added a one-liner that the example's `build.rs` invokes
`nros generate-rust` automatically on first build. Readiness timing
acknowledges the `just esp32 talker` rebuild.

### Batch 6 — PX4

`EXTERNAL_MODULES_LOCATION` cmake var in all 3 snippets missed the
`nano-ros/` suffix — PX4's root CMakeLists does
`add_subdirectory("${EXTERNAL_MODULES_LOCATION}/src" …)`, so the
parent-dir form fails configure. Added the suffix to the 3 snippets +
an inline note. Troubleshoot bullet updated to match the template's
real log line (`nros_rmw_uorb_register() -> <rc>`, not the fictional
`nano-ros: register failed`).

## Deferred (`phase-208-followups.md` F-items)

F1 (empty `[workspace]` on ~80 examples), F2 (codegen pre-step
documentation), F3 (`just doctor tier=default` probe target), F5
(`ros2 topic echo` QoS hint), F6 (more troubleshoot-10min stale
strings), F7 (installation.md cyclonedds heads-up + `~/.nros/sdk`
naming), F8 (bare-metal `-nic` flag), F11 (threadx_riscv64 cyclonedds
regen) all still open. None are BLOCKERS in the strict-follow sense.

## Files

- Per-tutorial reports: `docs/roadmap/book-audit/acc5/<tutorial>.md`.
- This SUMMARY.
- Phase-doc bookkeeping: `docs/roadmap/phase-208-book-starter-tutorial-audit.md::208.acc.5`.
