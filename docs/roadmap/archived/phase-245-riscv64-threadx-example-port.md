# Phase 245 — Port qemu-riscv64-threadx examples to the clean app-node shape

**Implements.** [issue 0049](../issues/0049-example-source-platform-rmw-leakage.md)
cluster C1, carved out of [phase-244](phase-244-example-source-cleanliness.md)
because it is a **re-architecture**, not a delete-the-wiring cleanup (10× the other
Wave-1 clusters). Aligns the examples with
[RFC-0032](../design/0032-entry-codegen-pipeline.md) (`nros::main!()` owns the boot
scaffold) while keeping the **app-node** taxonomy intact.

**Taxonomy (load-bearing — do NOT confuse the two).**
`examples/<platform>/<lang>/<role>` are **app nodes**: a single package that owns
`main()`. `examples/workspaces/*` are **workspace examples**: nodes are library
dirs (Node + Entry split). The riscv64-threadx examples are platform/lang app
nodes → they stay **single-package with a `main`**; they are NOT split into a
Node + Entry pair. (The earlier draft of this phase mistakenly targeted the
workspace Node+Entry shape; corrected.)

**Goal.** Port all `examples/qemu-riscv64-threadx/{rust,c,cpp}/<role>` examples (6
roles × 3 langs ≈ 18–20 pkgs) from the dual-entry, manual-executor shape to the
**clean single-package app-node shape**, preserving both build paths (pure-cargo
zenoh + CMake/CycloneDDS) and **all RMWs incl. CycloneDDS**. The clean shape per
language:

- **Rust** → one package. `src/lib.rs`: `#![no_std]` + `impl Node` + `register()`
  via `nros::node!(Role)` (agnostic logic only — node/pub/sub/timer + callback
  body; NO executor open / spin / `register_rmw` / locator). `src/main.rs`:
  `#![no_std] #![no_main]` + `nros::main!();` (reads
  `[package.metadata.nros.entry] deploy = "threadx-qemu-riscv64"` + the deploy
  overlay block). The board's `BoardEntry::run` owns executor + RMW + spin
  (zenoh/cargo path). For the **CycloneDDS/CMake** path the same crate exposes a
  thin `#[cfg(feature = "rmw-cyclonedds")] extern "C" fn app_main()` that calls
  the board's post-kernel `run_app_thread(register)` (see design note 2).
- **C** → `nros_app_main` + `<nros/*.h>` (framework owns the runtime).
- **C++** → `NROS_NODE_REGISTER` declarative component + baked `nros_system_main()`.

RMW selection lives in `Cargo.toml [features]` (Rust) / CMake `-DNROS_RMW` (C/C++);
locator + domain in `[package.metadata.nros.deploy.*]` — never in source.

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

**Target** (clean single-package app node — keeps `main`, both build paths):
- **Rust** → ONE crate (NOT a Node + Entry split). `src/lib.rs`: `#![no_std]` +
  `impl Node` + `register()` via `nros::node!(Role)` (agnostic logic only). The
  crate's `[lib] crate-type = ["staticlib", "rlib"]` stays (the staticlib is the
  CycloneDDS/CMake link surface). `src/main.rs`: `#![no_std] #![no_main]` +
  `nros::main!();` (reads `[package.metadata.nros.entry] deploy` +
  `[package.metadata.nros.deploy.*]`). The board's `BoardEntry::run` (impl'd in
  `nros-board-threadx-qemu-riscv64`) owns executor + spin + RMW on the zenoh/cargo
  path; the CycloneDDS path's thin `app_main` calls the board's `run_app_thread`.
  `lib.rs` keeps an unconditional `extern crate nros_board_threadx_qemu_riscv64
  as _;` so the standalone `staticlib` target inherits the board's
  `#[panic_handler]` + allocator even when no symbol is named (zenoh path enters
  via `main.rs`, not the lib).
- **C** → `int nros_app_main(int argc, char **argv)` + `<nros/init.h>` /
  `<nros/publisher.h>` (framework owns the runtime); drop manual
  `nros_support_init`/`nros_executor_init`/spin. (`nros_app_main` /
  `NROS_APP_MAIN_REGISTER` is the standard entry, not a leak.)
- **C++** → `NROS_NODE_REGISTER` declarative component + baked `nros_system_main()`.
- **Cyclone/CMake path** → unchanged shape (`src/cyclonedds_app.c` empty TU +
  descriptor reg + `CMakeLists.txt` corrosion-import of the staticlib); only the
  Rust `app_main` body changes (no manual executor/spin — delegates to the board).

---

## Design notes (explored)

1. **Board already supports the cargo path.** `nros-board-threadx-qemu-riscv64`
   impls `nros_platform::BoardEntry::run` (`src/lib.rs:185`) delegating to
   `nros_board_threadx::run_entry`, so `nros::main!()` (OwnedSpin) works with no
   board change. The deploy overlay (`run_with_deploy`, phase-244 E5) is NOT yet on
   this board — add it here for the P6 locator/domain de-hardcode (mirror the
   threadx-linux E5 override).
2. **Two build paths, two kernel-entry routes — the load-bearing fix is
   `run_app_thread` (verified 2026-06-13).** Both paths boot through the board's C
   ThreadX glue; the difference is *who calls `tx_kernel_enter()`* and what the
   app thread runs:
   - **zenoh/cargo:** `nros::main!()` emits `extern "C" fn main` →
     `BoardEntry::run(register)` → `nros_board_threadx::run_entry` → registers
     `app_task_entry_runtime` as the app callback → `tx_kernel_enter()`. The app
     thread opens the executor, runs `register`, spins. Kernel entered by Rust.
   - **CycloneDDS/CMake:** the board's **C** `startup.c::main` calls
     `tx_kernel_enter()` itself and dispatches to the example's Rust `app_main`
     *inside* the spawned ThreadX app thread. So **the kernel is already running
     when `app_main` is reached** — `app_main` must NOT call `BoardEntry::run`
     (that re-enters the kernel → double init). It must run only the *post-kernel
     body* (sleep → open executor → `register` → spin).

   Fix: factor that post-kernel body out of `app_task_entry_runtime` into a public
   `nros_board_threadx::run_app_thread<B, C, F, E>(config, setup) -> !`
   (`entry.rs`), re-exported per-board as
   `nros_board_threadx_qemu_riscv64::run_app_thread(setup)` (defaults `Config`).
   The example's cyclone `app_main` becomes a one-liner:
   `nros_board_threadx_qemu_riscv64::run_app_thread(register)`. No `cyclonedds_app.c`
   rewrite, no baked `nros_system_main`, no startup re-fold — the existing C
   startup + empty-TU + `CMakeLists.txt` shape is preserved; only the Rust
   `app_main` body changed (manual `run_app` → board `run_app_thread`). This is far
   lighter than the baked-entry approach the first draft assumed.
3. **Not host-checkable — verify via the firmware build.** The board crate's
   `build.rs` compiles ThreadX riscv64 assembly (`tx_thread_*.S`) via
   `riscv64-unknown-elf-gcc`, so `cargo check` on the host target fails in cc-rs
   (unrelated to the Rust source). B0 + every cluster must be verified through the
   actual riscv64 firmware build (cmake + Corrosion + the riscv64imac target), not
   host `cargo check`.
4. **bare-metal specifics** (vs the threadx-linux host sibling): linker script /
   reset entry (the board global-asm `_start` → C `main`; `nros::main!()`'s
   `target_os = "none"` arm emits that `main` on the cargo path, the board's
   `startup.c` provides it on the cyclone path), no host `getenv` (locator comes
   from the deploy overlay, not `option_env!`/`getenv`). The riscv64 startup stays
   in the board's `startup.c` — no fold needed once `run_app_thread` removes the
   double-kernel-enter hazard (design note 2).
5. **Reference the proven talker.** Once T-rust lands + both paths verify, every
   other `(lang, role)` follows it byte-for-byte: same single-package app-node
   layout, same `Cargo.toml [nros.entry]` + `[nros.deploy.*]`, same thin cyclone
   `app_main`. The agnostic-logic sibling at `examples/threadx-linux/<lang>/<role>`
   is the cross-check for the `register()` body.

---

## Work clusters (file-disjoint → parallelizable; ordered into waves)

Each `(lang, role)` is one example dir → file-disjoint → safe to parallelize. The
board-overlay enabler precedes the cargo-path de-hardcode.

### Wave 0 — board enablers
- [x] **B0 — `run_with_deploy` on `nros-board-threadx-qemu-riscv64`.** E5 override +
  `config_with_overlay` so the app-node's deploy metadata threads locator/domain
  into `Config`. Blocks the P6 leg of every cluster.
- [x] **B1 — `run_app_thread` post-kernel entry** (`nros-board-threadx` `entry.rs`,
  re-exported from `nros-board-threadx-qemu-riscv64`). The CycloneDDS path's
  kernel-already-entered fix (design note 2). Blocks the cyclone leg of every Rust
  cluster.

### Wave 1 — template-proving (do first, end-to-end, both build paths)
- [x] **T-rust — `rust/talker`** (clean single-package app node). `nros::node!()`
  logic in `lib.rs`, `nros::main!()` in `main.rs`, deleted
  `run_app`/`register_rmw`/`start_from_reset` + the `compile_error!` guard + the
  `const LOCATOR`; cyclone `app_main` → board `run_app_thread(register)`;
  `CMakeLists.txt` + `cyclonedds_app.c` unchanged. **Verified:** zenoh/cargo
  firmware ELF builds; CycloneDDS-feature staticlib compiles for riscv64; cyclone
  CMake firmware links (fresh configure — the stale build dir's pre-241.B.2
  `NROS_PLATFORM_CFFI_INCLUDE` cache is the only gotcha). Establishes the pattern
  for R*/C*/X*.
- [x] **T-c — `c/talker`** and **T-cpp — `cpp/talker`** — **DONE via
  [phase-246](phase-246-threadx-typed-entry-runtime.md) W2** (2026-06-13). Both
  ported to the typed component shape (`NROS_C_COMPONENT` / `configure(Node&)`),
  both RMW firmwares cross-build (riscv64 ELF, zenoh + CycloneDDS), runtime is the
  real-executor `ThreadxBoard::run_components` path (no manual init/spin/locator,
  no baker stub). Note below kept for the design trail.

  _(historical design trail — the path that got T-c/T-cpp here)_ — deferred to
  [phase-246](phase-246-threadx-typed-entry-runtime.md) (framework integration).
  The clean declarative C/C++ shape needs a *working* component runtime. An initial
  reading (against the **retired** RFC-0032/236 synthesizing-interpreter path)
  suggested this was unbuilt framework-wide; re-examined against
  [RFC-0043](../design/0043-entry-real-callback-binding.md) /
  [RFC-0044](../design/0044-rclcpp-faithful-component-model.md), the **typed-entry**
  runtime (component-as-object + identity-bound real callbacks + `run_components`)
  is **landed and proven** on native (E2E) + NuttX (QEMU E2E), and the CLI is **not**
  frozen (RFC-0043 edits `nros codegen entry`). The only gaps are the **ThreadX leg**
  (codegen `board_cpp_path` case + a `threadx_entry_main_typed.cpp.in` template +
  cmake carrier wiring) and **populating the example launches** — a bounded
  integration, scoped as phase-246. The phase-245 groundwork (`ThreadxBoard` adapter
  in `main.hpp`) feeds it; the baker `RUNTIME=cpp` mode it also added drove the
  *retired* interpreter path and is superseded by phase-246's typed template.
  Once phase-246 lands, T-c / T-cpp are straight ports onto the proven template.

### Wave 2 — remaining roles — **zenoh DONE (2026-06-14)**; cyclone tail pending
Migrated all 15 (5 roles × rust/c/cpp) off the OLD manual shape via a 30-agent
workflow; target shapes are the post-246 ones (not the pre-246
`nros_app_main`/`NROS_NODE_REGISTER` these headings predate):
- [x] **X* (cpp):** listener / service-server / service-client / action-server /
  action-client → `configure(Node&)` typed component + `nano_ros_node_register(TYPED)`
  carrier (copy of the threadx-linux cpp role components, riscv64 namespace).
- [x] **C* (c):** the same 5 → `NROS_C_COMPONENT` raw-callback component + TYPED carrier.
- [x] **R* (rust):** the same 5 → single-package app-node (`impl Node` +
  `nros::node!()` + `nros::main!()` + `[nros.entry]` + thin cyclone `app_main` →
  `run_app_thread`), mirroring the rust talker.
- All 15 **cross-build on the riscv64 zenoh path**; sample QEMU boot-gates pass
  (cpp/listener "Waiting for messages", cpp/service-server "Waiting for requests",
  rust/action-server "nros ThreadX Platform").
- [x] **CycloneDDS tail (done 2026-06-14)** — per-role `src/cyclonedds_app.c`
  strong-overrides the weak `nros_rmw_cyclonedds_register_app_descriptors` with the
  role type's descriptors: Int32 `register_Int32_0`; AddTwoInts
  `register_AddTwoInts_0/_1`; Fibonacci `register_Fibonacci_0..7` (from the idlc
  type support). c/cpp wire a CMakeLists `if(cyclonedds) target_sources(…)` block;
  rust gets a per-role cyclone CMakeLists (corrosion + cyclone, mirror the talker).
  **All 15 cyclone firmwares cross-build**; sample boot-gates confirm the runtime
  (c/listener "Waiting for messages"; cpp/action-server reaches `run_components`
  spin — the Fibonacci action server create succeeds, i.e. the 8 descriptors
  register). Migrated + built via two workflows (transient API rate-limits handled
  by resume + a couple of direct straggler builds).

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
