# Phase 469 — give the message path the entry path's `pack.toml`

**Status (2026-09-28). W1 LANDED; the later waves are still a record.** Opened from
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

## W1 — `pack.toml` on the message path. **DONE (2026-09-28).**

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

### What landed

`build.rs` reads every `packs/<dir>/pack.toml` and generates three things into
`OUT_DIR`: the template registry `render.rs` used to author as 28 `include_str!`
rows, the `Surface` enum plus its four lookups that `generator/naming.rs` used to
author as three `match`es, and a `PACK_INFO` table of what each manifest declares.
A build script rather than a run-time directory read, for the reason
`codegen/entry/pack.rs` already gives: `nros` runs from CMake and from build
scripts in trees that do not contain this checkout.

`packs/_codegen_version.jinja` moved to `packs/shared/`, so the message side has
the `shared` pack the entry side has. It was the one row discovery could not find,
and one authored `include_str!` is still an authored list. Bytes and registry key
unchanged, which is why the fingerprint did not move.

**Both traps held.**

1. `codegen_fingerprint` is unchanged —
   `a5b0c79b4cfc06a878a66b8cb85bd823cd7e0779cd313f09679892971cfa6346` before and
   after, 28 rows in the same order, measured by running the emitters in a
   throwaway test in both trees. That is what `registry_order` in each manifest
   buys: the fingerprint hashes `bundled_packs()` as a SEQUENCE, so deriving the
   order from directory order (`c, cpp, nros, rmw, rust, scaffold` alphabetically
   against the authored `c, rmw, nros, rust, cpp, scaffold`) would have re-staled
   the whole tree for byte-identical output. The numbers are the order the
   authored list had and carry no other meaning; the gate and the build script
   both refuse a duplicate.
2. `check-entry-pack-conformance` GREW a message root (`PackRoot`), and the two
   roots differ in what a manifest holds rather than being forced into one loop.
   The name still says `entry` deliberately: it is a name the growth-only
   `.config/gate-registry-baseline.txt` ratchet carries, and regenerating that is
   reserved for a RETIREMENT (issue 1071).

**What deliberately stayed in Rust**, and why:

* `Kind` — the ROS kind word (`msg` / `srv` / `action`) is identical on every
  surface and is not a pack author's choice, so a per-pack copy would be this
  phase's own defect one level down.
* which generator builds which context and calls which key. That is a rule with
  a reason, and RFC-0091 / `entry/pack.rs` draw the same line for routing.
* the filter sets (`crate::filters`). Unchanged by W1; RFC-0091 §6b already keys
  them by the pack that calls them.

**Measured.** `codegen_golden` 5/5, `rust_surface_golden` 2/2, lib 171/171,
`clippy --all-targets -D warnings` clean, `Cargo.lock` unchanged (`toml` was
already a normal dependency, and a lock entry is the union of dependency kinds).
The control over the uncovered surfaces — every artifact NAME plus the C++
srv/action bodies and the srv/action intra-package includes, over 4 packages x 5
type spellings (including `Weird_Pkg` and `camelCase`) x 4/3/3 msg/srv/action
shapes, 1,888 sections, zero generator errors — is **13,543,799 bytes
byte-identical**, same sha256. Discovery was proven by adding a throwaway
`packs/zz_probe/`: 28 -> 29 registry rows, it rendered, and it added a
`Surface::ZzProbe` variant, with no edit to `render.rs` or `naming.rs`. Both the
build script's refusals and the gate's new arm were mutation-tested rather than
assumed.

**One finding the probe produced**: a pack directory whose name has an underscore
generated a `non_camel_case_types` variant — a warning, i.e. an error under the
`-D warnings` every check lane runs. A new pack's directory name must not be able
to break a crate its author never opened, so `variant()` upper-camels per word.

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
  **Landed 2026-09-28 (`85bb4d62c`) — and the audit had named two of THREE.**
  `orchestration/metadata_refresh.rs`'s cmake metadata probe carried the same
  `_ => "cpp"`; S2's sweep found it and left it, because deciding what a Rust
  component sends a C/C++ probe looked like its own question. It was not:
  measured, the sole call site is guarded by `language != Rust`, so `Rust` is
  in the wildcard's type-level coverage and not in its reachable set. Closed
  as **issue 1528** — exhaustive match, `Rust` refused with its cause.
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

All four met by W1 (2026-09-28) — see *What landed* for the measurements.

* [x] `pack.toml` discovery replaces the authored `include_str!` list; a new pack
  directory is found without a `render.rs` edit.
* [x] Every golden byte-identical, `NROS_UPDATE_GOLDEN` never set; the uncovered
  surfaces checked by a control diff (13,543,799 bytes identical).
* [x] `codegen_fingerprint` unchanged by W1 — same digest, same 28 rows, same
  order. No re-stale was bought.
* [x] `check-entry-pack-conformance` covers the message side, in the same gate.
