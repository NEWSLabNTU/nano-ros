# Phase 474 — the entry emitters become data: `LoweredEntry` IS the context

**Status (2026-10-01). W1–W5 LANDED on `feat/data-driven-entry-emitters` (not yet merged); W6 (follow-ups) open.**
Implements RFC-0091 §6b's entry clause — "`LoweredEntry` must be the context,
not the seed for a per-surface projection" — which phase-432 named, phase-469
deferred ("Making an entry emitter data-driven … RFC-0091 §6b's blocker"), and
nothing owned.

**Prior:** phase-432 (archived — one entry producer, packs per language),
phase-453 (archived — the codegen campaign), phase-469 (message-pack discovery;
its *Deferred* section is this phase's entry), RFC-0068 Amendments 2–4 and its
declined `spelling.toml`, RFC-0091 §6b / §7 / §8 / §8b.

---

## Why

The campaign's aim, in the user's words: *every language goes through one
codegen path, and a new language can be added without a heavy Rust toolchain
change.* On the message side that is close to true (phase-469). On the entry
side it was not, and RFC-0091 §8 says so: a new entry language needed "an entry
emitter in Rust, `codegen/entry/emit_<lang>.rs`, that builds the pack's view of
the plan and renders it", plus a dispatch arm, plus a row in two authored
`include_str!` lists.

Measured before this phase (non-test lines):

| file | lines | what it did |
| --- | --- | --- |
| `emit_cpp.rs` | 797 (2,643 with tests) | 7 view structs, monitor slicing, probe tail, board class path, validation |
| `emit_c.rs` | 340 (969 with tests) | 4 view structs, runner lookup, admission, validation |
| `mod.rs` view helpers | ~330 | `ServicesView`, `DeclsView`, `QosRowView`, `TierView`, `SchedView`, `BootConfigView` + builders |
| `render.rs` / `pack.rs` authored lists | 11 + 4 rows | one `include_str!` per template / manifest |

Reading the two emitters side by side settles what §6b suspected: they are the
SAME projection written twice. Both walk the plan into per-tier setups with the
same node→tier filter and the same per-executor index (issue 1272), both build
the same tier rows, sched binds, boot config and services, and both refuse a
`time_slice_us` with the same sentence. What genuinely differs is small and
falls into three kinds — and only one of them is language.

## The target design

### What is DATA SHAPING, and moves into `LoweredEntry`

Everything a pack reads that is a FACT of the plan, computed once:

* per node — its resolved `name` and `namespace` (issue 1443's one derivation),
  the raw launch identity (issue 1456), its component KIND (`c` / `rust` /
  `rclcpp` / `configure`, RFC-0043/0044), its C++ class where it has one, and
  the params / remaps / QoS overrides it already carried;
* the image — board FAMILY and BOOT SHAPE (phase-432 W2.2), the family's C-ABI
  RUNNER names (`BoardFamily::c_abi_runners`, a C-ABI fact any C-ABI language
  calls), whether the runner takes the executor storage, the deduped component
  seams and headers;
* the executor layout — one of: a single executor (every node, its index is its
  position), sched contexts on it (`SchedView`, issue 1283), or one setup per
  tier with each node's index ON THAT TIER's executor (issue 1272) and the
  tier rows (issue 1172's group keys);
* services, the boot-config blob's five facts (issue 0794), the contract monitor
  tables sliced per executor (phase-462 W1), and the metadata-probe tail
  (phase-308).

`nros-entry-lower` owns the TYPES — it is the one crate the proc-macro can
afford (issue 0083), and the parity corpus already deserialises `LoweredEntry`
from it. Every new field is `#[serde(default)]`, so the Rust parity corpus is
unchanged. The BUILDER (`Plan` → `LoweredEntry`) stays in `nros-cli-core`,
because `Plan` does (RFC-0091 §7: "what IS shared is the OUTPUT type").

### What is LANGUAGE SPELLING, and stays in Rust as a filter

A spelling is a correctness property, and phase-469's declined-`spelling.toml`
argument holds here unchanged: a wrong spelling fails silently in generated
code, so it belongs where the compiler reaches it. Measured, the entry side has
exactly three:

| filter | registered for | why it is Rust |
| --- | --- | --- |
| `c_str` | C, C++ | string-literal escaping (RFC-0091 §8b defect 2) |
| `pkg_ident` | every pack | `sanitize_pkg` — "which identifier does a package become" has one answer (phase-432 W2.4) |
| `cpp_board_class` | C++ | `::nros::board::LinuxBoard` is C++'s spelling of a family (§8b defect 1) |

Each pack DECLARES the filters its templates call (`filters = [...]` in
`pack.toml`); the set is keyed by the pack that calls it, as RFC-0091 §6b
settled for the message side. A test holds both directions: every filter a pack
template calls is declared by that pack and registered, and every registered
filter is called by some pack.

Things that LOOKED like spelling and are not:

* the executor expression (`executor` vs `::nros::global_handle()`) — template
  text, so it moves INTO the template that writes it;
* the monitor table's symbol tag (`_t1`) and banner prose — the lowering carries
  the tier index and name, the template spells both;
* the boot shape's string — the serde repr of `BootShape`, already one
  derivation.

### What is a PACK REQUIREMENT, and becomes manifest data

What a pack can render is a fact about the pack, declared beside it and checked
by ONE generic admission step before lowering:

* `components = [...]` — the component kinds its templates know how to
  construct (C: `c`; C++: all four). The C pack's "not `c`" refusal is this,
  generated rather than authored.
* `c_abi_runners = true` — the pack calls the board's C-ABI runners, so it
  needs a family that exports `run_components` (and `run_tiers` for a tiered
  plan).

The ROUTING rule — a C entry on a family with no C runner renders through the
C++ pack — stays Rust (`entry_pack_for`), as phase-432 W3.2 argued. It now
reads the manifest's `c_abi_runners` rather than naming the C pack, and finds a
language's pack by the manifest that declares that language rather than by a
`match` table.

### The acceptance test for "a new entry language needs no Rust emitter"

1. **Emitter deletion.** `emit_c.rs` and `emit_cpp.rs` are DELETED. Both
   shipping C-family languages render through one generic function
   (`emit::emit`) from `LoweredEntry`, with every golden byte-identical and a
   510-render control diff (every golden plan x every entry point, with and
   without monitor rows, plus a namespaced/param/QoS variant of each) identical.
   The per-language Rust left is the filter registry rows above.
2. **A pack added purely as data renders.** A toy third pack (the Zig RFC-0091
   §8b used as its probe) lives under `testdata/entry-packs/`: a `pack.toml` and
   a template, no Rust. A unit test loads it through the same manifest type,
   admission and renderer the bundled packs use and renders the golden plans;
   its output is a golden. It is NOT compiled — there is no Zig toolchain on
   the host — and the report says so.
3. `check-entry-pack-conformance` covers the toy pack's root, and refuses an
   emitter: a `codegen/entry/emit_<x>.rs` for any pack other than the Rust
   parity renderer (RFC-0091 §7, which keeps that one by decision).

## Waves

### W1 — entry packs are discovered, not authored. **DONE (2026-10-01).**

`build_entry_packs.rs` generates the template and manifest registries from
every `packs/entry/<dir>/pack.toml`; manifests name `templates = [{ key, file }]`
rows. Compile-time refusal of an unclaimed `.jinja`, a missing file, a
duplicate key, a manifestless directory; the conformance gate reads the entry
root's rows as files in both directions. Negative control: an unclaimed
`c/stray.jinja` fails the gate and the build. Goldens byte-identical.

### W2 — `LoweredEntry` carries the image; one lowering builds it. **DONE.**

`nros-entry-lower/src/image.rs` declares the image types (`ComponentKind`,
`ComponentSeam`, `SetupNode`, `LoweredRunners`, `LoweredBoot`, `LoweredTiers`
/ `TierSetup` / `TierRow`, `LoweredSched`, `LoweredServices`,
`LoweredBootConfig`, `MonitorTable`, `LoweredProbe`); `LoweredNode` gained the
node's resolved name/namespace, raw launch identity, kind and class;
`LoweredEntry` gained the image. Every new field is `#[serde(default)]`, so the
parity corpus (read by the CLI AND the proc-macro) is untouched; the
proc-macro's one `LoweredNode` struct literal gained `..Default::default()`.

`codegen/entry/lower.rs` is the ONE builder: `lower_entry` (the Rust
producers' per-node bake, which `emit_rust::lower` now delegates to) and
`lower_image` (that plus the image, fallible). The six shared view structs in
`mod.rs` and both emitters' private views are gone into it.

`filters.rs` holds the three spellings (`c_str`, `pkg_ident`,
`cpp_board_class`); a pack declares the ones it calls, and a test holds both
directions (every called filter is declared and registered or a builtin; every
registered filter is called). `PackManifest` gained `context`, `filters`,
`components`, `c_abi_runners`, `metadata_probe`, `fixture`, and
`deny_unknown_fields` — a pack is data the renderer ACTS on now, so a misspelt
key must fail the parse rather than take a default.

### W3 + W4 — the C and C++ packs lose their emitters. **DONE.**

`emit_c.rs` and `emit_cpp.rs` are deleted. `emit/mod.rs` routes
(`entry_pack_for`, which now finds a language's pack by the manifest declaring
it and routes on the pack's own `c_abi_runners`), ADMITS from the manifest
(component kinds, C-ABI runner, probe tail), lowers, and renders. Both packs'
templates read `LoweredEntry` directly; the executor expression and the monitor
tag/banner moved into the templates that write them; the C pack's node body is
its own partial (one copy for both layouts). `cmd/codegen.rs`'s
`typed_entry_emitter` + emit `match` became `typed_entry_pack`, which reads the
pack's `context`. The emitters' 71 behavioural tests (23 + 48) moved unchanged to
`emit/tests_{c,cpp}.rs` behind one-line shims of the old entry points.

**Byte-identity, measured.** Every entry golden identical, `NROS_UPDATE_GOLDEN`
never set. A control beyond the goldens: every golden plan, plus a variant of
each with namespaces, empty launch identity, params/remaps/QoS on every node,
lifecycle, param services and session facts, rendered through all five entry
points (C, C++, C++ with monitor rows sliced per executor, the probe for a C++
and a C component) — 510 renders before and after: 450 byte-identical, and the
60 that differ are all the C pack's COMPONENT REFUSAL text, which is now
generated from the manifest (the phrase ``not `c` `` is kept; it names the component kind and
the packs that accept it). One error-ORDER change, not covered by any golden: a
C plan with both an over-long boot-config string and a `time_slice_us` tier now
reports the tier first, as the C++ pack always did.

**Line counts (non-test).**

| | before | after |
| --- | --- | --- |
| `emit_cpp.rs` | 797 | deleted |
| `emit_c.rs` | 340 | deleted |
| per-language emitter Rust, total | 1,137 | **0** |
| per-language spelling Rust (`filters.rs` rows) | `board_cpp_path` + the `c_str` closure, inside the emitters | 3 filters, 122 lines with the registry and the call scanner |
| shared: `mod.rs` | 1,485 | 1,009 (the six view structs left) |
| shared: `lower.rs` / `emit/mod.rs` / `image.rs` | — | 650 / 203 / 303 |

The shared code did not shrink in total — it was never the problem. What
changed is WHERE a language's cost lands: in a `pack.toml` and its templates,
not in Rust.

### W5 — a pack added as data, and the gate. **DONE.**

`testdata/entry-packs/zig/` — the language RFC-0091 §8b used as its probe — is
a manifest and one template. `emit/tests_data_pack.rs` loads it through the
same `PackManifest`, registers ONLY its declared filters, admits and lowers
through `admit_and_lower`, and renders every C golden plan (16) to
`testdata/entry/toy_*.zig.golden`. A second test drops a declared filter and
asserts the render fails `UnknownFilter`, so the declaration is load-bearing.
The outputs are NOT compiled — no Zig toolchain on the host — and the pack says
so. §8b's three defects are each absent from what it needed.

`check-entry-pack-conformance` gained the fixture root (manifest, files both
ways, `fixture = true`, no `language`, a golden for its extension) and refuses
any `codegen/entry/emit_<x>.rs` but the Rust parity renderer (RFC-0091 §7).
Negative controls: a stray `emit_zig.rs` reds it; so does the toy before its
goldens were recorded.

### W6 — follow-ups, not done here

* **Issue 1604** — the C pack never rendered the contract monitor rows. The
  lowering now hands them to every pack, so the fix is a C template block (no
  Rust); it is an output change and wants its own golden and a C fixture.
* The golden harness (`golden.rs`) still names its entry points in an
  `Emitter` enum. That is test code choosing WHICH entry point a row exercises,
  not a projection; it could become data (pack name + tail) when a third
  shipping pack exists to drive it.
* The Rust pack (`emit_rust.rs`) keeps its own view by decision (RFC-0091 §7):
  it is the parity rendering of the `nros::main!` proc-macro, and the proc-macro
  cannot afford minijinja (issue 0083). It is not a third emitter to retire.

## Acceptance

* [x] W1: no authored `include_str!` list for entry packs.
* [x] `emit_c.rs` and `emit_cpp.rs` deleted; every entry golden byte-identical
  with `NROS_UPDATE_GOLDEN` unset; the control diff identical but for the C
  refusal text (W3 + W4).
* [x] The per-language Rust is filter rows only; measured line counts above.
* [x] A toy pack renders from data alone, golden-locked; the conformance gate
  covers it and refuses a new `emit_<x>.rs` (W5).
* [x] `codegen_fingerprint` unchanged: it hashes `rosidl-codegen`'s emit corpus
  and bundled MESSAGE packs, and its whole crate closure (`rosidl-*`,
  `nros-lang`, `nros-core`, `nros-serdes`, from `cargo tree`) has no diff
  against `origin/main`. Argued from the closure, not from comparing two
  digests built side by side.
* [x] RFC-0091 Amendment 1; phase-469's deferred item points here.
