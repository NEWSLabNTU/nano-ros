---
id: 584
title: "`nros/ffi-size-markers` was enabled ONLY by a default feature both consumers disable — a `-p nros-c` build had no size markers"
status: resolved
type: bug
area: build
related: [issue-0464, issue-0582, phase-359, rfc-0054]
resolved_in: "phase-359 W3"
---

## What was true

`nros`'s `ffi-size-markers` feature puts `#[used]` on the `__NROS_SIZE_*`
opaque-storage statics. Its manifest states the purpose:

> the markers are `#[used]` so `--gc-sections` cannot drop the underlying
> `[u8; N]` arrays from the final binary

`nros-c` and `nros-cpp` derive their C/C++ opaque-storage macros from those
statics via `nros-sizes-build`, whose nested probe runs
`--no-default-features --features=<resolved>`, where `<resolved>` is the
CONSUMER's active features intersected with `nros`'s declared ones.

The feature was enabled in exactly one place — `nros`'s default set:

```toml
default = ["std", "ffi-size-markers"]
```

Both consumers disable it at the dep-site:

```toml
nros = { version = "0.5.0", path = "../nros", default-features = false }
```

So `ffi-size-markers` could never appear in `nros-c`'s `CARGO_FEATURE_*`, and
therefore never in the probe's forwarded feature list. It arrived only by
feature unification from some OTHER workspace member that pulled `nros` with
defaults on. Measured:

```
cargo tree -p nros-c    -e normal --format "{p} {f}"   ->  nros v0.5.0  alloc,std
cargo tree --workspace  -e normal --format "{p} {f}"   ->  nros v0.5.0  alloc,default,ffi-size-markers,metadata-mode,rmw-cffi,std
```

`nros-c`'s own manifest comment says the crate is
`host-only-reason = "cdylib/staticlib C ABI surface, built per-platform by
cmake not by the workspace lane"` — i.e. the shipped C/C++ path is exactly the
per-crate build that did **not** get the markers. The whole-workspace build that
did get them is the one nobody ships.

This is upstream of issue 0464. That issue documents three stacked fallbacks
under the size probe and a stale literal at the bottom; this is one reason the
probe has to fall back at all.

## What is true now

phase-359 W3 emptied `nros`'s `default`, which removed the accidental safety net
entirely — so the feature is now requested where it is needed, at all four
dep-sites (`[dependencies]` and `[build-dependencies]` of each consumer):

```toml
nros = { version = "0.5.0", path = "../nros", default-features = false, features = ["ffi-size-markers"] }
```

```
cargo tree -p nros-c               ->  nros v0.5.0  ffi-size-markers
cargo tree -p nros-c --features std ->  nros v0.5.0  alloc,ffi-size-markers,std
```

## What is NOT verified

That the missing feature produced a wrong size in a shipped artifact. `#[used]`
acts at the link/`--gc-sections` stage; an rlib carries the `#[unsafe(no_mangle)]
__NROS_SIZE_*` statics either way, and this host cannot build the C/C++ lanes
(every vendored `-sys` submodule is uninitialised). The resolution above is
justified by the resolver evidence and the manifest's own stated contract, not
by a measured wrong macro value. If issue 0464's fallback telemetry is ever
instrumented, this is a hypothesis worth testing against it.

## Gate

The class is "a load-bearing feature enabled only by a `default` that every
real dep-site disables". phase-359 W4's `check-feature-contract` should assert
it directly: for each crate, no feature listed in `default` may be UNREACHABLE
from every non-`default-features` dep-site in the workspace.
