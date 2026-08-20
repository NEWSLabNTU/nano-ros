---
id: 727
title: "`PlatformSink`'s extern pair breaks the workspace no-default-features test-compile — the sink is link-time platform code riding a library edge"
status: resolved
type: bug
severity: medium
area: core, testing
related: [issue-0708, issue-0710, issue-0589]
---

## Symptom

`just ci-matrix` (tier 2), `check-workspace-features`' lane
`cargo test --no-run --workspace --exclude nros-c --no-default-features`:

```
rust-lld: error: undefined symbol: nros_platform_log_write
>>> referenced by sinks.rs:72 … <nros_log::sinks::PlatformSink as nros_log::LogSink>::log
error: could not compile `nros-rmw-cffi` (test "two_backends")
```

First seen on the first tier-2 run this host could execute (post-#698); the
edge that exposes it is #708/#710's new `nros-rmw-cffi -> nros-log`
dependency ("the diagnostic sink").

## Analysis (measured)

* `nros_platform_log_write`/`_flush` are LINK-TIME requirements satisfied
  only by a `nros-platform-<rtos>` port — the exact rule nros-log's own
  `platform-clock` feature documents for `nros_platform_clock_us`. The
  sink's externs never got that treatment.
* A `platform-sink` default feature was added (this session) gating the
  extern block + `PlatformSink` + `sinks::default()`’s content — but it
  cannot help THIS lane: `--no-default-features` strips only the selected
  packages' own defaults, and every consumer deps nros-log with default
  features on, so unification re-enables the sink
  (`cargo tree -p nros-rmw-cffi --no-default-features` shows
  `nros-log feature "default"`).
* `cargo test --no-run -p nros-rmw-cffi` links in BOTH feature states —
  the linker GCs the unreferenced vtable (measured: 0 symbol refs in the
  built test bin). Only the `--workspace` build keeps it (unification from
  another member changes codegen), which is why tier 1 never saw it.

## The principled fix (phase-366's doctrine, one layer down)

The sink is the IMAGE's choice, like the panic ending: library crates that
dep nros-log for types + macros should carry
`default-features = false, features = ["max-level-trace","buffer-size-256"]`
edges, and `platform-sink` should be enabled by the crates that CALL
`init_default()` — the boards and entries, which are exactly the crates
that link a platform port. `init_default()` gates on the feature. Then the
workspace no-default lane has no sink to mislink, and a library consumer
stops paying a link-time platform requirement it never uses.

That flip touches every nros-log consumer manifest and the #708/#710 work
is still in flight in a parallel session — hence interim below rather than
racing it.

## What landed instead of an interim

The exclusion route was tried and immediately demonstrated its own
inadequacy: the next `--workspace` compile failed identically in
`nros-tests`' zephyr test binary — every nros-log-linking member can trip
this depending on GC luck, so per-crate excludes are whack-a-mole.

Landed fix: **weak host stubs**, the repo's existing discipline for exactly
this (issue 0050's audited weak-symbol allowlist).
`packages/core/nros-log/c/host_log_stub.c` defines the pair
`__attribute__((weak))` as no-ops; `build.rs` compiles it ONLY when
`TARGET == HOST`, so a cross build never sees a fallback and an embedded
image missing its port still fails loud at link. Any port's strong
definition wins on host too (native links nros-platform-posix via the
board). Allowlisted as override-default with the `[img:]` token. The
workspace no-default-features test-compile lane links clean with NO new
exclusions.

The `platform-sink` default feature (also added) stays: it documents the
link-time contract beside its `platform-clock` precedent and lets a
library-only consumer opt out explicitly.

The edge flip above remains the right eventual shape (the sink as the
image's choice); this issue tracks it. Severity drops accordingly.

## Resolved 2026-08-20 — structurally, and the weak stubs are REMOVED

This issue landed a fix (`ede77608e`): weak host-only stubs for
`nros_platform_log_write`/`_flush`, compiled by a new `nros-log/build.rs` when
`TARGET == HOST`, with an entry in `scripts/weak-symbols-allowlist.txt`. It
worked, and its diagnosis was sharper than the parallel session's — the hazard
is not merely that the sink is reachable but that **whether the unreferenced
vtable is GC'd before the link is codegen luck**, and the lane lost that bet.

It has been superseded rather than extended, on a project rule the fix could not
know about: **weak symbols are avoided here.** The stub, the `build.rs`, the
allowlist entry, the `platform-sink` feature (#0723's subject) and the
`check-board-log-sink.py` gate that enforced it are all gone.

What replaces them removes the requirement instead of satisfying it:

* **`PlatformSink` moved to `nros_platform_cffi::log`** — the crate that owns
  the ABI binding. "Does this binary need `nros_platform_log_write`?" is now a
  DEPENDENCY question, which is a property of the binary. A feature is a
  property of the BUILD, which is exactly why #0723 found the gate could not
  survive `cargo --workspace` unification, and why a weak stub was needed to
  cover what the gate could not. With the symbol referenced only from a crate a
  portless binary does not link, neither is required.
* **`nros_log::early`** holds records raised before `init` and replays them when
  the board installs its sinks — so removing the dispatch auto-install (#0710's
  mechanism) costs nothing. It is a better answer than the auto-install was: the
  early records land in the sink the board PICKED, not one dispatch guessed.

The extern is also declared exactly once now, in `nros-platform-cffi`'s bindgen
output from `<nros/platform.h>` — the SSoT RFC-0054 names — instead of a second
hand-written copy in the facade.

**What is lost by removing the stubs, stated plainly:** a host binary that links
a port but somehow fails to define the pair now fails at link rather than
silently dropping. That is the #0708 failure mode staying caught, which the
stub's own comment says it wanted to preserve for cross builds; the move
preserves it for host builds too.
