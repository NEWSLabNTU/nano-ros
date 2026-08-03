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

The stable template names are the keys of `PACKS` in `render.rs` (e.g.
`message.h`, `message_nros.rs`, `cargo.toml`, `_field.jinja`). `tests/
external_pack_smoke.rs` proves the override + fallback.

> **Do not set `NROS_TEMPLATE_DIR` during fixture or CI builds.** The fingerprint
> hashes the *bundled* packs; an external override would silently produce output
> that disagrees with the recorded fingerprint.

## Adding a language

Adding a target language is a **pack + a spelling filter**, not a rewrite:

1. add its `.jinja` templates (a new `packs/<lang>/`) and its rows in `PACKS`;
2. if it needs type spelling the existing filters don't cover, register one more
   filter wrapping a `*_spelling` function in `types.rs`;
3. add the generator entry that builds the view struct and calls
   `render::render("<template-name>", &ctx)`.

No per-language type logic lives in the builders — the packs and their filters own
it. Implemented by phase-335 (RFC-0068).
