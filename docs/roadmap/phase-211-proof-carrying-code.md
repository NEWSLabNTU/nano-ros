# Phase 211 — Proof-Carrying Code in nano-ros (Creusot + Frama-C)

**Goal:** Stand up proof-carrying-code infrastructure in nano-ros for both C
and Rust packages. Each verified package ships per-function pre/post
annotations that compile to Why3 proof obligations; consumers compose against
the published Why3 theories. Retire Verus in favour of Creusot as the single
Rust verifier (Frama-C/WP for C). Extract the registry tooling into a
standalone Why3 Component Registry (`wcr`) and deliver a Sentinel-style
safety-island demonstrator with an end-to-end GSN safety case.

**Status:** Proposed (2026-05-31).

> **Post-Phase-218**: References to `scripts/install-nros.sh` + the
> external `github.com/NEWSLabNTU/nros-cli` repo below predate the
> Phase 218 monorepo merge. The CLI now lives in-tree at `packages/cli/`
> (build via `just setup-cli`); proof-tool installer shims should
> extend the in-tree CLI rather than the retired curl script. The
> `nros-cli` GitHub repo is archived / read-only.

**Priority:** High (defines the verification posture nano-ros pitches to
Eclipse SDV, Autoware safety-island integrators, and downstream OEMs).

**Depends on:** Phase 191 (SDK provisioning), Phase 208 (book audit closure),
Phase 209 (C++ port friction) for parity on the user-facing onboarding story.

## Overview

nano-ros today ships 102 Verus proofs + 160 Kani bounded harnesses. That
provides spot verification but not composition: a downstream consumer cannot
chain a dep's guarantee `X` into their derived property `Y` without
re-verifying from scratch. The verification narrative is also fragmented
across Verus-internal logic, Kani's CBMC backend, and ad-hoc annotations.

Phase 211 unifies the verification story around Why3 as the proof-obligation
intermediate representation:

- **Rust** packages annotate with Creusot (`#[requires]`, `#[ensures]`,
  `#[predicate]`, `#[logic]`). Creusot extracts Why3 modules with pre/post
  contracts plus impl-meets-spec VCs.
- **C** packages annotate with ACSL (`/*@ requires …; ensures …; */`).
  Frama-C/WP emits the same Why3 IR.
- A **shared Why3 theory** (`proofs/spec/*.mlw`) is the consumer-facing
  contract; downstream Rust or C packages `use Foo` to import the theory and
  discharge their own VCs against it.
- A **session cache** + **attestation bundle** make verification re-runnable
  by skeptical consumers without re-deriving the proofs.
- Verus is removed at the end of Phase 5 of the work-item ladder; Kani's
  bounded model checking remains as orthogonal coverage.

The work culminates in `wcr` — a standalone proof-carrying-package registry
modeled on rosdistro + crates.io with sigstore signing and Why3 refinement
checks on spec bumps — and a Sentinel-style safety MCU demonstrator running
verified safety-gate nodes with a Goal Structuring Notation (GSN) safety case
citing each Why3 session as evidence.

## Architecture

### Verification IR + tooling

```
Rust source ──Creusot──┐
                       ├──> Why3 obligations ──> Z3 / Alt-Ergo / CVC5 / Coq
C source ─────Frama-C──┘
```

Single discharge pipeline, single proof artifact format per package. Cross-
language composition happens at the Why3 theory layer: both Rust and C
consumers import the same `Foo_spec.mlw` and compose against its `ensures`
clauses.

### Per-package layout

```
packages/<crate>/
├── Cargo.toml          (or CMakeLists.txt for C)
├── src/
├── proofs/
│   ├── spec/<theory>.mlw          # exported consumer-facing spec
│   ├── axioms/<axiom-family>.mlw  # HW/RTOS axioms this crate depends on
│   ├── ghosts/<ghost>.mlw         # ghost-state theories (not compiled)
│   └── session/
│       ├── why3session.xml
│       └── why3shapes.gz
└── nros-package.toml              # contract manifest (tier, deps, claims)
```

### Workspace layer

```
target/proofs/
├── registry/                       # auto-populated symlinks per member
├── cache/<vc-hash>/<prover-ver>/   # content-addressed discharge cache
└── attestations/<crate>.json       # per-crate attestation
```

### CLI / orchestration

The `nros why3` subcommand lives in `nros-cli` and shells out to the pinned
prover stack at `~/.nros/sdk/why3-stack/<ver>/bin/`. After Phase 7, the
subcommand migrates into a standalone `wcr` CLI; `nros why3` becomes a thin
alias.

```
nros why3 extract     # run Creusot / Frama-C per crate
nros why3 replay      # re-discharge cached sessions
nros why3 prove       # discharge new VCs
nros why3 verify      # extract + replay + prove (workspace)
nros why3 report      # workspace-level aggregation
nros why3 publish     # bundle + sign + push to registry (Phase 7+)
```

Workspace integration: `just verify-proofs` wraps the CLI; a new informational
`test-proofs` tier sits alongside `test-doc` / `test-miri`. CI consumes the
attestation JSON to detect drift.

### Tool versions

Pinned via `nros-sdk-index.toml` `[tool.why3-stack]`:

| Tool | Pinned version |
|---|---|
| Why3 | 1.7.0 |
| Creusot | 0.5.0 |
| Frama-C + WP | 30 (Zinc) |
| Z3 | 4.13.x |
| Alt-Ergo | 2.5.x |
| CVC5 | 1.1.x |
| Coq | 8.19 (fallback) |

Installed via `nros setup --tool why3-stack`. Shims at `~/.nros/bin/{creusot,
frama-c, why3, z3, alt-ergo, cvc5}` resolve the latest installed version in
the SDK store (same pattern as `~/.nros/bin/zenohd` from Phase 208.B).

## Work Items

### 211.1 — Phase 0: Tool provisioning + parity baseline (weeks 1-2)

**Files**

- `nros-sdk-index.toml` (extend with `[tool.why3-stack]` + `[source.creusot]`,
  `[source.frama-c]`)
- `scripts/install-nros.sh` (extend shim block for `creusot`, `frama-c`,
  `why3`, `z3`, `alt-ergo`, `cvc5`)
- `just/workspace.just` (extend `cargo-tools` probe for the why3 stack)
- `docs/research/phase-211-pcc-baseline.md` (new — iteration time,
  discharge time, annotation/code ratio measurements)

**Acceptance**

- `nros setup --tool why3-stack` succeeds on a clean Linux host.
- `which creusot && which frama-c && which why3` all resolve via
  `~/.nros/bin/` shims.
- One representative Verus proof from the existing 102 is ported to Creusot
  end-to-end; cold discharge ≤ 5 s, warm cache ≤ 1 s.
- Baseline measurements committed to the research doc.

### 211.2 — Phase 1: Rust pilot crate `nros-core` (weeks 3-6)

**Files**

- `packages/core/nros-core/proofs/spec/{result,message,error}.mlw`
- `packages/core/nros-core/proofs/axioms/alloc_axioms.mlw` (gated on `std`)
- `packages/core/nros-core/proofs/session/why3session.xml`
- `packages/core/nros-core/proofs/session/why3shapes.gz`
- `packages/core/nros-core/nros-package.toml` (new — `[proof]` section)
- `packages/core/nros-core/src/lib.rs` (annotate public API)
- `just/proofs.just` (new — `verify-proofs CRATE=<name>` recipe)
- `docs/proofs/getting-started.md` (new)

**Acceptance**

- 30–50 discharged VCs across `Result`, error enum, message-trait laws.
- `cargo build` is unchanged for non-proof users (annotations behind
  `cfg(feature = "proofs")`).
- `just verify-proofs CRATE=nros-core` discharges everything via Z3 /
  Alt-Ergo with ≤ 5 % Coq fallback.
- Cold-build verification ≤ 90 s; warm cache ≤ 10 s.
- Attestation JSON written to `target/proofs/attestations/nros-core.json`.

### 211.3 — Phase 2: Cross-crate composition `nros-cmd-gate` (weeks 7-9)

**Files**

- `packages/safety/nros-cmd-gate/Cargo.toml` (new crate)
- `packages/safety/nros-cmd-gate/src/lib.rs` (envelope clamp + FSM)
- `packages/safety/nros-cmd-gate/proofs/spec/{clamp,fsm}.mlw`
- `packages/safety/nros-cmd-gate/nros-package.toml`
- `target/proofs/registry/` (auto-populated workspace symlink dir)

**Acceptance**

- `nros-cmd-gate`'s VCs discharge using `nros-core::Result`'s exported
  `ensures` clauses (import via Why3 `use Nros_core_result`).
- Workspace report (`nros why3 report`) shows the dep graph plus per-crate
  VC counts.
- Spec-invalidation propagation works: editing `nros-core`'s `Result::ok`
  postcondition triggers `nros-cmd-gate` re-discharge.

### 211.4 — Phase 3: C cross-language `nros-c-gate-shim` (weeks 10-13)

**Files**

- `packages/safety/nros-c-gate-shim/CMakeLists.txt` (Frama-C wired into
  configure-time)
- `packages/safety/nros-c-gate-shim/src/clamp.c` + `.h` with ACSL
  annotations
- `packages/safety/nros-c-gate-shim/proofs/spec/c_clamp.mlw`
- `packages/safety/nros-c-gate-shim/nros-package.toml`
- `docs/proofs/cross-language.md` (new)

**Acceptance**

- Same `Clamp_spec.mlw` theory (originally exported by `nros-cmd-gate`) is
  imported by the C consumer's Frama-C-generated Why3 obligations.
- Single Why3 lemma proves a property used by both Rust and C consumers.
- `just verify-proofs CRATE=nros-c-gate-shim` discharges end-to-end.

### 211.5 — Phase 4: Infra hardening (weeks 14-17)

**Files (in `github.com/NEWSLabNTU/nros-cli`)**

- `crates/nros-cli/src/cmd/why3/mod.rs` (new — `extract / replay / prove /
  verify / report / publish` subcommands)
- `crates/nros-cli/src/why3/cache.rs` (content-addressed cache)
- `crates/nros-cli/src/why3/attestation.rs` (per-crate JSON emitter)
- `crates/nros-cli/src/why3/registry.rs` (workspace registry walker)

**Files (in nano-ros)**

- `just/proofs.just` (extend with `verify-proofs-replay`, `clean-proofs`,
  `proofs-report`)
- `book/src/internals/proofs.md` (new — onboarding chapter)
- `book/src/reference/why3-toolchain.md` (new)
- `.envrc` (export Why3 theory dirs for `why3 ide` discovery)

**Acceptance**

- `nros why3 verify` runs across the entire workspace in ≤ 3 min cold,
  ≤ 30 s warm.
- Cache hit ratio ≥ 90 % on incremental edits.
- Attestation JSONs validate against the published schema.
- A new contributor can verify from a clean clone in ≤ 15 min following
  the docs.
- `test-proofs` tier ships (informational only, does not gate `test-all`).

### 211.6 — Phase 5: Verus retirement (weeks 18-23)

**Files**

- `docs/proofs/verus-migration.md` (new — inventory + per-proof
  disposition)
- Per-crate `proofs/spec/*.mlw` updates capturing the migrated lemmas.
- `Cargo.toml` workspace: remove Verus dep, remove `verus.toml` configs.
- `just/verification.just`: deprecate `verify-verus` (alias to
  `verify-proofs` for one release, then remove).
- `CLAUDE.md` Verification section: rewritten — Creusot primary, Kani BMC
  secondary.
- `just/doctor.just`: drop Verus probe.
- `book/src/internals/verification.md`: rewritten.

**Acceptance**

- ≥ 80 of the original 102 proofs ported and green under Creusot.
- Remaining ≤ 22 marked `#[trusted]` with one follow-up issue per item.
- CI no longer requires the Verus binary.
- `nros setup --tool why3-stack` no longer fetches Verus.
- Release notes call out "Verus retired" as a contributor-visible change.

### 211.7 — Phase 6: RMW + platform layer expansion (weeks 24-32)

**Files (RMW subphase, weeks 24-28)**

- `packages/zpico/zpico-sys/proofs/` + `nros-rmw-zenoh/proofs/spec/
  {session,publish,subscribe}.mlw`
- `packages/xrce/nros-rmw-xrce/proofs/spec/{session,profile_udp}.mlw`
- `packages/dds/nros-rmw-cyclonedds/proofs/spec/{participant,service}.mlw`
  (C side via Frama-C)
- `packages/core/nros-bridge/proofs/spec/bridge.mlw` (cross-RMW invariant
  preservation)

**Files (platform subphase, weeks 29-32)**

- `packages/core/nros-platform-posix/proofs/{spec,axioms}/`
- `packages/core/nros-platform-freertos/proofs/{spec,axioms}/`
- `packages/core/nros-platform-zephyr/proofs/{spec,axioms}/`
- `packages/core/nros-platform-threadx/proofs/{spec,axioms}/` (NetX BSD
  `SO_RCVTIMEO` timeval gotcha encoded)
- `packages/testing/nros-tests/proofs/axioms/wcet/` (WCET claims per
  target, fed by `cargo-call-stack` + aiT where available)
- `docs/proofs/axiom-attestation.md` (new — provenance + qualifying-entity
  schema)

**Acceptance**

- Each RMW + platform crate ships ≥ 5 spec functions with discharged VCs.
- Cross-layer composition demo: `nros-cmd-gate` → `nros-rmw-zenoh` →
  `nros-platform-posix`, all VCs discharge with topo-sorted import.
- Axiom-attestation schema is documented and populated for at least one
  HW + RTOS combination.

### 211.8 — Phase 7: `wcr` extraction + registry alpha (weeks 33-44)

**Files (in new `github.com/NEWSLabNTU/wcr` repo)**

- `crates/wcr-cli/src/main.rs` (`wcr verify / publish / fetch / replay`)
- `crates/wcr-index/src/schema.rs` (rosdistro-shaped YAML index)
- `crates/wcr-sigstore/src/lib.rs` (Sigstore signing on publish)
- `crates/wcr-refine/src/lib.rs` (refinement check via Why3
  `new_spec ⇒ old_spec`)
- `docs/spec.md` (registry schema, hosted at `wcr.dev/spec` /
  `index.wcr.dev`)
- `index/<package>/<version>.yaml`

**Files (in nano-ros)**

- `nros-cli` wrapper: `nros why3` becomes a thin alias to the standalone
  `wcr` binary; both available behind the same shims.
- `.github/workflows/wcr-publish.yml` (publish nano-ros crates to `wcr` on
  release tag).

**Acceptance**

- nano-ros crates publish to the `wcr` index from CI.
- `wcr fetch nros-cmd-gate@<ver>` plus `wcr replay` works from a fresh
  clone with no nano-ros workspace context.
- Sigstore signatures verify via standard cosign tooling.
- Refinement check catches at least one synthetic "weakened spec" attempt
  in regression testing.

### 211.9 — Phase 8: Sentinel safety-island demonstrator (weeks 45-52)

**Files**

- `examples/sentinel-cyber/` (new — bare-metal or FreeRTOS Cortex-M demo)
- `examples/sentinel-cyber/safety_nodes/{mrm_handler,vehicle_cmd_gate,
  velocity_smoother,twist_gate,engage_arbiter,mrm_emergency_stop,
  mrm_comfortable_stop}/` (each with `proofs/`)
- `examples/sentinel-cyber/fake_planner/` (Linux side — untrusted, QM)
- `docs/safety-cases/sentinel-cyber.md` (GSN goal tree citing wcr
  packages + Why3 sessions)
- `docs/research/sentinel-poc-results.md` (TCB measurement, prover
  stats, effort log)
- `book/src/showcases/sentinel.md`

**Acceptance**

- End-to-end demo runs in QEMU and on a real Cortex-M target
  (STM32F4 or Nordic nRF candidate).
- Every Sentinel safety node ships proofs at Tier 1 or 2 of the
  package-contract scheme.
- GSN safety case is fully cited with hyperlinks into the Why3 sessions
  and axiom files.
- White paper + slides published (Eclipse SDV Day or equivalent venue
  target).

## Cross-cutting commitments

### TCB budget tracking

Each phase emits a `docs/research/pcc-tcb-budget-phase-NNN.md` snapshot
recording the trusted components added or removed. After 211.6 (Verus
retired) the budget shrinks; after 211.7 sigstore + transparency-log trust
is added.

### `nros-package.toml` schema (new file)

Per-package contract manifest carrying:

```toml
[proof]
tier            = 1   # 0=provenance only, 1=verified, 2=safety-island grade, 3=cert kernel
language        = "rust"   # rust | c | cpp
extractor       = "creusot-0.5"
verifier        = "why3-1.7"
provers         = ["z3-4.13", "alt-ergo-2.5"]

[proof.composition]
spec_files      = ["proofs/spec/result.mlw"]
axiom_deps      = ["heapless-axioms-0.8", "alloc-axioms-1.0"]
imports         = ["nros-core::Result_spec"]

[proof.attestation]
slsa_provenance = "attestations/slsa.json"
sigstore_bundle = "attestations/sigstore.bundle"
```

### Annotation conventions

- **Rust**: Creusot `#[requires]`, `#[ensures]`, `#[predicate]`, `#[logic]`,
  `pearlite! { … }`. Trusted bodies via `#[trusted]`; gated under the
  `proofs` Cargo feature so non-proof contributors are unaffected.
- **C**: ACSL contracts in `/*@ … */` blocks on every public function in
  proof-tier packages. Hand-written `proofs/spec/*.mlw` only where ACSL
  cannot express the spec.
- **C++**: deferred. `nros-cpp` remains axiomatic in this phase; revisit
  once Frama-Clang maturity warrants it.

### Concurrency posture

Single-thread invariants per node; orchestrator (RTOS scheduler) treated
axiomatically. Concurrent separation logic via Iris / Pulse is deferred to
post-211 work. This matches the Sentinel pattern where each safety node
runs as a fixed-priority periodic task.

### Documentation cadence

- After 211.2: `docs/proofs/getting-started.md` published.
- After 211.5: `book/src/internals/proofs.md` rewritten to reflect the
  Creusot + Kani split.
- After 211.7: `wcr.dev/spec` is the canonical schema reference.
- After 211.9: white paper drafted for external publication.

## Acceptance (phase-level)

Phase 211 closes when all of the following hold:

1. Phases 211.1–211.9 work-items meet their per-item acceptance.
2. nano-ros workspace has ≥ 10 crates with `proofs/` published at Tier ≥ 1.
3. `wcr` repository is publicly hosted with ≥ 10 nano-ros packages indexed.
4. Sentinel demonstrator runs in QEMU + on a characterized Cortex-M board
   with the GSN safety case end-to-end.
5. Verus binary is no longer referenced anywhere in the workspace, CI, or
   docs.
6. White paper + Eclipse SDV Day (or equivalent) talk material is
   published.

## Decision gates

| Gate | When | What we decide | Default if measurement fails |
|---|---|---|---|
| Creusot iteration viable? | End of 211.1 | Continue Creusot-only or fall back to dual Verus+Creusot | Extend Phase 0 with workflow optimization; if still hopeless after a week, reintroduce Verus for fast iteration with Creusot for published spec only |
| Cargo feature gating clean? | End of 211.2 | Annotation gating strategy | Refactor into `crate::specs::*` submodule with cleaner conditional compilation |
| Cross-language pays off? | End of 211.4 | Continue C cross-language or descope | Demote 211.4 to post-PoC and close at "Rust-only PoC" |
| Verus port debt manageable? | End of 211.6 | Acceptable trusted-stub ratio | Ship with ≤ 22 trusted stubs documented as follow-ups |
| RMW + platform coverage external-pitch ready? | End of 211.7 | Proceed to wcr extraction | Extend 211.7 by 4 weeks before starting 211.8 |
| wcr usable by external user? | End of 211.8 | Proceed to Sentinel demo | Defer 211.9 by 4 weeks pending bugfix sweep |

## Risk register

| Risk | Phase | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| Creusot iteration too slow | 211.1 / 211.2 | Medium | High | Per-VC cache + `creusot --no-prove` syntax check; keep Verus fallback through 211.2 |
| Annotation cost > 4× source effort | 211.2 / 211.7 | Medium | High | Scope to public API only; trusted-stub internal helpers |
| Verus port debt (hard cases) | 211.6 | Medium | Medium | Time-box per item; accept trusted stubs with follow-ups |
| HW axioms unmeasurable without per-board work | 211.7 / 211.9 | High | Medium | Synthetic bounds for QEMU; real HW only on characterized boards |
| Iris-level concurrent reasoning needed | 211.7 | High | Low | Scope single-thread; defer concurrent SL to post-211 |
| Registry hosting / abuse | 211.8 | Medium | Medium | Static GitHub-Pages-backed index initially; gate publish via PR review |
| Bus factor — single-contributor expertise | All | Medium | High | Document everything; pair-write critical infra; lean on upstream tooling for external mindshare |
| Frama-C plugin compat with pinned Why3 | 211.4 | Low | High | Confirm at 211.1 baseline; hold pin until both versions test green together |

## Notes

- The Why3 Component Registry is an independent project. `wcr` will live at
  `github.com/NEWSLabNTU/wcr` and serve nano-ros as a primary client but
  remain usable by any ROS-2-shaped or embedded-Rust-with-proofs stack.
  Governance evolves on its own cadence after Phase 211.8.
- Kani's 160 bounded harnesses are orthogonal to this phase. They remain
  the bounded-model-checking layer and continue under `just verify-kani` /
  CI tier `test-all`. Creusot is the deductive layer; both coexist.
- The annotation gating choice — `cfg(feature = "proofs")` vs always-on —
  is decided in 211.2. Default: gated, so non-proof contributors install
  no extra tooling.
- The roadmap deliberately retires Verus mid-phase (211.6) rather than at
  the start. Early phases benefit from Verus's faster dev loop while
  Creusot infra catches up; old Verus debt does not bleed into the
  broader expansion (211.7 onward).
- Concurrent execution proofs at Sentinel scale are out of scope. Per-node
  single-thread invariants suffice for the demonstrator; the orchestrator
  (RTOS scheduler) is axiomatic. Lifting this restriction is a multi-year
  follow-up (Iris / Pulse-shaped work).
- Phase 211 publishes to `docs/research/` as it progresses
  (`phase-211-pcc-baseline.md`, `pcc-tcb-budget-phase-NNN.md`,
  `sentinel-poc-results.md`). The final white paper consolidates these
  into a single external-facing document.
- Eclipse SDV Day positioning: the Sentinel demonstrator is the artifact
  that backs the "open Rust safety-island with verified packages"
  pitch. Without the demonstrator, the pitch is slideware; with it,
  nano-ros owns an unoccupied lane in the SDV stack.
