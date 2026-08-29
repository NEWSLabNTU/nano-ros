# Phase 400 — Rust dependency weight audit

**Status (2026-08-29). IN PROGRESS — W1 landed; W2 (the 42.6 % pair) and W3
measured and specified, neither implemented. Waves are ordered by measured
value, not by discovery order.** Opened because a Zephyr build profile showed cargo dominating the
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

## W2 measured on the real build — `cargo build --timings`, cold

`--timings` injected into the Zephyr talker's `EXTRA_CARGO_ARGS`, the leaf's
`rust/target` (1.7 GB) DELETED, then `just zephyr build-one rust/talker zenoh`.
The HTML embeds `const UNIT_DATA`; per-unit durations parsed from it.

**234 units, 220 actually compiled, 13.5 s wall, 118.3 CPU-s.**

| chain (DISJOINT sets) | CPU | share |
| --- | --- | --- |
| `bindgen` chain, exclusive | 20.6 s | 17.4 % |
| `cbindgen` chain, exclusive | 8.1 s | 6.8 % |
| **nros orchestration crates** | **3.1 s** | **2.6 %** |
| **shared support crates** | **43.8 s** | **37.0 %** |
| target-side + everything else | 42.7 s | 36.1 % |

The "shared support" row is `serde`/`serde_core`/`serde_derive`, `toml` +
`toml_edit` + `winnow`, `serde_json`, `memchr`, `zerocopy`, `syn`, `thiserror`,
`indexmap`/`hashbrown`, `quick-xml`, `yaml-rust2`, `walkdir`, `eyre`. Heaviest:
`zerocopy` 6.9 s, `winnow` 3.7 s, `syn` 2.6+2.0 s, `memchr` 2.5 s.

**Gating the orchestration half removes 14.9 s exclusively (3.1 s of it the nros
crates themselves), not 37.7 s.** An earlier
version of this document claimed 31.9 %, and it was WRONG in a way worth
recording because the mistake is easy to repeat: the "removable" set was computed
from the `nros` package graph and then matched by NAME against the leaf's build,
so every shared crate got attributed to orchestration. The three groups
overlapped and were summed independently, which also double-counted them.
`cbindgen` parses `cbindgen.toml`, so it wants `serde` and `toml` whether or not
`nros-macros` does; `bindgen` wants `regex`/`memchr`/`prettyplease`. Cold, the
nros orchestration crates themselves are `nros-macros` 0.59 s, `nros-pkg-index`
0.06 s, `nros-orchestration-ir` 0.07 s.

The 43.8 s shared bucket is the real prize, and it is NOT claimable by any single
change: those crates leave the build only when EVERY requirer of them does.

**A discarded measurement, recorded so the number is not quoted from the log.**
The FIRST run of this build reported 45.3 s wall / 255.3 CPU-s and put
`nros-macros` second at 16.4 s. Two things were wrong with it: the leaf's
`rust/target` was already populated, so most third-party units read `0.00s`
(fresh, not compiled) and the orchestration share came out at a misleading 7.4 %;
and the per-unit times were inflated by CPU contention from other cargo work
running at the same time — cold and uncontended, `nros-macros` itself is 0.6 s.
Take the 118.3 s table, not the 255.3 s one.

## Work items, ordered by measured value

Sizes are exclusive savings from the cold Zephyr `rust/talker` profile below
(118.3 CPU-s total). "Exclusive" means: crates that leave the build when THIS
lever lands and nothing else changes.

| wave | lever | exclusive | share |
| --- | --- | --- | --- |
| W2 | gate orchestration **and** cbindgen, together | **50.4 s** | **42.6 %** |
| W3 | `bindgen` -> committed output | 20.6 s | 17.4 % |
| W4 | attribute the contested pool inside the leaf's graph | (enabling) | — |
| W1 | *landed* — unused dep removed | 1 crate | — |

**Numbering note.** W1 keeps its number because it has landed and is cited by
commit subject. The rest were renumbered into value order; an earlier revision of
this doc had the bindgen work as W4, the attribution as W3, and cbindgen as a
separate W5.

### W2 — gate the orchestration half AND move cbindgen, as ONE change

**50.4 s, 42.6 % — and only if both halves land.** This is the phase's main
finding and it is not visible from either half alone:

    orchestration-exclusive                    14.9 s   12.6 %
    cbindgen-exclusive                          8.1 s    6.8 %
    contested (serde, syn, toml, memchr,
      indexmap, thiserror, ...)                27.4 s   23.1 %

`cbindgen` parses `cbindgen.toml`, so it wants `serde` + `toml` + `syn`; the
orchestration path wants the same crates for launch files. **Do either one alone
and the 27.4 s contested pool stays** — a plausible-looking change that measures
as nearly nothing. Together they take 50.4 s of 118.3.

The orchestration cut is verified clean: `main_macro.rs` is 3798 of the crate's
4692 lines with `lib.rs:41` its only caller, and `source_metadata_sidecars.rs`
has exactly one caller (`main_macro.rs:884`). So
`#[cfg(feature = "launch")] mod main_macro;` plus five optional deps, forwarded
by `nros`. The feature is REQUIRED, not granted — a `compile_error!` naming
`launch` when `main!(launch = ...)` is expanded without it.

Largest single unit freed is `zerocopy` at 7.3 s, reached ONLY through
`ahash` -> `hashbrown` -> `hashlink` -> `yaml-rust2` -> `ros-launch-manifest-types`.
It is orchestration's transitive tail, not something the macro crate names.

*Acceptance:* a Zephyr Rust leaf builds with the feature off; `main!(launch = ...)`
without it fails naming the feature; cold `--timings` on the same leaf shows
`zerocopy`, `yaml-rust2`, `serde`, `toml` and the `clap` stack ABSENT. Measure
the pair; do not ship half and quote this table.

### W3 — build-time `bindgen` to committed output

**20.6 s, 17.4 %** — the largest single-lever number, and the hardest. The repo
already solved this once: RFC-0054 commits bindgen output for the ABI crates
(`nros-{rmw,platform,board}-cffi/src/generated.rs`) and gates staleness with
`check-abi-bindings`. Four `*-sys` driver crates still generate at build time:
`zephyr-posix-sys`, `nuttx-sys`, `freertos-lwip-sys`, `threadx-netx-sys`.

**Not a straight copy of that pattern.** These bind the USER's RTOS headers
(`ZEPHYR_BUILD_DIR` and friends), not in-tree ones, so committed output asserts
which SDK produced it. Their allowlists are small — a handful of socket types —
which makes hand-mirroring the structs tempting; that is issue 0160's hazard,
where a mirror-only TU passes a shorter struct and the tail field is garbage. If
taken, it should be commit + a regenerate-and-diff gate per supported SDK,
mirroring `check-abi-bindings`.

*Acceptance:* a cold leaf build compiles no `bindgen`/`clang-sys` unit; the gate
fails when a header moves without regeneration.

### W4 — attribute the contested pool inside the LEAF's graph

Everything above uses reverse dependencies from the WORKSPACE
(`cargo tree -i`), not from the leaf, because the leaf does not resolve
standalone (`zephyr-build` comes from the west environment). The exclusive/
contested split is therefore provisional.

This wave is cheap insurance, not groundwork for its own sake: the orchestration
number has already moved 31.9 % -> 2.6 % -> 12.6 % across three attribution
methods, and only the last was computed from real dependency paths. Do this if a
wrong estimate would be expensive; skip it if W2 is going to be measured
end-to-end anyway, since the acceptance check there catches the same error.

*Acceptance:* a crate -> requirers table taken inside the west build.

### W1 — landed

`nros-launch-parser` removed from `nros-macros` — declared, referenced nowhere.
One crate (67 -> 66): everything it brought is also reached through
`nros-pkg-index`, which the crate genuinely uses.

## Directions, in measured order

1. **Gate the orchestration half of `nros-macros`** — see W2, and note it must
   ship WITH the cbindgen move or the contested pool stays put.
2. **`bindgen` at build time — 18.4 %.** The repo ALREADY has the alternative and
   proved it: RFC-0054 commits bindgen output for the ABI crates
   (`nros-{rmw,platform,board}-cffi/src/generated.rs`) and gates staleness with
   `check-abi-bindings`. Four `*-sys` driver crates still generate at build time
   (`zephyr-posix-sys`, `nuttx-sys`, `freertos-lwip-sys`, `threadx-netx-sys`).
   **Not a straight copy of that pattern:** these bind the USER's RTOS headers
   via `ZEPHYR_BUILD_DIR` and friends, not in-tree ones, so committed output
   would assert which SDK generated it. The allowlists are small (a handful of
   socket types), which makes it tempting to hand-mirror the structs — do NOT:
   that is issue 0160's hazard, where a mirror-only TU passes a shorter struct
   and the tail field is garbage. If this is taken, it should be commit + a
   regenerate-and-diff gate per supported SDK, mirroring `check-abi-bindings`.
3. **`cbindgen` — 6.8 %**, same shape one size down (`nros-zpico-build`,
   `nros-build-helpers`), and it drags the whole `clap` CLI stack into a build
   dependency.

## Not yet examined

* Whether the same profile holds for the C/C++ Zephyr leaves, for NuttX/FreeRTOS,
  or for a workspace (non-standalone) build where the phase-340 shared cargo
  group amortises host deps across leaves. Everything above is ONE leaf,
  `rust/talker`, zenoh, `native_sim`.
* `heapless` appearing TWICE at 5.1 s and 4.4 s — two feature-distinct units of
  the same version, 9.5 s combined, larger than `bindgen` itself. Not traced.
* `getrandom` at 3.0 s on an image that should not need OS entropy.
* `thiserror` appearing at BOTH 1.x and 2.x, and `hashbrown` at 0.14 and 0.17 —
  duplicate major versions compile twice. Not yet traced to their requirers.
* `cargo-machete` across the repo flags 467 rows, but it is largely UNUSABLE
  here: the top entries (`nros-rmw-zenoh` ×59, `nros-platform` ×50,
  `nros-platform-cffi` ×23) are FORCE-LINK deps, present so rustc's staticlib DCE
  does not drop their `#[no_mangle]` exports. Machete cannot see `extern crate`
  force-links. Its output needs per-row triage against that pattern before any of
  it is acted on; W1 is the one row confirmed by reading the source.
