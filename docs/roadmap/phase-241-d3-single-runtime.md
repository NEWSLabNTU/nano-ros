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

| Backend | In the umbrella | Extra link |
| --- | --- | --- |
| zenoh     | `nros-rmw-zenoh` rlib dep (force-linked) | — |
| xrce      | `nros-rmw-xrce-cffi` rlib dep (force-linked) | — |
| cyclonedds| nothing (C++ lib linked separately) | `libnros_rmw_cyclonedds` + `libddsc` + **libstdc++** (incl. C) |

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

#### W11 design — per-entry runtime staticlib (Option D)
- **Seam (unchanged):** `nros::node!()` emits `#[no_mangle] extern "C"
  __nros_component_<pkg>_register`. The CLI-generated C++ `main` already calls that symbol
  for every node (C/C++/Rust alike). **So `nros codegen entry` does NOT change** — only
  the link wiring does.
- **Runtime crate:** for an entry whose deployed node set contains ≥1 Rust node,
  `nano_ros_entry` synthesizes (at configure time, `file(WRITE)`) a crate
  `<build>/<entry>_runtime/`:
  - `Cargo.toml`: `[lib] crate-type=["staticlib"]`; deps = `nros-cpp` (umbrella, as rlib)
    with the workspace's `<backend>`+`<platform>`+`ros-*` features, plus one `path` dep per
    Rust node (rlib). Carries `[workspace]` + the nros-managed `[patch.crates-io]` block
    (copied from the workspace, same as node crates).
  - `src/lib.rs`: `pub use nros_cpp::*;` + a `#[used]` anchor on nros-cpp's exported
    `FORCE_LINK_ANCHOR` (re-pulls the C ABI + backend past staticlib DCE) + one `#[used]`
    anchor per Rust node's `__nros_component_<pkg>_register` (keeps each node's register
    symbol). Anchors are required because a `#[used]` living in a dependency rlib is DCE'd
    from the final staticlib root (same rule as W3's nros-c→nros-cpp anchor, generalized
    one level: nros-cpp→runtime).
- **Synthesis site: CMake `nano_ros_entry`** (not the CLI). All inputs are already in
  scope there — `NANO_ROS_RMW`/`NANO_ROS_PLATFORM` cache vars, `nros-metadata.json` (Rust
  node `pkg_dir`s + sanitized syms), the on-disk patch block — and the
  `corrosion_import_crate` + `target_link_libraries` it feeds live in the same function.
  The CLI has no backend/platform notion; pushing synthesis there would add new CLI flags
  and split one logical step across two layers.
- **cmake wiring changes:**
  - `nano_ros_node_register(LANGUAGE RUST)`: stop `corrosion_import_crate(…staticlib)`;
    only record the node in metadata (pkg_dir + sanitized sym). No per-node archive.
  - `nano_ros_entry`: if any deployed node is Rust → write + `corrosion_import_crate` the
    `<entry>_runtime` staticlib and link THAT instead of `NanoRos::NanoRosCpp`. Else:
    current path unchanged. (C entries with Rust nodes use `nros-c` in place of `nros-cpp`.)
  - `nros-cpp`: expose one `pub` `FORCE_LINK_ANCHOR` (C-surface + backend) so the runtime
    root can re-anchor it. Small, mirrors the existing private `_KEEP_C_SURFACE`.
- **Scope / blast radius:** ONLY Rust-node-bearing workspaces take the new path. Pure-C /
  pure-C++ workspaces, templates, and all single-crate examples are untouched (already
  green). C/C++ component libs are unchanged — their undefined C-ABI refs resolve from the
  runtime staticlib exactly as they did from the umbrella.
- **Work sub-items:**
  - W11.1 — `nros-cpp` `pub FORCE_LINK_ANCHOR` (C-surface + backend).
  - W11.2 — `nano_ros_node_register(LANGUAGE RUST)`: metadata-only, drop the staticlib import.
  - W11.3 — `nano_ros_entry`: detect Rust nodes; synthesize + import + link `<entry>_runtime`.
  - W11.4 — validation: `examples/workspaces/{c,cpp,mixed}` + templates rebuild 0-dup;
    mixed heartbeat node registers + ticks at runtime; re-confirm the 6 cross cells
    unaffected.
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
