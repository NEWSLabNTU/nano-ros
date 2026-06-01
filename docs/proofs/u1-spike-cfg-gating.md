# Spike — Cargo feature gating for Creusot annotations (U1 + U2)

**Status:** Spike plan landed 2026-06-01. Execution waits for `wcr setup
--tool wcr-stack` (Phase 211.1 day 3-5). This document records the
hypotheses, run matrix, decision criteria, and style guidance prepared
in advance.

**Context:** Phase 211 backlog items U1 (does
`#[cfg(feature = "proofs")]` gating work with Creusot annotations?) and
U2 (measurement procedure for the gating decision gate). Closing both
unblocks 211.2 (`nros-core` annotation work).

## Hypotheses

Three candidate strategies for integrating Creusot annotations into
nano-ros crates without forcing every contributor to install the
verification toolchain. Test in order; accept the first that holds.

### H1 — Always-link, no gating (preferred)

`creusot_contracts` lives in every annotated crate's normal
dependencies. The macros expand to no-ops when invoked by stock
`rustc`; only the Creusot driver elaborates them into proof obligations.

```toml
# Cargo.toml
[dependencies]
creusot_contracts = "0.10"
```

```rust
use creusot_contracts::*;

#[ensures(result@ == x@ + y@)]
pub fn add(x: u32, y: u32) -> u32 { x + y }

#[predicate]
#[open]
pub fn positive(x: Int) -> bool {
    pearlite! { x > 0 }
}
```

**Pros:** simplest. Source reads like normal annotated Rust. Spec items
live directly beside impl. Zero `cfg_attr` noise.

**Cons:** every annotated crate carries a `creusot_contracts` dep —
fetches from crates.io, increases dep-graph fan-out. Possible binary
bloat (measure before accepting).

### H2 — Feature-gated annotations (fallback)

`creusot_contracts` is optional under a `proofs` cargo feature.
Annotation attributes wrapped in `cfg_attr`. Pure spec items are
wholly gated.

```toml
[features]
default = []
proofs  = ["dep:creusot_contracts"]

[dependencies]
creusot_contracts = { version = "0.10", optional = true }
```

```rust
#[cfg(feature = "proofs")]
use creusot_contracts::*;

#[cfg_attr(feature = "proofs", ensures(result@ == x@ + y@))]
pub fn add(x: u32, y: u32) -> u32 { x + y }

#[cfg(feature = "proofs")]
#[predicate]
#[open]
fn positive(x: Int) -> bool { pearlite! { x > 0 } }
```

**Pros:** zero overhead when `proofs` off.

**Cons:** every annotation duplicated in `cfg_attr`. Spec items doubled
(impl + cfg-gated spec). Verbose. May warrant a `nros_proof! { … }`
declarative macro wrapper to reduce noise.

### H3 — Separate spec module (last resort)

Spec items isolated in `src/specs.rs`, gated by the `proofs` feature.
Impl is untouched. Annotations never appear on impl items directly.

```rust
// src/lib.rs
pub fn add(x: u32, y: u32) -> u32 { x + y }

#[cfg(feature = "proofs")]
pub mod specs;
```

```rust
// src/specs.rs
use creusot_contracts::*;
use crate::*;

#[ensures(result@ == x@ + y@)]
pub fn add_spec(x: u32, y: u32) -> u32 { add(x, y) }
```

**Pros:** complete isolation. The impl crate doesn't import
`creusot_contracts` at all in the non-proof build path. Cleanest
boundary.

**Cons:** indirection. Specs sit beside impl in a sibling file, not on
the impl itself. Refactor risk (rename impl → forget to rename spec).
Creusot extraction may need extra wiring to associate `add_spec` with
`add`.

## Dev-loop commands (Creusot 0.10+ surface, M7 resolution)

Creusot 0.10 (Feb 2026) uses a Cargo subcommand split, not a single
binary with mode flags. The three relevant invocations:

| Command | What it does | When to use |
|---|---|---|
| `cargo creusot` | compile Rust → Coma + emit Why3 obligations; **does not run any prover** | dev iteration: confirms annotation syntax + extraction succeed, fast |
| `cargo creusot prove` | invoke Why3find against the obligations; run Z3/Alt-Ergo/CVC5 | full verification run |
| `cargo creusot prove --replay` | skip proof search; reuse the existing `proof.json` | cached re-discharge after `proof.json` was committed |
| `cargo creusot -p <pkg>` / `cargo creusot prove -p <pkg>` | scope to one workspace member | targeted iteration |

Dev workflow on a single crate:

```
edit src/lib.rs
cargo creusot              # ~ syntax + extraction check, fast
cargo creusot prove        # full discharge when ready to verify
git add proofs/proof.json
# later, after pulling:
cargo creusot prove --replay   # re-verify quickly using the committed json
```

`wcr-cli` shells out to these subcommands transparently:

- `wcr extract` → `cargo creusot`
- `wcr verify` → `cargo creusot prove --replay`, falling back to
  `cargo creusot prove` if the cache misses
- `wcr replay` → `cargo creusot prove --replay`

No new wcr-side "fast iteration" command needed; the Creusot CLI
already provides the right granularity.

## Run matrix

Executed at 211.1 day 3-5 once `cargo install creusot` succeeds.

| Command | H1 expected | H2 expected | H3 expected |
|---|---|---|---|
| `cargo build` | ✅ green, 0 warnings | ✅ green | ✅ green |
| `cargo build --features proofs` | ✅ green | ✅ green | ✅ green |
| `cargo check --no-default-features` | ✅ 0 proof-related warnings | ✅ | ✅ |
| `cargo clippy --all-features` | ≤ 0 new lints | ≤ 0 | ≤ 0 |
| `cargo doc` | annotations rendered | hidden | impl + spec module both rendered |
| `cargo build --release` size delta | ≤ 1 % | 0 % | 0 % |
| Clean rebuild time delta | ≤ 2 % | 0 % | 0 % |
| `cargo creusot` (extract) | succeeds w/ annotations | succeeds with `--features proofs` | succeeds, extracts spec module |
| `cargo creusot prove` (discharge) | succeeds | succeeds with `--features proofs` | succeeds |

Size + time deltas measured with `cargo bloat` + `cargo build --timings`
against a baseline of the same crate without `creusot_contracts` in any
form.

## Decision criteria

**Accept H1** if all of:

- `cargo build` produces zero warnings.
- Release binary size delta ≤ 1 % (`cargo bloat`).
- Compile-time delta ≤ 2 % (timed clean builds).
- `cargo creusot` picks up annotations from a build without
  `--features proofs` set (i.e. the Creusot driver runs the extract
  step regardless of cargo feature state, treating contract macros
  as live during its own pass).

**Fall back to H2** if:

- H1 produces clippy noise we can't suppress with a workspace-level
  `#![allow(...)]` directive.
- H1 adds more than 1 % binary size for typical embedded targets
  (the bigger concern is `no_std` builds for `nros-core`).
- The Creusot driver itself requires `proofs`-style feature gating to
  selectively run on annotated modules.

**Fall back to H3** if:

- H2 has proc-macro hygiene issues (e.g. `cfg_attr` doesn't pass
  through `creusot_contracts` proc-macros cleanly).
- H2's `#[cfg(feature = "proofs")] #[predicate]` gating breaks Creusot
  extraction (predicate symbols not visible to ensures clauses).

## Edge cases to probe in the spike crate

| Edge | What we test |
|---|---|
| `#[trusted]` on `extern "C"` block | Creusot accepts axiomatic-only contract on FFI |
| `pearlite!` referencing `Int` ↔ `u32` | The `@` operator and `Int` coercion work in both modes |
| Trait impl with `#[ensures]` on method | Trait dispatch + Creusot — known fragile area |
| `Iterator` consumer with `#[invariant]` | Iterator combinators are a known Creusot pain point |
| `no_std` crate with `creusot_contracts` | `creusot_contracts` must work `no_std` for `nros-core`. **Critical for H1.** If it doesn't, H1 dies for embedded targets |

The `no_std` case is the make-or-break for H1. The spike crate ships
two top-level variants — one `std`, one `no_std` — and the matrix runs
against both.

## Spike crate layout

```
wcr/examples/spike-cfg-gating/
├── Cargo.toml                       (workspace-level)
├── h1-always-link/
│   ├── Cargo.toml
│   ├── src/lib.rs
│   └── README.md
├── h2-feature-gated/
│   ├── Cargo.toml
│   ├── src/lib.rs
│   └── README.md
├── h3-spec-module/
│   ├── Cargo.toml
│   ├── src/lib.rs
│   ├── src/specs.rs
│   └── README.md
└── README.md   (run matrix outcomes, winning hypothesis, style guide)
```

Each `lib.rs` exercises the same five test functions (`add`, `clamp`,
`fsm_step`, trait-impl method, FFI `extern "C"`). Matrix run against
each.

## Style guide (drafted in advance, finalized after spike)

### If H1 wins (likeliest)

```rust
//! Always import creusot_contracts. Annotations are part of the source.
use creusot_contracts::*;

#[requires(@e.v_max > 0)]
#[ensures(in_envelope(result@, e))]
pub fn clamp(v: i32, e: Envelope) -> i32 { ... }

#[predicate]
#[open]
pub fn in_envelope(v: Int, e: Envelope) -> bool {
    pearlite! { -@e.v_max <= v && v <= @e.v_max }
}
```

### If H2 wins

```rust
#[cfg(feature = "proofs")]
use creusot_contracts::*;

#[cfg_attr(feature = "proofs", requires(@e.v_max > 0))]
#[cfg_attr(feature = "proofs", ensures(in_envelope(result@, e)))]
pub fn clamp(v: i32, e: Envelope) -> i32 { ... }

#[cfg(feature = "proofs")]
#[predicate]
#[open]
pub fn in_envelope(v: Int, e: Envelope) -> bool {
    pearlite! { -@e.v_max <= v && v <= @e.v_max }
}
```

Likely paired with a `nros_proof! { ... }` declarative macro defined
in `nros-core` to shorten the `cfg_attr` chain.

### If H3 wins

```rust
// src/lib.rs
pub fn clamp(v: i32, e: Envelope) -> i32 { ... }

#[cfg(feature = "proofs")]
pub mod specs;
```

```rust
// src/specs.rs
use creusot_contracts::*;
use crate::*;

#[requires(@e.v_max > 0)]
#[ensures(in_envelope(result@, e))]
pub fn clamp_spec(v: i32, e: Envelope) -> i32 { clamp(v, e) }

#[predicate]
#[open]
pub fn in_envelope(v: Int, e: Envelope) -> bool {
    pearlite! { -@e.v_max <= v && v <= @e.v_max }
}
```

## Resolves both U1 and U2

U2 (the measurement procedure for the gating decision gate) is the run
matrix itself: `cargo check --no-default-features` producing zero
proof-related warnings IS the lower-bound acceptance for whichever
hypothesis wins.

The matrix's other rows (size delta, time delta, clippy, doc, wcr
extract) are the upper-bound acceptances for picking H1 over H2/H3.

## Prior

Creusot 0.10's contract macros are documented as inert outside the
driver. Verus and Prusti hold this property. We estimate:

- H1 holds: ~ 80 % probability
- H2 needed: ~ 18 %
- H3 needed: ~ 2 %

The `no_std` edge case is the highest-impact unknown. If
`creusot_contracts` requires `std`, the probability mass shifts hard
toward H2 or H3.

## Net ergonomics for 211.2 (nros-core)

- **H1** → minimal source noise, annotated Rust looks like normal Rust
  with extra attributes. Contributors see the spec inline.
- **H2** → ~ 3-5 `cfg_attr` lines per annotated function (or 1
  `nros_proof! { ... }` macro per item).
- **H3** → impl and spec live in sibling files. Refactor friction;
  IDEs may not jump-to-definition cleanly across the boundary.

## Outcome (filled in after execution)

| Field | Value |
|---|---|
| Hypothesis selected | _TBD — fill in after 211.1 spike runs_ |
| `cargo build` warnings count | _TBD_ |
| Release size delta | _TBD_ |
| Clean rebuild time delta | _TBD_ |
| `creusot_contracts` `no_std` compatible? | _TBD_ |
| Spike crate commit (in wcr repo) | _TBD_ |
| Style guide finalized in section | _TBD_ |

## See also

- `docs/roadmap/phase-211-proof-carrying-code.md` — phase doc + backlog
- `docs/research/phase-211-pcc-baseline.md` — baseline measurements (to
  be created in 211.1)
