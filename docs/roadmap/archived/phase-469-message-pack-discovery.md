# Phase 469 — give the message path the entry path's `pack.toml`

**Status (2026-10-01). COMPLETE — archived. W1, S2, S3, W2 and W3 landed; W3
closed the typing half (every language DECISION in the CLI is typed, and
`check-cli-language-literals` refuses a new string-typed one). The remaining
entry-emitter work — making an emitter data-driven — was never this phase;
RFC-0091 §6b names its blocker.** Opened from
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
  rather than through string literals. **The typing half is W2, below. The
  data-driven half is NOT this phase** — `emit_cpp.rs` is 2,631 lines of
  projection and RFC-0091 §6b names the blocker (`LoweredEntry` must BE the
  context, not the seed for a per-surface projection).

## W2 — a language is a TYPE inside the CLI, not a string. **DONE (2026-09-28).**

The typing half of the entry-emitter item above, plus the two sites S2 measured
and left. **Typing, not templating**: nothing about which Rust an emitter needs
changed, and every golden is byte-identical.

### What the sites actually were

`PlanNode.lang` was `Option<String>`, and FOUR consumers asked
`lang.as_deref() == Some("c")` — `emit_c::is_c_node`, `emit_cpp::is_c_node`,
`emit_cpp::is_rust_node`, `metadata::enrich_plan`. That is the same defect S2
fixed one verb over (`is_cpp = lang != "c"`, PR #1363) still live in the FIELD
those inputs flow into: a string comparison is invisible to the compiler, so a
fourth language falls off it silently and lands in generated code.

Typed, each with the reason it had been a string:

| was | now | why it had been a string |
| --- | --- | --- |
| `PlanNode.lang: Option<String>` | `Option<Language>` | nothing; four `==` sites read it |
| `ComponentMeta.lang` (serde) | `Option<Language>` | a wire format — but `Language`'s serde repr IS that format |
| `ComponentFacts.lang` | `Option<Language>` | carried the serde field |
| `RegisteredNode.language: String` | `Language` | its caller wrote `language.as_str().to_string()` off an already-parsed `Language`, so an alias could not reach a downstream compare |
| `ProbeExport.language: String` | `Language` | the sidecar's rendered value; `as_str()` is now its one producer |
| `CmakeEntry.lang: String` | `Language` | a rendered cmake property (`LANG {}`); `as_str()` at the emit site |
| `ScaffoldConfig.lang`, `ComponentScaffoldConfig.lang`, `WorkspaceScaffold.lang` | `Language` | three `match … as_str()` dispatches, two with UNREACHABLE `bail!("Unknown language")` arms |

Left a string on purpose: `CmakeProbeOptions.language`, because its only
producer is `metadata_refresh`'s `_ => "cpp"` wildcard (**issue 1528**, a
concurrent change). `metadata_probe_cmake` gains ONE `Language::parse` at its
own edge instead; when 1528 lands, that parse becomes the field's type.
*(Done in W3, below.)*

### The second enum: KEPT, and given the derivation it lacked

`sizing_descriptor::EntryLanguage { Rust, CFamily }` is a genuine NARROWING in
RFC-0091 §1's sense — two values where the enumeration has three, because C and
C++ give the SAME answer to "which registration path" and saying so is
information. Collapsing it would make `registration_path`'s table spell
`(Language::C, _)` and `(Language::Cpp, _)` as two arms that must agree, with
nothing stating that they must.

What it was missing is the DERIVATION `nros-lang`'s own docs ask for: it had no
relationship to `Language` at all. `From<Language>` is that relationship, and it
is what makes a fourth language a compile error there — whoever adds one must
answer "registers like Rust, or like C?".

### The second cmake parser: TAKEN, with the token sets UNCHANGED

`orchestration::workspace::infer_cmake_language` read the cmake `LANGUAGE` token
with its own `match`. The two token sets are genuinely different and both are
user-facing: `Language::parse` serves a CLI flag and takes `c++`; the cmake
keyword set is whatever `cmake/NanoRosNodeRegister.cmake`'s validator accepts,
which is `C`/`CPP`/`CXX`/`RUST`/`RS` after an uppercase — `C++` is a
`FATAL_ERROR` there. **Unifying them would change what a user may type in both
directions** (`--lang rs` newly legal; `LANGUAGE C++` newly accepted by one
reader while cmake still refuses it), and nothing asks for either.

So the TABLE moved to `nros-lang` as `Language::parse_cmake_keyword`, beside the
one it differs from, with tests pinning the difference both ways; the POLICY —
what an absent or unrecognised keyword earns, and how loudly (issue 0641) —
stayed in `workspace.rs`, because that is a statement about CMakeLists written
before the keyword existed, not about the language table. **Nothing changes for
a user.** The one visible move is the warning text, which now names `rs`, a
spelling the old match already accepted and the old message already omitted.

`nros new --lang` is the mirror case and resolves the other way: clap already
restricts it with `value_parser = ["rust", "c", "cpp"]`, so the three copies
behind that gate could never see anything else, and two of them carried
refusals that could never fire. Parsing once there widens nothing.

### Measured

**Goldens byte-identical, `NROS_UPDATE_GOLDEN` never set.**
`codegen::entry::golden` 3/3, `rosidl-codegen` `codegen_golden` 5/5 +
`rust_surface_golden` 2/2, `nros-cli-core` lib 1446/1446, `nros-lang` 14/14,
`cargo clippy --workspace --all-targets -D warnings` clean, and `git status`
reports no change under any `goldens/` tree.

**Compiler enforcement, by throwaway variant.** A fourth `Language` variant,
`cargo check --workspace --all-targets --keep-going`, iterating until the
workspace built (a failed crate hides its dependents, so one round is not the
answer). Sites that became a compile error: **5 before, 9 after.**

* before (5): `cmd/codegen.rs:451`, `cmd/codegen.rs:528`, `cmd/codegen.rs:639`,
  `codegen/entry/pack.rs:141`, `orchestration/workspace.rs:1496`.
* after (+4): the three scaffolder dispatches
  (`cargo-nano-ros/src/scaffold.rs` ×2, `workspace_scaffold.rs`) and
  `sizing_descriptor.rs`'s `From<Language>`. `cargo-nano-ros` went from 0
  enforced sites to 3.

**What did NOT appear, and why — because the honest answer is not "everything".**
The typed `PlanNode.lang` comparisons (`is_c_node`, `is_rust_node`,
`enrich_plan`'s `is_c`) are `==`, not matches, so a new variant does not break
them. That is CORRECT: they are PREDICATES, and "is this C?" has a right answer
for a new language (no). The DISPATCHES that must change are downstream —
`typed_entry_emitter`, `entry-node`'s `is_cpp`, `pack::entry_pack_for` — and all
three appear. Typing those fields buys a different thing: the comparison is
type-checked (a typo'd spelling cannot compile) and the serde field REFUSES an
unknown language at the file that names it rather than routing it to C++.

**Site count — the "~139" becomes a number.** Reproducible:
`git grep -hE '(lang|language|Lang)' <rev> -- 'packages/cli/*/src/*.rs' |
grep -cE '"(c|cpp|c\+\+|cxx|rust|rs)"'` — a language literal on a line that
names a language.

* **124 → 88** over the whole CLI source.
* Inside `nros-lang`, where the strings BELONG: 21 → 35 (the cmake keyword
  table and its tests moved in).
* **Outside `nros-lang`, i.e. the drift surface: 103 → 53**, a 49 % cut.
* Bare literal count (`"c"`/`"cpp"`/`"c++"`/`"cxx"`/`"rust"`/`"rs"`, any
  context): 246 → 186 over 44 → 40 files. Four files now carry none:
  `builder/cmake_root.rs`, `cmd/build.rs`, `codegen/entry/emit_c.rs`,
  `codegen/entry/registered_node.rs`.

One count went UP by design: `metadata_probe_cmake.rs` 5 → 6, the single
documented `Language::parse` standing in for issue 1528's field.

### What remained after W2 — 53 lines outside `nros-lang` (as recorded then)

Enumerated so the next reader does not re-triage them:

* **`orchestration/metadata_refresh.rs`** — issue 1528's `_ => "cpp"`, and the
  `CmakeProbeOptions.language: String` it feeds. Owned by a concurrent change;
  this wave deliberately did not touch the file.
* **`codegen/entry/pack.rs`** (13) — pack DIRECTORY names, not languages. Its
  first arm routes a `C` component on a board with no C `run_components` to the
  `cpp` pack, so the table coincides with `as_str()` rather than being it.
  Already enum-typed and already enforced.
* **`orchestration/planner.rs`** (18), `cmd/setup*.rs` (12),
  `leaf_entity_env.rs` (7) — mostly NOT languages: toolchain names, file
  extensions, feature strings that the grep's `lang` context catches by
  proximity. A real count here needs reading, not grepping.
* **`cmd/codegen_system.rs`** — `ComponentLang { Rust, Other }` serialised as
  `"rust"` / `"other"`. `"other"` is not a language and `Language` cannot
  produce it; this is a binary predicate with a serde contract, correctly out
  of scope.
* **template bodies** — generated file comments (`// Generated by \`nros new
  <n> --lang cpp\``), `language = "rust"` inside an emitted `nros.toml`. These
  are OUTPUT text, and `as_str()` is the wrong producer for a literal inside a
  template string.

### Deferred at W2, with the reason (all three resolved by W3)

* **Making an entry emitter data-driven.** Out of scope by the doc's own
  statement and RFC-0091 §6b's blocker; see the bullet above. **Done by
  phase-474 (2026-10-01):** `LoweredEntry` is the template context, `emit_c.rs`
  and `emit_cpp.rs` are deleted, and an entry language is a `pack.toml`, its
  templates and — only for a new spelling — a filter (RFC-0091 Amendment 1).
* **Typing `CmakeProbeOptions.language`.** Blocked on issue 1528's owner.
* **A GATE for this rule.** Considered and not written: the honest predicate is
  "a language literal in a decision position", and the 53 remaining lines show
  the grep cannot separate a decision from a toolchain name or a template body
  — a gate over that would be an allowlist, which is what the 2026-09-11 audit
  warned about. The compiler is the enforcement here, and the throwaway-variant
  probe is how it gets measured.

## W3 — the last string-typed decisions, read rather than grepped. **DONE (2026-10-01).**

Closes W2's three deferrals: `CmakeProbeOptions.language` is typed, every remaining site was
READ and classified, and the gate W2 declined is written — because the predicate changed, not
because the earlier argument was wrong. Goldens byte-identical, `NROS_UPDATE_GOLDEN` never set.

### Every site READ, not grepped

The 53-line count above is `git grep -hE '(lang|language|Lang)' -- 'packages/cli/*/src/*.rs' |
grep -cE '"(c|cpp|c\+\+|cxx|rust|rs)"'` outside `nros-lang`. The per-file numbers
the W2 section above quotes (planner 18, `cmd/setup*.rs` 12, `leaf_entity_env.rs` 7, `pack.rs` 13) were the
BARE-literal count (any context, 138 lines over 39 files), not the 53 — two different greps, and
neither finds a decision by itself. W3 read both lists, plus every extension comparison and every
string-typed language field, and found **two decision sites neither grep could see at all**:
`language_from_sources` and `cmd/build.rs`'s `has_cpp` spell the language as a DOTTED extension
(`".cpp"`), which matches no bare-spelling pattern.

| site | what it is | verdict |
| --- | --- | --- |
| `metadata_probe_cmake.rs` `CmakeProbeOptions.language: String` + its `Language::parse` | the last string-typed language field; its producer's wildcard was issue 1528 | **TYPED** — `Language`; `probe_language()` is the ONE exhaustive serve-or-refuse match (C/Cpp served, Rust refused naming the cargo harness), asked by the producer (`metadata_refresh::cpp_probe_options`) AND the emitter edge (`render_probe_main`) |
| `cmd/codegen.rs` `match args.language.as_str() { "c" => …, "cpp" => …, other => bail }` | the `--args-file` message verb's dispatch, a string match | **TYPED** — parse at the edge, exhaustive `match Language`, Rust refused naming `nros generate-rust`. Behaviour change: `--language c++`/`cxx` now accepted (`Language::parse`'s table); the only caller, `NanoRosGenerateInterfaces.cmake`, passes `c`/`cpp` |
| `cmd/generate_px4.rs` `match args.lang.as_str() { "rust" => …, "cpp" => …, other => bail }` | `generate-px4-msgs --lang` dispatch | **TYPED** — same shape; C refused by name (PX4 has a C++ module surface and a Rust XRCE crate, no C one) |
| `orchestration/planner.rs` `.unwrap_or("rust")` | `schema_components` FABRICATED a language for an artifact that stated none, and copied an unknown spelling into the plan verbatim | **TYPED + REFUSED** — read through `Language`'s serde; absent or unknown is an error naming the file and the component (`schema_components`/`schema_plan_json` now return `Result`). `PlanComponent.language` is `Language` too. Every producer states it (`SourceMetadata.language` is required; the cargo synthesis writes `ComponentLanguage::Rust`), and the full lib + integration suite found no artifact relying on the default |
| `orchestration/workspace.rs` `summary_to_synthetic_json`'s `"language": "rust"` | a PRODUCER of the fact "every cargo-resident component is Rust" | **TYPED** — `ComponentLanguage::Rust` through serde (same bytes) |
| `orchestration/workspace.rs` `language_from_sources` | the FOURTH extension→language table (S3 collapsed three cmake ones into `Language::of_sources` and left this) — it disagreed: no `.c++`, no `.rs` | **ROUTED** through `Language::of_sources`; a token the table cannot read (`${var}`, the text scanner's normal case) is dropped, not refused — cmake refuses a real unknown spelling at configure over EXPANDED paths. Rust-beside-C now warns with the table's own reason. New test: a `.c++` source reads as C++ (it read as the class guess) |
| `cmd/build.rs` `has_cpp` (`ends_with(".cpp") \|\| ".cc" \|\| ".cxx"`) | the fifth table; no `.C`, no `.c++` | **ROUTED** through `Language::of_source` |
| `cargo-nano-ros/src/lib.rs` `extension() == Some("rs")` | "which files of a generated tree are Rust sources" — the `of_source` question | **ROUTED** through `Language::of_source` (same answer: only `rs` decides Rust) |
| `cmd/generate.rs` `Lang { Rust, C, Cpp, All }` dispatch | a CLI enum that RE-SPELLS the enumeration (RFC-0091 §3 said `All` "becomes a CLI affordance over `Language`" — it had not) | **DERIVED** — `Lang::languages()` maps to `Language` (`All` = `Language::ALL`) and the work is an exhaustive `match Language`; same order (Rust, then C), same refusal of an explicit `cpp`, same silent skip of C++ under `all` |
| `codegen/entry/pack.rs` (13 bare) | `entry_pack_for`'s RHS = pack DIRECTORY names; the rest are tests/doc | **LEAVE** — already an exhaustive typed match; the strings are outputs (a C component routes to the `cpp` directory on a board with no C runner, so the table is not `as_str()`) — no hunk in this file |
| `codegen/entry/emit_cpp.rs` `node_shape` → `"c"`/`"rust"` | construction-SHAPE tokens for the template view, derived from typed predicates | **LEAVE** (outputs; concurrent emitter work owns the file) |
| `codegen/entry/golden.rs` `Emitter::C => "c"`, `Rust => "rs"` | golden FILE extensions | **LEAVE** (output text) |
| `cmd/codegen_system.rs` `ComponentLang::Rust => "rust"` / `"other"` | a binary predicate (cargo member or not) with a serde contract | **LEAVE** — not a language; `Language` cannot produce `"other"` |
| `cmd/ws.rs` `(false, true) => "rust"` | a package-KIND display label in `ws list` | **LEAVE** (output text) |
| `cmd/setup.rs` `"rust"` ×4, `modules/lang/rust` | the Rust TOOLCHAIN's report row; Zephyr's module directory | **LEAVE** (toolchain name / path) |
| `cargo-nano-ros` `ament_installer.rs` / `package_discovery.rs` `share/<pkg>/rust` | an ament install-tree directory | **LEAVE** (path) |
| `cmd/codegen_cyclonedds_descriptors.rs` `["-t","-l","c"]` | `idlc`'s own flag | **LEAVE** (tool argument) |
| `rosidl-codegen/src/filters.rs` `name: "c"` | a filter set's NAME; its key is the typed `language: Some(Language::C)` | **LEAVE** (label) |
| `rosidl-codegen/build.rs` pack `language` / `PackInfo.language: Option<&str>` | a pack MANIFEST field, read for presence only | **LEAVE** — no decision reads the spelling; noted in case one ever does |
| `cargo-nano-ros/src/scaffold.rs` `language = "rust"`, `cmd/new.rs` comments, template bodies | OUTPUT text inside emitted files, and prose | **LEAVE** — `as_str()` is the wrong producer for a literal inside a template |
| `source_stamp.rs` `ends_with(".rs")` | which of the CLI's OWN files cargo compiles; runs inside a build script that cannot reach `nros-lang` | **LEAVE** — a file type, not a choice among languages |
| the remaining ~100 bare literals | test fixtures (`"language": "rust"` JSON/TOML rows, ~40), `Lang::parse("c")` tests, `"c"` as an arbitrary component/package/tool NAME in tests (`setup/session.rs` ×6, `leaf_entity_env.rs` ×7, `entity_inventory.rs`, `model_gate.rs`, …), doc comments quoting the old defects | **LEAVE** — none is in a decision position |

### The gate: written this time, because the predicate changed

W2 declined a gate over "a language literal near the word `lang`" — that predicate matched the
53 lines above and could not separate a decision from a fixture, so it would have been an
allowlist. With the sites READ, the decisions turn out to share a POSITION, not a neighbourhood:
a spelling is a decision only as a `match`/`matches!` PATTERN, an `==`/`!=` operand (bare or in
`Some(…)`), or the argument of a comparing call (`unwrap_or`, `eq_ignore_ascii_case`,
`ends_with`, `starts_with`, `strip_*`). Every LEAVE row above is a spelling on the right of an
arm, in a fixture, in a path or in prose — none is in any of those positions.

**`check-cli-language-literals`** (`scripts/check-cli-language-literals.py`, fast lane). Scope:
`packages/cli/*/src/**/*.rs` minus `nros-lang`, comments stripped by the shared
`scripts/lib/comments.py`. Spellings are HARVESTED from `nros-lang/src/lib.rs` (7 today: `c`,
`cc`, `c++`, `cpp`, `cxx`, `rs`, `rust`), plus their DOTTED forms for the C-family entries only —
a property of the predicate, argued in the script header, not a site list: the CLI has no C/C++
source of its own, so a `".cpp"` comparison in it can only be about a user's source, while a
`".rs"` one is the CLI asking about its own crate. **Zero exemption entries.**

Measured, not asserted:

* **This tree: 0 hits** over 226 files.
* **`origin/main` (`d19d065ae`): 9 hits — exactly the six sites W3 converted** (`codegen.rs` ×2,
  `generate_px4.rs` ×2, `planner.rs`, `workspace.rs` ×2, `build.rs`, `cargo-nano-ros/lib.rs`),
  and no other line. No false positive.
* **Before S2 (`85bb4d62c^`): 30 lines** — every site S2, S3, W2 and W3 typed (`is_cpp = lang !=
  "c"`, the four `lang.as_deref() == Some("c")`, the scaffolders' `match … as_str()`,
  `workspace.rs`'s `if lang == "c"` guards, …). The rule would have refused each as it landed.
* Self-test on every run: 10 decision shapes must flag, 7 non-decision shapes (a producer arm, a
  JSON and a TOML fixture, a path, a comment, the CLI's own `.rs` filter, `||`) must not; plus a
  MUTATION of real code — `cmd/codegen.rs`'s `Language::C =>` arm rewritten to `"c" =>` in memory
  must flag — and a reach check that the scope still contains the six files that held sites.

What it cannot see, stated: a decision made by a non-literal string (`lang == other_var`), and a
WILDCARD (`_ => "cpp"`), which is a producer-side literal. The first has no instance in the tree;
the second is exactly what typing removes, because a typed `match` has no wildcard to write.

### The compiler half, measured by throwaway variant

Same method as W2: a throwaway fourth variant (`Zig`, with its arms inside `nros-lang`),
`cargo check --workspace --all-targets --keep-going`, iterating with `todo!()` arms until the
workspace built (a failed crate hides its dependents — `cargo-nano-ros` hides `nros-cli-core`).

* **`origin/main` (`d19d065ae`): 9** — `cargo-nano-ros` scaffold ×2 + workspace scaffold;
  `nros-cli-core` `cmd/codegen.rs` ×3 (`typed_entry_emitter`, the emit dispatch, `entry-node`'s
  `is_cpp`), `pack.rs::entry_pack_for`, `metadata_refresh`'s issue-1528 match,
  `sizing_descriptor`'s `From<Language>`.
* **This branch: 12** — the same nine (1528's match MOVED into `probe_language`, still one site),
  plus `cmd/codegen.rs`'s args-file dispatch, `generate_px4`, and `generate.rs`'s
  `Lang::languages` dispatch. Once `nros-cli-core` built, the rest of the workspace (`nros-build`,
  `nros-cli`, …) built with no further error.

The conversions that do NOT show up here are the right ones not to: `schema_components` PARSES (a
fourth variant is a valid value there, not a missing arm), and the three extension sites ASK
`of_source`/`of_sources`, whose own match is inside `nros-lang`. Those are guarded by the gate
above and by the tests that landed with them, not by exhaustiveness.

The compiler half and the gate half together are the claim: every language DECISION in the CLI
is either an exhaustive `match` over `Language` (12, each a compile error for a new variant) or a
question put to `nros-lang` (`parse`, `of_source(s)`, serde) — and the gate refuses the third
shape, a string compared in place, with no list of exceptions.

### Measured

* `nros-cli-core` lib **1483/1483** (1481 before + 2 new: the planner refusal and the one
  extension table); every `nros-cli-core` integration target green — 13 of them first
  needed `just setup-launch-resolve` in this worktree, an unmet precondition and not a code red.
* `nros-lang`, `nros-build`, `rosidl-codegen` (incl. `codegen_golden`, `rust_surface_golden`)
  green; entry goldens (`codegen::entry::golden`) green; `git status` shows no `goldens/` change.
* `codegen_fingerprint` **unchanged**: `a5b0c79b4cfc…6346` from this branch's
  `nros codegen-fingerprint`, and its inputs (`rosidl-codegen`, `-lower`, `-parser`) differ from
  `origin/main` by zero lines — so the same digest by construction. Not measured: a main-built
  binary's digest side by side. W3 touches no pack, template or emitter.
* `check-cli-language-literals`, `check-gate-lists`, `check-gate-selftests` green.

## Deliberately NOT proposed: the filter set as data

RFC-0068 Stage 3's `spelling.toml` was considered and **declined**, and this
phase does not revive it. Two reasons: one spelling function takes **eight
inputs**, so the table would be a small language rather than a config file; and a
wrong spelling **fails silently in generated code** — it compiles somewhere else,
later, as a type error with no path back to the row that produced it. A
correctness property belongs in Rust where the compiler reaches it. RFC-0091 §6b
keys the filter set by the pack that calls it, which is the shape that survived.

## Acceptance

All four met by W1 (2026-09-28) — see *What landed* for the measurements. W2's
and W3's own acceptance is in their *Measured* sections above.

* [x] `pack.toml` discovery replaces the authored `include_str!` list; a new pack
  directory is found without a `render.rs` edit.
* [x] Every golden byte-identical, `NROS_UPDATE_GOLDEN` never set; the uncovered
  surfaces checked by a control diff (13,543,799 bytes identical).
* [x] `codegen_fingerprint` unchanged by W1 — same digest, same 28 rows, same
  order. No re-stale was bought.
* [x] `check-entry-pack-conformance` covers the message side, in the same gate.
* [x] (W3) Every language DECISION in the CLI is an exhaustive `match` over
  `Language` or a question put to `nros-lang`; the classification table above
  covers every remaining literal; `check-cli-language-literals` holds the rule
  with zero exemptions and runs its own negative control.
