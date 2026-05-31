# Phase 211 — Proof-Carrying Code in nano-ros (Creusot + wcr)

**Goal:** Stand up proof-carrying-code infrastructure for Rust packages in
nano-ros, backed by a standalone Why3 Component Registry (`wcr`) repo that
nano-ros consumes as a tool. Each verified package ships per-function pre/post
annotations (Creusot); the registry artifact is the **Creusot-emitted Why3
theory itself** (auto-generated, never hand-authored). Consumers compose by
importing the dep's emitted theory. Retire Verus in favour of Creusot. Deliver
a Sentinel-style safety-island demonstrator with an end-to-end GSN safety
case.

**Status:** Proposed (2026-05-31).

**Priority:** High (defines the verification posture nano-ros pitches to
Eclipse SDV, Autoware safety-island integrators, and downstream OEMs).

**Depends on:** Phase 191 (SDK provisioning), Phase 208 (book audit closure),
Phase 209 (C++ port friction) for parity on the user-facing onboarding story.

**Sibling work:** [`github.com/NEWSLabNTU/wcr`](https://github.com/NEWSLabNTU/wcr)
(to be created — standalone proof-carrying registry tool + library). Phase 211
work in *this* repo is the nano-ros integration + adoption side; `wcr` is the
tool/format side and develops independently after creation.

## Overview

nano-ros today ships 102 Verus proofs + 160 Kani bounded harnesses. That
provides spot verification but not composition: a downstream consumer cannot
chain a dep's guarantee `X` into their derived property `Y` without
re-verifying from scratch. The verification narrative is also fragmented
across Verus-internal logic, Kani's CBMC backend, and ad-hoc annotations.

Phase 211 unifies the verification story around Why3 as the proof-obligation
intermediate representation, with three deliberate scope decisions:

1. **Rust-only PoC.** Cross-language (C via Frama-C/WP) work is deferred to
   post-PoC. Bundle format stays cross-language-ready by construction —
   adding C is adding files, not redesigning the format.
2. **No manual spec authoring.** Creusot's emitted Why3 module IS the
   published proof artifact for a package. Authors never hand-edit a
   `proofs/spec/*.mlw` file. Spec drift is impossible by construction.
3. **`wcr` is a sibling repo from day one.** Not built into `nros-cli` then
   extracted — created standalone, consumed by nano-ros via the existing
   SDK index + shim pattern (mirroring `zenohd` and the `nros` CLI itself).

The work culminates in:

- 2-3 nano-ros crates annotated, verified, bundled, replayable.
- `wcr` 0.1.0 published with frozen schema for proof manifest, attestation
  predicate, and `.wcr.tar.zst` bundle layout.
- Verus retired from the workspace; Kani's bounded harnesses retained as
  orthogonal coverage.
- A Sentinel-style safety MCU demonstrator running verified safety-gate
  nodes with a GSN safety case citing each Why3 session as evidence.

## Architecture

### Tool layering

```
                  nano-ros                              wcr (sibling repo)
   ┌──────────────────────────────────────┐    ┌─────────────────────────┐
   │ packages/<crate>/src/**.rs           │    │ wcr CLI (Rust binary)   │
   │   #[ensures], #[requires]            │    │   ├── extract (creusot) │
   │   #[predicate], pearlite!{...}       │    │   ├── verify            │
   │                                      │    │   ├── replay            │
   │ packages/<crate>/nros-package.toml   │    │   ├── bundle            │
   │   [proof] section                    │◄───┤   ├── fetch             │
   │                                      │    │   └── attest            │
   │ packages/<crate>/proofs/             │    │                         │
   │   axioms/<family>.mlw  (rare)        │    │ wcr-core, wcr-bundle,   │
   │   ghosts/<ghost>.mlw   (rare)        │    │ wcr-cache, wcr-attest   │
   │                                      │    │ (libraries on crates.io)│
   │ just/proofs.just                     │    └─────────────────────────┘
   │   verify-proofs → wcr verify         │
   └──────────────────────────────────────┘
```

`wcr` ships prebuilt from `github.com/NEWSLabNTU/wcr` releases. nano-ros adds
a `[tool.wcr-stack]` entry to `nros-sdk-index.toml` so `nros setup --tool
wcr-stack` lands `wcr` + Creusot + Why3 + provers under
`~/.nros/sdk/wcr-stack/<ver>/`. A `~/.nros/bin/wcr` shim forwards to the
latest installed version (same pattern as the `zenohd` shim from Phase
208.B Track A).

### Verification IR + tooling

```
Rust source ──Creusot──> Why3 obligations ──> Z3 / Alt-Ergo / CVC5 / Coq
```

Single discharge pipeline. The Creusot-emitted Why3 module IS the proof
artifact — author never hand-writes a separate spec file. Consumer imports
the dep's emitted theory; Why3 `use` brings in `val` signatures and
`ensures`/`requires` clauses for composition.

Cross-language extension (post-PoC) plugs in by adding a Frama-C/WP path:

```
                                              [post-PoC]
C source ─Frama-C/WP─> Why3 obligations ──> Z3 / Alt-Ergo / CVC5 / Coq
```

Bundle format and consumer composition mechanism are unchanged when the C
path lands.

### Per-package on-disk layout

Author-maintained (in git):

```
packages/<crate>/
├── Cargo.toml
├── src/
│   └── lib.rs                          (annotated with Creusot)
├── proofs/                             ← author-maintained, only when needed
│   ├── axioms/<family>.mlw             (HW/RTOS axioms — rare for core crates)
│   └── ghosts/<ghost>.mlw              (ghost theories — rare)
└── nros-package.toml                   ([proof] section)
```

Notably absent: `proofs/spec/`. No hand-written canonical spec file. The
spec lives in Creusot's emitted output (regenerated each verify run).

Generated (in `target/proofs/`, gitignored):

```
target/proofs/<crate>/
├── extracted/<theory>.mlw              ← Creusot output — this IS the spec
├── session/why3session.xml             ← discharge record
├── session/why3shapes.gz
├── spec-hash.txt                       ← SHA-256 of normalized extracted .mlw
└── attestation.intoto.jsonl
```

Bundled (release artifact):

```
target/proofs/bundles/<crate>-<ver>.wcr.tar.zst
└── extracted + session + axioms + ghosts + manifest + attestation + SBOM
```

### Workspace layer

```
target/proofs/
├── registry/                           ← workspace theory path (Why3 -L)
│   ├── theories/<crate>/<theory>.mlw   (symlinks aggregating per-crate)
│   └── deps/<external-crate>/<ver>/... (fetched dep bundles)
├── cache/                              ← content-addressed
│   └── <vc-hash>/<prover>-<ver>/result.json
├── attestations/<crate>.intoto.jsonl
└── bundles/<crate>-<ver>.wcr.tar.zst
```

### `nros-package.toml [proof]` schema (v0.1, frozen at PoC)

```toml
[proof]
schema_version = "0.1"
tier           = 1                       # 0=provenance, 1=verified, 2=safety-island, 3=cert-kernel
language       = "rust"

[proof.tools]
extractor = "creusot-0.5.0"
verifier  = "why3-1.7.0"
provers   = ["z3-4.13.0", "alt-ergo-2.5.4", "cvc5-1.1.2"]

[proof.composition]
emits   = ["Nros_core_result", "Nros_core_message"]    # auto-discovered theories
imports = [
  { theory = "Nros_core_result", from = "nros-core@^0.4" },
]
axiom_deps = []                                         # author-maintained

[proof.spec_hashes]
# Auto-populated by `wcr extract` from normalized extracted .mlw
"Nros_core_result"  = "sha256:..."
"Nros_core_message" = "sha256:..."

[proof.attestation]
provenance_file = "attestations/slsa.json"
sigstore_bundle = "attestations/sigstore.bundle"        # post-PoC populated
```

### Bundle layout (`.wcr.tar.zst`, v0.1, frozen)

```
<crate>-<ver>.wcr.tar.zst
├── manifest.toml                                       (copy of [proof] section)
├── proofs/
│   ├── axioms/<family>.mlw
│   ├── ghosts/<ghost>.mlw
│   ├── extracted/rust/<theory>.mlw                     (Creusot output)
│   └── session/rust/why3session.xml + .why3shapes.gz
├── attestations/
│   ├── proof.intoto.jsonl
│   ├── slsa.intoto.jsonl
│   └── sigstore.bundle                                 (post-PoC)
└── sbom.cdx.json                                       (CycloneDX)
```

Forward-compatibility: scaffold `extracted/<lang>/` + `session/<lang>/`
hierarchy from day one even though only `rust/` is populated in PoC.
Adding C later = adding files under `extracted/c/`, no schema migration.

### Tool versions (pinned via `nros-sdk-index.toml [tool.wcr-stack]`)

| Tool | Pinned version |
|---|---|
| `wcr` | 0.1.0 |
| Creusot | 0.5.0 |
| Why3 | 1.7.0 |
| Z3 | 4.13.x |
| Alt-Ergo | 2.5.x |
| CVC5 | 1.1.x |
| Coq | 8.19 (fallback for stubborn VCs) |

Installed via `nros setup --tool wcr-stack`. Shim at `~/.nros/bin/wcr`.

## Work Items

### 211.1 — Phase 0: Tool provisioning + `wcr` scaffold (weeks 1-2)

**Files (in new `github.com/NEWSLabNTU/wcr` repo)**

- `Cargo.toml` (workspace) + crate scaffold for `wcr-core`, `wcr-cli`,
  `wcr-extract`, `wcr-bundle`, `wcr-cache`, `wcr-attest`
- `schemas/proof-section-v0.1.json` (JSON Schema)
- `schemas/attestation-predicate-v0.1.json`
- `schemas/bundle-layout-v0.1.md`
- `docs/architecture.md`, `docs/format-stability.md`
- `examples/toy-clamp/` (minimal reference package)
- `crates/wcr-core/src/spec_hash.rs` (canonical Why3 normalize + SHA-256)
- `crates/wcr-core/src/manifest.rs` (parse `nros-package.toml [proof]`)
- `crates/wcr-extract/src/creusot.rs` (Creusot wrapper)
- `crates/wcr-cli/src/main.rs` with `extract`, `report` subcommands

**Files (in nano-ros)**

- `nros-sdk-index.toml` (add `[tool.wcr]` + `[tool.wcr-stack]` entries +
  `[source.creusot]`)
- `scripts/install-nros.sh` (extend shim block for `wcr`)
- `just/workspace.just` (extend `cargo-tools` probe for `wcr`)
- `docs/research/phase-211-pcc-baseline.md` (new — iteration time,
  discharge time, annotation/code ratio measurements)

**Acceptance**

- `wcr` 0.0.x scaffold released from sibling repo
- `nros setup --tool wcr-stack` succeeds on a clean Linux host
- `which wcr && which creusot && which why3` resolve via `~/.nros/bin/`
- One representative Verus proof from the existing 102 ported to Creusot
  end-to-end; cold discharge ≤ 5 s, warm cache ≤ 1 s
- Toy reference package in `wcr` repo: `wcr extract` + `why3 prove`
  end-to-end on a 2-function `examples/toy-clamp/`
- Baseline measurements committed to the research doc

### 211.2 — Phase 1: Rust pilot crate `nros-core` (weeks 3-5)

**Files (in nano-ros)**

- `packages/core/nros-core/proofs/axioms/alloc_axioms.mlw` (gated on `std`)
- `packages/core/nros-core/nros-package.toml` (new — `[proof]` section)
- `packages/core/nros-core/src/lib.rs` (annotate public API with Creusot)
- `just/proofs.just` (new — `verify-proofs CRATE=<name>` recipe wrapping
  `wcr verify`)
- `docs/proofs/getting-started.md` (new)
- `.gitignore` updates for `target/proofs/`

**Files (in `wcr`)**

- `wcr-cli` 0.1.0 release with `verify` subcommand
- `wcr-core::session` (Why3 session XML reader/writer)
- `wcr-attest::intoto` (in-toto v1 statement builder)

**Acceptance**

- 30-50 discharged VCs across `Result`, error enum, message-trait laws
- `cargo build` unchanged for non-proof users (annotations behind
  `cfg(feature = "proofs")`)
- `wcr verify -p nros-core` (or `just verify-proofs CRATE=nros-core`)
  discharges everything via Z3/Alt-Ergo with ≤ 5 % Coq fallback
- Cold-build verification ≤ 90 s; warm cache ≤ 10 s
- Attestation JSONL written to `target/proofs/nros-core/attestation.intoto.jsonl`
- First `nros-core-<ver>.wcr.tar.zst` bundle produced

### 211.3 — Phase 2: Cross-crate composition `nros-cmd-gate` (weeks 6-7)

**Files (in nano-ros)**

- `packages/safety/nros-cmd-gate/Cargo.toml` (new crate)
- `packages/safety/nros-cmd-gate/src/lib.rs` (envelope clamp + FSM)
- `packages/safety/nros-cmd-gate/nros-package.toml` (with
  `imports = [{ theory = "Nros_core_result", from = "nros-core@^0.4" }]`)

**Files (in `wcr`)**

- `wcr-core::registry` (workspace theory path resolution + symlink
  population under `target/proofs/registry/`)
- `wcr-cli::fetch` (local-fs mode — read bundle from a directory; HTTPS
  mode deferred to 211.8)

**Acceptance**

- `nros-cmd-gate`'s VCs discharge using `nros-core`'s emitted theory
  (`use Nros_core_result`)
- Workspace report (`wcr report`) shows dep graph + per-crate VC counts
- Spec-invalidation propagation: edit `nros-core`'s `Result::ok`
  postcondition → `nros-cmd-gate` re-discharge triggered automatically
- Composition demo works without any hand-written spec file

### 211.4 — Phase 3: Infra hardening (weeks 8-9)

**Files (in `wcr`)**

- `wcr-cache::content_addressed` (cache key composition + invalidation)
- `wcr-bundle::pack` / `wcr-bundle::unpack` (`.wcr.tar.zst` round-trip)
- `wcr-cli::bundle`, `wcr-cli::replay`, `wcr-cli::report` subcommands
- Workspace dep graph (DOT output)
- Parallel discharge via Why3's native `-j`

**Files (in nano-ros)**

- `just/proofs.just` extended with `verify-proofs-replay`, `clean-proofs`,
  `proofs-report`, `proofs-bundle`
- New `test-proofs` tier (informational only — does not gate `test-all`)
- `book/src/internals/proofs.md` (new)
- `book/src/reference/wcr-format.md` (new — points at `wcr.dev/spec`)
- `.envrc` exports Why3 theory dirs for `why3 ide` discovery

**Acceptance**

- `wcr verify` runs the entire workspace in ≤ 3 min cold, ≤ 30 s warm
- Cache hit ratio ≥ 90 % on incremental edits
- Bundle round-trip works: `wcr bundle` → `wcr fetch --from <dir>` →
  `wcr replay` discharges identically on a fresh clone
- Attestation JSONL validates against the published schema
- New contributor can verify from a clean clone in ≤ 15 min following docs

### 211.5 — Phase 4: PoC closeout + Sentinel mini-demo (week 10)

**Files (in nano-ros)**

- `packages/safety/nros-velocity-smoother/` (or 2 additional Sentinel-shaped
  crates) with `proofs/` + Creusot annotations
- `docs/research/phase-211-poc-results.md` (TCB measurement, prover stats,
  effort log, lessons learned)
- `book/src/showcases/proof-carrying-pkg.md` (new)
- Talk slides / demo script

**Acceptance**

- ≥ 100 discharged VCs across PoC crates
- TCB explicitly listed in attestations
- Mini-demo: 3-4 nano-ros crates annotated + bundled + cross-composed
- White-paper draft published
- All format commitments confirmed frozen and documented

### 211.6 — Phase 5: Verus retirement (weeks 11-16)

**Files**

- `docs/proofs/verus-migration.md` (new — inventory + per-proof disposition)
- Per-crate Creusot annotations replacing migrated Verus proofs
- `Cargo.toml` workspace: remove Verus dep, remove `verus.toml` configs
- `just/verification.just`: deprecate `verify-verus` (alias to
  `verify-proofs` for one release, then remove)
- `CLAUDE.md` Verification section: rewritten — Creusot primary, Kani BMC
  secondary
- `just/doctor.just`: drop Verus probe
- `book/src/internals/verification.md`: rewritten

**Acceptance**

- ≥ 80 of the original 102 proofs ported and green under Creusot
- Remaining ≤ 22 marked `#[trusted]` with one follow-up issue per item
- CI no longer requires the Verus binary
- `nros setup --tool wcr-stack` no longer fetches Verus
- Release notes call out "Verus retired" as contributor-visible change

### 211.7 — Phase 6: RMW + platform layer expansion (weeks 17-24)

**Files (RMW subphase, weeks 17-20)**

- `packages/zpico/zpico-sys/proofs/` + `nros-rmw-zenoh/proofs/`
- `packages/xrce/nros-rmw-xrce/proofs/`
- `packages/dds/nros-rmw-cyclonedds/proofs/` (Rust portions only in PoC;
  C portions deferred to cross-language post-PoC work)
- `packages/core/nros-bridge/proofs/`

**Files (platform subphase, weeks 21-24)**

- `packages/core/nros-platform-posix/proofs/{axioms,ghosts}/`
- `packages/core/nros-platform-freertos/proofs/{axioms,ghosts}/`
- `packages/core/nros-platform-zephyr/proofs/{axioms,ghosts}/`
- `packages/core/nros-platform-threadx/proofs/{axioms,ghosts}/`
  (NetX BSD `SO_RCVTIMEO` timeval gotcha encoded as axiom)
- `packages/testing/nros-tests/proofs/axioms/wcet/` (WCET claims per
  target, fed by `cargo-call-stack` + aiT where available)
- `docs/proofs/axiom-attestation.md` (new — provenance + qualifying-entity
  schema)

**Acceptance**

- Each RMW + platform crate ships ≥ 5 spec functions with discharged VCs
- Cross-layer composition demo: `nros-cmd-gate` → `nros-rmw-zenoh` →
  `nros-platform-posix`; all VCs discharge with topo-sorted import
- Axiom-attestation schema documented and populated for at least one HW +
  RTOS combination

### 211.8 — Phase 7: `wcr` registry alpha + cross-language extension (weeks 25-36)

**Files (in `wcr`)**

- `wcr-cli::publish` (Sigstore signing via cosign + transparency log)
- `wcr-cli::fetch` HTTPS mode (against `index.wcr.dev`)
- `wcr-core::refine` (refinement check via Why3 `new_spec ⇒ old_spec`
  on every spec bump)
- `wcr-extract::frama_c` (new backend for C, enabling cross-language work)
- `wcr-cli::extract --language=c` support
- `index.wcr.dev` static index hosted on GitHub Pages
- `wcr-cli::fetch` HTTPS mode integration tests
- `wcr.dev/spec` publication of the v0.1 schema set (frozen since PoC)

**Files (in nano-ros)**

- `.github/workflows/wcr-publish.yml` (publish nano-ros crates to `wcr`
  on release tag)
- Optional: a single C cross-language demo crate (e.g.
  `packages/safety/nros-c-gate-shim/`) demonstrating bidirectional
  composition

**Acceptance**

- nano-ros crates publish to the `wcr` index from CI
- `wcr fetch nros-cmd-gate@<ver>` plus `wcr replay` works from a fresh
  clone with no nano-ros workspace context
- Sigstore signatures verify via standard cosign tooling
- Refinement check catches at least one synthetic "weakened spec" attempt
  in regression testing
- Cross-language demo: a C ACSL-annotated function composes against a
  Rust Creusot-emitted theory, or vice versa

### 211.9 — Phase 8: Sentinel safety-island demonstrator (weeks 37-44)

**Files (in nano-ros)**

- `examples/sentinel-cyber/` (new — bare-metal or FreeRTOS Cortex-M demo)
- `examples/sentinel-cyber/safety_nodes/{mrm_handler,vehicle_cmd_gate,
  velocity_smoother,twist_gate,engage_arbiter,mrm_emergency_stop,
  mrm_comfortable_stop}/` (each with `proofs/`)
- `examples/sentinel-cyber/fake_planner/` (Linux side — untrusted, QM)
- `docs/safety-cases/sentinel-cyber.md` (GSN goal tree citing `wcr`
  packages + Why3 sessions)
- `docs/research/sentinel-poc-results.md` (TCB measurement, prover stats,
  effort log)
- `book/src/showcases/sentinel.md`

**Acceptance**

- End-to-end demo runs in QEMU and on a real Cortex-M target
  (STM32F4 or Nordic nRF candidate)
- Every Sentinel safety node ships proofs at Tier 1 or 2 of the
  package-contract scheme
- GSN safety case fully cited with hyperlinks into the Why3 sessions
  and axiom files
- White paper + slides published (Eclipse SDV Day or equivalent venue)

## Cross-cutting commitments

### TCB budget tracking

Each phase emits a `docs/research/pcc-tcb-budget-phase-NNN.md` snapshot
recording the trusted components added or removed. After 211.6 (Verus
retired) the budget shrinks; after 211.8 sigstore + transparency-log trust
is added.

### Format-stability commitments (frozen at PoC, never broken)

| Element | Status |
|---|---|
| `.wcr.tar.zst` bundle internal layout | frozen at 211.4 |
| `nros-package.toml [proof]` schema (v0.1) | frozen at 211.1 |
| In-toto predicate `https://wcr.dev/proof/v0.1` | frozen at 211.1 |
| Spec hash canonical form | frozen at 211.1 |
| Theory naming convention | frozen at 211.1 |
| Cache key composition | frozen at 211.4 |
| Attestation file naming | frozen at 211.1 |
| SBOM format (CycloneDX) | frozen at 211.4 |

### Annotation conventions

- **Rust**: Creusot `#[requires]`, `#[ensures]`, `#[predicate]`,
  `#[logic]`, `pearlite! { … }`. Trusted bodies via `#[trusted]`; gated
  under the `proofs` Cargo feature so non-proof contributors are unaffected
- **C / Frama-C ACSL**: deferred to 211.8 (post-PoC cross-language
  extension)
- **C++**: deferred indefinitely; revisit once Frama-Clang maturity warrants

### No manual spec authoring

The Creusot-emitted Why3 IS the spec artifact. Authors annotate Rust
source; `wcr extract` produces the bundle's `extracted/rust/<theory>.mlw`;
consumers import that theory by `use My_crate_module`. No hand-written
`proofs/spec/*.mlw` file exists in the per-package layout. Eliminates
spec-vs-impl drift by construction.

### Concurrency posture

Single-thread invariants per node; orchestrator (RTOS scheduler) treated
axiomatically. Concurrent separation logic via Iris / Pulse is deferred
to post-211 work. This matches the Sentinel pattern where each safety
node runs as a fixed-priority periodic task.

### Documentation cadence

- After 211.2: `docs/proofs/getting-started.md` published
- After 211.5: PoC closeout doc + white-paper draft
- After 211.6: `book/src/internals/proofs.md` rewritten to reflect the
  Creusot + Kani split
- After 211.8: `wcr.dev/spec` is the canonical schema reference
- After 211.9: white paper drafted for external publication

## Acceptance (phase-level)

Phase 211 closes when all of the following hold:

1. Work-items 211.1–211.9 meet their per-item acceptance.
2. nano-ros workspace has ≥ 10 crates with `proofs/` published at Tier ≥ 1.
3. `wcr` repository is publicly hosted with ≥ 10 nano-ros packages indexed.
4. Sentinel demonstrator runs in QEMU + on a characterized Cortex-M board
   with the GSN safety case end-to-end.
5. Verus binary is no longer referenced anywhere in the workspace, CI, or
   docs.
6. White paper + Eclipse SDV Day (or equivalent) talk material is published.
7. At least one cross-language demo (Rust ↔ C composition) lands in 211.8.

## Decision gates

| Gate | When | What we decide | Default if measurement fails |
|---|---|---|---|
| Creusot iteration viable? | End of 211.1 | Continue Creusot-only or fall back to dual Verus+Creusot | Extend Phase 0 with workflow optimization; if still hopeless after a week, reintroduce Verus for fast iteration with Creusot for published spec only |
| Cargo feature gating clean? | End of 211.2 | Annotation gating strategy | Refactor into `crate::specs::*` submodule with cleaner conditional compilation |
| Mini-Sentinel demo lands? | End of 211.5 | Greenlight Verus retirement | Defer 211.6 by 2 weeks pending bugfix sweep |
| Verus port debt manageable? | End of 211.6 | Acceptable trusted-stub ratio | Ship with ≤ 22 trusted stubs documented as follow-ups |
| RMW + platform coverage external-pitch ready? | End of 211.7 | Proceed to wcr alpha | Extend 211.7 by 4 weeks before starting 211.8 |
| `wcr` alpha usable + cross-language demo works? | End of 211.8 | Proceed to Sentinel demo | Defer 211.9 by 4 weeks pending bugfix sweep |

## Risk register

| Risk | Phase | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| Creusot iteration too slow | 211.1 / 211.2 | Medium | High | Per-VC cache + `creusot --no-prove` syntax check; keep Verus fallback through 211.2 |
| Annotation cost > 4× source effort | 211.2 / 211.7 | Medium | High | Scope to public API only; trusted-stub internal helpers |
| Verus port debt (hard cases) | 211.6 | Medium | Medium | Time-box per item; accept trusted stubs with follow-ups |
| HW axioms unmeasurable without per-board work | 211.7 / 211.9 | High | Medium | Synthetic bounds for QEMU; real HW only on characterized boards |
| Iris-level concurrent reasoning needed | 211.7 | High | Low | Scope single-thread; defer concurrent SL to post-211 |
| Registry hosting / abuse | 211.8 | Medium | Medium | Static GitHub-Pages-backed index initially; gate publish via PR review |
| Frama-C/WP compat with pinned Why3 (deferred) | 211.8 | Low | High | Confirmed at scaffold time before 211.8 kickoff |
| Bus factor — single-contributor expertise | All | Medium | High | Document everything; pair-write critical infra; lean on upstream tooling for external mindshare |

## Notes

- The `wcr` repo is independent. Lives at `github.com/NEWSLabNTU/wcr` and
  serves nano-ros as a primary client but remains usable by any
  ROS-2-shaped or embedded-Rust-with-proofs stack. Governance evolves on
  its own cadence after 211.8.
- Kani's 160 bounded harnesses are orthogonal to this phase. They remain
  the bounded-model-checking layer and continue under `just verify-kani` /
  CI tier `test-all`. Creusot is the deductive layer; both coexist.
- The annotation gating choice — `cfg(feature = "proofs")` vs always-on —
  is decided at 211.2 kickoff. Default: gated, so non-proof contributors
  install no extra tooling.
- Verus retirement (211.6) sits mid-roadmap rather than at the start.
  Early phases benefit from Verus's faster dev loop while Creusot infra
  catches up via the `wcr` stack; old Verus debt does not bleed into the
  broader expansion (211.7 onward).
- Cross-language work (Frama-C/WP, ACSL, C consumer/dep composition) is
  **post-PoC** under 211.8. The bundle format is cross-language-ready by
  construction (`extracted/<lang>/` directory hierarchy in place from
  211.4 onward) so adding C is adding files, not redesigning the format.
- Concurrent execution proofs at Sentinel scale are out of scope.
  Per-node single-thread invariants suffice for the demonstrator; the
  orchestrator (RTOS scheduler) is axiomatic. Lifting this restriction
  is a multi-year follow-up (Iris / Pulse-shaped work).
- The Sentinel demonstrator (211.9) backs the "open Rust safety-island
  with verified packages" pitch. Without it the pitch is slideware; with
  it nano-ros owns an unoccupied lane in the SDV stack.
- The PoC core (211.1 – 211.5) is **10 weeks**, not 13. C cross-language
  removal + shorter polish phase shrank the original 13-week PoC scope.
  Total phase 211 envelope is ≈ 44 weeks (10 months) for full execution
  through Sentinel demo, with optional pause/checkpoint between 211.5
  PoC closeout and 211.6 Verus retirement.
