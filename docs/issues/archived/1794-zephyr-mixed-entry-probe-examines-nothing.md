---
id: 1794
title: "The Zephyr fixture freshness probe reports `examined 0 input(s)` for the mixed workspace entry — half 2 never accounts, and half 1 looks for a `.d` this road does not write"
status: resolved
resolved: 2026-10-11
type: bug
area: [testing]
severity: medium
found: 2026-10-11
related: [0466, 1005, 1045, 0445, 1535, phase-477]
---

## Measured

`entry_e2e` `zephyr/mixed/entry_pubsub`, minutes after building the fixture
with `NROS_ZEPHYR_FIXTURE_FILTER=workspace-entry-mixed just zephyr
build-fixtures` (PR #1891's verification, issue 1535):

```
[nros-tests] WARNING: fixture reported FRESH by a DEGRADED probe — it compared NOTHING.
probe: examined 0 input(s); exempted 0 regenerated-in-place header + 0 cargo OUT_DIR
product + 0 config-header stamp beside its header.
  binary: …/build/zephyr-workspace-builds/3.7/build-ws-mixed-entry-zenoh/zephyr/zephyr.exe
```

The cell ran and passed, so the verdict was right this time — by luck. A
FRESH verdict from this probe is not evidence.

## Two gaps in `require_prebuilt_binary_fresh_zephyr`

1. **Half 2 compares inputs and never says so.**
   `zephyr::source_dir_is_stale` walks the leaf's authored sources and the
   configure inputs, but nothing in `zephyr.rs` calls
   `staleness::note_candidate`, which is the only thing that increments the
   `examined` count. So for every Zephyr image whose half 1 is silent, the
   accounting line reads 0 whatever half 2 actually compared. The DEGRADED
   warning cannot tell "compared nothing" from "compared things and did not
   count them" — the shape issue 0445 says to distrust.
2. **Half 1 looks only where zephyr-lang-rust writes.**
   `zephyr_staticlib_dep_file` searches `<build>/rust/target/*/<profile>/librustapp.d`.
   The generated mixed entry builds its Rust half on the cmake road
   (`nros-rust/`, `nros-rust-ws-nros_ws_runtime/` in the build root), so
   there is no `rust/target` and the Rust closure is not watched through a
   `.d` at all — even though this image links Rust nodes
   (`rust_heartbeat_pkg`).

## Sweep

Every Zephyr resolver goes through the same function, so gap 1 affects every
C, C++ and generated-entry Zephyr fixture. Gap 2 affects every image whose
Rust is built by Corrosion rather than `rust_cargo_application`:

```
git grep -n 'require_prebuilt_binary_fresh_zephyr' -- packages/testing/nros-tests/src
git grep -n 'librustapp.d' -- packages/testing/nros-tests/src
```

## What would close it

Half 2 accounts through `note_candidate` like every other arm, and half 1
finds the Rust dep-info the cmake road writes (or says it measured none, via
`note_unmeasured_input_set`). The mixed cell's `probe:` line then names a
non-zero count, and `NROS_STRICT_STALENESS_PROBE=1` passes on it.

## Resolution (2026-10-11)

Both gaps closed in `require_prebuilt_binary_fresh_zephyr`, and it is still one
function every Zephyr resolver goes through.

1. **Half 2 accounts.** `zephyr::source_dir_is_stale` flattens its candidates
   to files and puts each through `staleness::note_candidate`, the shared
   accounting + exemption rule, before the content comparison. Each file is
   counted once.
2. **Half 1 finds both roads.** `zephyr_staticlib_dep_files` keeps the
   zephyr-lang-rust lookup (`rust/target/*/<profile>/librustapp.d`, found by
   profile because that road's `build.ninja` names a `debug/` archive it never
   writes) and adds every archive `build.ninja` LINKS whose `.d` sits beside it
   — the cmake road's `nros-rust/…/libnros_c.a`, `libnros_cpp.a`, the
   generated entry's `libnros_ws_runtime.a`, the C++ FFI image staticlib. Asking
   the manifest rather than walking profile dirs keeps a leftover profile out.
   The C single-example images had the same gap: they link `libnros_c.a` and
   `libnros_rmw_zenoh_staticlib.a` and no `.d` was read.
3. **One comparison, not two.** Half 1 no longer returns an mtime verdict of
   its own: its inputs join half 2's single content-aware check, because two
   checks against the one `.nros-srcbaseline` would each read the other's
   files as additions. Its strictness stays: when a Rust input is already
   newer than the image, the no-baseline answer is STALE
   (`first_sight_is_fresh = false`), as on the cargo dep-info arm.

Measured, same fixtures, `NROS_STRICT_STALENESS_PROBE=1`:

| image | before (main) | after |
| --- | --- | --- |
| `build-ws-mixed-entry-zenoh` | `examined 0 input(s)` — DEGRADED, cell refused | 3388 examined, 1 exempt; `zephyr/mixed/entry_pubsub` ran |
| `build-c-talker-zenoh` | `examined 0` | 3262 examined, 1 exempt |
| `build-c-listener-zenoh` | `examined 0` — `case_02_zenoh_c_pubsub_e2e` refused | 3262 examined; passed |

Hermetic test: `fixtures::binaries::tests::zephyr_probe_watches_every_linked_cargo_staticlib`
(the three `build.ninja` token shapes, a C archive with no `.d`, a leftover
profile nothing links).
