# Phase 246 — ThreadX typed-entry runtime (extend RFC-0043/0044 to bare-metal)

**Implements.** [RFC-0043](../design/0043-entry-real-callback-binding.md) (entry
real-callback binding) + [RFC-0044](../design/0044-rclcpp-faithful-component-model.md)
(rclcpp-faithful component model), extending the **already-landed** typed-entry
pipeline to the ThreadX family (threadx-linux + bare-metal riscv64). Unblocks
[phase-245](phase-245-riscv64-threadx-example-port.md) clusters **T-c / T-cpp**
(and Wave-2 C\*/X\*), which depend on a *working* declarative C/C++ component
runtime on ThreadX.

**Status.** Planned (design below, 2026-06-13). Build-verifiable in this dev env
for threadx-linux (host) + the riscv64 cross firmware; full QEMU+zenohd pub/sub
E2E is env-limited (boot-gate + artifact assertion instead — see
[phase-245](phase-245-riscv64-threadx-example-port.md) note 3).

---

## Why this is an integration phase, not a runtime invention

The earlier phase-245 finding ("a working clean C/C++ node exists nowhere; the
runtime is unbuilt; `nros plan` is frozen") was **based on the retired
RFC-0032/236 path** (the synthesizing `EntryNodeRuntime` interpreter +
`DeclaredNode`/`record_callback_effect` string seam, where `register()` was a TODO
stub). [RFC-0043](../design/0043-entry-real-callback-binding.md) **supersedes**
that: the Entry path now routes to the **real Rust executor** with callbacks bound
by **identity** (closures / member-fn-pointer trampolines), no string names, no
interpreter. That path is **landed and proven** (verified in code 2026-06-13):

- **Component = stateful object.** `class Talker { … void on_tick(); Result
  configure(::nros::Node&); }` — `configure` creates entities + binds real bodies
  via `nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, ms, this)` (no
  alloc, no name). Proven: `examples/templates/multi-node-workspace-cpp-typed/`
  publishes on native E2E (2026-06-12); NuttX C/C++ pub/sub/service/action E2E in
  QEMU (2026-06-13). `nros-cpp/include/nros/component.hpp` + `component_node.hpp`.
- **Typed entry codegen.** `nros codegen entry --lang cpp --typed --metadata
  <json> --workspace <dir> --launch <pkg:file.launch.xml> --out <main.cpp>`
  (`packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs::emit_typed`) emits:
  `#include` the component header → static component + node storage (no heap) →
  `__nros_entry_setup()` constructs + `configure(node)` (or RFC-0044 placement-new
  with handle) → `Board::run_components(&setup)` (init → setup → `spin_once` loop →
  shutdown). Shape-branched on `"configure"` vs `"rclcpp"` (Phase 242.4).
- **`run_components` on every board.** `NativeBoard` / `ZephyrBoard` / `NuttxBoard`
  — and `ThreadxBoard` (added as phase-245 groundwork, `main.hpp`). Real executor
  spin (`detail::component_spin_loop`), no synthesizing interpreter.
- **`nros plan`** parses `system.launch.xml` `<node pkg= exec= name=/>` rows →
  `PlanNode`s, enriched from `nros-metadata.json` (`nano_ros_node_register(HEADER
  …)`) to `{class_name, class_header, lang, shape}`
  (`codegen/entry/{mod.rs,metadata.rs}`). It **bails on zero nodes**. **The CLI is
  not frozen** — RFC-0043 (Stable, 2026-06) edits `nros-cli-core/codegen/entry`
  directly; the 212.H.4 freeze (the reason `NanoRosThreadxSystemCodegen.cmake`
  exists as a stand-in) is superseded.

**So "empty launch everywhere" is un-migrated example data, not a framework
hole.** What is genuinely missing is the **ThreadX leg** of this pipeline — and a
populated launch per ThreadX C/C++ example.

---

## The ThreadX gap (current vs target)

| Piece | Native / NuttX / Zephyr | ThreadX (today) |
| --- | --- | --- |
| `run_components` board adapter | ✅ | ✅ (`ThreadxBoard`, phase-245 groundwork) |
| `board_cpp_path()` case (emit_cpp.rs) | ✅ | ❌ falls through to `NativeBoard` |
| `*_entry_main_typed.cpp.in` template | ✅ nuttx + zephyr | ❌ none |
| cmake carrier wiring (`nano_ros_node_register` TYPED branch) | ✅ nuttx/zephyr | ❌ uses the legacy `NanoRosThreadxSystemCodegen` stub |
| populated `system.launch.xml` | ❌ (all empty placeholders) | ❌ |
| component in `configure(Node&)` shape | ✅ template/native | ❌ examples are manual `nros_app_main` |

**Boot-entry symbol — already aligned.** NuttX's typed template ends in
`extern "C" int nros_app_main(...)` + `NROS_APP_MAIN_REGISTER_VOID()` (emits `void
app_main(void)`). Bare-metal riscv64-threadx boots **the same symbol**: the board
`startup.c` calls `nros_threadx_set_app_main(app_main)` + the link keeps it live
via `-u app_main`. So a ThreadX typed template is the NuttX one with
`NuttxBoard` → `ThreadxBoard` — no new entry-dispatch design. (`ThreadxBoard`
runs in the already-entered app thread, like NuttX; it must NOT re-enter the
kernel — `run_components` doesn't, by construction.)

---

## Design decisions

1. **Unify ThreadX onto the typed carrier; fully retire the stand-in baker
   (maintainer-decided 2026-06-13).** Route ThreadX C/C++ examples through the same
   `nano_ros_node_register(TYPED …)` + `nros codegen entry --typed` path
   NuttX/Zephyr use, rendering a new `cmake/templates/threadx_entry_main_typed.cpp.in`.
   `NanoRosThreadxSystemCodegen.cmake` is **deleted outright** (not just demoted) —
   its 212.H.4 reason ("stand-in until the CLI subcommand lands") is moot now that
   `nros codegen entry` is that subcommand. Both its modes go: the phase-245
   `RUNTIME=cpp` mode (which drove the **retired** `ThreadxBoard::run(NodeContext*)`
   interpreter) **and** the legacy `stub` mode (the NULL-context marker used by the
   rust-component `multi_pkg_workspace_threadx` fixture). Because deletion strands
   that fixture, the retirement is staged as a **gradual migration** (W3): first
   migrate the rust fixture onto the Rust dispatch path (`ExecutorNodeRuntime` /
   `run_entry`, the FreeRTOS/NuttX shape), THEN delete the baker. `ThreadxBoard::run`
   (the interpreter adapter) is dropped alongside (`run_components` is the only
   ThreadX entry that remains).
2. **`board_cpp_path()` += ThreadX.** Add `"threadx" | "threadx-linux" |
   "threadx-qemu-riscv64" | "qemu-riscv64-threadx" => "::nros::board::ThreadxBoard"`
   (`emit_cpp.rs:133`). The cmake side derives the board key from the deploy
   target / board runner, mirroring the Zephyr/NuttX derivation.
3. **Locator + domain bake** (embedded: compile-time, never env — CLAUDE.md).
   The ThreadX carrier branch sets `@NROS_ENTRY_LOCATOR@` (from deploy metadata /
   a `NROS_THREADX_LOCATOR` default `tcp/10.0.2.2:7553`) and
   `NROS_ENTRY_DOMAIN_ID` (`CONFIG_NROS_DOMAIN_ID` / deploy `domain_id`), exactly
   as NuttX bakes `NROS_NUTTX_LOCATOR`. CycloneDDS ignores the locator (no router);
   domain still applies.
4. **No-heap arena — already satisfied.** The typed entry uses `static`
   component + node storage (configure shape) or a `static alignas(C) unsigned
   char buf[sizeof(C)]` placement-new slot (rclcpp shape). No `alloc`. Correct for
   bare-metal riscv64 (RFC-0043 Open Q2 resolved by static storage). Confirm the
   `sizeof(Component)` include path composes under the riscv64 toolchain.
5. **Both build paths, both languages.** zenoh/cargo-driven? No — the C/C++
   ThreadX firmware is **CMake-driven** (it always was; only the Rust examples use
   the cargo path). So both the zenoh and CycloneDDS C/C++ firmwares are CMake
   builds; the typed entry TU + the component link into each, plus the RMW backend
   (`nano_ros_link_rmw`). The CycloneDDS variant keeps its descriptor-registration
   TU (`cyclonedds_app.c` `register_*`); only the entry/runtime changes.
6. **C path parity.** A C component is `NROS_C_COMPONENT` (struct state + a C
   configure fn binding C-ABI callbacks on the executor via the C FFI). The typed
   entry routes a `lang == "c"` node through the `__nros_c_component_<pkg>_{create,
   configure}` seam (Phase 240.4), handing it the node's `ffi_handle()`. Same
   executor, same template — only the per-node construct differs (already handled
   by `emit_typed`).
7. **Rust ThreadX examples are out of scope** — they already run via the Rust
   executor (`nros::main!()` self-bringup + `run_app_thread`, phase-245 T-rust).
   The launch/typed-entry pipeline is the C/C++ (and Rust *workspace*) story; the
   riscv64 Rust *app-node* talker needs none of it.

---

## Resolved (maintainer feedback 2026-06-13)

- **Q1 — retire the rust `stub` baker → YES**, gradually (W3). Migrate
  `multi_pkg_workspace_threadx` (`threadx_corrosion_bringup`) onto the Rust
  dispatch path (`ExecutorNodeRuntime` / `run_entry`) first, then delete
  `NanoRosThreadxSystemCodegen.cmake`. A *mixed* C/C++/Rust ThreadX system stays
  out of scope (each system is single-language for now).
- **Q2 — carrier → `nano_ros_node_register(TYPED)`** (the NuttX/Zephyr branch),
  not a new `nano_ros_entry()` arm. One carrier path for all embedded boards.
- **Q4 — drop `ThreadxBoard::run` + the baker `RUNTIME=cpp` mode in THIS phase**
  (it owns them). The shared `EntryNodeRuntime` interpreter deletion stays with the
  cross-platform retirement (RFC-0043 §Retirement / phase-240.6).

## Open (verify during impl)

- **Q3 — CycloneDDS firmware composition.** Confirm the typed entry TU links into
  the per-example CycloneDDS riscv64 firmware (the descriptor TU + idlc-generated
  type support + `ThreadxBoard::run_components`) without the old `app_main`
  hand-wiring. (The only genuinely unverified seam — everything else mirrors a
  proven NuttX/native path.)

---

## Work items (ordered waves; each build-gated, gradual migration)

The migration is staged so each wave lands green before the next, and the legacy
ThreadX baker is removed only **after** its last consumer moves off it (W3).

### W0 — codegen + template (the reusable core)
- [ ] **W0.1** `board_cpp_path()` ThreadX case (`emit_cpp.rs`):
  `"threadx" | "threadx-linux" | "threadx-qemu-riscv64" | "qemu-riscv64-threadx"
  => "::nros::board::ThreadxBoard"`. Unit-test in `tests/entry_typed_plan.rs`
  (add a threadx board row).
- [ ] **W0.2** `cmake/templates/threadx_entry_main_typed.cpp.in` — mirror the
  NuttX typed template (`NuttxBoard` → `ThreadxBoard`; same shape-branch + the
  `app_main` `NROS_APP_MAIN_REGISTER_VOID()` tail).
- [ ] **W0.3** `nano_ros_node_register` ThreadX TYPED branch (locator/domain bake +
  render W0.2 + link the component lib + `nros_platform_link_app`).
- [ ] **W0.4** Drop the phase-245 baker `RUNTIME=cpp` mode (`NanoRosThreadxSystemCodegen.cmake`)
  + `ThreadxBoard::run` (the interpreter adapter) from `nros-cpp/main.hpp`. Leaves
  `ThreadxBoard::run_components` as the sole ThreadX entry. (The `stub` mode + the
  whole baker file stay until W3 removes its last consumer.)

### W1 — threadx-linux proving (host)
- [x] **W1.1** Ported `examples/threadx-linux/cpp/talker` to the `configure(Node&)`
  component shape (`Talker.{hpp,cpp}` with `on_tick` publisher, mirror of the proven
  NuttX/native shape); CMakeLists → TYPED carrier; **dropped the baker +
  `src/main.c` + the launch placeholder** (single-node carrier needs no launch).
  **Build green** (host ELF links: board `startup.c` `main` + carrier-rendered
  `app_main` + `Talker.cpp`, no conflict). Bounded host-run reaches
  `ThreadxBoard::run_components` (boot → app thread → our `app_main` → `nros::init`);
  the publish itself is the **NuttX-proven identical path** (`EntryNodeRuntime` +
  `configure` + `bind_timer`). Full networked publish-assert (`Published: N`) is the
  veth+zenohd harness's job — env-limited here (the QEMU/zenohd E2E caveat).
- [x] **W1.2** Ported `examples/threadx-linux/c/talker` (`NROS_C_COMPONENT`, raw CDR
  `Int32`, mirror of NuttX C); CMakeLists → TYPED carrier, dropped `main.c`. **Build
  green** — C entry renders the `__nros_c_component_*_{create,configure}` seam →
  `ThreadxBoard::run_components`.

### W2 — bare-metal riscv64 (the phase-245 unblock)
- [ ] **W2.1** `qemu-riscv64-threadx/cpp/talker` → typed component shape, both
  build paths (zenoh + CycloneDDS CMake firmware). Cross-build + QEMU boot-gate.
  (Resolves Q3 — the CycloneDDS composition seam.)
- [ ] **W2.2** `qemu-riscv64-threadx/c/talker` → same.
- [ ] **W2.3** Hand phase-245 T-c / T-cpp the proven template; they become the
  remaining-role ports (Wave-2 C\*/X\*).

### W3 — retire the legacy baker (gradual; last, after consumers move off)
- [ ] **W3.1** Inventory `NanoRosThreadxSystemCodegen.cmake` consumers (grep
  `nros_threadx_codegen_system` / `nros_threadx_link_app`): the rust-component
  `multi_pkg_workspace_threadx` fixture (`threadx_corrosion_bringup`) + any
  threadx-linux C/C++ cells W1/W2 have already migrated.
- [ ] **W3.2** Migrate `multi_pkg_workspace_threadx` (Rust components) onto the
  Rust dispatch path — `ExecutorNodeRuntime` + the FreeRTOS/NuttX `run_entry`
  shape, NOT the C++ typed entry (Rust components dispatch via
  `register_dispatch_slot`). Add a ThreadX Rust system entry if one doesn't exist.
  Keep `threadx_corrosion_bringup` green (a real spin, not the NULL-context marker).
- [ ] **W3.3** Delete `cmake/NanoRosThreadxSystemCodegen.cmake` +
  `nros_threadx_link_app` once W1/W2/W3.2 reference none of it; drop the dead
  `system.toml`/launch placeholders the baker required. Update any `include()`s.
- [ ] **W3.4** Sweep: confirm no example/fixture still emits a NULL-context
  `nros_system_main` stub; the only ThreadX entry is the typed C/C++ carrier or the
  Rust `run_entry`/`nros::main!()` path.

## Acceptance

- `examples/threadx-linux/{c,cpp}/talker` build + **publish** on host via the
  typed entry (`run_components`, real `on_tick`), launch populated, no manual
  `nros_support_init`/executor/spin in source.
- `examples/qemu-riscv64-threadx/{c,cpp}/talker` cross-build on **both** zenoh and
  CycloneDDS CMake paths + boot-gate green on riscv64 QEMU.
- Source carries only the agnostic component (`configure` + member callbacks); no
  RMW/platform selection, no locator, no executor/spin boilerplate.
- `cmake/NanoRosThreadxSystemCodegen.cmake` **deleted** (both `cpp` + `stub`
  modes); ThreadX C/C++ routes through the unified `nano_ros_node_register(TYPED)`
  → `nros codegen entry --typed` carrier, and ThreadX Rust through
  `ExecutorNodeRuntime`/`run_entry`. No NULL-context `nros_system_main` stub
  survives. `multi_pkg_workspace_threadx` (`threadx_corrosion_bringup`) green on
  the Rust dispatch path (real spin, not a marker).
- `ThreadxBoard::run` (interpreter) + baker `RUNTIME=cpp` removed;
  `ThreadxBoard::run_components` is the only ThreadX C/C++ entry.
- phase-245 T-c / T-cpp unblocked → reopened as real ports on this template.
