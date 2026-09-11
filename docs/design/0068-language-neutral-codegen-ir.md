---
rfc: 0068
title: "Language-neutral codegen IR — parse → resolve → lower → render"
status: Stable
since: 2026-08
last-reviewed: 2026-09-11
implements-tracked-by: [phase-335]
supersedes: []
superseded-by: null
---

# Language-neutral codegen IR — parse → resolve → lower → render

> **Relationship to other RFCs.** Refines the codegen pipeline first sketched in
> [RFC-0023](0023-codegen-workspace-discovery.md) (discovery + call shape). Carries the
> env-invariant crate identity of [RFC-0067](0067-env-invariant-msg-dep-identity.md) unchanged.
> The RIHS engine ([RFC-0056]) and the C-ABI layout SSoT ([RFC-0054]) become **stages** here
> rather than helpers reached during emission. Tracked by **phase-335**. Issue: **#402**.
> **Amended by [RFC-0091](0091-one-entry-codegen-producer-many-language-packs.md)** —
> see **Amendments** at the end. The original text is kept; passages an amendment
> supersedes are marked *[Amendment N]*.

**Goal.** Adding a target language to codegen becomes **dropping a data pack**, never a Rust
package change — while every embedded/target-critical fact (type hash, fixed-capacity storage,
`repr(C)` layout, plainness, serialized size) is still computed once by trusted Rust and can
never be recomputed wrong by a template. *[Amendment 2: "never a Rust package change" held
for type spelling only.]*

## The problem

`nros` generates Rust, `nros` (embedded-idiomatic), C, C++ and Cyclone IDL from ROS
`.msg/.srv/.action`. Today all five backends live in one Rust crate (`rosidl-codegen`) as
per-language functions (`rust_type_for_field`, `c_type_for_field_with_capacity`,
`repr_c_type_for_field`, `compute_serialized_size_max`, …) plus askama templates. Two structural
faults follow:

1. **Adding a language requires a Rust change.** The derived facts a generator needs — RIHS
   hash, resolved dependency identities, per-field fixed-capacity, `repr(C)` layout, plainness,
   serialized-size — are computed *inside* the Rust crate and reached only by calling Rust. A new
   language must call those functions (or reimplement them), so it is new Rust code. External
   contributors cannot add a language without touching the core.

2. **The serializable IR is at the wrong stage.** The one serializable IR we have,
   `rosidl_parser::Ast`, is captured at the **parse** stage — it holds only what the raw `.msg`
   text said. None of the derived facts above are in it, so the AST alone cannot drive a correct
   generator. The facts are computed *later*, ad hoc, during emission (`rihs.rs` invoked inline,
   sizing scattered across `types.rs`), which is also why the hash and the layout are hard to
   golden-test in isolation.

The fix is one idea applied twice: **compute every derived fact in a small set of trusted,
language-neutral Rust stages, then make emission a data-driven consumer of those facts.**

## Design — four stages

```
 .msg/.srv/.action/.idl
      │  STAGE 0  PARSE       rosidl-parser (unchanged)
      ▼
   Ast = Message / Service / ActionSpec       text-faithful · recursive typed
      │                                         FieldType · serde · lang- & target-neutral
      │  STAGE 1  RESOLVE
      ▼
   ResolvedType = Ast
        + fully-qualified type refs (NamespacedType → pkg::Name closure)
        + RIHS type hash                         (RFC-0056 engine moves here)
        + type-description closure               (for the hash + introspection)
      │                                          LANGUAGE-neutral, TARGET-neutral · serde
      │  STAGE 2  LOWER   ( ⊗ CodegenConfig ⊗ TargetProfile )
      ▼
   LoweredType = ResolvedType, per field made CONCRETE:
        storage : Fixed{cap} | Bounded{cap} | Heap | Inline     (CapacityResolver output)
        plain   : bool                                          (POD blit candidate)
        align, repr_c_field_order, serialized_size_max
        cdr_op  : neutral read/write op id                      (NOT a language method name)
      │                                          LANGUAGE-neutral, TARGET-specific · serde
      │                                          ← every embedded/target fact lives HERE
      │  STAGE 3  RENDER   ( per language, DATA-driven )
      ▼
   generated Rust / nros / C / C++ / Cyclone-IDL / <new-language>
```

*[Amendment 1]* Stage 2 no longer takes a `TargetProfile`, and `LoweredType` is
target-AGNOSTIC: the diagram's "⊗ TargetProfile", "TARGET-specific" and "every
embedded/target fact lives HERE" are superseded.

### Stage 0 — Parse (keep as-is)
`rosidl-parser` already emits a faithful, `serde`-serializable, recursively-typed AST. It is a
strength (stronger than a flat `base_type: String`) and does not change.

### Stage 1 — Resolve (new: `rosidl-resolve`)
Needs the whole package set: resolve every `NamespacedType` to a fully-qualified identity, build
the transitive type-description closure, compute the RIHS hash (the RFC-0056 engine relocates
here from `rosidl-codegen/rihs.rs`). Output `ResolvedType` is **target-neutral** — one hash
serves every target. Supports **resolve-only dependency packages** (resolve `std_msgs` etc. for
correct hashes without emitting them — RFC-0067 / phase-333 direction, relieves the `0.0.0`
per-package tension of issue #378).

### Stage 2 — Lower (new: `rosidl-lower`)
> **Amended by [RFC-0091](0091-one-entry-codegen-producer-many-language-packs.md)
> (2026-09): `TargetProfile` is retired, and this stage is target-AGNOSTIC.**
> The paragraph below is the original design, kept as written. What replaced
> each fact it names, why, and the commits are in **Amendment 1** at the end of
> this RFC. Storage, plainness and serialized size are unaffected: they are CDR
> and capacity-config facts, not target facts.

*[Amendment 1 — superseded: no `TargetProfile`; layout is the compiler's.]*
`ResolvedType ⊗ CodegenConfig ⊗ TargetProfile → LoweredType`. This is where the embedded facts
are baked: per-field storage from the `CapacityResolver`/`StorageMode` config; plainness;
alignment and `repr(C)` field order and serialized-size under a `TargetProfile`
(`{ ptr_width, enum_width / short_enums, alignment rules }`). Target-specific, so **hash once,
lower per target** — the arm-short-enums layout (project memory: repr(C) enums are 1 byte on
armv7a-nuttx, 4 on x86_64) is a lowering fact, never baked into the hash.

### Stage 3 — Render (data-driven, per language)
A **LanguagePack** is a directory of pure data:

```
packs/<language>/
  spelling.toml   neutral prim/container → language syntax + serialize op names + keyword-escape
  pack.toml       which template renders which output file, per kind (message/service/action)
  *.jinja         templates over the LoweredType context (minijinja — RUNTIME engine)
```

`render(LoweredType, pack) → files`. No Rust per language *[Amendment 2: a filter set
and a generator per kind are Rust]*. A **runtime** template engine
(`minijinja`) is required so a pack is *data*, not a compiled artifact: default packs are bundled
via `include_dir!` (fast, no I/O), and a `--template-dir` override loads a drop-in language with
**zero rebuild**. Templates never compute a layout fact — they read the facts Stage 2 already put
in `LoweredType`.

## What stays in Rust (and why that is correct)
All *computation*: dependency resolution, RIHS hashing, capacity resolution, target layout
(`repr(C)` order, alignment, serialized-size, plainness). Templates only *spell*.
*[Amendment 1: target layout — `repr(C)` order and alignment — is the compiler's, not
codegen's; serialized size and plainness stay here.]* This is the
load-bearing correction over a naïve "emit a JSON manifest like a hosted stack": a manifest that
models only the ROS type system (no storage/plainness/layout) is useless to a `no_std` /
32-bit / short-enum C or C++ emitter. **The IR content — the embedded facts — is what makes the
data-driven boundary safe here, not the serialization format.**

## Why four stages, not three or six
- Parse vs Resolve — resolve needs the whole package set (cross-refs, closure); parse is per-file.
- Resolve vs Lower — resolve is target-neutral (one hash for all targets); lower is target-specific
  (short-enum layout differs per arch). Merging them recomputes hashes per target or bakes
  host-64-bit layout into the hash. Keeping them apart gives **hash once, lower per target**.
  *[Amendment 1: Lower is no longer target-specific. The split stands on the other
  ground: Lower applies the build's `CodegenConfig` (capacity, storage mode), which
  the hash must not depend on.]*
- Lower vs Render — neutral facts vs language spelling. That split *is* the fix.

## Serialization
`ResolvedType` and `LoweredType` derive `serde`, but the default Rust path stays **in-process**
(no serialize→parse→render round-trip). The serialized form is materialized only when a non-Rust
generator or an external template pack wants it (`nros codegen export` verb), and is versioned
(a `schema_version` on the manifest) for compatibility. So the common case pays nothing; the
extension case is opt-in.

## Escape hatch
A language needing genuinely imperative emission (unusual move semantics, bespoke storage
branching) may still register a Rust backend over `LoweredType`. It is opt-in, not the default —
common languages ride the data path; only exotic ones pay Rust.

## Non-goals
- No change to generated-crate identity (RFC-0067 stands: `version = "0.0.0"`, ament → metadata).
- No new wire behavior, no CDR-format change; `cdr_op` ids are a neutral spelling of the existing
  serialization, not a new one.
- Not adding a language in this RFC — it makes adding one cheap.

## Open questions
1. **Template engine.** `minijinja` (runtime, packs are data — preferred) vs staying with askama
   (compile-time, a pack edit needs a rebuild). The per-language-burden goal argues for runtime;
   confirm perf against `rosidl-codegen/benches/generation_benchmark.rs`.
2. **Multi-target in one run.** Lower N times (cheap; resolve/hash shared) vs a target-conditional
   `LoweredType`. Default: lower per active target (the build already knows it).
   *[Amendment 1: dissolved — there is no target input to lower per.]*
3. **Pack distribution.** Bundled-only vs `--template-dir` external packs vs both. Both, with
   bundled as the default, is the working assumption.
4. **Fingerprint inputs.** `fingerprint.rs` must hash the pack files (a template edit ⇒ stale);
   confirm the toolchain-fingerprint corpus (RFC-0061) extends to pack contents.

## Migration
Staged, fingerprint-guarded, no big-bang — see **phase-335**. After the first backend is emitted
from a data pack with zero Rust, the per-language-burden claim is disproven; after all five, it
is gone. *[Amendment 2: all five render from packs; "zero Rust" did not hold.]*

## Amendments

### Amendment 1 (2026-09-06, RFC-0091 / phase-432 Track 1) — `TargetProfile` is retired

**What it was.** Stage 2 was specified as
`ResolvedType ⊗ CodegenConfig ⊗ TargetProfile → LoweredType`, the profile being
`{ ptr_width, enum_width / short_enums, alignment rules }`, so that `repr(C)`
field order, alignment and serialized size would be computed for the target
being built. What shipped (phase-335 W1.b(ii), `b20ed2edb`) was
`TargetProfile { ptr_width, enum_width }` plus a `LoweredType.target` field, and
"hash once, lower per target" rested on it.

**Why it went.** Measured before it was deleted (RFC-0091 §5):

- one consumer outside `rosidl-lower`, and it passed `TargetProfile::host()`
  unconditionally. No caller ever supplied an embedded profile, so every ARM
  build was lowered as the host, and nothing broke;
- `enum_width` was read nowhere. The hazard it existed for (short enums: a
  `repr(C)` enum is 1 byte on armv7a-nuttx and 4 on x86_64) cannot occur,
  because the C and Rust message packs emit no `enum`;
- `ptr_width` was read once, as a nested field's stand-in alignment. `align`
  feeds only `plain`, and a nested field is never plain, so the value changed
  no outcome;
- `LoweredType.target` was written and never read.

**Where each fact went.**

| Fact | Now |
| --- | --- |
| `ptr_width` (nested-field align) | `NESTED_ALIGN_STANDIN` in `rosidl-lower/src/lowered.rs`, a named constant whose doc says its value cannot matter. `a_nested_field_is_never_plain_so_its_align_cannot_matter` fails if that stops being true |
| `enum_width` / short enums | not needed: no pack emits an `enum`. If one ever does, its representation is PINNED in the source (`uint8_t` against `#[repr(u8)]`) |
| `repr(C)` field order, alignment | the COMPILER, per target. That the two `repr(C)` surfaces (`packs/rmw`, `packs/cpp`) agree with the C header is MEASURED by `just check repr-memory-agreement`, which cross-compiles both for a non-host target and compares symbol sizes |
| `LoweredType.target` | deleted |
| storage, plainness, serialized size | unchanged: CDR and capacity-config facts, never target facts |

The principle that replaces the profile: generated code is target-agnostic,
the compiler resolves the target, and a representation two languages share is
pinned in the source and gated by measurement. A profile is a model of the
toolchain, and a model can be wrong silently.

**Commits.** `1e2db5242` (W1.1, delete `TargetProfile`), `8773f03e4` (W1.2,
the memory-agreement gate), `e4487c0d1` (the decision in RFC-0091 §5 and the
note at Stage 2 above). Phase-432 Track 1.

**Superseded here**, each marked *[Amendment 1]* in place and otherwise left as
written: the Stage 2 line of the design diagram; Stage 2's
`⊗ TargetProfile → LoweredType` paragraph; "target layout" under "What stays in
Rust"; the Resolve-vs-Lower rationale under "Why four stages" (the split
stands, on the `CodegenConfig` ground); and open question 2, which dissolves.

### Amendment 2 (2026-09-11, RFC-0091 §9) — "no Rust per language" held for type spelling only

The Goal ("never a Rust package change"), Stage 3 ("No Rust per language") and
Migration ("zero Rust") describe the target, not the result. Measured after
phase-335 and phase-432, a message language's Rust is:

- a **filter set** for its type spellings (`rosidl_codegen::filters::FILTER_SETS`).
  This is Rust by design: a spelling is a correctness property, and RFC-0091 §6b
  keys the set by the pack that calls it;
- a **generator per kind** (`generator/{msg,srv,action,cpp}.rs`) that assembles
  the surface's context and names its files, and an arm in `nros generate`.

Entries, which this RFC did not scope, need more: each entry language has a
Rust emitter that builds its view (RFC-0091 §8). The Escape hatch above is
closer to how entries work today than Stage 3 is. No code changed with this
amendment; it corrects the claim. The procedure is in
`book/src/internals/codegen-packs.md`.
