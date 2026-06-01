# Proof-Carrying Packages

**Status:** Skeleton — Phase 211 PoC. Filled in incrementally as the
PoC delivers concrete artifacts. This chapter covers the consumer-side
view: how a nano-ros user reads + replays + composes against the proofs
that ship with verified packages.

For the contributor-side workflow (annotating crates, producing
bundles, debugging dischargers) see
[`docs/proofs/getting-started.md`](../../../docs/proofs/getting-started.md).

## What a proof-carrying package is

Every nano-ros crate at Tier ≥ 1 ships:

- **Why3 theory** declaring what the crate's public functions guarantee
  (pre/post conditions, invariants, ghost state references)
- **Why3 session** recording how each verification condition was
  discharged (which prover, what timeout, what transformations)
- **In-toto attestation** binding the theory + session to a specific
  build of the crate

These ride alongside the normal Cargo source distribution; consumers
get them automatically when fetching the `.wcr.tar.zst` bundle.

## What "verified" means here

A Tier-1 nano-ros crate's verified surface = its **public API**. For
each public function:

- Inputs satisfying the declared preconditions
- → produce outputs satisfying the declared postconditions
- → under the trusted-axiom set declared in the bundle

The bundle's attestation names every trust commitment: extractor
version, verifier version, prover versions, axiom families used, and
any `#[trusted]` stubs (functions whose bodies were not verified — see
the trusted-stub inventory in `wcr report`).

If you trust those names + their versions, you trust the verification.

## How to consume

When a Tier ≥ 1 nano-ros crate is one of your dependencies, the
proof-carrying side is transparent: `cargo build` doesn't change. To
inspect or replay the proofs:

```sh
nros setup --tool wcr-stack          # one-time toolchain install
wcr fetch nros-cmd-gate@0.4.1        # pull bundle
wcr replay nros-cmd-gate@0.4.1       # re-discharge sessions against local provers
```

A successful replay confirms — using prover binaries on your machine —
that the upstream-recorded discharge holds. Exit 0 means proven; non-
zero means investigate (see exit-code table in the [phase
doc](../../../docs/roadmap/phase-211-proof-carrying-code.md)).

## Composition

If your own crate is also proof-carrying, you can compose against a
dep's spec:

```toml
[package.metadata.nros.proof.composition]
imports = [
  { theory = "Nros_cmd_gate_clamp", from = "nros-cmd-gate@^0.4" },
]
```

`wcr verify` on your crate then:

1. Fetches `nros-cmd-gate`'s bundle.
2. Replays its session (confirms the dep's claims).
3. Imports its emitted Why3 theory.
4. Discharges your own VCs using the dep's `ensures` clauses as
   available lemmas.

No re-extraction of the dep's source. The dep's spec is the
contract — your proof rides on top.

## Trust dial

| Trust mode | Command | What you verify |
|---|---|---|
| Spec-only | (no replay) | dep author + spec text |
| Session replay | `wcr replay` | dep's session re-discharges on your machine |
| Re-extraction | `wcr replay --re-extract` | spec is what Creusot produces from the embedded source |
| Audit-grade | `wcr replay --re-extract --with-file-list` | every embedded source file's hash matches the source.lock listing |

PoC default is session-replay. Tier-2+ packages may require re-
extraction; Tier-3 cert kernels may require audit-grade.

## Topology

```
your crate ──────────────[imports]─────────────────► nros-cmd-gate
   │                                                     │
   │  wcr verify                                         │
   │     1. fetch dep bundle                             │
   │     2. replay dep session ─────────────────────► confirms upstream
   │     3. load dep theory into Why3 -L path            │
   │     4. extract your crate (Creusot)                 │
   │     5. discharge your VCs using dep's ensures       │
   ▼
your bundle (your theory + your session + dep references)
```

The composition is **at the Why3 theory layer**, not at the Rust
source layer. Different languages can interoperate: in 211.8 a C crate
verified by Frama-C/WP plugs into the same composition mechanism.

## What you should and shouldn't worry about

**Don't worry about:**

- Creusot version drift — the bundle records the exact extractor version
  it was built against; cache invalidates explicitly when provers bump
- Spec drift between code + extracted theory — the spec IS the extracted
  theory, regenerated each verify run
- Cross-crate spec evolution — major-bump policy enforced at the
  registry layer (211.8+); minor bumps must refine, not weaken

**Do worry about:**

- The axiom set you transitively trust — every consumed `axiom_deps`
  entry is a trust assertion; review the attestor for high-stakes
  packages
- `trusted_stubs` count — high counts signal verification debt
- TCB members listed in attestation — prover + extractor + Why3 + rustc
  versions; bumps may invalidate cached results

`wcr report` summarises both axes for your workspace.

## Glossary

- **Spec hash** — SHA-256 over the canonical-normalized extracted Why3
  theory, version-tagged with the extractor (e.g.
  `creusot-0.10.0/sha256:...`)
- **Bundle** — `.wcr.tar.zst` archive packaging one `(crate, version)`
  with its proofs + attestations + source (per tier)
- **Theory** — Why3 module emitted by the extractor, importable by
  consumers via `use <Theory_name>`
- **Axiom crate** — package with `kind = "axiom"` carrying hand-authored
  Why3 axioms covering an external dep with no upstream Creusot specs
- **Replay** — re-running the session XML against pinned prover
  binaries; succeeds iff every recorded `valid` claim still holds
- **TCB** — Trusted Computing Base; the set of binaries whose
  correctness you must accept

## Where to go next

- Contributor flow: [getting started](../../../docs/proofs/getting-started.md)
- Format specs: [v0.1 schemas](../../../docs/proofs/schemas/)
- Policy on external deps:
  [M3 — external-dep policy](../../../docs/proofs/m3-external-dep-policy.md)
- Phase roadmap:
  [Phase 211](../../../docs/roadmap/phase-211-proof-carrying-code.md)
- Companion bounded-model-checking layer (Kani): see
  [Formal Verification](verification.md)
