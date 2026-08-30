---
id: 643
title: "`nros-node`'s `env = [\"std\"]` fails the feature contract its own phase
  wrote — a capability grants the heap"
status: resolved
type: bug
area: core, build
related: [phase-359, phase-360, issue-0642]
---

## Symptom

`just ci-matrix` fails at `check-feature-contract`:

```
[FAIL] clause (a/manifest) — `std` lists `alloc`; nothing else enables either
  packages/core/nros-node/Cargo.toml: feature `env` enables ['std'].
      A capability/backend/platform feature REQUIRES the heap, it does not
      grant it — emit `compile_error!` naming the feature the user must add.
      A `dep/std` forward counts: it reaches the same image by another route.
```

Everything before it passes: `check-no-tracked-models`, `check-cbindgen-pin`,
`check-cbindgen-headers`, `check-nuttx-shared-tree-headers`,
`check-nuttx-libc-struct-sizes`, `check-source-manifest` (all nine sub-checks),
`check-version-lockstep`, and clauses (a/source), (b), (c), (d), (e) of the
contract itself — including "exactly one `#[global_allocator]`".

## Cause

`packages/core/nros-node/Cargo.toml:38`:

```toml
env = ["std"]
```

from `1badb6f72` — *"feat(phase-359 W10): `env` is a capability, not the `std`
flavour — Linux core moves to alloc"*. The commit's own framing is what the gate
objects to: if `env` is a capability, then by the contract it must REQUIRE the
heap rather than grant it.

The tension is real, not a typo. `env` genuinely needs `std::env`, so it cannot
be satisfied on an `alloc`-only build — but the contract's whole point is that a
capability feature must not silently pull `std` into an image that never asked
for it. Both halves are defensible; they are not both implementable as written.

## Why this is not fixed here

This is a design decision inside an ACTIVE phase (359 W10), and the three
candidate resolutions are not equivalent:

1. `env` keeps needing `std` but stops ENABLING it — a `compile_error!` naming
   the feature the user must add, which is exactly the remedy the gate's message
   proposes;
2. `env` stops needing `std` — read the environment through a platform
   capability instead of `std::env`, which is the direction "`env` is a
   capability" was already heading;
3. the contract gains an exemption for capabilities that are inherently hosted,
   which weakens a gate that has been catching real defects.

Picking among those belongs to whoever is running phase-359, not to a passing
sweep. Recorded rather than patched.

## Found by

A tier-2 sweep, which also surfaced two other reds on main from the same window
of in-flight work — both mechanical omissions, both fixed:

* `declarative-safety-listener` calling the `std`-gated `runtime.spin()` after
  `d48e78ea0` swept its manifest off `std`, taking out the whole `native`
  fixture family;
* the weak-symbol allowlist not updated after `51905cd53` added
  `nros_platform_panic` to five ports (issue 0050's gate).

Plus #0642, the lang-item gate failing on stale gitignored probe residue.

That is four separate blockers between a green tree and a completed tier-2 run,
which is the argument for running the sweep more often rather than less.


## 2026-08-16 — resolved upstream, by the phase that wrote it

Fixed in `03ca659c8`, *"fix(phase-359 W10): `env` REQUIRES the standard library,
it does not grant it"* — `env = ["std"]` became `env = []`. That is resolution 1
of the three above, chosen by whoever is running phase-359, which is where the
choice belonged.

Verified here rather than assumed: `just check feature-contract` now reports

```
ok  (a/manifest) `std` lists `alloc`; nothing else enables either
ok  (a/source) the heap gate has one spelling
ok  (b) no `no_std` crate defaults to `std`/`alloc`
ok  (c) every declared `std`/`alloc` feature is used
ok  (d) no `default` feature is unreachable
ok  (e) exactly one `#[global_allocator]`
check-feature-contract: OK (216 crate(s), 6 clauses)
```

The fix landed while this issue was being pushed, so the two crossed. Filing it
still paid: the sweep had to stop somewhere, and "here is the blocker, here are
the three resolutions, the choice is the phase owner's" is the handoff that let
it be fixed by the person holding the context rather than guessed at by the one
holding the build.
