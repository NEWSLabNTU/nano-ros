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

### W5 — retire the standalone backend staticlibs
- Delete `nros-rmw-zenoh-staticlib` / `nros-rmw-xrce-cffi-staticlib` **iff** no SDK-
  matrix consumer still imports them (confirm via grep of cmake + docs first); else
  keep + document why.
- **Acceptance:** workspace builds; no dangling references.

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

## Risks

- **Force-linking the backend** from an rlib dep — proven idiom, low risk.
- **C++ needing C symbols** — the 285-symbol anchor; mechanical but verbose.
- **linkme vs ctor auto-register** — with one instance the DUPCHECK collision that
  drove the ctor workaround is gone; can likely re-enable plain linkme-register.
- **SDK-matrix decoupling** may still want standalone backend staticlibs — confirm
  before deleting those crates.
- **High blast radius on the link path** — validate per-cell incrementally.
