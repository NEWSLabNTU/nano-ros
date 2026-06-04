# Phase 221 — C language Node + Entry pkg completion

**Goal.** Bring the C language to parity with Rust (and the parallel
Phase 219 C++ work) for the three-role workspace shape: Node pkg, Entry
pkg, and the cmake-fn surfaces that link them.

**Status.** PROPOSED 2026-06-04. Audit done; work items below.

**Priority.** P2 — C nodes today work in the single-app (fused) shape;
the multi-Node / Entry-pkg shape is unsupported. Most C contributors
land in app-node-only territory, but ROS 2 migration cases that mix C
with C++ Node pkgs hit a wall.

**Depends on.** Phase 212.M.5.a.1 (per-pkg mangled register symbol —
landed), Phase 212.L.9 (`nano_ros_node_register()` cmake fn —
language-scoped), Phase 219 (C++ Entry pkg + `nano_ros_entry(LAUNCH …)`
— in progress; shares the cmake fn surface this phase needs).

---

## 1. Audit (state on 2026-06-04)

| Role | Rust | C | C++ |
|---|---|---|---|
| **App-node (fused)** | ✓ `nros::init` + manual spin | ✓ `nros_init` + manual spin | ✓ `nros::init` + executor loop |
| **Node pkg (composable lib)** | ✓ `nros::node!(T)` proc-macro | ⚠ header has `NROS_NODE_REGISTER(register_fn)` 1-arg form (`nros-c/include/nros/node_pkg.h` line 186), **NO EXAMPLE USES IT**, `nano_ros_node_register()` cmake fn unverified for LANGUAGE C | ✓ `NROS_NODE_REGISTER(UserClass, "pkg::Class")` macro + cmake fn shipped |
| **Entry pkg (binary that boots)** | ✓ `nros::main!(...)` four forms | ✗ no surface at all — C has no proc-macro equivalent, no `nano_ros_entry()` C-language path | ⏳ Phase 219 (`NROS_MAIN` macro + `nano_ros_entry(LAUNCH …)`) |
| **Bringup pkg (declarative)** | shared — language-agnostic |
| **Mixed-lang composition** | C Node pkg → C++/Rust Entry via the C-FFI register trampoline. Already the wire shape; **NO EXAMPLE PROVES IT END-TO-END.** |

So C has:
- ✓ App-node working (`examples/native/c/talker/`).
- ⚠ Node pkg surface declared but not adopted; no integration test;
  cmake fn LANGUAGE-C support unverified.
- ✗ Entry pkg surface absent.

The "mixed-language Entry" path (C Node pkg linked into a C++ or Rust
Entry pkg) is what unblocks most realistic C use cases. It already
works at the symbol level because the per-pkg mangled register fn
(`__nros_component_<pkg>_register`) is C-ABI, but no example
demonstrates it.

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

Recommendation: **Path B for v1 (this phase). Defer Path A until a
real "pure-C-only fleet" use case shows up.** The audit suggests no
internal user is blocked on pure-C Entry today; the audit is reactive
to a C++ migration walkthrough, not a C one.

---

## 3. Work items

### 221.A — C Node pkg adoption proof

- [ ] **221.A.1** Audit the existing `NROS_NODE_REGISTER(register_fn)`
      surface in `packages/core/nros-c/include/nros/node_pkg.h` against
      the Phase 212.L.9 + 212.M.5.a.1 invariants:
        - per-pkg mangled symbol `__nros_component_<pkg>_register`
          emitted from `NROS_PKG_NAME` (cmake glue).
        - class-name export symbol (`__nros_component_<pkg>_class_name`)
          for the lint / codegen side.
        - present-marker symbol (`__NROS_NODE_PKG_<pkg>_EXPORT_PRESENT`).
      Match Rust + C++ exactly. Bump the macro if the C surface
      drifted during 214.J's rename.
- [ ] **221.A.2** Update `nano_ros_node_register()` (cmake fn) to
      accept `LANGUAGE C` explicitly. Today the fn assumes C++ for
      the `target_sources` / `add_library` shape; the C case needs a
      `set_target_properties(... LINKER_LANGUAGE C)` if no C++ TU is
      linked.
- [ ] **221.A.3** New example: `examples/native/c/talker_pkg/` — a
      pure-C Node pkg that exports the register trampoline via
      `NROS_NODE_REGISTER`. Pairs with a sibling
      `examples/native/c/listener_pkg/`. Both build into static libs.
- [ ] **221.A.4** Integration test: `phase221_c_node_pkg_links.rs` in
      `nros-tests/tests/` — builds the two C Node pkgs as static libs,
      asserts the mangled symbols are present in the nm output.

**Files:** `packages/core/nros-c/include/nros/node_pkg.h`,
`cmake/NanoRosNodeRegister.cmake`, `examples/native/c/{talker,listener}_pkg/`
(new), `packages/testing/nros-tests/tests/phase221_c_node_pkg_links.rs`
(new).

### 221.B — Mixed-language Entry pkg example

- [ ] **221.B.1** New example: `examples/native/templates/c-and-cpp-
      mixed-workspace/` — Bringup pkg + 1 C Node pkg + 1 C++ Node pkg
      + 1 C++ Entry pkg (`NROS_MAIN(...)` from Phase 219) that links
      both. Shows the canonical mixed-language shape.
- [ ] **221.B.2** Book chapter:
      `book/src/getting-started/workspace-mixed-language.md` —
      walks through the template above. Cross-links to
      `workspace-node-pkgs.md` and `workspace-entry-pkg.md`.
- [ ] **221.B.3** Integration test: `phase221_c_in_cpp_entry.rs` —
      builds the mixed-lang template, runs the binary, asserts both
      C and C++ nodes publish/subscribe (`/from_c` → `/to_cpp`).
- [ ] **221.B.4** Update `book/src/getting-started/workspace-from-app-
      node.md` to mention the mixed-language pattern in the "When you
      outgrow one app" section — calling out that pure-C Entry pkgs
      are NOT supported (defer to Path A in a future phase) but C
      Node pkgs link cleanly into C++/Rust Entry pkgs.

**Files:** `examples/native/templates/c-and-cpp-mixed-workspace/`
(new tree), `book/src/getting-started/workspace-mixed-language.md`
(new), `packages/testing/nros-tests/tests/phase221_c_in_cpp_entry.rs`
(new), `book/src/getting-started/workspace-from-app-node.md`.

### 221.C — `nros new` scaffolding for C

- [ ] **221.C.1** `nros new --component --lang c talker_pkg` —
      scaffolds a C Node pkg per the §221.A shape. Today the CLI
      rejects `--lang c` for `--component`; either lift the
      restriction (per §221.A.2's cmake-fn update) or surface a
      clear error pointing at the mixed-language pattern.
- [ ] **221.C.2** `nros new --entry --lang c` — explicitly REJECTS
      with a message naming the mixed-language path. (Per Path B
      decision in §2.)

**Files:** `packages/cli/nros-cli-core/src/cmd/new.rs` (or wherever
the `new` verb dispatches).

### 221.D — `nros check` lint for C-side antipatterns

- [ ] **221.D.1** `nros check` rejects a C Node pkg whose
      `Cargo.toml`-equivalent (`package.xml` + `CMakeLists.txt`)
      doesn't carry `nano_ros_node_register()`. Mirror the existing
      Rust-side lint (Phase 212.G).
- [ ] **221.D.2** Lint: if any pkg in the workspace lists `<exec_depend>`
      on a C Node pkg, the consuming Entry pkg must be C++ or Rust —
      `nros check` rejects pure-C Entry-pkg drafts with a pointer to
      the mixed-language doc.

**Files:** `packages/cli/nros-cli-core/src/check/` (or wherever
check rules live).

---

## 4. Acceptance

- [ ] `cargo nextest run -p nros-tests --test
      phase221_c_node_pkg_links` passes — proves the C Node pkg
      surface emits the right symbols.
- [ ] `cargo nextest run -p nros-tests --test
      phase221_c_in_cpp_entry` passes — proves the mixed-lang Entry
      runs end-to-end.
- [ ] The mixed-language template at
      `examples/native/templates/c-and-cpp-mixed-workspace/` is the
      canonical reference from the book chapter.
- [ ] `nros new --component --lang c <name>` scaffolds a valid C
      Node pkg.
- [ ] `nros new --entry --lang c <name>` errors with a clear
      mixed-lang pointer.

---

## 5. Notes

- This phase intentionally **does not** ship pure-C Entry pkgs. That
  path (Path A in §2) waits on a real use case + a codegen design
  that doesn't fight C's macro limits.
- C Node pkgs link into C++ Entry pkgs via the existing FFI
  register trampoline; **no new runtime symbol**, no new RMW work.
- The phase is **mostly examples + cmake-fn polish + scaffold +
  lint**. The big-ticket items (per-pkg mangled symbols, generic
  codegen) already shipped via Phase 212.M.5 / L.9 / 219.
- Once Phase 219 ships the C++ Entry pkg (`NROS_MAIN` +
  `nano_ros_entry(LANGUAGE CXX LAUNCH …)`), the §221.B mixed-lang
  example becomes the natural anchor for the C → C++ → Rust language
  ladder the book documents.
