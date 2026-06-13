# Phase 245 — Port qemu-riscv64-threadx examples to the Phase-212 Node+Entry shape

**Implements.** [issue 0049](../issues/0049-example-source-platform-rmw-leakage.md)
cluster C1, carved out of [phase-244](phase-244-example-source-cleanliness.md)
because it is a **re-architecture**, not a delete-the-wiring cleanup (10× the other
Wave-1 clusters). Aligns the examples with
[RFC-0024](../design/0024-multi-node-workspace-layout.md) (Node-pkg = agnostic
logic) + [RFC-0032](../design/0032-entry-codegen-pipeline.md) (`nros::main!()` owns
the boot scaffold).

**Goal.** Port all `examples/qemu-riscv64-threadx/{rust,c,cpp}/<role>` examples (6
roles × 3 langs ≈ 18–20 pkgs) from the dual-entry, manual-executor shape to the
**clean threadx-linux shape**, while preserving both build paths (pure-cargo zenoh
+ CMake/CycloneDDS). The sibling `examples/threadx-linux/{rust,c,cpp}` is the
**byte-for-byte target template** — same ThreadX family, only host (NSOS) vs
riscv64 bare-metal differs.

**Status.** Planned (2026-06-13). Design explored below. Prereqs present in dev env
(THREADX_DIR / NETX_DIR / riscv64-unknown-elf-gcc / riscv64imac target), so it is
build-verifiable.

---

## Why it's a re-architecture (current vs target)

**Current** (e.g. `rust/talker`): one crate, two entry points + manual runtime:
- `main.rs`: `#![no_std] #![no_main]` + `extern "C" fn main()` → `start_from_reset()`.
- `lib.rs`: `register_rmw()` (cfg `rmw-zenoh`/`rmw-cyclonedds`), `run_app(&Config)`
  (open `Executor`, create node/pub, `register_timer`, `loop { spin_once }`),
  `start_from_reset()` (cargo entry → board `run(config, run_app)`),
  `#[unsafe(no_mangle)] extern "C" fn app_main()` (CMake/Cyclone entry).
- `src/cyclonedds_app.c` + `CMakeLists.txt` (corrosion_import_crate +
  add_executable + nano_ros_link_rmw) drive the Cyclone path.
- C/C++ examples: manual `nros_support_init` + `nros_executor_init` + spin loop.

Leaks (issue 0049): P1 (manual executor/spin), P2 (`#![no_std]`/`#![no_main]` in
src), P3 (`register_rmw()`), P4 (`compile_error!` RMW guard + cfg forks), P6
(hardcoded `const LOCATOR`), P10 (C `nros_*_type_t{}` literals).

**Target** (the threadx-linux shape, already clean):
- **Rust** → split into a **Node pkg** (`<role>/src/lib.rs`: `#![no_std]` +
  `impl Node` + `register()` + body via `nros::node!()`; NO executor/spin/rmw) and
  an **Entry pkg** (`<role>_entry/`: `[[bin]]`, `[package.metadata.nros.entry]
  deploy = "threadx-qemu-riscv64"` + `[package.metadata.nros.deploy.*]`,
  `src/main.rs = nros::main!();`). The board's `BoardEntry::run` (already impl'd in
  `nros-board-threadx-qemu-riscv64`) owns executor + spin + RMW.
- **C** → `int nros_app_main(int argc, char **argv)` + `<nros/init.h>` /
  `<nros/publisher.h>` (framework owns the runtime); drop manual
  `nros_support_init`/`nros_executor_init`/spin. (`nros_app_main` /
  `NROS_APP_MAIN_REGISTER` is the standard entry, not a leak.)
- **C++** → `NROS_NODE_REGISTER` declarative component + baked `nros_system_main()`.
- **Cyclone/CMake path** → align to the threadx-linux CMake shape (which routes the
  Cyclone build through the baked system entry, not a per-example `cyclonedds_app.c`
  + hand-wired `app_main`).

---

## Design notes (explored)

1. **Board already supports the cargo path.** `nros-board-threadx-qemu-riscv64`
   impls `nros_platform::BoardEntry::run` (`src/lib.rs:185`) delegating to
   `nros_board_threadx::run_entry`, so `nros::main!()` (OwnedSpin) works with no
   board change. The deploy overlay (`run_with_deploy`, phase-244 E5) is NOT yet on
   this board — add it here for the P6 locator/domain de-hardcode (mirror the
   threadx-linux E5 override).
2. **Two build paths must both keep working.** The pure-cargo zenoh path (board
   `run`) and the CMake/Cyclone path. threadx-linux proves both coexist on the
   clean shape; replicate its CMake structure (per-role `CMakeLists.txt` that bakes
   `nros_system_main` + Corrosion-imports the Node staticlib) rather than the
   current per-example `cyclonedds_app.c` + `app_main`.
3. **bare-metal vs host deltas** (the only things that differ from threadx-linux):
   linker script / reset entry (the board/`nros::main!()` `target_os = "none"` arm
   already emits `extern "C" fn main`), no host `getenv` (locator comes from the
   deploy overlay, not `option_env!`/`getenv`), and the riscv64 startup that
   `cyclonedds_app.c` currently provides → fold into the board / baked entry.
4. **Reference, don't reinvent.** For each `<role>` and lang, the corresponding
   `examples/threadx-linux/<lang>/<role>{,_entry}` is the template; the port is
   mostly "make the riscv64 example look like its threadx-linux sibling, swap the
   deploy target + linker/startup."

---

## Work clusters (file-disjoint → parallelizable; ordered into waves)

Each `(lang, role)` is one example dir → file-disjoint → safe to parallelize. The
board-overlay enabler precedes the cargo-path de-hardcode.

### Wave 0 — board enabler
- [ ] **B0 — `run_with_deploy` on `nros-board-threadx-qemu-riscv64`.** Add the E5
  override (copy the threadx-linux `config_with_overlay`) so the Entry pkgs' deploy
  metadata threads locator/domain into `Config`. Blocks the P6 leg of every cluster.

### Wave 1 — template-proving (do first, end-to-end, both build paths)
- [ ] **T-rust — `rust/talker` → `talker` (Node) + `talker_entry` (Entry).** The
  reference migration: split the crate, `nros::node!()` Node, `nros::main!()` entry,
  delete `run_app`/`register_rmw`/`start_from_reset`/`app_main`, rewire
  `CMakeLists.txt` + retire `cyclonedds_app.c` to the baked entry. Build-verify BOTH
  zenoh (cargo) and cyclonedds (cmake) paths. Establishes the pattern for R*/C*/X*.
- [ ] **T-c — `c/talker`** and **T-cpp — `cpp/talker`** — same, mirroring
  `threadx-linux/{c,cpp}/talker`. Proves the C (`nros_app_main`) + C++
  (`NROS_NODE_REGISTER`) target shapes on riscv64.

### Wave 2 — remaining roles (parallel; each follows the Wave-1 template)
- [ ] **R* (rust):** listener, service-server, service-client, action-server,
  action-client → Node + Entry split. (action roles also drop P10 manual type
  registration → folds in phase-244 E3 once it lands.)
- [ ] **C* (c):** the same 5 roles → `nros_app_main` shape.
- [ ] **X* (cpp):** the same 5 roles → `NROS_NODE_REGISTER` shape.

(15 disjoint dirs in Wave 2 — dispatchable as parallel agents once the Wave-1
template is proven + reviewed.)

---

## Acceptance

- Every `examples/qemu-riscv64-threadx/<lang>/<role>` (+ new `_entry` for rust)
  builds on **both** the zenoh-cargo and cyclonedds-cmake paths
  (`riscv64-unknown-elf-gcc` + cmake + Corrosion + the riscv64imac target).
- The `threadx_riscv64_qemu_*` build-fixture / E2E gates stay green.
- Issue-0049 rubric over the group → 0 `major`; residual `minor` = node-lib
  `#![no_std]` only (E4-accepted).
- Source carries only agnostic Node logic; RMW/platform selection lives in
  `Cargo.toml [features]` / deploy metadata / `CMakeLists.txt`.
- Update issue 0049 C1 + phase-244 C1 → done; archive this phase doc.
