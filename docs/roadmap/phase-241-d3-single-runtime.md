# Phase 241.D3-rev — single shared runtime (one Rust staticlib per binary)

Status: **Approved — in progress** (2026-06-13) · Branch `issue-42-d3-link-determinism` ·
Implements/revises RFC-0042 D3 · Supersedes the slice-4 provider approach.

## Problem

RFC-0042 D3 / slice 4 aimed to drop the blind `--allow-multiple-definition` ODR
mask. It introduced `nros-rmw-cffi-provider` so the cffi `REGISTRY` + C entry
points are defined exactly once, then dropped the flag. That fixed the **dangerous**
duplicate — the stateful `REGISTRY` (multiple copies → divergent registries).

Running the full C/C++ e2e matrix (not done in the slice-4 "host-validated" pass)
shows the flag also masked a **second** class: every `crate-type=staticlib` Rust
archive bundles its **own** copy of `std`/`compiler_builtins`. A C++ example links
**four** Rust staticlibs — `libnros_c.a` + `libnros_cpp.a` + `libnros_rmw_zenoh_staticlib.a`
+ `libnros_rmw_cffi_provider.a` — so GNU ld errors with:

```
multiple definition of `std::panicking::EMPTY_PANIC'
multiple definition of `rust_eh_personality'
multiple definition of `std::sys::args::unix::imp::ARGV_INIT_ARRAY'
multiple definition of `nros_platform::__FORCE_LINK_CFFI'
```

These are **identical** code/data (same std, same toolchain) — masking them is
benign, but the provider does nothing about them. Threadx_linux C and native-cpp
fixtures fail. The provider fixed cffi; the std closure stayed duplicated.

## Decision

Collapse each binary to **exactly one Rust staticlib** so std is monomorphized
once. The umbrella is the existing FFI crate itself:

| Binary language | Single staticlib | Bundles (rlib deps) |
| --- | --- | --- |
| C            | `libnros_c.a`   | `nros-c` (root) + selected Rust backend |
| C++          | `libnros_cpp.a` | `nros-cpp` (root) + `nros-c` + selected Rust backend |

A **root** staticlib crate keeps its own `#[no_mangle]` symbols (proven: today's
`libnros_c.a` exports all 285 C entries). The backend (`nros-rmw-zenoh` /
`nros-rmw-xrce-cffi`) becomes a **feature-gated rlib dependency** force-linked via
the existing `pub use <backend>::register` + `.init_array` ctor idiom. One cargo
staticlib build ⇒ one `std` ⇒ no `EMPTY_PANIC`/`rust_eh_personality` duplicates ⇒
the flag stays dropped, for real.

**Prototype evidence (2026-06-13):** a throwaway umbrella staticlib bundling
`nros-c` + `nros-rmw-zenoh` (posix) carried exactly **one** copy each of
`EMPTY_PANIC`, `rust_eh_personality`, `ARGV_INIT_ARRAY`, a single `REGISTRY`, and
the backend + cffi `#[no_mangle]` entries — confirming one-staticlib ⇒ one std.

### This subsumes slice 4

One staticlib ⇒ the `nros-rmw-cffi` rlib appears once ⇒ `REGISTRY` + the 6 C entry
points self-define once with **no** provider and **no** `external-registry` feature.
Retire:
- `nros-rmw-cffi-provider` crate (delete).
- `external-registry` feature + every passthrough (nros, nros-c, nros-cpp,
  nros-rmw-zenoh(+staticlib), nros-rmw-xrce-cffi(+staticlib), provider).
- The `#[cfg_attr(not(feature="external-registry"), unsafe(no_mangle))]` gates →
  back to plain `#[unsafe(no_mangle)]` on `REGISTRY` + the 6 C fns (pre-slice-4).
- The separate `nros-rmw-zenoh-staticlib` / `nros-rmw-xrce-cffi-staticlib` archives
  (their role moves into the umbrella). Keep the crates only if still needed by the
  SDK-matrix decoupling; otherwise delete.

This reverts **Phase 134.fix** (which dropped the backend rlib dep from nros-c to
avoid the *multi-staticlib* duplicate-closure hazard). With one staticlib that
hazard cannot occur — there is one cffi instance, one backend closure.

### Locked design choices (user, 2026-06-13)

1. **No libstdc++ in C binaries.** The C umbrella excludes `nros-cpp`. Zenoh/xrce
   are pure Rust → no C++ runtime.
2. **Cyclone DDS for all languages incl. C.** Cyclone's RMW wrapper is C++ (a
   separate CMake lib, *not* a Rust staticlib — no std dup). When Cyclone is the
   backend, **wire libstdc++** into the link even for C binaries (the binary needs
   the C++ runtime Cyclone pulls). The umbrella carries only the cffi shim; Cyclone
   registers its vtable C++-side against the umbrella's `nros_rmw_cffi_register_named`.

### RMW backend dispatch

**W13/R1 — this table is now GENERATED, not hand-maintained.** The per-backend dispatch
(umbrella cffi feature, rlib dep, extra link libs, needs-C++-linker) is data on
`RmwDispatch` in `cargo-nano-ros`'s `resolve_rmw()` (the RFC-0031 SSoT), rendered to
`cmake/NanoRosRmwDispatch.cmake` (the `nros_rmw_dispatch(<rmw>)` function) and
drift-guarded by the `rmw_cmake_dispatch_is_current` test. The W11 synthesized
`nros_ws_runtime` crate pulls its nros-cpp cffi feature from `nros_rmw_dispatch` (no more
hardcoded map in `NanoRosRuntimeCrate.cmake`). The table below mirrors that generated data:

| Backend | umbrella cffi feature | rlib dep | extra link |
| --- | --- | --- | --- |
| zenoh     | `rmw-zenoh-cffi` | `nros-rmw-zenoh` (force-linked) | — |
| xrce      | `rmw-xrce-cffi` | `nros-rmw-xrce-cffi` (force-linked) | — |
| cyclonedds| `rmw-cyclonedds-cffi` | none (C++ lib linked separately) | `nros_rmw_cyclonedds` + `ddsc` + **stdc++** (incl. C; needs-cxx) |

The cmake **link wiring** for cyclonedds (whole-archive, the `NROS_RMW_CYCLONEDDS_DDSC_LIBRARY`
path, the per-OS `force_load`/`--whole-archive` flags) stays platform-specific in the root
`CMakeLists.txt` / `NanoRosLink.cmake`; fully keying those off the manifest rides the R2/R3
registration-path rework (which reworks `NanoRosLink.cmake` anyway).

Embedded firmware (threadx/freertos/nuttx) already links a single cargo unit — unaffected.

## C++ needs the C API (resolved 2026-06-13)

`nros-cpp`'s FFI references **43** distinct `nros-c` C symbols (`nros_init_multi`,
`nros_executor`, `nros_param_declare_*`, `nros_heap_*`, …), and user C++ code may
call any of the 285. So the C++ umbrella **must** bundle `nros-c` as an rlib dep
and **force-link its full C surface** — `nros-cpp` referencing only the 43 it uses
would let DCE drop the rest from `libnros_cpp.a`.

Force-link mechanism (revised 2026-06-13): **`--whole-archive` on the single
umbrella archive** at the cmake link, not a generated Rust anchor. Whole-archiving
`libnros_cpp.a` includes every member — the full C surface from the bundled
`nros-c`, the C++ FFI, and the backend `.init_array` ctor — and because it is the
**only** Rust archive on the link line (cyclone is C++, carrying no Rust std), the
std symbols appear exactly once: no duplicate, no `--allow-multiple-definition`,
no 285-symbol anchor to maintain. (Native C++ examples make zero raw C-API calls,
but user C++ may, so retaining the full surface is the robust default.) The backend
`pub use register` force-link in `nros-c::rmw_backend` stays as belt-and-suspenders
for any non-whole-archive consumer (e.g. the host dup-symbol fixture).

## Work items

Order matters — Rust-side single-instance (W1–W3) must land before the CMake
rewire (W4), and the per-cell validation (W7) gates merge.

### W1 — un-gate cffi to plain `#[no_mangle]`; delete the provider
- `nros-rmw-cffi`: `REGISTRY` + the 6 C entry points back to unconditional
  `#[unsafe(no_mangle)]`; delete the `external-registry` feature + the
  `nros_rmw_cffi_export!` macro (its job moves back in-crate).
- Delete crate `nros-rmw-cffi-provider`; drop it from workspace members.
- Remove the `external-registry` passthrough from `nros`, `nros-c`, `nros-cpp`,
  `nros-rmw-zenoh(+staticlib)`, `nros-rmw-xrce-cffi(+staticlib)`.
- **Acceptance:** `cargo build -p nros-rmw-cffi` → `nm` shows `REGISTRY` (B) + all
  6 C fns (T); no `external-registry` token remains (`! grep -rn external-registry`).

### W2 — `nros-c` bundles the selected backend (umbrella, C path)
- Add `nros-rmw-zenoh` / `nros-rmw-xrce-cffi` as **optional** deps behind
  `rmw-zenoh` / `rmw-xrce` features (mutually exclusive; `rmw-cffi` stays the shim).
- Force-link the backend `register` + `.init_array` ctor (lift from
  `nros-rmw-zenoh-staticlib::auto_register_ctor`); re-enable plain
  `linkme-register` if the single-instance DUPCHECK now allows it (decide in W2).
- **Acceptance:** `cargo build -p nros-c --features platform-posix,rmw-zenoh` →
  `libnros_c.a` `nm`: `nros_init` + `nros_rmw_zenoh_register` + `REGISTRY` present,
  **one** `EMPTY_PANIC` / `rust_eh_personality`. A host C talker links with NO
  `--allow-multiple-definition` and publishes.

### W3 — `nros-cpp` umbrella bundles `nros-c` + backend; C-surface anchor
- `nros-c`: add a generated `force_link.rs` — `#[used] static` array of all public
  `extern "C"` fn pointers — so the full C surface is retained when `nros-c` is an
  rlib dep (not just the staticlib root).
- `nros-cpp`: add `nros-c` (rlib) + the backend as feature-gated deps; force-link.
- **Acceptance:** `libnros_cpp.a` `nm`: the 43 referenced C symbols **and** a
  sampled non-referenced one (e.g. `nros_publisher_create`) present; **one**
  `EMPTY_PANIC`. A host C++ talker links with NO flag (just `libnros_cpp.a`) and runs.

### W4 — CMake rewire to one archive
- C link = `nros_c-static` only; C++ link = `nros_cpp-static` only (drop the
  redundant `nros_c-static` + `nros_cpp-static` pairing). Remove the provider link,
  the `-u <backend>_register` forcing, the `--whole-archive` wraps.
- Pass the backend as a **feature** to the umbrella cargo build, not a separate
  staticlib import.
- Cyclone arm: link `libnros_rmw_cyclonedds` + `libddsc` and **always** wire
  `stdc++` (incl. C binaries).
- **Acceptance:** `just native build-cpp` + the native C fixtures link clean with
  NO flag; `staticlib_duplicate_symbols` still green.

### W5 — standalone backend staticlibs: RETAINED (resolved 2026-06-13)
- The grep found live consumers, so they are **kept**, not deleted:
  - **Zephyr** (`zephyr/CMakeLists.txt`) imports `nros-rmw-zenoh-staticlib` via
    corrosion and links the cargo-built archive directly — the west build is its own
    link model, separate from the cmake umbrella that W4 rewired.
  - The archive-symbol / header-parity / zpico-build-matrix tests consume the
    `libnros_rmw_*_staticlib.a` artifacts + `scripts/check-zenoh-archive-symbols.sh`.
- W4 already removed them from the **non-Zephyr** cmake C/C++ link (now the umbrella).
- **Outcome:** no deletion; documented in RFC-0042 D3 + here.

### W6 — docs
- Update RFC-0042 D3 (living) + mark the slice-4 provider/`external-registry`
  approach **Superseded** here. Cross-link from the phase-241 issue/roadmap.
- **Acceptance:** RFC-0042 D3 describes the single-runtime model; no stale
  "provider" guidance remains as the active design.

### W7 — full per-cell e2e validation
- Build + run, in order: native C/C++ (zenoh, xrce, cyclone) → threadx_linux →
  freertos → threadx_riscv64 → nuttx → esp → zephyr; then `just test-all`.
- **Acceptance:** every cell links with NO `--allow-multiple-definition`; e2e
  green (or any red is a pre-existing/unrelated cause, characterized).
- **W7 cascade fixes (the single-runtime split un-aliased NanoRos/NanoRosCpp,
  surfacing latent gaps each behind the prior):**
  - `c48a4df53` — `ws sync` non-atomic manifest write raced under the parallel
    fixture build (same example synced for zenoh/xrce/cyclone) → truncation. Atomic
    write (temp + rename).
  - `ba5f97c3a` — `nano_ros_entry` defaulted LANG to cpp → C examples linked
    NanoRosCpp (a 2nd std). Infer LANG from source extension.
  - `a4e32bc47` — C++ umbrella lost the per-build variant header + C ABI includes
    (NanoRosCpp no longer links nros_c-static). Mirror `nros_config_generated.h` in
    nros-cpp + pull `nros_c-static` includes.
  - `90abbf2b7` + `3cdd08147` — C++ binary DCE'd nros-c's C surface (nros-cpp bundles
    nros-c as rlib). Generated `#[used]` C-surface anchor (column-0 / ungated entry
    points only; gated ones excluded to avoid undefined refs).
  - threadx_linux `main` collision (`19b90605f`, weak board main) + posix variant
    self-heal.
  - **Pre-existing, separately tracked:** threadx_linux `fixture-0005` header path
    (phase-243); nuttx red (noted in 241.D header §); param/lifecycle wiring (W8–W10).

### W8 — param/lifecycle feature passthrough on the umbrella
- The param/lifecycle C/C++ surfaces ARE implemented (nros-c `param-services` /
  `lifecycle-services`, alloc-gated). The single-runtime umbrella never exposed them:
  `nros-cpp` has no passthrough feature, so it cannot forward to `nros-c`.
- Add `param-services = ["nros-c/param-services"]` + `lifecycle-services =
  ["nros-c/lifecycle-services"]` to `nros-cpp` (nros-c already has them).
- **Acceptance:** `cargo build -p nros-cpp --features …,param-services` resolves +
  `nros_executor_register_parameter_services` is present in `libnros_cpp.a`.

### W9 — enable param/lifecycle on the hosted cmake umbrella
- The C++ executor headers expose param/lifecycle; the native `parameters` example
  uses them. Enable `param-services` + `lifecycle-services` on the **hosted** (posix)
  C and C++ umbrella cmake builds (`nros-c` / `nros-cpp` `_features`). Embedded
  (no_std / alloc-constrained) stays opt-in; per-example opt-in via `nano_ros_entry`
  is a later refinement if size matters.
- **Acceptance:** the native cpp `parameters` (and any lifecycle) example links with
  no undefined `nros_executor_*param*` / `*lifecycle*` symbols.

### W10 — validate param/lifecycle examples
- Build + run the native `parameters` example (+ lifecycle if present), C and C++.
- **Acceptance:** the `cpp_parameters` e2e test passes (was failing on undefined
  gated symbols); no regression in the other native examples.

### W11 — workspace Rust-component cffi dup (CONFIRMED real, 2026-06-14)
- **Trigger:** a `LANGUAGE RUST` node component (`nano_ros_node_register`) is compiled
  to its own staticlib (`librust_heartbeat_pkg.a`) that depends on `nros` with
  `rmw-cffi` → it bundles a SECOND copy of `nros-rmw-cffi`'s `#[no_mangle]` C ABI
  (`REGISTRY`, `nros_rmw_cffi_{lookup,register,register_named,registered_names,
  set_custom_transport}`). The entry binary links that staticlib AND the umbrella
  (`libnros_cpp.a`), which carries the same symbols → GNU-ld `multiple definition`.
  Reproduced: `examples/workspaces/mixed` (c + cpp + rust nodes) fails to link.
- **Was masked** by `--allow-multiple-definition` before single-runtime removed it. NOT
  benign: two `REGISTRY` statics = split registry → backend registered in one, looked
  up in the other → runtime miss (the exact stateful-REGISTRY hazard W1 closed).
- **Scope:** only the cffi `#[no_mangle]` C ABI conflicts. std/compiler_builtins are
  weak-symbols and ld dedups them; the backend lives only in the umbrella. So the fix
  is narrow — keep the cffi C ABI defined in exactly one archive (the umbrella).
- **Fix options considered:** (B) externalize the component's cffi as `extern` imports —
  surgical but revives the W1-deleted provider split and leaves a fragile weak-dedup std
  seam; (C) scoped `--allow-multiple-definition` — rejected, split-REGISTRY runtime bug.
- **DECISION (2026-06-14): Option D — per-entry runtime staticlib.** The honest
  completion of single-runtime: ONE Rust staticlib per binary, zero residual duplication,
  and Rust nodes get the same "just a library" UX as C/C++ nodes (no per-node cargo
  feature boilerplate).

**RESOLVED 2026-06-14** (`c7f8999e7` W11.1, `ae90ffb57` W11.2/W11.3). `examples/workspaces/mixed`
links 0-dup without `--allow-multiple-definition`; native_entry boots + spins (C talker
publishes, C++ listener subscribes, Rust heartbeat registers via one shared REGISTRY);
`{c,cpp}` workspaces + the `robot_entry` templates stay green (synth is a no-op without a
Rust node).

#### W11 design — per-configure runtime umbrella (Option D)
- **Seam (unchanged):** `nros::node!()` emits `#[no_mangle] extern "C"
  __nros_component_<pkg>_register`. The CLI-generated C++ `main` already calls that symbol
  for every node (C/C++/Rust alike). **So `nros codegen entry` does NOT change** — only
  the umbrella's archive changes.
- **Granularity = per cmake-configure (== per-arch).** A workspace targets multiple
  boards by baking a separate `build/<board>/` tree per board (deployment contract:
  `nros codegen-system --bringup` → vendor tool builds each), and one cmake configure is
  single-arch (one toolchain, one `NANO_ROS_PLATFORM`). So a runtime umbrella scoped to
  the configure is automatically per-arch; multi-arch = multiple build trees = multiple
  umbrellas, each correct for its arch. No single configure ever hosts two arches.
  → the runtime crate is synthesized **once per configure** and bundles that configure's
  Rust nodes, NOT once per entry. (Within a single-arch configure, all entries share it.)
- **Runtime crate:** when a workspace configure contains ≥1 Rust node, `nano_ros_workspace`
  synthesizes (configure-time `file(WRITE)`) a crate `<build>/nros_ws_runtime/`:
  - `Cargo.toml`: `[lib] crate-type=["staticlib"]`; deps = `nros-cpp` (umbrella, as rlib)
    with the configure's `<backend>`+`<platform>`+`ros-*` features, plus one `path` dep per
    Rust node (rlib). Carries `[workspace]` + the nros-managed `[patch.crates-io]` block.
  - `src/lib.rs`: a `#[used]` anchor on nros-cpp's `FORCE_LINK_ANCHOR` (W11.1 — re-pulls
    the full C ABI + C++ FFI + backend past staticlib DCE), a `#[used]` `.init_array` ctor
    on `nros_cpp_auto_register_backend` (the backend auto-register, DCE'd as a dep rlib
    otherwise), and one `#[used]` anchor per Rust node's `__nros_component_<pkg>_register`.
- **The umbrella IS the runtime crate.** After importing nano-ros, `nano_ros_workspace`
  re-points `nros-cpp-headers` (== `NanoRos::NanoRosCpp`) — currently
  `target_link_libraries(nros-cpp-headers INTERFACE nros_cpp-static)` at
  `packages/core/nros-cpp/CMakeLists.txt:152` — to link the `nros_ws_runtime` staticlib
  instead of `nros_cpp-static`. All of nros-cpp-headers' INTERFACE includes / cyclone /
  stdc++ wiring is preserved; only the archive swaps. `nros_cpp-static` stays built but
  unreferenced (harmless). Components + entries link `NanoRos::NanoRosCpp` **unchanged**.
- **Why per-configure beats per-entry here:** per-entry would force C/C++ component libs to
  become umbrella-includes-only (so the entry can own the one archive), a fiddly
  transitive-include split on the shared component-link path that risks the green C/C++
  cells. Per-configure keeps components/entries byte-for-byte unchanged; the only edit is
  the one-line archive swap on the umbrella alias, gated on "configure has a Rust node".
- **Scope / blast radius:** ONLY workspaces that contain a Rust node take the swap. Pure-C
  / pure-C++ workspaces, templates, and single-crate examples never trigger it (no Rust
  node → no synth → `nros-cpp-headers` keeps `nros_cpp-static`). Already-green cells stay
  on their exact current path.
- **Work sub-items:**
  - W11.1 — `nros-cpp` `pub FORCE_LINK_ANCHOR` + `nros_cpp_auto_register_backend` (DONE,
    `c7f8999e7`).
  - W11.2 — `nano_ros_node_register(LANGUAGE RUST)`: stop the per-node staticlib
    `corrosion_import_crate`; record metadata + an empty INTERFACE placeholder so the
    CLI's auto-link sidecar `target_link_libraries(entry … rust_pkg_component)` is a no-op
    (the node's symbols come from the runtime umbrella).
  - W11.3 — `nano_ros_workspace`: pre-scan SUBDIRS for Rust node pkgs; if any, synthesize
    `nros_ws_runtime`, `corrosion_import_crate` it, and re-point the `nros-cpp-headers`
    archive. New helper module `cmake/NanoRosRuntimeCrate.cmake`.
  - W11.4 — validation: `examples/workspaces/{c,cpp,mixed}` + templates rebuild 0-dup;
    mixed heartbeat node registers + ticks at runtime; re-confirm the 6 cross cells.
- **Acceptance:** `examples/workspaces/mixed` links with zero cffi dups under GNU-ld
  without `--allow-multiple-definition`; the Rust heartbeat node registers and ticks
  (one shared REGISTRY); c/cpp workspaces stay green.

### W12 — embedded no_std umbrella: panic/allocator dedup (resolved 2026-06-14)
- `nros-cpp` now bundles `nros-c` as a hard dep. On a no_std target each crate that
  emits a `staticlib` artifact needs exactly one `#[panic_handler]` and one
  `#[global_allocator]`; nros-c already supplies both. nros-cpp's own per-platform
  `freertos_alloc` / `zephyr_alloc` / `threadx_alloc` `#[global_allocator]` modules
  (and zephyr_alloc's `#[panic_handler]`) collided → REMOVED. Only the Zephyr
  `critical-section` impl stays (not a duplicate).
- `panic-halt` feature forwards to `nros-c/panic-halt` (same panic-halt crate instance
  → one handler).
- cbindgen regen dropped the now-absent `nros_platform_alloc/dealloc` (triple-dup) and
  `nros_rmw_zenoh_register` externs from `nros_cpp_ffi.h` / `nros_generated.h`.
- **Acceptance:** `cargo rustc --lib --target=thumbv7m-none-eabi … --package nros-cpp
  --crate-type=staticlib` builds clean (panic_handler + global_allocator both resolve);
  freertos + threadx_riscv64 cross cells stay 0-dup.

## Risks

- **Force-linking the backend** from an rlib dep — proven idiom, low risk.
- **C++ needing C symbols** — the 285-symbol anchor; mechanical but verbose.
- **linkme vs ctor auto-register** — with one instance the DUPCHECK collision that
  drove the ctor workaround is gone; can likely re-enable plain linkme-register.
- **SDK-matrix decoupling** may still want standalone backend staticlibs — confirm
  before deleting those crates.
- **High blast radius on the link path** — validate per-cell incrementally.

## W13 — D3 bullet-1/2 completion + 0050 W3.1 (tracked by issue 0062)

Single-runtime delivers D3 bullet 3 (the dup). Bullets 1+2 and issue 0050 W3.1
remain — captured in [issue 0062](../issues/0062-d3-completion-one-registration-path-and-link-manifest.md),
to land **on top of** this model (not a competing design). Summary so the seam is
visible from here:

- **R1 — dispatch table → generated data (bullet 2). DONE (2026-06-14).** The
  per-backend dispatch is data on `RmwDispatch` in `resolve_rmw()` (the RFC-0031 SSoT),
  rendered to `cmake/NanoRosRmwDispatch.cmake` (`nros_rmw_dispatch(<rmw>)`) and
  drift-guarded by `rmw_cmake_dispatch_is_current`. `NanoRosRuntimeCrate.cmake` (the W11
  synth) pulls its cffi feature from it — the hardcoded `_nros_runtime_backend_feature`
  map is gone. The cmake cyclonedds **link wiring** (whole-archive / `force_load` /
  `NROS_RMW_CYCLONEDDS_DDSC_LIBRARY` path) stays platform-specific; the manifest carries
  the data (`NROS_RMW_NEEDS_CXX_LINKER` / `EXTRA_LINK_LIBS`) for it to key off when
  `NanoRosLink.cmake` is reworked under R2/R3. See [RMW backend dispatch](#rmw-backend-dispatch).
- **R2 — close 0050 W3.1. BLOCKED on R3 — NOT a plain deletion (audited 2026-06-14).**
  The weak default and the cmake stub are BOTH load-bearing: hosted needs the weak no-op
  to satisfy `nros_support_init`'s *unconditional* `nros_app_register_backends()` call
  (the `.init_array` ctor does the real registration); **bare-metal startup does NOT walk
  `.init_array`** (`weak_register_backends.c`'s own comment + memory
  `freertos-entry-rmw-backend-link-register`), so the cmake strong stub is the *only*
  registration path there. Deleting either breaks a path. R2 therefore requires the R3
  one-trigger restructure first: a single *guaranteed* registration (drop the
  unconditional call OR a strong ctor everywhere incl. bare-metal startup) before the
  weak default + stub can die. The phase-247 image gate will catch a regression.
- **R3 — one trigger (bullet 1). Designed → [phase-249](phase-249-one-registration-trigger.md).**
  Four belt-and-suspenders triggers (linkme slice / `.init_array` ctor / explicit call /
  board entry) exist because none is universal; the explicit generated call is the only
  one that fires on every target. R3 makes it THE trigger (C/C++: a generated strong
  `nros_app_register_backends`; Rust: `main!`/board-entry explicit register from the R1
  manifest), retiring linkme + the ctors + the weak default. Phased P1–P4 (migrate before
  delete), per-platform e2e gated; **P4 closes R2** (the weak-default + stub deletion).
