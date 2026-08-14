---
id: 581
title: "`just book` has been broken on main, and each failure hid the next: a retired feature, eight dead doc links, a stale Doxyfile path"
status: resolved
type: bug
area: docs
related: [rfc-0054, rfc-0073, phase-321, phase-352]
---

## Symptom

`just book` fails on a pristine `main`:

```
error: none of the selected packages contains this feature: rmw-zenoh
selected packages: nros, nros-platform-cffi, nros-platform-api, nros-rmw, nros-rmw-cffi, nros-rmw-zenoh
help: packages with the missing feature: nros-c, nros-board-linux
error: recipe `book` failed with exit code 101
```

Reproduced on a stashed, pristine tree, so it is not a local-state artifact.

The consequence is bigger than "the API docs do not build": `cargo doc` is the
FIRST step of the recipe, so `mdbook build` never ran either. **A book-only
change could not be previewed at all**, which is how a book that several pages
of this repo point at came to be unbuildable without anyone noticing.

## Four failures, each hidden behind the previous

The recipe stops at the first error, so every fix revealed the next. That is the
real cost of a broken pipeline: nothing downstream of the first failure has been
evaluated for as long as it has been broken.

### 1. `--features rmw-zenoh` — a feature that no longer exists

RFC-0054 moved the RMW backends behind the CFFI seam. `nros` now carries
`rmw-cffi`, `rmw-cyclonedds`, `rmw-lending` — and no `rmw-zenoh`. The recipe's
own comment was stale with it, naming a `cfg(any(rmw-zenoh, rmw-xrce, rmw-dds,
rmw-cffi))` gate; the gate in `nros/src/lib.rs` is `#[cfg(feature = "rmw-cffi")]`
today, and `rmw-xrce` / `rmw-dds` are gone.

### 2. Eight unresolved intra-doc links

Only visible once cargo got far enough to run rustdoc. Three distinct causes:

| cause | n | fix |
| --- | --- | --- |
| **RFC-0073 fallout** — `PlatformClock::clock_ms` was retired; the trait has `clock_ns` / `clock_resolution_ns` | 1 | rename the link |
| **unqualified links** — `Node`, `SchedClass::*` are not in scope in `node_runtime.rs` (nothing imports them there), though `nros` re-exports `SchedClass` under `rmw-cffi` | 5 | qualify with `crate::` |
| **feature-gated targets** — `CallbackCtx::integrity` and `IntegrityStatus` are `#[cfg(feature = "safety-e2e")]`; the links are correct, the doc build just did not enable it | 2 | add `safety-e2e` to the doc features |

The third fix is deliberately not "delete the link": enabling `safety-e2e`
documents the safety API in the deployed rustdoc, which is what the recipe
exists for.

### 3. A public doc linking to a private item

Enabling `safety-e2e` exposed one more: `install_node_typed`'s docs linked
`[`register_node_borrowed`]`, a private fn. Named in prose, not linked — an
implementation detail should not be a public doc target.

### 4. `doc-rmw-cffi` pointed at a directory with no Doxyfile

`(cd packages/rmw/cffi && doxygen Doxyfile)` → `configuration file Doxyfile not
found!`. phase-321 W2.e moved the RMW **shim** crates into `packages/rmw/`; the
**ABI** crate and its Doxyfile stayed in `packages/core/nros-rmw-abi` — where
`PROJECT_NAME = "nros rmw-cffi"` and `OUTPUT_DIRECTORY = target/doxygen/rmw-cffi`
say plainly that it is this recipe's file. The recipe followed the crates; the
Doxyfile did not move with them.

All four doxygen recipes were checked, not just the failing one:
`doc-c`, `doc-cpp`, `doc-platform-cffi` resolve correctly.

## Verified

`just book` exits 0, and the outputs land rather than merely the exit code
being green:

```
book/book/api/rust           (incl. nros/struct.Executor.html)
book/book/api/c              167 html
book/book/api/cpp            189 html
book/book/api/rmw-cffi        35 html
book/book/api/platform-cffi
Built: book/book/index.html
```

`struct.Executor.html` is the check that matters for #1: the recipe's stated
purpose is that the reference stub's `[Executor](struct.Executor.html)` link
does not 404, and that is now true rather than merely unasserted.

## What would have caught this

Nothing runs `just book` in CI on this repo's own lanes — it is a `[group("docs")]`
recipe a contributor invokes by hand. A doc build that no gate runs will rot at
exactly the rate the code under it changes, and this one accumulated four
independent breakages from three different refactors (RFC-0054, RFC-0073,
phase-321). Wiring it into a lane is the durable fix and is not done here.
