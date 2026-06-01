# Proof-carrying nano-ros — getting started

**Status:** Skeleton (Phase 211.1). Filled in incrementally as PoC work
lands. The flow below is the eventual contributor workflow; some steps
gate on tool releases (wcr 0.0.x, Creusot 0.10 install).

## What this is

nano-ros publishes proof-carrying packages: every Tier ≥ 1 crate ships
a Why3 theory describing what the code guarantees, plus a session
recording how the guarantees were discharged. Consumers compose against
the theory; the registry layer (post-PoC, Phase 211.8) hosts the
bundles + signs them.

This guide is for the **contributor side** — annotating crates,
producing bundles, replaying proofs. The user-side reading-only path
(consuming bundles in your own projects) is documented in
`book/src/internals/proofs.md`.

## Install the toolchain

One command after a fresh clone:

```sh
nros setup --tool wcr-stack
```

This installs:

- `wcr` — the proof-carrying-code CLI (forwarded via `~/.nros/bin/wcr`)
- `creusot` + `cargo-creusot` — Rust → Why3 extractor (pinned 0.10.0)
- `why3` — proof-obligation IR + dispatch (pinned 1.7.0)
- `z3`, `alt-ergo`, `cvc5` — SMT discharge backends
- `coq` — interactive fallback for stubborn VCs

Sanity:

```sh
just proofs doctor
# expected: all [OK] rows
```

## Annotate your first crate

Pick a small public function. Add Creusot annotations:

```rust
use creusot_contracts::*;

#[ensures(result@ == x@ + y@)]
pub fn add(x: u32, y: u32) -> u32 {
    x + y
}
```

(The exact gating style — whether `creusot_contracts` is always linked
or feature-gated under `cfg(feature = "proofs")` — is decided by the
U1 spike landing at Phase 211.1 day 3-5. Style guide finalized in
`docs/proofs/u1-spike-cfg-gating.md` after spike execution.)

Add the `[package.metadata.nros.proof]` table to the crate's
`Cargo.toml`:

```toml
[package.metadata.nros.proof]
schema_version = "0.1"
kind           = "code"
tier           = 1
language       = "rust"

[package.metadata.nros.proof.tools]
extractor = "creusot-0.10.0"
verifier  = "why3-1.7.0"
provers   = ["z3-4.13.0", "alt-ergo-2.5.4"]

[package.metadata.nros.proof.composition]
emits   = []   # auto-populated by `wcr extract`
imports = []
axiom_deps   = []
local_axioms = []
```

Validation: the schema lives at
[`docs/proofs/schemas/proof-section-v0.1.json`](schemas/proof-section-v0.1.json).

## Run the dev loop

Fast inner loop — extract only, no prover:

```sh
cargo creusot -p <your-crate>
```

Full discharge:

```sh
just proofs verify-proofs-crate CRATE=<your-crate>
```

The first verify run is cold; subsequent runs hit the per-VC cache
under `target/proofs/cache/`. Target: ≤ 10 s warm-cache for `nros-core`-
sized crates.

Replay-only after pulling someone else's session:

```sh
just proofs verify-proofs-replay
```

## Produce a bundle

Once everything verifies green:

```sh
just proofs proofs-bundle-crate CRATE=<your-crate>
# output: target/proofs/bundles/<your-crate>-<version>.wcr.tar.zst
```

Bundle layout spec:
[`docs/proofs/schemas/bundle-layout-v0.1.md`](schemas/bundle-layout-v0.1.md).

Bundle contents:

- `manifest.toml` — proof section + package identity
- `source/` — embedded source (Tier 0-1 default)
- `source.lock` — Merkle tree hash + URI
- `proofs/extracted/rust/<theory>.mlw` — Creusot output
- `proofs/session/rust/why3session.xml` — discharge record
- `attestations/proof.intoto.jsonl` — in-toto v1 statement (predicate
  schema at
  [`docs/proofs/schemas/attestation-predicate-v0.1.json`](schemas/attestation-predicate-v0.1.json))
- `sbom.cdx.json` — CycloneDX SBOM

## Cross-crate composition

When your crate uses a proven dep's spec:

```toml
[package.metadata.nros.proof.composition]
imports = [
  { theory = "Nros_core_result", from = "nros-core@^0.4" },
]
```

`wcr verify` walks the import graph topologically, fetches dep bundles
into the workspace registry under `target/proofs/registry/`, and
discharges your VCs using the dep's exported `ensures` clauses as
available lemmas.

Cycles are caught by `wcr verify` Phase 0 step (Tarjan SCC) — see C2
resolution in the phase doc.

## External deps without proofs (heapless, num-traits, …)

Three-tier policy at
[`docs/proofs/m3-external-dep-policy.md`](m3-external-dep-policy.md):

- **T1** (dep used by ≥ 3 nano-ros crates) — shadow axiom crate at
  `packages/axioms/<dep>-nros-axioms/`, imported via `axiom_deps`
- **T2** (dep used by 1-2 crates) — local
  `proofs/axioms/<dep>-axioms.mlw`, listed in `local_axioms`
- **T3** (dev-deps, examples, non-safety host-side) — `#[trusted]` at
  the usage site, counted in attestation `trusted_stubs`

`heapless-nros-axioms` is the reference T1 crate, landing at Phase
211.2.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `wcr-extract:` followed by Creusot crash | `cargo creusot` failed — check annotation syntax; Creusot version mismatch |
| `wcr-prove:` discharge timeout | bump prover time limit; consider Coq fallback (M4); strengthen spec |
| `wcr-deps:` cycle detected | inter-crate `imports` form a cycle — break it (C2) |
| `wcr-hash:` mismatch | extracted bytes drifted from manifest's recorded hash; run `wcr extract` to re-record |
| `wcr-manifest:` schema violation | `[package.metadata.nros.proof]` malformed; validate against `schemas/proof-section-v0.1.json` |
| `wcr-cache:` missing for replay-only | run `wcr verify` for a full re-extract instead |

Full exit-code table:
[`docs/roadmap/phase-211-proof-carrying-code.md`](../roadmap/phase-211-proof-carrying-code.md)
(Architecture section).

## Related docs

- [Phase 211 roadmap](../roadmap/phase-211-proof-carrying-code.md) —
  full plan, work items, format-stability commitments
- [U1 spike — Cargo feature gating](u1-spike-cfg-gating.md)
- [M3 external-dep policy](m3-external-dep-policy.md)
- [v0.1 schemas](schemas/) — proof-section, attestation-predicate,
  source.lock, bundle-layout
- [`book/src/internals/proofs.md`](../../book/src/internals/proofs.md) —
  user-facing chapter on the proof-carrying-package model
