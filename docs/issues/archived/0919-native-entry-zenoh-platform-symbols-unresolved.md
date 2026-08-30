---
id: 919
title: "`bridge-cyclonedds`'s `native_entry` fails to link in CI on 16 zenoh-pico
  platform symbols — CI resolves a different `zpico-sys` unit than any local build
  produces"
status: resolved
type: bug
area: ci
related: [issue-0616, issue-0270, issue-0897, phase-340]
---

## Problem

Every `host-tests` run on `main` fails in `Build workspace fixtures`:

```
rust-lld: error: undefined symbol: z_sleep_ms
rust-lld: error: undefined symbol: _z_mutex_init
… 16 in total …
  >>> in archive /__w/nano-ros/nano-ros/examples/workspaces/bridge-cyclonedds/
      target-fixtures/nros-relwithdebinfo/deps/libzpico_sys-9fab60fac0f311ee.rlib
error: could not compile `native_entry` (bin "native_entry")
```

All 16 are zenoh-pico's platform layer — `z_sleep_ms`, `z_malloc`,
`z_random_fill`, `z_clock_elapsed_ms`, `_z_task_{init,detach}` and the
`_z_mutex_*` family. On POSIX these are **not** taken from upstream
`src/system/unix/system.c` (`config/posix/nros-platform.toml` deliberately
lists only `network.c` and `tls.c`); they come from our own alias TU,
`zpico-sys/c/zpico/platform_aliases.c`, compiled only when

```rust
env::var_os("CARGO_FEATURE_PLATFORM_ALIASES").is_some() && !use_freertos && any_explicit
```

So the failing unit is a `zpico-sys` built **without** the alias TU.

## Not the workspace it looks like

The package is `native_entry`, which exists in more than one workspace. The
archive path is the only thing that names which: **`examples/workspaces/
bridge-cyclonedds`**, not `workspaces/rust`. Reading "native_entry" and
assuming the Rust workspace cost several wrong turns here — the path is in the
log and should be read first.

## What is established

**CI resolves a `zpico-sys` unit that no local build reproduces.** A cold local
build of the same entry produces exactly one unit, `f3b15e4085f53b66`, and it
HAS `libzpico_platform_aliases.a`. CI's failing unit is `9fab60fac0f311ee` and
by the error has no aliases. Different unit hash means different feature
resolution — this is not link ORDER, and not a stale artifact.

**Eight local routes all link successfully**, on a host with both submodules
initialised at main's pinned commits:

| route | result |
| --- | --- |
| cold `cargo build` on the generated root manifest, `dev` | links |
| the same, plus `--target x86_64-unknown-linux-gnu` | links |
| cold, `--profile nros-relwithdebinfo` + `RUSTFLAGS=-C debuginfo=0` | links |
| into the REAL shared `target-fixtures/nros-relwithdebinfo` dir | links |
| `cargo build -p native_entry` from the workspace root (what `nros build` execs) | links |
| the full `just native build-workspace-fixtures` sweep | see below |
| after a fresh `nros sync` (the stale-generated-input suspicion) | links |
| the real `nros build demo_bringup:native --workspace . --offline` | links |

That last one is worthless as evidence and was reported as a pass earlier in
error: it rebuilt nothing. `native_entry` was dated 08-05 and the alias archive
08-03/08-05, three weeks stale — the museum-binary case CLAUDE.md warns about,
walked into and quoted as a result.

`nros build --dry-run` gives the real command, which route 5 reproduces:

```
cargo build -p native_entry --frozen --profile nros-relwithdebinfo --target-dir target-fixtures
```

## The generated inputs WERE stale, and it still was not the cause

Worth recording because it looked like the answer. The local
`generated/nros-selection/native_entry/Cargo.toml` predated the current `nros`,
and a fresh `nros sync` rewrites it:

```diff
-nros-board-linux = { …, features = ["rmw-zenoh"] }
+nros-board-linux = { …, default-features = false, features = ["rmw-cyclonedds", "rmw-zenoh"] }
```

So every route above the line had been run against an input CI does not use.
Re-running them against the fresh one still links, and `nros build` regenerates
the entry root BYTE-IDENTICALLY, so the root is not the variable either.

That matters for the next person: the chain that supplies the missing feature is
a single thread, and `cargo tree -e features -i zpico-sys` shows it —

```
nros-rmw-zenoh "platform-posix"
  └── nros-board-linux                 (the ONLY enabler)
        └── nros-board-linux "rmw-zenoh"
              └── native_entry_nros_selection   (the ONLY enabler)
```

The generated root takes the board `default-features = false`, so if the
selection crate ever stops naming `rmw-zenoh`, `platform-posix` disappears while
`nros-rmw-zenoh` STAYS in the graph (the root depends on it directly, with its
own defaults, which do not include `platform-posix`). That is precisely the
state the error describes. It is one edit away in a GENERATED file, and nothing
checks it.

## The adjacent finding, which is real regardless

That one shared fixture target dir holds **six distinct `libzpico_sys-*.rlib`**
and ten `zpico-sys` build directories. Five of the ten are script-only (no
`out/`); every unit that actually ran has the alias archive — so locally the
multiplicity is benign. But multiplicity itself is issue 0616's shape: one
`--target-dir` serving several roots gives several units of a shared crate,
and a transitive lookup (`-L dependency=`, no `--extern`) may bind either. 0616
records that the failure is intermittent while the cause is permanent, and that
`cargo tree` cannot see it because it reports one workspace.

## What is NOT established

- **Why CI's resolution differs.** Submodule states match, the pinned commits
  match, the command matches, the profile and RUSTFLAGS match. Something in the
  CI graph enables a different feature set for `zpico-sys`, and no local
  inspection can see it — the resolution has to be printed from inside the job.
- Whether the `any_explicit` / `auto_posix` asymmetry below is the mechanism or
  merely consistent with it.

## The asymmetry worth checking first

`nros-zpico-build/src/runner.rs`:

```rust
let any_explicit = use_posix || use_zephyr || use_bare_metal
                 || use_freertos || use_nuttx || use_threadx;
if !any_explicit && auto_posix { /* configures zenoh-pico FOR POSIX */ }
…
if …PLATFORM_ALIASES… && !use_freertos && any_explicit { /* compiles the alias TU */ }
```

`auto_posix` infers the platform for CONFIGURATION and not for the ALIAS TU. A
build that reaches `zpico-sys` without an explicit `platform-*` feature — which
is legal, and what `auto_posix` exists to support — therefore gets zenoh-pico
compiled for POSIX with its platform symbols left undefined. That is exactly
the reported artifact. It is a genuine inconsistency whether or not it is this
failure's cause, and it fails at LINK time in a consumer rather than at
configure time in the crate that made the decision.

In `workspaces/rust` the feature arrives via `nros-board-linux`'s
`nros-rmw-zenoh = { …, features = ["platform-posix"] }`. In
`bridge-cyclonedds` the generated root takes `nros-rmw-zenoh` with **no
features** while the selection crate takes it `default-features = false,
features = ["ros-humble"]` — and `nros-rmw-zenoh`'s own default is
`["platform-aliases", "link-ip"]`, which does **not** include `platform-posix`.
Unification is additive so the board's copy should still supply it; that it
apparently does not in CI is the thing to measure.

## Direction

1. Print the resolution from inside the failing job — `cargo tree -e features
   -p zpico-sys` and `cargo build -v` — since that is the one fact no local
   run can supply.
2. Then decide between: making the alias TU follow `auto_posix` (treat the
   inferred platform as explicit for both purposes), or making the inference
   REFUSE rather than half-configure, so the failure lands at configure time
   with a reason instead of at link time with 16 symbols.
3. Independently of this bug, `any_explicit` gating configuration and code
   generation differently is worth removing.

## Reproduced, then fixed

The eight routes above all missed because they all had a graph where
`platform-posix` was enabled. Reproducing needs the opposite: **zpico present
WITHOUT that feature**. The bridge entry is exactly that shape — it depends on
`nros-rmw-zenoh` DIRECTLY (so `platform-aliases` is on by default, and zpico is
compiled) while reaching it through a board that selects only another RMW.

Forced locally by making the generated selection crate name one RMW:

```diff
-nros-board-linux = { …, features = ["rmw-cyclonedds", "rmw-zenoh"] }
+nros-board-linux = { …, features = ["rmw-cyclonedds"] }
```

and the build fails with the reported 16 symbols, deterministically:

```
rust-lld: error: undefined symbol: z_sleep_ms
rust-lld: error: undefined symbol: z_malloc
rust-lld: error: undefined symbol: _z_mutex_lock
…
```

## The defect is four lines

```rust
let any_explicit = use_posix || use_zephyr || … ;
if !any_explicit && auto_posix {
    use_posix = true;          // a platform IS now resolved
}
```

`any_explicit` is computed BEFORE the inference and never updated, so on a
hosted target that resolved POSIX by inference it stays **false** while
`use_posix` is **true**. The alias TU was gated on `any_explicit`, so
zenoh-pico got configured FOR POSIX and then the platform forwarders that
configuration requires were not emitted.

The question that gate needs answered is "is a platform RESOLVED?", not "did
the consumer name one?". It now asks `platform_resolved`, computed AFTER the
inference. A build with no platform at all is a different case and still not
the alias TU's business — `backend_count` rejects it.

Verified on the same configuration: the graph that failed with 16 undefined
symbols links, and the normal two-RMW graph still links.

## What made it expensive

The failure is reported at LINK time in a consumer, naming a binary that
happens to link zpico, with nothing pointing at the crate that made the
decision. Everything else followed from that: the wrong workspace read out of
the package name, a museum binary mistaken for a passing build, and eight
routes that could not reproduce because none of them had the graph shape.

The CI diagnostic added alongside this (`cargo tree -e features -i zpico-sys`
on the failure path) would have shown `platform-posix` absent in one line. It
is kept: the next failure of this family should not need the same eight routes.
