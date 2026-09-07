---
id: 1210
title: "ARCHITECTURE §2's normative `std`/`alloc` contract teaches the
  require-vs-grant rule from `metadata-mode`, which stopped granting `std`"
status: open
type: docs
area: [docs, core, api]
related: [0669, 0687, 0598, phase-359, phase-361]
---

## What

`docs/design/ARCHITECTURE.md:52-99` is the normative `std`/`alloc` contract —
"Every crate feature table points here rather than restating it." Its central
distinction is require-vs-grant, and it is taught with exactly one worked
example, at `docs/design/ARCHITECTURE.md:76-83`:

> **A purely INTERNAL requirement is the other way round, and the distinction is
> the whole rule.** `metadata-mode` is `["std"]` because `metadata_mode.rs`
> itself uses `std::sync::Mutex`, `Box::leak` and `format!` — nothing about that
> is the consumer's choice […]

Followed by the corollary at `:96-99`:

> A capability that grants what it needs can never reach its own guard —
> `metadata-mode` carried a `compile_error!` for two phases that could not fire,
> because the manifest edge satisfied the condition it tested.

**`metadata-mode` no longer enables `std`.** `packages/api/nros/Cargo.toml:62`:

```toml
metadata-mode = ["nros-rmw/sync-spin"]
```

and `packages/api/nros/src/metadata_mode.rs:46` records the change:

> issue 0687 follow-up — the PORTABLE mutex, not `std::sync::Mutex`.

`scripts/check-std-census.py` carries the same note in its baseline comments:

> issue 0669 sibling — `metadata-mode` stopped being a `std` capability. Its only
> `std` was `std::sync::Mutex` guarding a process-global recorder […] On
> `nros_rmw::sync::Mutex` the capability is heap + a lock, so it requires
> `alloc`.

## Why it matters

Measured today, **there are zero instances of the grants-it kind in the tree.**
Every remaining `std`-requiring capability is the require-not-grant kind, each
with a live `compile_error!`:

| site | capability | shape |
| --- | --- | --- |
| `packages/api/nros/src/lib.rs:1647` | `env` | `cfg(all(feature = "env", not(feature = "std")))` → `compile_error!` |
| `packages/api/nros-cpp/src/lib.rs:4014` | `metadata-mode` | same shape |
| `packages/api/nros-cpp/src/lib.rs:4018` | `env` | same shape |
| `packages/api/nros-c/src/lib.rs:141` | `global-allocator` | same shape |

`python3 scripts/check-std-census.py --check-guards` is green: "every
capability-gated `std::` site names a `compile_error!`".

So the clause has fully converged, and the document still presents it as a live
two-sided split whose "other way round" side is illustrated by a feature that
switched sides. A reader arriving at the rule for the first time — which is the
document's stated audience, since every crate feature table defers to it — is
taught the distinction from a false instance, and is invited to check
`nros/Cargo.toml` and find the opposite of what the text says. Worse, the
corollary at `:96-99` is a *warning about a hazard* anchored to the same feature,
so it now reads as describing current code.

This is the drift rule in `docs/design/README.md` failing in the direction it is
usually right about: the RFC-side statement was updated (phase-359/361 landed the
change) and the ARCHITECTURE synthesis was not.

## Fix sketch (not applied)

Keep the rule — it is correct and load-bearing. Change what illustrates it:

1. Re-tense the `metadata-mode` paragraph to past ("`metadata-mode` **was**
   `["std"]` because …; issue 0669 moved it to `alloc` + a portable mutex, and
   the fix was to make it *require* rather than *grant*"). The history is the
   best teaching material available, and it explains the corollary honestly.
2. State the measured position explicitly: **no capability in the tree grants
   `std` today**, and `check-std-census --check-guards` is what holds it there.
   That converts a rule the reader must trust into one the reader can re-run.
3. Note the one live edge that is *shaped* like granting and is not the same
   thing: `packages/api/nros-c/Cargo.toml:37`,
   `std = ["alloc", "env", "nros/std", …]` — here `std` grants the `env`
   *capability*, which is the reverse direction and not forbidden by any clause,
   but is the nearest thing in the tree to the retired pattern and will be read
   as a counter-example by the next person doing this sweep.
