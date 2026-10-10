---
id: 1794
title: "The Zephyr fixture freshness probe reports `examined 0 input(s)` for the mixed workspace entry — half 2 never accounts, and half 1 looks for a `.d` this road does not write"
status: open
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
