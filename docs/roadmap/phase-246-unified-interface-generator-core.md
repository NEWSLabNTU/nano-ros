# Phase 246 — unify the two interface generators (shared codegen core)

**Goal.** Collapse the drift surface between nano-ros's two
`nros_generate_interfaces()` implementations —
[`cmake/NanoRosGenerateInterfaces.cmake`](../../cmake/NanoRosGenerateInterfaces.cmake)
(canonical, library-target model) and
[`zephyr/cmake/nros_generate_interfaces.cmake`](../../zephyr/cmake/nros_generate_interfaces.cmake)
(Zephyr `app`-target model) — by extracting the **context-free, copy-pasted**
logic into one shared `cmake/NanoRosCodegenCore.cmake` that both include. Keep
two thin entry points (their deployment models genuinely differ); kill the
recurring "fix it in one, forget the other" bug class.

**Status.** COMPLETE (2026-06-13). 246.1–246.3 extracted to
`cmake/NanoRosCodegenCore.cmake` + validated (native canonical + ASI FVP zephyr
both build clean each wave). 246.2b (codegen-tool + interface-file
resolvers) DONE (+ Zephyr bundled-tier + CODEGEN_CONFIG parity). 246.4 (link
wiring) intentionally NOT unified — documented divergence, not a gap (opposite
ld-order directions + target models). Only the `nros_find_interfaces()` dedup
remains as an untouched stretch. Original detail follows. **246.1 DONE +
verified** — `cmake/NanoRosCodegenCore.cmake` holds `_nros_collect_rs_closure`,
`_nros_export_rs_closure`, `_nros_write_ffi_lib_rs`; both generators call them for
lib.rs assembly + closure compute/export. Validated: native C++ listener
(canonical, deduped lib.rs) + ASI FVP (`zephyr.elf`) both build clean. **246.2
DONE + verified** — args-JSON writer + output-prediction extracted to the core;
both generators call them; native `cpp_listener` + ASI `zephyr.elf` rebuild clean.
246.2b (codegen-tool + interface-file resolvers) deferred. **246.3 DONE +
verified** — `_nros_ffi_cargo_args` extracted (truthiness-guarded profile/target/
build-std); both generators call it; native `cpp_listener` + ASI `zephyr.elf`
rebuild clean. 246.4 (link wiring) pending.

**Priority.** P2 — tech-debt, but high-leverage: the same conceptual code drifted
**three times** during the ASI Zephyr-3.7 bring-up alone, each a separate
debugging session.

**Closes / informed by.** [issue 0052](../issues/archived/0052-zephyr-ffi-interface-gen-duplicate-include.md)
(FFI `include!()` de-dup drift) and
[issue 0056](../issues/archived/0056-zephyr-ffi-staticlib-link-order-undefined-refs.md)
(whole-archive link applied only to the zephyr copy) — both flagged
"funnel the two generators" as the real fix.

---

## Background — the drift surface

Selection is platform-gated, so only ONE generator is loaded per build (the
function-name collision is benign): `zephyr/CMakeLists.txt` includes the zephyr
copy; the root `CMakeLists.txt` + `cmake/platform/nano-ros-*.cmake` include the
canonical one. Templates (`cpp_ffi_Cargo.toml.in`, `ffi_lib_rs.in`) are **already
shared** — the zephyr copy walks up to `cmake/` — so they are NOT part of the
drift and stay as-is.

Every drift that has bitten us lives in **shared-concept** code that was
copy-pasted between the two:

- **lib.rs `include!()` assembly** — `_ffi_rs_all` + `REMOVE_DUPLICATES` + emit.
  Drifted on de-dup (0052) and relative-vs-absolute paths (Phase 214.B.1).
- **FFI staticlib link wiring** — whole-archive (0056) landed only in zephyr;
  canonical uses `INTERFACE_LINK_LIBRARIES` ordering.
- **`_rs_closure` PARENT_SCOPE + CACHE export** — currently byte-identical in
  both, pure latent drift.
- **codegen-tool resolution**, **interface-file collection** (canonical has a
  bundled-fallback tier the zephyr copy lacks), **args-file JSON**, **output-path
  + name transform**, **FFI cargo-args** (profile / `--target` / `-Z build-std`).

## What stays per-entry-point (legitimate platform difference)

These are NOT drift — they are the actual reason two entry points exist:

- **Target model.** Canonical builds `${target}__nano_ros_cpp` (INTERFACE) +
  an IMPORTED `*_ffi_lib`; zephyr emits into the Zephyr `app` target
  (`target_sources`/`target_link_libraries(app …)`).
- **Codegen timing.** Canonical `add_custom_command` (build-time, DEPENDS);
  zephyr `execute_process` (configure-time + mtime skip).
- **Rust target.** Zephyr `nros_detect_rust_target()` → `NROS_RUST_TARGET` +
  `rust-toolchain.toml`; canonical `Rust_CARGO_TARGET` + `.cargo/config.toml`.
- **Profile.** Zephyr `nros-fast-release` (env-flexible); canonical `--release`.
- **serdes resolution.** Zephyr stages a standalone crate
  (`nros-serdes-standalone-Cargo.toml`); canonical resolves install/in-tree.
- **Kconfig tool lookup** (zephyr only).

---

## Design — shared core module, two thin entry points

New `cmake/NanoRosCodegenCore.cmake`, `include()`d by BOTH generators. Extracted
helpers (each replaces a copy-pasted block):

| helper | replaces / kills |
| --- | --- |
| `_nros_assemble_ffi_lib_rs(CRATE_SRC DEPS OWN_RS PATH_MODE)` | lib.rs `include!()` de-dup + path drift (0052, 214.B). `PATH_MODE = relative\|absolute`. |
| `_nros_export_rs_closure(TARGET DEPS OWN_RS)` | the duplicated PARENT_SCOPE + `_NROS_PKG_*` CACHE export. |
| `_nros_resolve_codegen_tool(OUT_VAR)` | cache-var / `find_program` drift (zephyr keeps its Kconfig pre-check, then falls through to this). |
| `_nros_resolve_interface_files(...)` | 3-tier collection (local → ament → bundled); gives zephyr the missing bundled tier. |
| `_nros_write_codegen_args_json(...)` | the duplicated JSON args-file writer (+ content-compare mtime preserve). |
| `_nros_compute_output_paths(...)` | name transform + header/`_ffi.rs` path lists. |
| `_nros_ffi_cargo_args(OUT_VAR PROFILE RUST_TARGET BUILD_STD ...)` | profile / `--target` / `-Z build-std` assembly. |

Each `nros_generate_interfaces()` keeps its target-model + timing wiring and
calls the helpers for the guts.

**FFI link wiring is the one risky extraction.** Canonical's
`INTERFACE_LINK_LIBRARIES`-ordering (forwards `NanoRos::NanoRosCpp` transitive
deps) and zephyr's raw `--whole-archive` solve the same ld-order problem
differently. Whole-archive is order-independent and would work for both, but
canonical also relies on the interface-lib path for dep forwarding. **Deferred to
a follow-up wave** — not wave 1; document the shared invariant first.

---

## Waves (each gated on a green build before the next)

- **246.1 — the proven bugs.** Extract `_nros_assemble_ffi_lib_rs` +
  `_nros_export_rs_closure`; both generators call them. Highest value (the two
  things that actually drifted into shipped bugs).
- **246.2 — codegen plumbing.** `_nros_write_codegen_args_json` (args JSON
  build + content-compare write) + `_nros_predict_generated_outputs` (name
  transform + header/source/`_ffi.rs` path lists). These were the two LARGEST
  byte-identical blocks (~105 lines each side). DONE.
  - **246.2b — DONE.** Codegen-tool resolution + interface-file resolution now
    share `_nros_resolve_codegen_tool(<cache_var>)` and
    `_nros_resolve_interface_file(target relpath out [BUNDLED_PREFIX p])`. Each
    generator keeps its OWN pre-checks (zephyr west `-D` pre-set + Kconfig
    `CONFIG_NROS_CODEGEN_TOOL`; canonical profile var) and passes its own cache-
    var name (the distinct `_NROS_ZEPHYR_CODEGEN_TOOL` — read by
    `nros_find_interfaces.cmake` — vs `_NANO_ROS_CODEGEN_TOOL` — must NOT be
    unified), then calls the shared find/validate/cache. The interface resolver
    is now a thin per-generator wrapper supplying the bundled prefix. **Parity
    gained:** the Zephyr tree now has the bundled-interface fallback tier AND the
    `CODEGEN_CONFIG` (RFC-0033 per-field capacity) keyword.
- **246.3 — cargo invocation.** `_nros_ffi_cargo_args` (build skeleton + profile
  / `--target` / `-Z build-std` conditionals). DONE. Toolchain pinning
  (`+<tc>` prefix + `.cargo/config.toml` canonical; `rust-toolchain.toml` zephyr)
  stays per-generator. Guard on truthiness, not `STREQUAL ""` — an omitted
  one-value keyword leaves `_A_<K>` unset and `STREQUAL ""` then compares the
  literal name, firing the branch with an empty value (`--target` / `-Z
  build-std=` with no value → cargo error). `if(_A_<K>)` derefs + treats
  unset/empty as false.
- **246.4 — link wiring. ANALYZED → intentionally NOT unified.** The two link
  blocks solve OPPOSITE ld-order problems for DIFFERENT target models:
  - Canonical: the nros-cpp runtime (`NanoRos::NanoRosCpp`, defining
    `nros_cpp_publish_raw`) must come AFTER the ffi `.a` → solved by
    `INTERFACE_LINK_LIBRARIES` topological ordering on a per-package INTERFACE
    library that the consumer links.
  - Zephyr: the ffi `.a` must come AFTER the app/component objects (whose inline
    msg-header fns call ffi `deserialize/publish`) → solved by `--whole-archive`
    onto the `app` target (issue 0056).

  A shared helper would be over-parameterized boilerplate-wrapping of ~4 lines
  AND would touch the exact code that produced issue 0056 — risk ≫ value. Kept
  separate, with the divergence documented in BOTH generators. The only change
  made: dropped the Zephyr IMPORTED `<pkg>_cpp_ffi` target, which became
  vestigial after 0056 switched to raw-path whole-archive (nothing link-listed
  it; only its `_build` custom target was used).

  Shared invariant (documented in both): the per-package ffi `.a` symbols must
  co-resolve with their callers at final link — each path enforces it in the
  way that fits its target model.

**Validation gates (every wave):**
1. A native C++ example builds (canonical path) — e.g. `examples/native/cpp/listener`.
2. A Zephyr C++ example builds (zephyr path).
3. **ASI FVP still links** — `zephyr.elf` from the pinned consumer
   (board `fvp-aemv8r-smp`, the reference Zephyr+C++ consumer).

**Acceptance.** Both generators contain zero copy-pasted shared-concept blocks
for the items in the table above; a fix to lib.rs assembly / closure export /
cargo args touches exactly one place; all three validation builds pass.

## Side benefits

- Zephyr gains the bundled-interface fallback tier + (optionally) the
  `CODEGEN_CONFIG` keyword (RFC-0033 per-field capacity) for free.
- `nros_find_interfaces()` exists in both trees too
  (`zephyr/cmake/nros_find_interfaces.cmake`) — same dedup opportunity, tracked
  here as a stretch once the core lands.

## Non-goals

- NOT collapsing into a single `nros_generate_interfaces()` with mode flags
  (rejected Option B — one branchy function across app-vs-library +
  configure-vs-build-time models is harder to read than two thin wrappers).
- NOT changing the templates (already single-source).
- NOT changing codegen timing or target models.
