# Phase 400 — Rust dependency weight audit

**Status (2026-08-29). IN PROGRESS — W1 landed, W2 proposed and measured but not
implemented.** Opened because a Zephyr build profile showed cargo dominating the
wall clock. This phase is about what the image COMPILES, not about how the build
is scheduled — phase-371 (CLOSED) covered scheduling and is worth reading first
for its record of six retracted hypotheses.

## Method, and why it is stated first

Phase-371's lesson was that plausible build-performance conclusions are usually
wrong, so every number below names how it was taken.

* Crate sets come from `cargo tree --edges normal --prefix none | sed 's/ (\*)$//'
  | sort -u`, not from reading manifests. A manifest read misses transitive
  reachability, which is the whole question.
* Times are COLD (`rm -rf` the target dir), `--release`, and record **CPU as well
  as wall** (`/usr/bin/time`). On a 32-core host wall time hides cost: 48 extra
  crates that compile in parallel barely move the wall clock of ONE leaf while
  still competing for cores with every other leaf in a fixture sweep.
* sccache was verified OFF for these measurements (`RUSTC_WRAPPER` unset, 8
  lifetime compile requests). A cached measurement of dependency weight measures
  the cache.

**One method that did NOT work, recorded so nobody repeats it:** deleting deps
from a manifest and re-running `cargo tree` to see what disappears. `cargo tree`
ERRORS on the broken manifest and prints nothing, so the diff reports the entire
graph as removed. The valid form is a difference of SUBTREES —
`tree(nros-macros) − tree(syn) − tree(quote) − tree(proc-macro2) − tree(nros
without macros)`.

## The finding: `nros`'s `macros` feature triples the graph

Measured against the feature set every Zephyr Rust leaf uses
(`default-features = false, features = ["alloc", "rmw-cffi", "macros"]`):

| | crates | wall | CPU | max RSS |
| --- | --- | --- | --- | --- |
| `alloc,rmw-cffi` | **19** | 1.63 s | 4.1 s | 227 MB |
| `alloc,rmw-cffi,macros` | **67** | 5.26 s | 24.0 s | 364 MB |

The 48 added crates are a host-side toolchain: `serde` + `serde_derive` +
`serde_json` + `serde_yaml_ng`, `toml` + `toml_edit` + `toml_datetime` +
`toml_write` + `winnow`, `yaml-rust2` + `unsafe-libyaml`, `quick-xml` +
`encoding_rs` + `memchr`, `walkdir`, `eyre`, `thiserror` ×2 (both 1.x and 2.x),
`hashbrown` ×2, `indexmap`, `ahash`, `zerocopy`, and the three
`ros-launch-manifest` git crates.

They arrive through `nros-macros`, which is a proc-macro crate and therefore
compiles for the HOST on every leaf. Standalone examples are their own workspace
roots with their own target dirs (RFC-0026), so this is paid PER LEAF, not once.

### It is reachable, but not used by these images

`nros-macros`'s heavy deps are used by exactly two of its source files:

```
toml, nros-pkg-index, nros-orchestration-ir,
ros-launch-manifest-model            -> src/main_macro.rs
serde_json, nros-orchestration-ir    -> src/source_metadata_sidecars.rs
```

That is the LAUNCH ORCHESTRATION path — `nros::main!(launch = "bringup")`. The
Zephyr talker uses `force_link_backend!`, `zephyr_component_main!` and `node!`,
none of which touch it. Cargo compiles the whole proc-macro crate regardless of
which macro a leaf expands, so every non-orchestrating image pays for the
orchestrating one.

## W1 — landed: `nros-launch-parser` was declared and never referenced

`nros-macros` depended on `nros-launch-parser` with no `::` reference anywhere in
its `src/`. Confirmed independently by `cargo-machete`. Removed.

**Honest size of the win: one crate.** Everything `nros-launch-parser` brings
(`quick-xml`, `eyre`, `serde_json`, `walkdir`) is also reached through
`nros-pkg-index`, which the crate genuinely uses — 67 → 66. It is worth doing
because a dependency nobody references is a false statement about what the crate
needs, not because it is fast.

## W2 — proposed, measured, NOT implemented: gate the orchestration half

Put `main_macro.rs` and `source_metadata_sidecars.rs` (and their five deps)
behind a `launch` feature on `nros-macros`, off by default, forwarded by `nros`.

**Upper bound, measured by subtree difference: 43 crates leave the graph** (42
net of `nros-macros` itself, which stays for `node!`). `syn`, `quote`,
`proc-macro2` and `unicode-ident` remain — they are the macro machinery, not the
orchestration.

The time saving is **NOT yet measured**. It cannot be, without implementing the
gate: the 24.0 CPU-s figure above includes `syn`, which stays. Quoting the full
20 s delta as the saving would be wrong. What is measured is the crate-count
delta and the fact that the graph is reachable-but-unused for these leaves.

Design note for whoever takes it: per the `std`-deletion rule in CLAUDE.md, whose
requirement it is decides the spelling. Launch orchestration is a capability the
CONSUMER picks, so the feature is REQUIRED (a `compile_error!` naming `launch`
when `main!(launch = …)` is expanded without it), not silently granted.

## Not yet examined

* Everything below `nros` — `nros-platform`, `nros-rmw-zenoh`, `zpico-sys` (which
  also compiles C), the generated message crates, and the `zephyr` crates. The
  Zephyr profile that motivated this phase has not been broken down per crate;
  `just profile` found no timing artifacts under the leaf, and a west build with
  `cargo build --timings` has not been run.
* `thiserror` appearing at BOTH 1.x and 2.x, and `hashbrown` at 0.14 and 0.17 —
  duplicate major versions compile twice. Not yet traced to their requirers.
* `cargo-machete` across the repo flags 467 rows, but it is largely UNUSABLE
  here: the top entries (`nros-rmw-zenoh` ×59, `nros-platform` ×50,
  `nros-platform-cffi` ×23) are FORCE-LINK deps, present so rustc's staticlib DCE
  does not drop their `#[no_mangle]` exports. Machete cannot see `extern crate`
  force-links. Its output needs per-row triage against that pattern before any of
  it is acted on; W1 is the one row confirmed by reading the source.
