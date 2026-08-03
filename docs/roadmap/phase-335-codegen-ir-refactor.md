# phase-335 — language-neutral codegen IR (implement RFC-0068)

**Implements:** [RFC-0068](../design/0068-language-neutral-codegen-ir.md)
**Closes:** issue #402
**Touches:** [RFC-0023](../design/0023-codegen-workspace-discovery.md) (codegen pipeline),
[RFC-0056](../design/0056-ros-edition-axis.md)/phase-304 (RIHS engine relocates into Resolve),
[RFC-0054](../design/0054-c-header-abi-ssot.md) (repr(C) layout becomes a Lower fact),
[RFC-0067](../design/0067-env-invariant-msg-dep-identity.md)/phase-333 (resolve-only deps),
[RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md) (fingerprint gains pack inputs)

**Status.** IN PROGRESS. **W1 landed** (rosidl-resolve + rosidl-lower crates, the
Resolved/Lowered IR, the production hash path routed through the IR, hash-once/lower-
per-target golden). **W2 landed** (C backend on minijinja packs, byte-identical).
**W3 substantially landed** — ALL FIVE type-emission backends now render from
minijinja data packs, each byte-identical (codegen golden + C compile + comparison +
per-backend suites): **C** (`packs/c/`), **rmw** (`packs/rmw/`), **nros**
(`packs/nros/`), **idiomatic** (`packs/rust/`), **C++** (`packs/cpp/`). `rosidl-codegen`
has ONE `minijinja::Environment` holding every pack + a generic `render(name, ctx)`.

W3 remainder (lower value, tracked): **scaffolding** templates (`cargo`/`lib`/`build`/
`cargo_nros`/`lib_nros`) are per-package BOILERPLATE, not per-language type logic — still
on askama; converting them is disambiguation-tedious (one var name holds several template
types) with little payoff, so deferred. **idl (W3.d)** is NOT a rosidl-codegen askama
backend: the msg→Cyclone-IDL emitter lives in the separate `nros-msg-to-idl` crate and
uses `.em` (empy) templates, a different mechanism outside this askama→minijinja migration —
reclassify, do not convert here. The deeper "no per-language TYPE logic in Rust" acceptance
(moving `types.rs` `*_type_for_field` spelling into the packs / LoweredType) is the W1.c-full
+ W6 work, still pending.

### Learnings that refine the remaining plan (2026-08 W2)

1. **The askama templates were ALREADY `.jinja`** and most view structs are flat
   data, so a backend converts by: copy template → mechanical dialect deltas
   (`else if`→`elif`, `&&`→`and`, `||`→`or`, `!x`→`not x`, `{% call m::f %}`→
   `{{ m.f() }}`, import rename) → derive `Serialize` on the view struct → render
   via `render::render_c`-style call. **Operator substitution MUST be scoped to
   `{% %}` lines** — emitted C/Rust `||`/`&&` (NULL checks, boolean ops) are plain
   text and get corrupted by a blanket sed.
2. **minijinja whitespace matched askama byte-for-byte** for the C templates — no
   normalization commit was needed (W3.e may still be needed for a backend that
   uses askama-specific whitespace).
3. **W3 backends split by difficulty:**
   - **EASY (data-only jinja, convert exactly like C):** `message_rmw` /
     `service_rmw` / `action_rmw` (the RRR/rmw Rust backend), and the `lib` /
     `lib_nros` / `build` / `cargo` / `cargo_nros` scaffolding templates. Only
     `{{ var }}` + `{% for/if %}`; the `.foo()`/`::` in them are in the EMITTED
     code, not jinja.
   - **HARD (askama filters + method calls in jinja):** `*_idiomatic` (168/112/56
     askama-isms), `*_nros` (86/40/20), `message_cpp_types` (71), `*_cpp` /
     `cpp_exports`. These call context methods and custom filters
     (`templates::filters::snake_case`) inside `{{ }}`, which minijinja cannot do
     against a data context. Each needs its method-call values PRE-COMPUTED into
     the serde context (extend the view struct) and its filters REGISTERED on the
     minijinja `Environment` — real per-template work, not a sed.
4. **W1 sizing-helper deletion deferred:** `compute_serialized_size_max` etc. in
   `types.rs` are still used by the un-converted cpp backend; delete them when cpp
   converts (folds into W6), not before.

## Goal

Make **adding a codegen target language a data-pack change, not a Rust change**, without ever
letting a template recompute an embedded/target fact. Restructure `rosidl-codegen` from
"one crate of per-language functions + askama templates" into the four-stage pipeline of
RFC-0068: **parse → resolve → lower → render**, where render is a runtime-templated consumer of a
language-neutral, target-concrete IR.

## The one ordering constraint that matters

The derived facts (RIHS hash, resolved refs, fixed-capacity storage, `repr(C)` layout, plainness,
serialized-size) must be lifted into the `ResolvedType`/`LoweredType` IR **before** any backend
is moved off its Rust helpers. Move a backend first and it still reaches into `types.rs` — no
decoupling happens.

> **W1 (the IR) blocks W2–W6.** There is no useful partial progress on the render rewrite until
> every emission input is a field on `LoweredType`.

Second constraint: **W2 (prove the data path on one backend) precedes W3 (migrate the rest).**
Do not convert five backends before one is shown byte-identical.

## Invariant that guards every wave

Every wave is **byte-identical-output preserving** until W6, proven by `fingerprint.rs`
(RFC-0061): run the codegen golden corpus before and after each wave; the emitted bytes must not
change. A wave that changes output is a bug in that wave, not an accepted cost. (The only wave
allowed to change bytes is a deliberate, reviewed formatting normalization, called out in W3.)

## Work items

### W1 — the IR stages (BLOCKS EVERYTHING)

- [x] **W1.a** New crate `rosidl-resolve`. Define `ResolvedType` = `Ast` + fully-qualified type
      refs + RIHS hash + type-description closure. Relocate the RIHS engine out of
      `rosidl-codegen/rihs.rs` into this crate (it is already self-contained). `serde`-derive.
- [x] **W1.b** New crate `rosidl-lower`. Define `TargetProfile` (`ptr_width`, `enum_width` /
      `short_enums`, alignment rules) and `LoweredType` = `ResolvedType ⊗ CodegenConfig ⊗
      TargetProfile`, carrying per field: `storage` (Fixed/Bounded/Heap/Inline from the existing
      `CapacityResolver`), `plain`, `align`, `repr_c_field_order`, `serialized_size_max`,
      `cdr_op`. Move the sizing/layout logic out of `rosidl-codegen/types.rs` into here.
- [ ] **W1.c** Rewire the existing (askama) backends to consume `LoweredType` instead of
      recomputing from `Ast`. No new engine yet, no output change.
- [x] **W1.d** Golden test: `ResolvedType` hash values pinned against the same REP-2011 vectors
      `rihs.rs` pins today; `LoweredType` layout pinned for x86_64 AND an armv7a short-enum
      `TargetProfile` (guards the arm-short-enums class, project memory).
- **Acceptance:** `just ci` green; codegen golden corpus byte-identical; `rihs.rs` and the sizing
  helpers in `types.rs` are deleted (their logic now lives in resolve/lower), not duplicated.

### W2 — prove the data-driven render on ONE backend (C)

- [x] **W2.a** Add `minijinja` (runtime engine) + the `LanguagePack` loader: `packs/<lang>/`
      with `spelling.toml` + `pack.toml` + `*.jinja`, bundled via `include_dir!`.
- [x] **W2.b** Author `packs/c/` reproducing the current C output from `LoweredType`. C first: no
      move semantics, simplest storage rules.
- [x] **W2.c** Assert the pack-rendered C is **byte-identical** to the askama C over the golden
      corpus; then delete the Rust C templates + the C `*_type_for_field` helpers.
- **Acceptance:** C messages/services/actions emitted with ZERO C-specific Rust; corpus
  byte-identical; **the per-language-burden claim is now disproven for one language.**

### W3 — migrate the remaining backends

- [x] **W3.a** `packs/cpp/` (move semantics, storage-mode branching → confirm the escape hatch is
      not needed; if it is, document why).
- [x] **W3.b** `packs/rust/`.
- [x] **W3.c** `packs/nros/` (embedded-idiomatic; heaviest storage/plainness use — the real test
      of whether `LoweredType` carries enough).
- [ ] **W3.d** `packs/idl/` (Cyclone IDL; consumes `ResolvedType`, needs no layout facts).
- [ ] **W3.e** If a reviewed formatting normalization is unavoidable (askama vs minijinja
      whitespace), land it as ONE explicit output-changing commit with the new golden baseline —
      never smuggled inside a "refactor" wave.
- **Acceptance:** all five backends render from packs; `rosidl-codegen` holds pipeline glue +
      pack loader only, no per-language type logic; corpus byte-identical (modulo W3.e).

### W4 — make "add a language" real

- [ ] **W4.a** `fingerprint.rs` (RFC-0061) hashes pack files, so a template/spelling edit marks
      fixtures stale (a pack is a codegen input).
- [ ] **W4.b** `--template-dir` override: an external pack dir loads with no rebuild; bundled
      packs stay the default.
- [ ] **W4.c** A CI smoke pack (a trivial toy language) proving a brand-new pack renders with zero
      Rust change — the executable form of the goal.
- [ ] **W4.d** Book/docs page: "adding a codegen language = a pack" (spelling.toml + templates).
- **Acceptance:** the smoke pack renders in CI; stale detection fires on a pack edit; docs land.

### W5 — resolve-only dependency packages

- [ ] **W5.a** At the Resolve seam, support resolve-only dep packages (hash deps without emitting)
      and evaluate a single shared `*_msgs` crate against per-package `0.0.0` crates for the #378
      / RFC-0067 tension. Land whichever the RFC-0067 owners endorse.
- **Acceptance:** stdlib msg deps resolve for correct hashes without emitting; leaf lockfile /
      registry-resolution behavior unchanged or improved vs phase-333 baseline.

### W6 — close out

- [ ] **W6.a** Delete any remaining dead per-language Rust in `rosidl-codegen`.
- [ ] **W6.b** RFC-0068 `Draft → Stable`; resolve issue #402 (move to `archived/`).
- [ ] **W6.c** Update RFC-0023 to point at the four-stage pipeline as the current shape.
- **Acceptance:** `rg` finds no `*_type_for_field`-style per-language emit fn in the codegen crate;
      #402 archived; RFC-0068 Stable.

## Out of scope

- No new target language shipped by this phase (it makes shipping one cheap).
- No wire/CDR-format change; `cdr_op` is a neutral spelling of today's serialization.
- No generated-crate-identity change (RFC-0067 stands).

## Risks

- **minijinja perf** in the build path — measure W2 against
  `rosidl-codegen/benches/generation_benchmark.rs`; expect noise-level, but gate on it.
- **`LoweredType` completeness** — W3.c (`nros` embedded backend) is the true test that the IR
  carries enough; if a fact is missing it surfaces there, add it to Stage 2, never to a template.
- **Whitespace drift** between engines — contained to the single W3.e commit, not spread.
