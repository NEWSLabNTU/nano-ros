---
id: 402
title: "Message codegen has no language-neutral IR — parse/resolve/hash/sizing logic is entangled with per-language emission"
status: open
type: enhancement
area: codegen
related: [rfc-0023, issue-0378, phase-333]
---

## Idea

Introduce a **language-neutral intermediate representation** for messages/services/
actions: parse the ROS interface once, resolve the dependency graph once, compute
RIHS type hashes once, decide layout/sizing once — emit that as a self-contained
IR — and let each target backend (Rust / C / C++ / …) be a **thin template that
consumes the IR** rather than re-deriving any of it.

The IR is the single source of truth; a backend that wants to add a language should
only have to write templates over the IR, never re-implement hashing, dependency
resolution, or sizing.

## Where we are today

We already have the *inputs* of this idea, but not the boundary:

* `rosidl-parser` produces an in-process AST (`ast.rs`: `PrimitiveType`, `FieldType`,
  `Field`, `Message`) — a parsed IR, but Rust-in-process only, not a serializable
  artifact a non-Rust backend could consume.
* `rosidl-codegen` holds the resolver, `rihs.rs` (hashing), `idl_generator.rs`, and
  `templates.rs` — parsing/resolution/hashing/sizing and the per-language *emission*
  are entangled in one Rust crate. Each language path re-walks the same structures.

So the derived facts (type hash, dependency closure, storage/sizing mode, plainness)
live inside the emission pass instead of in a shared, inspectable IR. Adding or
changing a language means touching the entangled path; a second backend cannot reuse
the resolution/hashing without linking our Rust codegen.

## Why it's worth doing

* **One hasher / one resolver.** RIHS and dependency resolution computed exactly once;
  no risk of a second language computing a subtly different hash (the drift class this
  repo keeps closing — cf. the sizes-header mirror, issue-0378 identity work).
* **Backends become dumb fillers.** New language = templates over the IR. Lowers the
  cost of the C/C++/(future) story and keeps them byte-identical by construction.
* **Inspectable + testable.** A materialized IR can be golden-tested and diffed
  independently of any emitter.
* Naturally carries the **"resolve-only dependency packages"** idea: the IR records
  dep type-descriptions (for correct hashes) without emitting code for them — relevant
  to the `0.0.0` path-dep tension (issue-0378 / RFC-0067 / phase-333).

## Consequences / open questions (this is why it's an issue, not a patch)

1. **Serialization format is NOT decided — and need not be JSON.** A serialized neutral
   IR (JSON / MessagePack / a stable Rust-native encoding) buys language-agnostic
   consumers and golden tests, but costs a serialize→parse→render round-trip per build.
   The alternative is an in-process **trait boundary** (a stable `Ir` type the Rust
   backends consume directly, non-Rust backends reached via a thin export) — no I/O
   cost, but no non-Rust consumer without an export step. Decide per the actual
   consumer set; do not assume JSON.

2. **The IR must encode EMBEDDED constraints, or it is useless to us.** Unlike a
   hosted generator, our targets are `no_std` with fixed-capacity buffers. The IR has
   to carry: storage/sizing mode (bounded vs unbounded, fixed-capacity), alignment and
   **plainness** (is a struct a POD blit candidate — all fields plain AND single uniform
   alignment, else repr(C) padding forbids the fast path), `heapless` vs alloc choice,
   and per-target size classing. An IR that only models the ROS type system (like a
   hosted stack's) would drop exactly the facts our C/C++ embedded emitters need.

3. **Performance.** Codegen already runs in the build path (CONFIGURE_DEPENDS, stale
   probes). A heavier parse+serialize+render pipeline must not regress incremental
   build time; measure against `rosidl-codegen/benches/generation_benchmark.rs`.

4. **Target-platform compat.** The same IR feeds a host tool AND drives C/C++ that
   compiles for ARM short-enums / 32-bit targets. Layout facts in the IR must be
   target-parameterized (or target-free with the emitter applying target rules), never
   host-64-bit literals baked in (the `generated.rs` layout-literal footgun, RFC-0054).

## Direction (to be designed, not prescribed here)

Extract the parse→resolve→hash→size stages behind a neutral IR type with the embedded
fields above; make the Rust/C/C++ emitters consume the IR; decide the
serialized-vs-in-process boundary from the consumer set. Fold in resolve-only deps.
Likely an RFC (amending RFC-0023) before code, given the format + layout decisions.

## Not doing

No specific serialization format is chosen by this issue. No new language is added by
this issue — it only refactors the seam so adding one is cheap.
