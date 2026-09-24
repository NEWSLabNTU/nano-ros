---
id: 1468
title: "`lifecycle_services` reaches into `parameter_services` unconditionally, so
  `lifecycle-services` without `param-services` does not compile — and it is on
  `main`, where it stops the rust-core fixture build"
status: resolved
type: bug
area: core, build, ci
severity: high
found: 2026-09-23
related: [1177, 1226, 1353, phase-461, phase-466]
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

## Resolved 2026-09-24 — option 2, and the gate gap has a PR-reachable answer

Fixed by the split this issue's option 2 describes, plus a lane row. Both are
measured below; nothing here is read off a manifest.

### What landed

`parameter_services.rs` held two things that had no business sharing a gate:
the parameter SERVICES, which need the `rcl_interfaces` message crate
`param-services` brings, and the parameter family's message GEOMETRY, which is
arithmetic over `config`'s declared shapes and `nros_params`' resolved
capacities and needs neither. The geometry is `crate::param_sizing` now, gated
on the DISJUNCTION of the two module gates:

```rust
#[cfg(all(
    any(feature = "param-services", feature = "lifecycle-services"),
    any(has_rmw, test)
))]
pub mod param_sizing;
```

`any(has_rmw, test)` is carried over from both module gates unchanged, so no
image compiles this that did not compile it before. `parameter_services`
re-exports it whole (`pub use crate::param_sizing::*;`), so every existing path
still resolves. `MAX_PARAM_SERVICE_SETS` became `MAX_SERVICE_SETS`: it was
never the parameter family's — it is the node table's bound restated for sets,
and the lifecycle family's ring count is its five queryables over exactly those
sets, which is why the lifecycle side was reading it under a name it had no
business naming.

The numbers keep meaning what phase-461 W2 said they mean, verbatim: the
lifecycle payloads are strictly smaller than a `set_parameters` over every
declared parameter, so the parameter bound is an upper bound and a second knob
would be a second number saying less; the depth is the same 1 because
`ros2 lifecycle set` sends one request and waits.

### Why not options 1 or 3

**Option 1 (`lifecycle-services = ["param-services", ...]`)** is honest about
the coupling the code had, and it makes every lifecycle-only image carry
`nros-rcl-interfaces`, six parameter service handlers, and — the part that is
not merely bytes — `PARAM_SERVICE_QUERYABLES`, six more slots against an
8-slot embedded `ZPICO_MAX_QUERYABLES` (issues 0460 and 1378, where
`[param_services]`'s six plus `[lifecycle]`'s five are already the eleven that
overrun the default). Paying that for three compile-time integers is the wrong
trade in a tree that counts both.

**Option 3 (gate the re-export on `param-services`)** is cheapest and makes
`LIFECYCLE_INBOX_*` silently absent in a configuration where the lifecycle
family is otherwise fully present — the quiet absence RFC-0089 Part I exists to
refuse. Making that absence loud means a `compile_error!`, which is option 1
with extra steps.

**A fourth option, priced and rejected:** derive the slot size in `build.rs` so
`config` could carry it ungated. It cannot be done, and the reason is already
written where the derivation lives: the bound depends on the store's
capacities, which `nros-params`' build script resolves into `nros_params::MAX_*`.
A `const fn` over those consts sees every rung with no second reader of any
knob; a build script cannot read another crate's consts at all.

### The class, swept

Every `crate::<module>` reference made from a module whose own `cfg` gate does
not imply the referenced module's gate, over `packages/`:

```
python3 -c 'import re,pathlib
G=re.compile(r"((?:^[ \t]*#\[cfg\([^\n]*\)\]\n)*)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;",re.M)
for lib in sorted(pathlib.Path("packages").rglob("src/lib.rs")):
    t=lib.read_text(errors="replace")
    g={m[1]:frozenset(re.findall(r"feature = \"([^\"]+)\"",m[0])) for m in G.findall(t)}
    for f in sorted(lib.parent.rglob("*.rs")):
        rel=f.relative_to(lib.parent).as_posix()[:-3]
        own=next((m for m in g if rel==m or rel.startswith(m+"/")),None)
        if own is None: continue
        b=f.read_text(errors="replace")
        for tgt,tf in g.items():
            if tgt!=own and tf and not tf<=g[own] and re.search(r"crate::"+tgt+r"\b",b):
                print(f,own,sorted(g[own]),"->",tgt,sorted(tf))'
```

22 (file, target) pairs, 96 reference sites. **One was a real break — this
one.** It is also the only pair where BOTH sides are feature-gated; the other
21 reference a gated module from an UNGATED one, where the site is itself
inside a matching `#[cfg]` block. That last clause is a claim about the
compiler, so it was compiled rather than read:

| pair | built | result |
| --- | --- | --- |
| `nros` `guide/configuration.rs`, `init.rs` -> `crate::env` | `-p nros --no-default-features --features std,rmw-cffi` | clean |
| `nros-cpp` `params_shim.rs` -> `crate::metadata_hooks` | `-p nros-cpp --no-default-features --features panic-platform` | clean |
| `nros-board-common` `freertos_config.rs` -> `crate::freertos_build` | `-p nros-board-common --no-default-features` | clean |
| `nros-board-mps2-an385` `entry.rs` -> `crate::rtic`, `rtic.rs` -> `crate::entry`, `node.rs` -> `crate::network` | `ethernet`, `board-entry,ethernet`, `rtic,ethernet` on `thumbv7m-none-eabi` | clean |
| the same board's `serial` rows | — | not reachable in an agent worktree: `zpico-sys` panics `zenoh-pico source not provisioned` (#0390). A precondition, not a span. |
| `nros-node` `executor/*` -> `crate::{mock,time_source,parameter_services,lifecycle_services}` | each gating feature alone (below) | clean |

### The gate gap, and its reach

`check-workspace-all` runs on every pull request and did not catch this,
because `cargo check --workspace` UNIFIES features: `nros-node` is built there
with `param-services` AND `lifecycle-services` on, always, so a module reaching
across the gate between them is invisible by construction.

`check::workspace-features` is where per-feature rows live, and **a row added
there would not have caught it either** — its only caller is `just check build`,
which no merge-gating event runs (`.github/workflows/gate.yml`: `check build` is
`schedule`/`workflow_dispatch` only). That is issue 1177's still-open finding
and issue 1226's shape: a gate that WORKS is not a gate that RUNS. So **the
finding here is the reach, not the check** — and the row went where a pull
request sees it.

`scripts/check-feature-gated-modules.sh`, called from `check::compile-smoke`
(a required PR context, which already carries a feature-shape row for the same
reason — issue 1260's `param-services` row for nros-c/nros-cpp). For every
`#[cfg(… feature = "X" …)]` immediately above a `mod` declaration in
`nros-node/src/lib.rs`, it compiles the crate with X and nothing else over
`std,alloc,rmw-cffi`. The list is READ from `lib.rs`, not written down, because
a new gated module is the case it exists for; it derives four today
(`lifecycle-services`, `param-services`, `rmw-cffi`, `sim-time`) and costs
~12 s warm. It carries its own selftest on the normal path
(`check-gate-selftests`): a synthetic `lib.rs` with a known answer, so a
derivation that stops matching fails loudly instead of checking nothing.

This does not settle 1177 — which crate, which tier, and whether the whole
per-feature row set moves — and that issue stays open with the question intact.

### Negative control

The gate, run against `origin/main`'s `packages/core/nros-node/src/` (restored
with `git checkout origin/main -- …` over a WIP commit, never `git stash`):

```
  - nros-node: lifecycle-services alone (over std,alloc)
error[E0432]: unresolved import `crate::parameter_services`
   --> packages/core/nros-node/src/lifecycle_services.rs:569:16
note: found an item that was configured out
   --> packages/core/nros-node/src/lib.rs:154:9
    | #[cfg(all(feature = "param-services", any(has_rmw, test)))]
    | pub mod parameter_services;
error[E0433]: cannot find `parameter_services` in `crate`
   --> packages/core/nros-node/src/lifecycle_services.rs:582:47
error[E0747]: unresolved item provided when a constant was expected
   --> packages/core/nros-node/src/lifecycle_services.rs:590:5
error: could not compile `nros-node` (lib) due to 3 previous errors
check-feature-gated-modules: nros-node does not compile with only `lifecycle-services`
  A module gated on one feature is reaching into a module gated on another.
  The shared item belongs in a module below BOTH gates (issue 1468).
```

exit 1, and the other three rows pass on that same pre-fix tree — so the row
that fails is the one that names the defect, not the whole lane going dark.
And the selftest's own control: breaking the awk's attribute pattern in a copy
of the script produces

```
check-feature-gated-modules SELFTEST FAILED: the derivation does not read lib.rs
  expected: alpha beta delta gamma
  got:
```

### Measured, on the fix

Every combination checked with `cargo check -p nros-node`:

| shape | result |
| --- | --- |
| neither service feature (`std,alloc,rmw-cffi`) | PASS |
| `param-services` only | PASS |
| `lifecycle-services` only — **the reported break** | PASS |
| both | PASS |
| each of the three above with no RMW (`has_rmw` off) | PASS |
| `--no-default-features`, nothing at all | PASS |
| both + `sim-time` (the `cfg` that reads `sim_time_seeded`) | PASS |
| `param-services` / `lifecycle-services` / both on `thumbv7m-none-eabi` | PASS |
| `--all-targets` with both, and with lifecycle only | PASS |

`lifecycle-services` with neither `std` nor `alloc` still fails, unchanged and
deliberately: `error: lifecycle-services allocates: add "alloc" to this crate's
features`, the crate's own `compile_error!`. Clippy `-D warnings` is clean on
`nros-node --all-targets` with both features and on the lifecycle-only shape.
`just check fast` leaves four reds, all of them this agent worktree's
environment rather than this change: `capability-conditionals`,
`xrce-vendored-versions`, `xrce-source-manifest` (a sandbox `PermissionError`
on `/nonexistent`) and `codegen-version-refusal` case E. A fifth,
`gate-selftests`, was red on the first run and was this change's own doing —
the new gate owed a selftest — and is green now.
