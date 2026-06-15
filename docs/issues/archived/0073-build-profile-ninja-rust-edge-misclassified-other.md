---
id: 73
title: build-profile mislabels some corrosion Rust edges as "other" (nuttx nros-nuttx-ffi)
status: resolved
type: bug
area: build
related: [phase-251]
resolved_in: phase-251
---

## Resolution

Fixed by detecting cargo builds via output **path**, not just name token:
`is_rust_build()` now also returns true for a `cargo-target/` directory or a
`<target-triple>/release|debug/` binary (the triple test = ≥ 2 dashes in the
parent component, which avoids matching a plain `build/release/app` C output).
nuttx's `cargo-target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi` (31.4 s) now
lands in **compile**. Verified on the real build: `compile 100%` (was
`compile 4% / other 72%`); `other` dropped to 1.5 s. Locked with
`ninja_classifies_untokenized_cargo_output_as_compile` (incl. a non-triple
false-positive guard). `packages/testing/nros-build-profile/src/collect/ninja.rs`.

## Problem

`nros-build-profile` (phase-251) classifies a ninja edge as a Rust/compile build via
`is_rust_build()` in `packages/testing/nros-build-profile/src/collect/ninja.rs`, which
matches output names containing `cargo_build` / `cargo-build` or ending in `.rlib`.

Some corrosion targets don't carry any of those tokens. Profiling the real nuttx C++
talker (`examples/qemu-arm-nuttx/cpp/talker/build-zenoh`) showed:

```
nuttx/cpp-talker-zenoh   ninja-cmake   45.8s   compile=4% link=31% other=72%
  • 1 unit = 95% of other (nros-nuttx-ffi, 31.4s)
```

The 31.4 s `nros-nuttx-ffi` edge is the **Rust FFI staticlib build** — the dominant cost
of the whole build — but its stamp output is named `nros-nuttx-ffi` (no `cargo[-_]build`
token, no known ext), so it falls into the `Other` bucket. The profile then reports
`compile=4%`, which is misleading: the real compile cost is hidden in `other=72%`.

Same class would hit any corrosion target whose stamp/output name doesn't follow the
`*_cargo_build` / `_cargo-build_*` convention.

## Evidence

- `is_rust_build()` token list: `cargo_build`, `cargo-build`, `.rlib` (ninja.rs).
- Real run above: `nros-nuttx-ffi` (31.4 s) → `other`, dominating a 45.8 s build while
  `compile` reads 4 %.
- Other platforms classify correctly because their corrosion outputs *do* carry the
  token (`nros_rmw_zenoh_staticlib_cargo_build` on zephyr, `_cargo-build_nros_cpp` on
  native cmake).

## Direction (not yet scoped)

Options, roughly in order of robustness:

1. **Group-aware detection** — if any output of the *same edge* is a `.a`/staticlib whose
   stem matches a known corrosion crate, treat the whole edge as a Rust compile. Needs the
   `.a` to share the edge (it may be a separate edge — verify against a real nuttx log).
2. **Corrosion-name heuristic** — also match `*-ffi` / `*_ffi` stamp names, or names that
   pair with a sibling `lib<name>.a` link edge. Fragile; risks false positives.
3. **Read the ninja `build.ninja`** for the edge's `command` (contains `cargo`), instead of
   guessing from output names. Most accurate; more work, and `build.ninja` must be present.

Low severity: the **total** is correct and the dominant unit is still surfaced by the
`dominant_unit` hint ("nros-nuttx-ffi, 31.4 s"), so a reader still sees the real cost —
only the stage attribution is wrong. Worth fixing for accurate per-stage percentages on
the C/C++-on-RTOS (corrosion) builds.
