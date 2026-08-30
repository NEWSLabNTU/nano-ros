---
id: 491
title: "A PATH-valued env var was fingerprinted as a STRING, so rows sharing one
  cargo group rebuilt each other forever"
status: resolved  # fixed + gated 2026-08-10
type: bug
area: build
related: [phase-340, phase-345, rfc-0070, issue-0490, issue-0451, issue-0196, rfc-0048]
---

## Symptom

Two rows in the same shared cargo group cannot both be fresh. Building
`examples/qemu-arm-freertos/rust/listener` makes `…/talker` dirty, and building
talker makes listener dirty — indefinitely. Measured on a settled tree, after
issue 0490 was fixed (which is why 0490 had to go first: it made every row dirty
for an unrelated reason and hid this one):

```
A  talker alone, probed twice          -> fresh, fresh
B  six sibling rows, probed in order   -> five dirty on pass 1, all six on pass 2
C  build logging-smoke, re-probe talker-> talker dirty
```

Reproduced on this branch, on a freshly built `build/fixtures-cargo/freertos`
group (7 rows, `nros-minsizerel`), with a probe that resolves exactly like
`scripts/test/rust-fixture-stale.sh` and counts recompiled units:

```
pass 1  talker 6  listener 6  service-server 6  service-client 6
        action-server 6  action-client 6   logging-smoke 0
pass 2  identical — the same six rows, the same six units, forever
pass 3  talker 6, then talker again 0   (a row IS stable in isolation)
```

## Cause — confirmed from cargo's own fingerprints

`cargo:rerun-if-env-changed=NAME` makes cargo compare that variable's value as
**text**. One directory has many spellings, and this repo produces three for the
same first-party source dir:

| spelling | produced by |
|---|---|
| `<repo>/packages/platform/nros-platform-freertos/src` | `just/sdk-env.just` (absolute, `justfile_directory()`-rooted) — every `just` recipe |
| `<repo>/examples/qemu-arm-freertos/rust/talker/../../../../packages/platform/nros-platform-freertos/src` | the leaf `.cargo/config.toml`'s `{ value = "../../../../…", relative = true }`, resolved against THAT leaf — one spelling per leaf |
| (unset) | a bare `cargo build` with neither |

Both were sitting in the group's fingerprints at once — `nros-board-freertos`'s
`run-build-script-*.json`, two units, same directory:

```
nros-board-freertos-7e19c943…  NROS_PLATFORM_FREERTOS_SRC =
  …/examples/qemu-arm-freertos/rust/talker/../../../../packages/platform/nros-platform-freertos/src
nros-board-freertos-9bdb9728…  NROS_PLATFORM_FREERTOS_SRC =
  …/packages/platform/nros-platform-freertos/src
```

and cargo's fingerprint log names the comparison it fails:

```
dirty: EnvVarChanged { name: "NROS_PLATFORM_FREERTOS_SRC",
  old_value: Some(".../listener/../../../../packages/platform/…/src"),
  new_value: Some(".../talker/../../../../packages/platform/…/src") }
```

`nros-board-freertos` and `nros-zpico-build` both declared such variables, so
each sibling re-ran both build scripts and `UnitDependencyInfoChanged` cascaded
up to the leaf bin.

Per-leaf `target/` dirs hid this completely: each leaf had its own fingerprint
namespace, so the spellings never met. **Sharing the dir is what surfaced it** —
a cost of the phase-340 group mechanism, present since B3 wave 2.

Two things the original write-up got wrong, both refuted by measurement:

* **It is not only a sibling-vs-sibling effect.** The BUILD runs under `just`
  (absolute) and the staleness PROBE does not (leaf-relative), so build and
  probe flip the same fingerprint even for a single row. That is why no
  generator-side normalisation can fix it: `nros sync` owns one of the three
  spellings.
* **A per-leaf `rerun-if-changed` path is harmless.** Cargo reads the watched
  path list back from the STORED build-script output, never from a second
  resolution, so it never compares two spellings of it (measured on a synthetic
  two-leaf group: `rerun-if-changed` on the raw per-leaf value → 0 units on
  every alternating probe; `rerun-if-env-changed` on the same value → the full
  cascade). Only the ENV value is re-read and compared.

## The rule has TWO producers, and fixing one looked like fixing it

After the Rust-side sweep below, every FreeRTOS row went to 0 units — and every
ThreadX row still rebuilt 6. The second producer is DATA:
`config/*/nros-platform.toml` carries a `rerun_if_env_changed` list which
`nros-zpico-build/src/runner.rs` replays through
`println!("cargo:rerun-if-env-changed={var}")`, and `config/threadx`'s listed
all four ThreadX path variables. A gate that reads only Rust literals passes
against a green tree while the defect keeps running from a TOML file — the
issue-0196 shape, caught here only because the FIX was verified by a BUILD on
both platforms rather than by the gate.

## Fix

**Path-valued build inputs are fingerprinted by their CONTENT, never by their
env spelling.** `packages/tooling/nros-build-paths` owns the one spelling:

* `canonical(path)` — resolve, no directive;
* `watch_path(path)` — canonical + `cargo:rerun-if-changed`, skipped when the
  path is absent (a trigger on a missing path is permanently dirty, issue 0490);
* `env_path` / `env_path_watched` / `env_or_repo_path` — the readers.

Every `cargo:rerun-if-env-changed` on a path-shaped name is gone — 16 Rust
files / 57 sites plus the three platform manifests that carried the list form
(`config/{threadx,nuttx,freertos-lwip}/nros-platform.toml`), swept with the gate
below. The manifest's `include_paths` / `include_paths_conditional` (which
interpolate `{env:…}`) are now watched by content in `runner.rs`, so the shim
still rebuilds when those headers change. `nros-board-freertos` now resolves the two
platform paths through `nros-build-paths` instead of panicking when they are
unset — which also fixes a silent false-FRESH: `logging-smoke-freertos-mps2` is
the one FreeRTOS row with no `[env]` block, so its probe hit

```
NROS_PLATFORM_FREERTOS_SRC not set — overlays should set it via `.cargo/config.toml [env]`
```

and `rust-fixture-stale.sh`, whose stderr goes to `/dev/null`, read the failed
build as "not stale".

The leaf `.cargo/config.toml` `[env]` blocks are UNCHANGED. They are the
authored half (RFC-0048 W9), their values are still correct, and with the
directive gone their spelling no longer reaches a fingerprint.

**Known cost, stated plainly:** cargo no longer notices that one of these
variables now names a DIFFERENT directory — nothing re-runs the script, so it
keeps watching the old path. In-tree that cannot happen (the paths are fixed by
the checkout). An out-of-tree consumer who repoints one must `cargo clean` that
build dir.

Gate: `scripts/check-path-env-fingerprints.py` (`just check fast`), the sibling
of 0490's `check-build-rs-rerun-paths`. It reads BOTH producers, self-tests both
directions on each (a synthetic Rust source and a synthetic manifest), and
carries a per-name `ALLOWED` map, each entry stating why that variable's
spelling cannot vary within one target dir (cmake-per-build-dir vars, cargo
`links` metadata, NuttX-relative subpath knobs). Tripwired live in both arms:
re-adding the two removed lines to `nros-board-freertos/build.rs` and
re-adding `THREADX_CONFIG_DIR` to `config/threadx/nros-platform.toml` each made
it exit 1 naming that file.

## Result

Same probe harness, same groups, before vs after — each row probed in manifest
order, twice over, on a tree just built by `just <plat> build-examples`:

```
freertos       (7 rows)   6 units per probe  ->  0     (logging-smoke was already 0)
threadx-riscv64 (7 rows)  6 units per probe  ->  0
```

`0` in every pass, including pass 2 (the "settled tree rebuilds nothing" pass
that used to be identical to pass 1).

Wall clock, controlled A/B (only the two `rerun-if-env-changed` lines in
`nros-board-freertos/build.rs` differ; both trees rebuilt from the same
`just freertos build-examples`, warm, one discarded warm-up, then talker and
listener probed ALTERNATELY for 3 reps each):

```
with the directives     3.90 3.95 3.98 4.06 4.32 3.89  s   (4 units each)
without them            0.11 0.11 0.11 0.12 0.12 0.14  s   (0 units each)
```

~4 s per probe per row, on every staleness probe and every incremental build of
every row in a shared group. (The A/B restores two of the five directives, hence
4 units rather than the 6 the full defect produced.)
