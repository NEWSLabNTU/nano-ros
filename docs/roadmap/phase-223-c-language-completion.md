# Phase 223 — C Node pkg + mixed-language workspace completion

**Goal.** Complete the C Node-pkg workflow and document the canonical
mixed-language path: C Node pkgs hosted by a C++ or Rust Entry pkg.
Phase 219 already supplied the pure-C Entry mechanics (`emit_c`,
`nano_ros_entry(LANGUAGE C)`, `<nros/main.h>`); this phase makes C Node
pkgs first-class in CMake/scaffolding and gives users a native
C/C++ reference workspace.

**Status.** IMPLEMENTED 2026-06-04.

**Priority.** P2 — C nodes worked in the single-app (fused) shape, but
the multi-Node / Entry-pkg workflow lacked a first-class C Node-pkg
template, scaffold, and mixed-language reference. Most C contributors
land in app-node-only territory, but ROS 2 migration cases that mix C
with C++ Node pkgs needed a supported path.

**Depends on.** Phase 212.M.5.a.1 (per-pkg mangled register symbol —
landed), Phase 212.L.9 (`nano_ros_node_register()` cmake fn), and
Phase 219 (C/C++ Entry codegen + `nano_ros_entry(LAUNCH …)` — landed).

---

## 1. Audit (state on 2026-06-04)

| Role | Rust | C | C++ |
|---|---|---|---|
| **App-node (fused)** | ✓ `nros::init` + manual spin | ✓ `nros_init` + manual spin | ✓ `nros::init` + executor loop |
| **Node pkg (composable lib)** | ✓ `nros::node!(T)` proc-macro | ✓ `NROS_NODE_REGISTER(register_fn)` + `nano_ros_node_register(LANGUAGE C)` | ✓ `NROS_NODE_REGISTER(UserClass, "pkg::Class")` macro + cmake fn shipped |
| **Entry pkg (binary that boots)** | ✓ `nros::main!(...)` four forms | ✓ Phase 219 supplied `nros codegen entry --lang c` + `nano_ros_entry(LANGUAGE C)`; mixed-language workflow still recommends C++/Rust Entry hosts | ✓ `NROS_MAIN` + `nano_ros_entry(LAUNCH …)` |
| **Bringup pkg (declarative)** | shared — language-agnostic |
| **Mixed-lang composition** | C Node pkg → C++/Rust Entry via the C-FFI register trampoline; covered by the mixed-language workspace template and build test. |

So C now has:
- ✓ App-node working (`examples/native/c/talker/`).
- ✓ Node pkg surface adopted by examples/scaffolds and covered by CMake tests.
- ✓ Entry codegen mechanics from Phase 219.

The "mixed-language Entry" path (C Node pkg linked into a C++ or Rust
Entry pkg) is what unblocks most realistic C use cases. It works at
the symbol level because the per-pkg mangled register fn
(`__nros_component_<pkg>_register`) is C-ABI, and the mixed-language
workspace template now demonstrates the C Node pkg -> C++ Entry flow.

---

## 2. Two paths considered

**Path A — full C Entry pkg (`NROS_MAIN` + `nano_ros_entry()` LANG=C).**
Mirror Phase 219's C++ work. User writes `main.c` with `NROS_MAIN(...)`
+ a CMakeLists carrying `nano_ros_entry(LANGUAGE C ...)`.
Pros: language-symmetric with the C++ side.
Cons: C macros can't easily produce the per-launch-XML register-call
emission the way C++ templates or Rust proc-macros do; the codegen
would have to emit a C source file at configure time and inject it into
the Entry-pkg target. Doable but heavier than the C++ template path.

**Path B — C Node pkg + mixed-language Entry only.**
Drop the C-Entry-pkg ambition. Document the canonical mixed-language
pattern: C Node pkg compiled as a static lib, linked into a C++ or Rust
Entry pkg via the existing FFI register trampoline. Add one example
proving it.
Pros: zero new codegen surface; uses the C-ABI symbol that already
works; matches the realistic mixed-language migration use case.
Cons: C contributors who want a pure-C deploy still hit the wall;
must opt into C++ or Rust as their Entry-pkg host language.

Recommendation after Phase 219 landed: keep Path A mechanics available
but make Path B the documented default for multi-node C work. C users
can keep Node code in C while relying on the richer C++ or Rust Entry
host for launch-driven composition.

---

## 3. Work items

### 223.A — C Node pkg adoption proof

- [x] **223.A.1** Audit the existing `NROS_NODE_REGISTER(register_fn)`
      surface in `packages/core/nros-c/include/nros/node_pkg.h` against
      the Phase 212.L.9 + 212.M.5.a.1 invariants:
        - per-pkg mangled symbol `__nros_component_<pkg>_register`
          emitted from `NROS_PKG_NAME` (cmake glue).
        - class-name export symbol (`__nros_component_<pkg>_class_name`)
          for the lint / codegen side.
        - present-marker symbol (`__NROS_NODE_PKG_<pkg>_EXPORT_PRESENT`).
      Match Rust + C++ exactly. Bump the macro if the C surface
      drifted during 214.J's rename.
- [x] **223.A.2** Update `nano_ros_node_register()` (cmake fn) to
      accept `LANGUAGE C` explicitly. Today the fn assumes C++ for
      the `target_sources` / `add_library` shape; the C case needs a
      `set_target_properties(... LINKER_LANGUAGE C)` if no C++ TU is
      linked.
- [x] **223.A.3** New example/template coverage: the canonical C Node
      pkg lives in `examples/templates/c-and-cpp-mixed-workspace/src/c_talker_pkg/`.
- [x] **223.A.4** Integration test coverage: CMake tests verify
      `LANGUAGE C` metadata, and the mixed template build test verifies
      the C static lib links into a C++ Entry pkg.

**Files:** `packages/core/nros-c/include/nros/node_pkg.h`,
`cmake/NanoRosNodeRegister.cmake`,
`examples/templates/c-and-cpp-mixed-workspace/src/c_talker_pkg/`,
`packages/testing/nros-tests/tests/phase212_l9_cmake_fns.rs`,
`packages/testing/nros-tests/tests/phase223_c_mixed_workspace.rs`.

### 223.B — Mixed-language Entry pkg example

- [x] **223.B.1** New example: `examples/templates/c-and-cpp-
      mixed-workspace/` — Bringup pkg + 1 C Node pkg + 1 C++ Node pkg
      + 1 C++ Entry pkg (`NROS_MAIN(...)` from Phase 219) that links
      both. Shows the canonical mixed-language shape.
- [x] **223.B.2** Book chapter:
      `book/src/getting-started/workspace-mixed-language.md` —
      walks through the template above. Cross-links to
      `workspace-node-pkgs.md` and `workspace-entry-pkg.md`.
- [x] **223.B.3** Integration test: `phase223_c_mixed_workspace.rs` —
      configures and builds the mixed-lang template. Runtime
      publish/subscribe instantiation remains outside Phase 223.
- [x] **223.B.4** Update `book/src/getting-started/workspace-from-app-
      node.md` to mention the mixed-language pattern in the "When you
      outgrow one app" section — calling out that pure-C Entry
      mechanics exist from Phase 219, while the recommended multi-node
      C workflow is to keep Node code in C and host launch-driven
      composition from C++/Rust Entry pkgs.

**Files:** `examples/templates/c-and-cpp-mixed-workspace/`
(new tree), `book/src/getting-started/workspace-mixed-language.md`
(new), `packages/testing/nros-tests/tests/phase223_c_mixed_workspace.rs`
(new), `book/src/getting-started/workspace-from-app-node.md`.

### 223.C — `nros new` scaffolding for C

- [x] **223.C.1** `nros new --component --lang c talker_pkg` —
      scaffolds a C Node pkg per the §223.A shape. Today the CLI
      rejects `--lang c` for `--component`; either lift the
      restriction (per §223.A.2's cmake-fn update) or surface a
      clear error pointing at the mixed-language pattern.
- [x] **223.C.2** Reconciled with Phase 219: there is no separate
      `--entry` flag in the current CLI, and Phase 219 already supplied
      `nros new <name> --lang c --platform native` plus dormant C Entry
      codegen. Keep that surface; document mixed-language Entry as the
      recommended multi-node workflow.

**Files:** `packages/cli/nros-cli-core/src/cmd/new.rs` (or wherever
the `new` verb dispatches).

### 223.D — `nros check` lint for C-side antipatterns

- [x] **223.D.1** `nros check` lint is not added as a separate rule.
      C Node pkgs are CMake-declared, and the CMake configure/build
      path now validates the canonical `nano_ros_node_register(...)`
      shape directly.
- [x] **223.D.2** Pure-C Entry rejection is not added. Phase 219 landed
      the C Entry mechanics; Phase 223 documents mixed-language Entry
      as the default rather than forbidding the implemented surface.

Historical lint proposal:

- `nros check` rejects a C Node pkg whose
      `Cargo.toml`-equivalent (`package.xml` + `CMakeLists.txt`)
      doesn't carry `nano_ros_node_register()`. Mirror the existing
      Rust-side lint (Phase 212.G).
- Lint: if any pkg in the workspace lists `<exec_depend>`
      on a C Node pkg, the consuming Entry pkg must be C++ or Rust —
      `nros check` rejects pure-C Entry-pkg drafts with a pointer to
      the mixed-language doc.

**Files:** `packages/cli/nros-cli-core/src/check/` (or wherever
check rules live).

---

## 4. Acceptance

- [x] `cargo nextest run -p nros-tests --test
      phase212_l9_cmake_fns` covers `nano_ros_node_register(LANGUAGE C)`.
- [x] `cargo nextest run -p nros-tests --test
      phase223_c_mixed_workspace` configures/builds both the mixed-lang
      Entry template and the pure-C Entry template.
- [x] The mixed-language template at
      `examples/templates/c-and-cpp-mixed-workspace/` is the
      canonical reference from the book chapter.
- [x] The pure-C template at
      `examples/templates/pure-c-workspace/` proves C Node pkgs can
      link into a C Entry pkg generated by `nano_ros_entry(LANG c ...)`.
- [x] `nros new --component --lang c <name>` scaffolds a valid C
      Node pkg.
- [x] `nros new <name> --lang c --platform native` remains available
      from Phase 219; mixed-language Entry is documented as the
      recommended multi-node C workflow.

---

## 5. Notes

- Pure-C Entry mechanics shipped in Phase 219 and are now build-proven
  by the pure-C workspace template. C++/Rust Entry hosts remain the
  canonical mixed-language migration path.
- C Node pkgs link into C++ Entry pkgs via the existing FFI
  register trampoline; **no new runtime symbol**, no new RMW work.
- The phase is **mostly examples + cmake-fn polish + scaffold +
  docs**. The big-ticket items (per-pkg mangled symbols, generic
  codegen) already shipped via Phase 212.M.5 / L.9 / 219.
- Phase 219 shipped the C++ Entry pkg (`NROS_MAIN` +
  `nano_ros_entry(LANGUAGE CXX LAUNCH ...)`), so the §223.B mixed-lang
  example is now the natural anchor for the C -> C++ -> Rust language
  ladder the book documents.
