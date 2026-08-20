---
id: 727
title: "`PlatformSink`'s extern pair breaks the workspace no-default-features test-compile — the sink is link-time platform code riding a library edge"
status: open
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
