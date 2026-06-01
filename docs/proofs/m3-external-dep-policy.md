# External-dep-without-proofs policy (M3)

**Status:** Landed 2026-06-01. Implements PoC backlog item M3 from Phase
211.

**Context:** Most external Rust crates (`heapless`, `num-traits`,
`serde`, `bitflags`, …) ship no Creusot annotations. When a nano-ros
crate's verified spec calls into one of them, `wcr verify` needs a Why3
theory describing the dep's behavior. This policy defines who writes it
+ where it lives + how it's trusted.

## Three-tier policy

Disposition is keyed to how broadly a dep is used across nano-ros
crates:

| Tier | Rule | Mechanism |
|---|---|---|
| **T1** — dep used by ≥ 3 nano-ros crates | Shadow axiom crate at `packages/axioms/<dep>-nros-axioms/` (NEWSLabNTU-attested) | Imported via `axiom_deps` in consumer's `[package.metadata.nros.proof.composition]` |
| **T2** — dep used by 1-2 nano-ros crates | Local axioms in the consumer crate's `proofs/axioms/<dep>-axioms.mlw` | Listed under a new manifest field `local_axioms` |
| **T3** — dev-dep, example-only, or host-side non-safety-critical | `#[trusted]` at the usage site | No spec; usage counted in trusted-stub inventory |

**Long-term migration path:** when an upstream maintainer accepts
Creusot annotations, land the spec upstream + retire the shadow crate.
T1 packages have an explicit retirement timeline; tracker doc covers
status.

## Tier classification for PoC-era crates

Survey of external deps reachable from `nros-core` + `nros-cmd-gate`
(the two crates verified in 211.2 + 211.3):

| Dep | Tier | Disposition | Lands at |
|---|---|---|---|
| `heapless` | T1 (nros-core, nros-cmd-gate, future RMW crates) | Shadow `heapless-nros-axioms` | 211.2 |
| `num-traits` | T2 (nros-cmd-gate scalar math, if used) | Local axioms | 211.3 |
| `serde` | T3 (config parse path; non-safety) | `#[trusted]` | 211.2 |
| `serde_json` / `toml` | T3 | `#[trusted]` | 211.2 |
| `defmt` | T3 (formatter; side-effecting, ignored by spec) | macros likely ignored by Creusot | n/a |
| `bitflags`-derived | T2 (per-use) | Local axioms | per-crate |
| `cortex-m` | T1 (every embedded crate) | Shadow `cortex-m-nros-axioms` (HW axioms too) | 211.7+ (post-PoC) |
| `linkme` | T3 (FFI registration; pre-main lifecycle) | `#[trusted]` on register fns | 211.2 |
| `embedded-hal` | T1 (board crates) | Shadow `embedded-hal-nros-axioms` | post-PoC |

`creusot_contracts` already ships specs for `core::*` and much of
`alloc::*`; the stdlib does not need a shadow crate. Coverage gaps in
stdlib are handled per-callsite (T2 local axiom or T3 trusted).

## Shadow axiom crate layout (T1)

```
packages/axioms/heapless-nros-axioms/
├── Cargo.toml                             ← kind = "axiom"
├── ATTESTOR.md                            ← attestor identity + signing
├── proofs/spec/heapless_axioms.mlw        ← the axiom theory (shipped to consumers)
├── audit/
│   ├── heapless-0.8.0.audit.md            ← per-upstream-version review
│   └── heapless-0.9.0.audit.md
└── README.md
```

### `Cargo.toml`

```toml
[package]
name    = "heapless-nros-axioms"
version = "1.0.0"
edition = "2024"
description = "Axiomatic Why3 spec for the heapless crate (no_std fixed-capacity collections)"

[package.metadata.nros.proof]
schema_version  = "0.1"
kind            = "axiom"
tier            = 1
covered_versions = ["heapless = \"^0.8.0\""]

[package.metadata.nros.proof.attestation]
attestor        = "did:web:newslabntu.csie.ntu.edu.tw#axiom-key-2026"
review_doc      = "audit/heapless-0.8.0.audit.md"
last_reviewed   = "2026-06-01"
```

### `proofs/spec/heapless_axioms.mlw` (excerpt)

```why3
module Heapless_vec_axioms
  use int.Int
  use option.Option

  type vec 'a       (* abstract — opaque to consumers *)

  function len      (v: vec 'a) : int
  function capacity (v: vec 'a) : int

  axiom len_bounded:
    forall v: vec 'a. 0 <= len v <= capacity v

  axiom capacity_positive:
    forall v: vec 'a. capacity v >= 0

  val push (v: vec 'a) (x: 'a) : option 'a
    writes  { v }
    ensures { len (old v) < capacity (old v) ->
              result = None /\ len v = len (old v) + 1 }
    ensures { len (old v) = capacity (old v) ->
              exists e. result = Some e /\ len v = len (old v) }

  val pop (v: vec 'a) : option 'a
    writes  { v }
    ensures { len (old v) > 0 ->
              exists x. result = Some x /\ len v = len (old v) - 1 }
    ensures { len (old v) = 0 ->
              result = None /\ len v = 0 }
end

module Heapless_string_axioms ... end
module Heapless_queue_axioms  ... end
```

Authored from heapless's public API documentation + source review. No
extraction — these are hand-written axioms.

### Consumer side

```toml
# nros-core/Cargo.toml
[package.metadata.nros.proof.composition]
axiom_deps = [
  { family = "heapless-nros-axioms", version = "^1.0" },
]
```

`wcr verify` for `nros-core` pulls `heapless-nros-axioms`'s Why3 theory
into the workspace registry alongside the consumer's own VCs. Consumer
proofs cite `Heapless_vec_axioms.push`'s `ensures` clauses as available
lemmas.

## Local axiom layout (T2)

Example for `num-traits` used only in `nros-cmd-gate`:

```
packages/safety/nros-cmd-gate/
├── Cargo.toml
├── src/
└── proofs/
    └── axioms/
        └── num_traits_axioms.mlw    ← local-only; not a separate package
```

Manifest:

```toml
# nros-cmd-gate/Cargo.toml
[package.metadata.nros.proof.composition]
axiom_deps     = []                                                # T1 shadow crates
local_axioms   = ["proofs/axioms/num_traits_axioms.mlw"]           # T2 local files
```

`local_axioms` is a new schema field (extends v0.1; backward-compatible
— additive). Each local axiom theory ships in the consumer's bundle
under `proofs/axioms/`. Same hash + cache mechanism as shadow crates.

## Trusted-stub policy (T3)

Usage site marker:

```rust
use creusot_contracts::*;

#[trusted]
fn parse_config(input: &str) -> Result<Config, ConfigError> {
    serde_json::from_str(input).map_err(ConfigError::from)
}
```

Trusted stubs accounted for in the attestation:

```json
{
  "trusted_stubs": [
    {
      "function": "nros_cmd_gate::parse_config",
      "rationale": "serde_json deserialize — host-side config parse, not safety-critical",
      "issue":     "https://github.com/NEWSLabNTU/nano-ros/issues/...",
      "tier":      3
    }
  ]
}
```

Trusted-stub count is surfaced in `wcr report`. Inventory tracked over
time so trust-debt is visible.

## Versioning

Shadow axiom crate semver per upstream-dep-version coverage:

| Bump | Trigger |
|---|---|
| patch | Refine existing axiom without changing behavior (e.g. stronger spec discovered) |
| minor | Add axioms for new upstream version (extending `covered_versions`) |
| major | Upstream breaking API change; old axioms no longer apply to current versions |

Consumer pins via standard Cargo semver. Spec hash on the axiom theory
file invalidates downstream caches the same way as a code crate's spec
change.

## Audit process for shadow axiom crates

For each new upstream version covered:

1. Read the dep's public-API documentation + source for the version.
2. Write or update axioms covering each public function used by ≥ 1
   nano-ros crate.
3. Land an `audit/<dep>-<ver>.audit.md` listing:
   - Functions reviewed + axiom name
   - Source commit hash audited
   - Coverage gaps (functions deliberately un-spec'd)
   - Reviewer + date + signing identity
4. Update `covered_versions` in the shadow crate's `Cargo.toml`.
5. Bump shadow crate version per the semver table above.
6. CI re-verifies every downstream consumer.

When upstream eventually accepts our axioms (some maintainers will,
some won't) → land the same `.mlw` theory upstream → retire our shadow
crate at the next major bump.

## Edge cases

| Case | Handling |
|---|---|
| Transitive dep (e.g. `heapless` uses `hash32` internally) | Shadow crate covers the dep's **public API only**. Internal transitive deps invisible to consumers stay un-axiomatized |
| Stdlib coverage gap (`core::ptr::write_volatile`, etc.) | T2 local axiom or T3 trusted stub per use frequency |
| Macro-generated code (`bitflags!`, `defmt::println!`) | If macro expansion is idiomatic Rust → Creusot handles. If not → wrap call in `#[trusted]` fn at boundary |
| Build-script-generated code (e.g. `prost` proto) | Treat as a synthetic dep; T3 trusted unless we author specs for the generated code |
| Generic dep functions where `T` is constrained by trait we don't axiomatize (e.g. `Vec<T: Clone>`) | Axiomatize the trait separately; compose |

## Cross-cutting: attestation transparency

Every consumer bundle's attestation lists every axiom dep with:

- Family name + version
- Spec hash
- Attestor identity (DID, sigstore identity, or org+date)
- Audit date

`wcr report` aggregates the workspace view, e.g.:

> "This workspace trusts heapless-nros-axioms@1.0.0 attested by
> NEWSLabNTU on 2026-06-01 covering heapless@0.8.x."

Auditable. Every external trust commitment is named.

## Upstream contribution tracker (post-PoC)

`docs/proofs/upstream-axiom-contributions.md` (created when first
shadow crate is mature enough to upstream — likely 211.7+):

- List of shadow crates we maintain
- Outreach status with each upstream maintainer
- PRs filed / merged
- Retirement timeline for each shadow crate

For PoC scope: no upstream outreach. We maintain shadow crates fully.

## PoC scope

Concrete deliverables in PoC window:

| Phase | Deliverable |
|---|---|
| 211.2 | `packages/axioms/heapless-nros-axioms/` created with v1.0.0 + audit doc for heapless@0.8.0 + minimum-viable Vec / String / Queue axioms |
| 211.2 | `nros-core` declares `axiom_deps = [{ family = "heapless-nros-axioms", version = "^1.0" }]` and discharges using the imported axioms |
| 211.3 | `nros-cmd-gate` exercises a T2 local axiom for any non-shadow-crate dep used (likely none required for the envelope clamp) |
| 211.4 | Bundle round-trip carries axiom dep bundle alongside consumer bundle; replay re-discharges with both |
| 211.5 | Trusted-stub inventory documented in PoC closeout report |

Post-PoC (211.7+): `cortex-m-nros-axioms` + `embedded-hal-nros-axioms`
+ RTOS API shadow crates land as RMW and platform crates get verified.

## See also

- `docs/roadmap/phase-211-proof-carrying-code.md` — phase doc (this
  policy implements backlog item M3)
- `docs/proofs/u1-spike-cfg-gating.md` — Cargo feature gating spike
  (independent of M3 but affects how axiom_deps interact with feature
  flags)
- C3 resolution in the phase doc — establishes axiom crates as a
  first-class package class with `kind = "axiom"`
