# Phase 469 — give the message path the entry path's `pack.toml`

**Status (2026-09-27). NOT STARTED — a record, not work in flight.** Opened from
the 2026-09-27 codegen-path audit to carry its **S4** item, which is phase-sized
and was deliberately left out of the two conflict-free fixes that landed with it
(the one artifact-naming derivation, `generator::naming`, and three retracted doc
lines). Nothing here is measured beyond what that audit measured; the two
constraints in *Traps* are the reason this is a phase and not a patch.

## Why this phase exists

The audit asked the user's question — *is the codegen path unified across
languages, and can a language be added without heavy Rust work?* — and measured
**three producers**:

* **messages** via `rosidl-codegen` (the four-stage pipeline, RFC-0068);
* **C/C++ entries** via `codegen::entry`, which already discovers its packs;
* **the shipping Rust entry** via the `nros::main!` proc-macro — 4,397 lines of
  `quote!`, which can never be a pack, decided deliberately in RFC-0091 §7 and
  issue 0083.

Roughly **139 sites name a language across ~45 files**, and Rust is required to
add one. On the MESSAGE path the specific asymmetry is that the entry path has
`pack.toml` discovery and the message path does not: `render.rs` carries **26
authored `include_str!` rows**, and the artifact-naming parameters — the kind
word, the header extension, the guard suffix, the translation-unit extension —
are Rust rows in `generator::naming` rather than data. The entry path already
demonstrates the shape this wants, so this is convergence, not invention.

RFC-0068 Amendment 2 and RFC-0091 §6b are the honest statement of where the
"adding a language = dropping a pack" goal actually stands; read those before
planning against this phase.

## W1 — `pack.toml` on the message path

Give each `packs/<lang>/` a `pack.toml` the way an entry pack has one, so:

* `render.rs`'s 26 `include_str!` rows become DISCOVERY of the packs that exist,
  rather than a list a new language must be added to by hand;
* the artifact-naming parameters become pack DATA. `generator::naming`
  (landed 2026-09-27) already reduced six authored copies of "what is this
  artifact called?" to one derivation over a `Surface` x `Kind` table; W1 is the
  next step — the table's rows move into the packs, so a surface declares its
  own suffixes instead of adding a Rust variant.

**Acceptance is byte-identity of every golden**, exactly as the naming refactor's
was: `tests/codegen_golden.rs` + `tests/rust_surface_golden.rs` green with
`NROS_UPDATE_GOLDEN` unset, plus a control over what the goldens do not cover
(C++ srv/action, artifact NAMES, srv/action intra-package includes).

### Traps — both found by the audit, both about how this fails quietly

1. **`bundled_packs()` must keep returning `(name, content)`, so
   `codegen_fingerprint` is unchanged.** The fingerprint hashes that map
   (RFC-0061 / phase-335 W4.a) and every fixture in the tree is keyed on it, so
   moving the fingerprint **in the same commit as a refactor re-stales every
   fixture for a change that emits identical bytes** — a whole-tree rebuild
   bought with nothing. If discovery must change the map's shape, that is its own
   commit, with the re-stale stated.
2. **`check-entry-pack-conformance` is already the right shape and reads
   `Language` live — so it GROWS a message-side arm, it is not duplicated.** A
   second gate over the same rule is the "one fact, two authored spellings"
   defect this phase exists to remove, one level up; and a gate whose reach is
   narrower than its rule is the 0196 shape the tree has paid for repeatedly.

## Later waves — the audit's remaining items (its S2 / S3 / S5)

Recorded so the sequence is not rediscovered. Each was **owned by a concurrent
session on 2026-09-27**, which is why none of them landed with S1:

* **A language decided by a boolean or by a fallback.** `cmd/codegen.rs`'s
  `is_cpp = lang != "c"` makes every non-C language C++; `orchestration/
  workspace.rs`'s `_ => "cpp"` does the same in a match arm. Reason: a third
  language is silently mis-rendered rather than refused, so the failure lands in
  generated code instead of at the decision. (Issue 1426 territory.)
* **Language inferred from a file EXTENSION, in three cmake sites.** Reason: the
  extension answers a different question than the language does, and the
  inference has no single producer.
* **The entry emitters.** The Rust entry's proc-macro stays Rust by decision
  (RFC-0091 §7 / issue 0083), so "a language adds no Rust" can never hold for
  entries; what is in scope is that each entry language's emitter be the ONLY
  Rust it needs, and that the ~139 language-naming sites route through `Language`
  rather than through string literals.

## Deliberately NOT proposed: the filter set as data

RFC-0068 Stage 3's `spelling.toml` was considered and **declined**, and this
phase does not revive it. Two reasons: one spelling function takes **eight
inputs**, so the table would be a small language rather than a config file; and a
wrong spelling **fails silently in generated code** — it compiles somewhere else,
later, as a type error with no path back to the row that produced it. A
correctness property belongs in Rust where the compiler reaches it. RFC-0091 §6b
keys the filter set by the pack that calls it, which is the shape that survived.

## Acceptance

* `pack.toml` discovery replaces the authored `include_str!` list; a new pack
  directory is found without a `render.rs` edit.
* Every golden byte-identical, `NROS_UPDATE_GOLDEN` never set; the uncovered
  surfaces checked by a control diff.
* `codegen_fingerprint` unchanged by W1 (or moved in its own commit, with the
  whole-tree re-stale stated up front).
* `check-entry-pack-conformance` covers the message side, in the same gate.
