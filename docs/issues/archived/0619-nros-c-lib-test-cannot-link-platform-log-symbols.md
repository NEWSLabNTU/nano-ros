---
id: 619
title: "lib TESTS cannot link the platform symbol set: `nros-c` on `nros_platform_log_write`, `nros-cpp` on `nros_platform_clock_ns`"
status: resolved
type: bug
area: build/api
related: [issue-0618, issue-0617, issue-0420]
---

## Symptom

`just ci-matrix` reaches `test-all` and dies before running a single test:

```
error: linking with `cc` failed: exit status: 1
  = note: /usr/bin/ld: libnros_log-….rlib(…nros_log….rcgu.o): in function `nros_log::sinks::emit':
      packages/core/nros-log/src/sinks.rs:72: undefined reference to `nros_platform_log_write'
      packages/core/nros-log/src/sinks.rs:62: undefined reference to `nros_platform_log_flush'
      collect2: error: ld returned 1 exit status
error: could not compile `nros-c` (lib test) due to 1 previous error
```

Reproduces standalone:

```sh
cargo test --no-run -p nros-c --profile nros-relwithdebinfo --locked
```

## Mechanism

`nros-log`'s `PlatformSink` calls the platform ABI's `nros_platform_log_write` /
`_flush` as `extern "C"`. Those symbols are supplied by a platform's C port
(`nros-platform-posix`, `-zephyr`, `-threadx`, …), which a real image links.

A `cargo test` binary for `nros-c` is a final artifact too, but it links no
platform port — so the sink's calls have no definition. The crate's LIBRARY
builds fine; only its test harness fails, which is why this is invisible until
something runs `cargo test` over the workspace.

This is issue 0618's family in a different register: a library assumes the FINAL
ARTIFACT provides something, and the assumption is unchecked. There it was
`#[panic_handler]` / `#[global_allocator]` (lang items); here it is an
`extern "C"` symbol from the platform ABI. Same root — the provider is decided
outside the crate, and nothing verifies each artifact ends up with exactly one.

## Not caused by the feature campaign

Checked, because the timing invited the assumption: reverting
`packages/api/nros-cpp/Cargo.toml` to its pre-`nros-rmw/alloc` state reproduces
the failure identically, so this is not fallout from phase-361's opt-in
direction and is distinct from 0617's three link failures.

## Impact

Blocks `just ci-matrix` (tier 2) and `just test-all` at the compile step, so no
test result is obtainable from those lanes regardless of the state of anything
else.

## Resolution (2026-08-16)

The first candidate below, once corrected for the fact that
`nros-platform-posix` is **not a cargo crate** — it is a directory of C sources
(`src/{platform,net,timer}.c`) compiled by `nros-platform-cffi`'s
`posix-c-port` feature. So the dev-dependency to take is
`nros-platform-cffi = { features = ["posix-c-port"] }`, which is what
`nros-tests` and the test bins already do; `c-stub-test` is the one that cannot
work (see below).

Two things were needed, and the second is the non-obvious one:

1. `nros-cpp` gains that dev-dependency.
2. `nros-cpp/src/lib.rs` gains `#[cfg(test)] use nros_platform_cffi as _;`.
   Without it the fix looks applied and changes nothing: rustc drops a
   dev-dependency that no code references, and the build script's
   `cargo:rustc-link-lib` goes with it. The link line already carried
   `-L .../nros-platform-cffi-*/out` while the archive itself was still absent —
   a `-L` with no `-l`, which reads like the dep is wired when it is not.

The no-op-sink direction was not taken, per this issue's own warning about 0420.

### The `nros-c` half no longer reproduces

> **Superseded 2026-08-16 — this section was wrong, see *Completing the `nros-c`
> half* below.** Kept as written because how it went wrong is the useful part.

`cargo test --no-run -p nros-c --profile nros-relwithdebinfo` — the exact repro
recorded under *Symptom* — now links clean, and still does with the
dev-dependency removed, so it was fixed in passing by the platform work earlier
in this campaign rather than by anything here. No dev-dependency was added to
`nros-c`: one was written, measured to be dead weight, and taken back out.

### Completing the `nros-c` half (2026-08-16)

It did still reproduce. Both spellings fail on a clean tree — the dev profile
the new gate line runs, and the `nros-relwithdebinfo` repro recorded above as
verified clean:

```sh
cargo test --no-run -p nros-c --quiet
cargo test --no-run -p nros-c --profile nros-relwithdebinfo
```

Both on the same two symbols, `nros_platform_log_write` / `_flush`. So the gate
line added by the resolution above landed RED, and because
`check-workspace-features` runs early it took all of tier 1 with it — every
later gate in `just ci` was unreachable, which is how this surfaced.

The fix is `nros-cpp`'s, unchanged: the `posix-c-port` dev-dependency plus
`#[cfg(test)] use nros_platform_cffi as _;` in `nros-c/src/lib.rs`.

**Why the measurement said "dead weight".** Removing a dev-dependency that
nothing references changes nothing — that is this issue's own point (2), one
paragraph up, in the other direction. Step 2 is what makes step 1 observable, so
a dev-dep tested WITHOUT its anchor is indistinguishable from one that is not
needed, and the experiment returns "no effect" whether the fix was required or
not. The anchor here was therefore mutation-tested rather than assumed: with the
`use` removed and the dev-dependency still in place, the same two undefined
references come straight back.

**Record-keeping.** This was archived `resolved` while half of it was red on
`main`. The status stays `resolved` now that it is actually fixed, but the first
resolution should have been checked by running the gate it introduced.

### The gate that should have caught it

`nros-c` was excluded from `check-workspace-features`, the only test-compile
gate — so its lib test was covered by nothing at all, which is how it rotted to
a hard link error unnoticed. The exclude is specifically about
`--no-default-features` (no panic handler, no platform port), so it stays, and
`nros-c` gets its own gate line at DEFAULT features where it does link.
Issue-0196 rule: a gate must cover the class it claims to.

## Directions (as filed, kept for the record)

Not diagnosed further, so these are candidates rather than a plan:

- Give the test harness a platform port — the posix one — as a `[dev-dependency]`
  of `nros-c`, matching what any real consumer links.
- Or make `PlatformSink` weak/optional so a build with no platform port
  compiles to a no-op sink rather than an undefined reference. Note issue 0420
  is exactly the danger there: a silently no-op log facade on threadx/nuttx was
  its own bug, so a no-op must be a deliberate, visible fallback rather than a
  link-order accident.

Whichever way it goes, the general rule from 0618 applies: name what the final
artifact must provide, and check it, instead of letting the linker discover it.

## Second instance: `nros-cpp` / `nros_platform_clock_ns` (2026-08-16)

Same class, different crate and symbol. `check-workspace-features` runs

    cargo test --no-run --workspace --exclude nros-c --no-default-features

and now dies on the crate the `--exclude` does not name:

```
rust-lld: error: undefined symbol: nros_platform_clock_ns
error: could not compile `nros-cpp` (lib test)
```

`CffiPlatform` states the contract in its own doc — "the crate that pulls
`CffiPlatform` into a final binary is responsible for ensuring the symbols are
supplied at link time" — and a lib TEST is a final binary. `nros-cpp` reaches it
transitively through `nros-platform`, and nothing supplies the symbols, so the
harness cannot link. Verified pre-existing by stashing all local work.

### An attempted fix that does NOT work, recorded so it is not retried

`nros-platform-cffi`'s `c-stub-test` feature exists for exactly this: it
compiles `tests/c_stubs/platform_stubs.c`, which defines every `nros_platform_*`
symbol. Taking it as a `[dev-dependencies]` entry of `nros-cpp` looks right and
fails:

```
error: features `c-stub-test` and `posix-c-port` are mutually exclusive
       — both define the canonical `nros_platform_*` symbols
```

Feature unification enables `posix-c-port` from elsewhere in the workspace
graph, so the dev-dependency turns both on and the build script's own guard
fires. Backed out, lock included.

That failure is worth keeping because it points at #0618's framing: the panic
handler, the allocator and now the platform symbol set are IMAGE singletons, and
a dev-dependency is a library-level lever. It cannot express "supply these only
when nobody else does".

### While proving it, `c-stub-test` turned out to be broken independently

Fixed in `7d995209c`: `platform_stubs.c` never included `<nros/platform.h>`
despite claiming to implement it, so it hand-declared every signature and could
not see phase-359 W10's `nros_platform_task_attr_t`; and its integration test
still called `clock_ms`/`clock_us`, retired by phase-352. The feature had not
compiled since W10 landed, unnoticed because no default build enables it. That
does not fix this issue, but any solution routed through the stubs needed it
working first.

### Cost, for prioritisation

This is the ONLY thing keeping tier 1 red on a fully provisioned host as of
2026-08-16 03:5x — the Zephyr lane now completes all 70 leaves (#590), and every
other gate passes.
