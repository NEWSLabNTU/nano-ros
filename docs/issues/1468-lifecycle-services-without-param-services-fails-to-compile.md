---
id: 1468
title: "`lifecycle_services` reaches into `parameter_services` unconditionally, so
  `lifecycle-services` without `param-services` does not compile — and it is on
  `main`, where it stops the rust-core fixture build"
status: open
type: bug
area: core, build, ci
severity: high
found: 2026-09-23
related: [1353, phase-461]
---

## What happens

`host-tests` push run **35880789702** (head `e0ef2d5a2`), job **107248387646**,
step **`Build rust core fixtures`** — NOT the `just ci tier1` step this lane has
been failing on, and NOT a disk failure (the job reports `50% used, 74G free`
before the build and `45% used, 81G free` after its reclaim):

```
error[E0432]: unresolved import `crate::parameter_services`
   --> packages/core/nros-node/src/lifecycle_services.rs:569:16
note: found an item that was configured out
   --> packages/core/nros-node/src/lib.rs:154:9
    | #[cfg(all(feature = "param-services", any(has_rmw, test)))]
    | pub mod parameter_services;

error[E0433]: cannot find `parameter_services` in `crate`
   --> packages/core/nros-node/src/lifecycle_services.rs:582:47

error: could not compile `nros-node` (lib) due to 2 previous errors
make: *** [.../linux-rust-all-19822-12425.mk:16: fixture-0002] Error 101
```

Reproduced on `main` at `e0ef2d5a2`, outside CI, in one command:

```
cargo check -p nros-node --no-default-features --features lifecycle-services,rmw-cffi,std
  error[E0432]: unresolved import `crate::parameter_services`
  error[E0433]: cannot find `parameter_services` in `crate`
  error[E0747]: unresolved item provided when a constant was expected
```

(The third error is a cascade off the second: `LIFECYCLE_INBOX_SLOT_BYTES` is
one of the re-exports that failed to resolve, and it is a const generic
argument.)

## Why the two modules are independent

`nros-node` declares them as two features with no implication between them:

```
param-services     = ["dep:nros-rcl-interfaces"]
lifecycle-services = ["dep:nros-lifecycle-msgs"]
```

and gates each module on its own (`lib.rs:153` and `lib.rs:185`), both
additionally behind `any(has_rmw, test)`. `lifecycle-services` + `rmw-cffi`
without `param-services` is therefore a legal, reachable configuration — and it
is the one the `examples/native/rust/lifecycle-node` fixture builds.

phase-461 W2 (`e0ef2d5a2`) made `lifecycle_services.rs` re-export
`PARAM_INBOX_DEPTH` / `PARAM_INBOX_SLOT_BYTES` from `parameter_services` and
read `MAX_PARAM_SERVICE_SETS` from it, deliberately — the commit's own comment
argues the two families share a geometry and that a second knob "would be a
second number saying less". The reasoning is about the numbers; what it did not
carry is that the module holding them is behind a different feature.

## Why it matters

It is on `main`. Any build selecting `lifecycle-services` without
`param-services` fails, which includes the rust-core fixture set — so the
`host-tests` lane now fails at `Build rust core fixtures`, two steps EARLIER
than the 1353 disk failure it has been failing on all day. A reader who knows
this lane as "the ENOSPC one" will mis-attribute it.

## What this is NOT

- **Not issue 1353.** That is the tier running out of disk. This run had 74 G
  free at the start and 81 G after the reclaim, and never reached the tier.
- **Not a feature that does not exist.** `lifecycle-services` is declared,
  documented and exercised by a shipped example fixture.
- **Not caught by the PR gate.** `just check workspace-all` and the per-feature
  lanes in `just/check/lanes.just` exercise `param-services` and `std` and
  `rmw-cffi` combinations, but no lane selects `lifecycle-services` WITHOUT
  `param-services`, so phase-461 W2 went green on its PR.

## What would close it

The numbers are genuinely shared, so there is more than one defensible fix and
this issue deliberately does not pick one:

1. **Make the dependency real**: `lifecycle-services = ["param-services", ...]`.
   Honest about the coupling the code now has, and costs every
   lifecycle-only image the parameter service's code and its
   `nros-rcl-interfaces` dependency.
2. **Move the shared geometry below both**: put `PARAM_INBOX_DEPTH`,
   `PARAM_INBOX_SLOT_BYTES` and `MAX_PARAM_SERVICE_SETS` somewhere neither
   feature gates, and have both families read them there. Keeps one number,
   keeps the features independent, and is the larger change.
3. **Give `lifecycle_services` its own constants** — rejected by phase-461 W2's
   own reasoning, recorded here so the next reader does not re-propose it
   without seeing that argument.

Whichever lands, the gate gap is part of the fix: a lane that builds
`lifecycle-services` without `param-services`, so this combination cannot go
green on a PR again.
