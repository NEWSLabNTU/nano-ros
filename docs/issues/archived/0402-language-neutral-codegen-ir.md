---
id: 402
title: "Message codegen has no language-neutral IR — parse/resolve/hash/sizing logic is entangled with per-language emission"
status: resolved
type: enhancement
area: codegen
related: [rfc-0068, phase-335, rfc-0023, issue-0378, phase-333]
resolved_in: phase-335 (RFC-0068)
---

> **Designed in [RFC-0068](../../design/0068-language-neutral-codegen-ir.md) (Stable); implemented
> in waves by [phase-335](../../roadmap/phase-335-codegen-ir-refactor.md).**

## Problem (as filed)

Parse / dependency-resolution / RIHS hashing / sizing were entangled with per-language emission
in `rosidl-codegen`: each language re-walked the same structures, and derived facts (hash,
plainness, storage mode) lived in the emission pass instead of a shared, inspectable IR. A second
backend could not reuse resolution/hashing without linking our Rust codegen.

## Resolution — the four-stage pipeline (RFC-0068), landed by phase-335

**parse → resolve → lower → render**, byte-identical-output-preserving until the final wave:

- **Resolve** (`rosidl-resolve`): `Resolved{Message,Service,Action}` carries the fully-qualified
  name, the canonical REP-2011 type-description closure, and the RIHS01 hash — computed once, via a
  single hasher/resolver. Cross-package deps reach the DAG only through a caller closure and are
  **resolve-only** (hashed, never emitted); a structurally-embedded dep becomes its own `0.0.0`
  path crate per RFC-0067/phase-333 (issue-0378 settled the identity axis, not re-opened here).
- **Lower** (`rosidl-lower`): `LoweredType = ResolvedType ⊗ CodegenConfig ⊗ TargetProfile` — the
  embedded, target-parameterized constraints our `no_std` C/C++ emitters need (storage/sizing mode,
  alignment/plainness, `heapless` vs alloc), never host-64-bit literals baked in.
- **Render**: every backend (C / RMW-Rust / idiomatic-Rust / nros / C++ + scaffolding) renders from
  runtime `minijinja` **data packs** (`packs/{c,rmw,rust,nros,cpp,scaffold}/`). No backend spells a
  type itself — the type strings are composed in the pack by registered spelling filters. Askama is
  gone. Adding a language is dropping a pack (+ a spelling filter if needed), no Rust rewrite:
  external packs load via `NROS_TEMPLATE_DIR` / `set_template_dir` with no rebuild, the fingerprint
  hashes bundled pack content, a smoke test proves override+fallback. See
  `book/src/internals/codegen-packs.md`.

**Consumer-set decision (open question #1):** the boundary is an **in-process trait boundary**
(the `Resolved*`/`Lowered*` IR types the Rust backends consume directly), not a serialized JSON-IR
— no serialize→parse→render round-trip in the build path. A non-Rust consumer would be reached via
a thin export, not adopted speculatively.

**Sole residual:** the Rust-side `SequenceStructDef` element repr (`elem_repr_c`) is still spelled
in the builder — it feeds a Rust mirror struct, not a language-selectable output; documented in
`generator/common.rs`.
