# Codegen — the pack pipeline

nano-ros generates ROS 2 message/service/action types (and their per-package
scaffolding) from `.msg`/`.srv`/`.action` files. The generator is a four-stage
pipeline (RFC-0068): **parse → resolve → lower → render**. This page covers the
last stage — **render** — and how to change or add a target language.

## Render = a data pack + a runtime template

Every backend renders through one `minijinja` environment
(`packages/cli/rosidl-codegen/src/render.rs`) over **data packs** under
`packages/cli/rosidl-codegen/packs/`:

| pack | output |
| --- | --- |
| `packs/c/` | C headers + sources |
| `packs/rmw/` | RRR-compatible Rust message layer |
| `packs/rust/` | idiomatic (rclrs-style) Rust |
| `packs/nros/` | embedded (`no_std`) Rust |
| `packs/cpp/` | C++ headers + the Rust FFI glue |
| `packs/scaffold/` | per-package `Cargo.toml` / `lib.rs` / `build.rs` |
| `packs/shared/` | partials more than one pack includes (the codegen-version stamp) |

Each pack directory carries a **`pack.toml`** that declares it: its language, its
`registry_order`, its templates as `{ key, file }` rows, and — for a pack that
names C-family artifacts — the `header_extension` / `guard_suffix` /
`source_extension` that `generator::naming` derives every artifact name from.
`build.rs` reads every manifest and generates the template registry and the
`Surface` enum, so **a pack directory is a pack because it exists and describes
itself** (phase-469 W1). Nothing in `render.rs` names a template.

A pack is just `.jinja` templates. The Rust side hands each template a
**`serde`-serialized view struct** (the render context) and never spells a type
itself — the type strings are composed **in the pack** by registered filters:

- `c_type` / `c_array_suffix`, `cpp_type` / `cpp_array_suffix`,
  `cpp_repr_c_type` / `cpp_view_repr_type`
- `rust_type_rmw` / `rust_type_idiomatic`, `nros_type`
- `snake_case`

The view struct carries only **neutral facts** (the parsed `field_type`, resolved
capacity, storage mode, `current_package`, …); the filter maps those to the
language's syntax. This is RFC-0068's "what vs how" seam: *what* a type is lives in
the IR; *how* a language spells it lives in the pack + its filter.

## Changing a template

Edit the `.jinja` under `packs/<lang>/` and rebuild the CLI (`just setup-cli`).
The codegen **fingerprint** (RFC-0061) hashes every bundled pack's content, so any
template edit marks the affected fixtures stale — no separate bookkeeping.

## Overriding a pack at runtime — no rebuild

Point the renderer at an external directory of `.jinja` files: a file named
`<template-name>` or `<template-name>.jinja` there **overrides** the bundled pack
of that name; anything absent falls back to bundled.

```sh
export NROS_TEMPLATE_DIR=/path/to/my/pack
nros generate-rust …          # uses the override, no recompile
```

(Equivalently, `rosidl_codegen::render::set_template_dir(dir)` from Rust, called
once before the first render.)

The stable template names are the `templates` keys in each `packs/<dir>/pack.toml`
(e.g. `message.h`, `message_nros.rs`, `cargo.toml`, `_field.jinja`); `nros`'s own
copy of the generated registry is at `target/**/build/rosidl-codegen-*/out/
pack_registry.rs`. `tests/external_pack_smoke.rs` proves the override + fallback.

> **Do not set `NROS_TEMPLATE_DIR` during fixture or CI builds.** The fingerprint
> hashes the *bundled* packs; an external override would silently produce output
> that disagrees with the recorded fingerprint.

## Adding a language

### Step 0 — decide which SURFACES you need

This is the step that sizes the work, and skipping it is why "add a language"
sounds bigger than it is. A pack is a **(language × surface)** pair, not a
language: Rust has four message packs, and the `cpp` pack emits Rust as well as
C++.

There are two independent axes. On the **message** side: idiomatic,
embedded-idiomatic (`no_std`), FFI/ABI (`repr(C)`), bridge glue, packaging. On
the **entry** side: the entry TU, and the component seam. A language can take
an entry surface with no message surface (it consumes another language's
messages) or the reverse.

RFC-0091 §6 has the full table, including what each surface *drags in* — an
FFI surface inherits the cross-language memory-agreement gate, a bridge surface
is two spellings of one type that must move together. **A language can ship
with two surfaces and gain the rest later**: a Zig component installed by a C
or C++ entry needs the idiomatic and FFI message surfaces and the component
seam, and nothing else.

### Step 1 — the message pack

1. add its `.jinja` templates (a new `packs/<lang>/`) **and a `pack.toml`
   declaring them**. No Rust edit: the registry is discovered from the manifest,
   and a pack directory with no manifest — or a `.jinja` no manifest claims —
   fails the build naming the file. If the pack names C-family artifacts, its
   three naming fields make it a `generator::naming` surface and the `Surface`
   variant is generated for it;
2. if it needs type spelling the existing filters don't cover, add a **filter
   set** for the language (`rosidl_codegen::filters::FILTER_SETS`) wrapping a
   `*_spelling` function in `types.rs`. The filter set is all the Rust that
   type SPELLING takes; the set is keyed by the pack that CALLS the filter,
   not by the syntax it emits — `cpp_repr_c_type` returns Rust;
3. add a generator per kind (`generator/{msg,srv,action}.rs`, or one file like
   `generator/cpp.rs`) that builds the context, names the output files and
   calls `render::render("<template-name>", &ctx)`, and its arm in
   `nros generate` (`cmd/generate.rs`).

### Step 2 — the entry pack, if the language writes entries

An entry pack is DATA. Since phase-474 there is no per-language entry emitter:
one renderer (`codegen/entry/emit/mod.rs`) lowers the plan once into
`nros_entry_lower::LoweredEntry` and hands THAT to the pack's template.

1. `packs/entry/<surface>/entry.<ext>.jinja` over `LoweredEntry`. Nodes (with
   their component kind, resolved name and namespace, params, remaps, QoS),
   board family and boot shape, the C-ABI runner names, the executor layout
   (one executor, sched contexts, or per-tier setups), tier rows, the boot
   blob and the monitor tables all arrive computed and RAW — a template decides
   where a value goes and spells it, never how to quote it;
2. `packs/entry/<surface>/pack.toml` — the extension, whether the TU is
   C-family, the entry template, its `templates = [{ key, file }]` rows, and
   what the renderer must know: `context = "lowered-entry"`, the `filters` its
   templates call, the component kinds it constructs (`components`), and
   whether it calls the board's C-ABI runners (`c_abi_runners`). CMake reads
   it through `nros codegen entry-pack`. The build discovers the directory —
   there is no registry row to add;
3. one variant on `Language` in `nros-lang`. Every consumer sees it — one
   enumeration;
4. **only if** the language needs a spelling no existing filter provides (the
   set is `c_str`, `pkg_ident`, `cpp_board_class`): one function and one row in
   `codegen/entry/filters.rs`. A spelling is a correctness property, so it is
   Rust; everything else about the pack is not.

`testdata/entry-packs/zig/` is a whole entry pack written this way — a manifest
and one template — and a unit test renders every golden C plan through it.

### Step 3 — the goldens

Add the coordinate to the entry golden harness (`codegen/entry/golden.rs`; its
`Emitter` enum names the entry point a row renders through), run
`NROS_UPDATE_GOLDEN=1 cargo test -p nros-cli-core --lib codegen::entry::golden`,
and **read the diff**. The generated source is a file, not a claim.

`just check entry-pack-conformance` refuses a half-wired pack in EITHER family —
entry and message; the name is historical (see the recipe comment). A directory
with no manifest, a language pack missing the fields its consumer needs, a
registry key or `registry_order` claimed twice, a template file that does not
exist, a `.jinja` no manifest claims, a `Language` variant nothing renders, or a
language whose bytes are recorded in no golden — and, since phase-474, a new
`codegen/entry/emit_<x>.rs`, because an entry language is a pack. Run it before
you believe the pack works.

### What this does NOT make cheap

The **toolchain story** — how CMake compiles the language, how it links
`libnros`, how its components declare themselves. The codegen cost becomes a
pack; the build-integration cost does not. Budget for it separately.

No per-language TYPE SPELLING lives in the message builders — the packs and
their filters own it, and since phase-469 W1 so do the artifact-naming suffixes
(file and guard names). The builders still hold the per-language Rust that is a
RULE rather than a parameter — which generator builds which context, and the ROS
kind word, which is identical on every surface. An entry language brings no
emitter (phase-474): its pack and, at most, a spelling filter. Implemented by
phase-335 (RFC-0068), phase-432 (RFC-0091), phase-469 and phase-474.
