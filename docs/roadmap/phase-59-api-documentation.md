# Phase 59 — API Documentation

**Goal**: Produce publication-quality API reference for both Rust and C users from
a single set of Rust source comments, with no external toolchain dependencies beyond
what `just setup` already provides.

**Status**: In Progress
**Priority**: Medium
**Depends on**: Phase 57 (Code Quality)

## Overview

nano-ros exposes two public APIs: a Rust API (`nros-node`, `nros-core`, etc.) and
a C API (`nros-c`). Both are documented via `///` doc comments in Rust source, but
the audiences differ:

- **Rust users** consume docs via `cargo doc` (rustdoc), which expects Markdown.
- **C users** consume docs via Doxygen on the cbindgen-generated header
  `nros_generated.h`, which expects `@param`/`@retval`/`@pre` tags.

Today the doc comments are Rust Markdown. cbindgen faithfully converts `///` to
`/** */`, so the text appears in the C header — but `# Parameters` headings and
`` * `name` - description `` bullets are not Doxygen-structured. Rust-isms like
`usize::MAX`, `Box<T>`, and module paths leak into C-facing docs.

### Design Decision

Write idiomatic Rust Markdown in source (optimised for `cargo doc`), then
**post-process** the cbindgen output in `build.rs` to produce Doxygen-tagged
C headers. This avoids:

- Duplicate doc comments (unmaintainable)
- `cfg_attr`-gated docs (verbose, cbindgen doesn't evaluate `cfg_attr` on `#[doc]`)
- External script dependencies (Python, sed)

## Architecture

```
Rust source (/// Markdown)
        │
        ├──→ cargo doc ──→ Rust HTML (rustdoc)
        │
        └──→ cbindgen ──→ nros_generated.h (Markdown in /** */)
                                │
                                └──→ doxygen_postprocess() ──→ nros_generated.h (Doxygen tags)
                                        (in build.rs)                  │
                                                                       └──→ doxygen ──→ C HTML
```

### Post-Processor Transformations

The `doxygen_postprocess()` function in `build.rs` performs a single-pass
line-by-line state-machine transformation:

| Input (Rust Markdown in `/** */`)              | Output (Doxygen)                       |
|------------------------------------------------|----------------------------------------|
| ` * # Parameters`                              | *(removed — items follow directly)*    |
| ` * * \`name\` - description`                  | ` * @param name description`           |
| ` * # Returns`                                 | *(removed)*                            |
| ` * * \`NROS_RET_OK\` on success`              | ` * @retval NROS_RET_OK on success`    |
| ` * * Non-zero if valid, 0 if invalid`         | ` * @return Non-zero if valid, ...`    |
| ` * # Safety`                                  | *(removed)*                            |
| ` * * All pointers must be valid`              | ` * @pre All pointers must be valid.`  |
| `usize::MAX`                                   | `SIZE_MAX`                             |
| `` `Box<CExecutor>` ``                         | `opaque internal pointer`              |
| `nros_node::Executor`                          | `the internal executor`                |

### Doxygen Configuration

A `Doxyfile` at `packages/core/nros-c/Doxyfile` consumes:

- `include/nros/nros_generated.h` (post-processed, all types + functions)
- `include/nros/visibility.h`, `platform.h`, `types.h` (hand-written)

Output: `target/doc/c-api/html/` (git-ignored, alongside `target/doc/` from rustdoc).

## Work Items

- [x] 59.1 — build.rs Doxygen post-processor
- [x] 59.2 — Fix Rust-isms in source doc comments
- [x] 59.3 — Fix underscore-prefixed C parameter names
- [x] 59.4 — Audit doc coverage for undocumented public items
- [x] 59.5 — Add Doxyfile and justfile recipes
- [x] 59.6 — Document missing public items in nros-node
- [x] 59.7 — Document missing public items in nros-core and nros-serdes
- [x] 59.8 — Expand sparse module-level docs
- [x] 59.9 — Add crate-level examples to nros-rmw and nros-serdes
- [ ] 59.10 — Fix broken intra-doc links in nros-rmw trait docs

### 59.1 — build.rs Doxygen Post-Processor

Add `doxygen_postprocess()` to `packages/core/nros-c/build.rs`. Called
immediately after `bindings.write_to_file()`. Pure Rust, no regex crate
needed — uses `str::strip_prefix` / `split_once` for pattern matching.

State machine with four states: `None`, `Params`, `Returns`, `Safety`.
Detects ` * # Section` headers, transforms subsequent ` * * ` bullets
into the corresponding Doxygen tag, resets on blank doc lines (` *`).

Global string replacements for Rust-isms run on every line regardless
of state.

**Files**:
- `packages/core/nros-c/build.rs`

### 59.2 — Fix Rust-isms in Source Doc Comments

Fix doc comments in Rust source that reference Rust-specific concepts
visible to C users. The post-processor handles the mechanical
transformations, but some comments need manual rewording:

- Replace `usize::MAX = not registered` with `SIZE_MAX = not registered`
  in struct field docs (these appear in the C header as struct comments)
- Replace `Box<CExecutor>` / `Box<ActionServerInternal>` with
  "opaque internal pointer" in struct field docs
- Remove Rust module paths (`nros_node::Executor`) from function docs
- Replace `*const c_char` with `const char *` in doc text
- Replace backtick references to Rust-internal methods
  (`add_action_server_raw_sized()`) with C-facing descriptions

**Files**:
- `packages/core/nros-c/src/executor.rs`
- `packages/core/nros-c/src/action/server.rs`
- `packages/core/nros-c/src/action/client.rs`
- `packages/core/nros-c/src/subscription.rs`
- `packages/core/nros-c/src/timer.rs`
- `packages/core/nros-c/src/guard_condition.rs`
- `packages/core/nros-c/src/param_server.rs`

### 59.3 — Fix Underscore-Prefixed C Parameter Names

cbindgen preserves Rust `_name` parameter naming (meaning "unused").
In the C header these look like internal/deprecated parameters. Rename
in Rust source to drop the underscore prefix where the parameter is
meaningful to C callers:

- `_origin` → `origin` in CDR read/write functions
- `_context` → `context` in trigger functions
- `_ready` → `ready` in `nros_executor_trigger_always`
- `_count` → `count` in `nros_executor_trigger_always`

Some of these will require `#[allow(unused_variables)]` on the
function or a `let _ = origin;` to suppress warnings.

**Files**:
- `packages/core/nros-c/src/cdr.rs`
- `packages/core/nros-c/src/executor.rs`

### 59.4 — Audit Doc Coverage for Undocumented Public Items

Run `cargo doc` with `-Wrustdoc::missing_docs` (or `#![warn(missing_docs)]`)
on nros-c and the core public crates to identify undocumented public items.

Fix any gaps in:
- All `#[repr(C)]` struct fields (appear in C header)
- All `extern "C"` functions (appear in C header)
- Key public types in `nros-node` and `nros-core`

Not in scope: exhaustive doc coverage of every internal type — focus on
items that appear in the public Rust or C API.

**Files**:
- `packages/core/nros-c/src/*.rs`
- `packages/core/nros-node/src/lib.rs`
- `packages/core/nros-core/src/lib.rs`

### 59.5 — Add Doxyfile and Justfile Recipes

Create a minimal Doxyfile for the C API. Add justfile recipes for both
doc targets:

```bash
just doc-rust    # cargo doc --workspace --no-deps
just doc-c       # doxygen packages/core/nros-c/Doxyfile
just doc         # both (replaces current `just doc`)
```

Doxyfile settings:
- `INPUT = include/nros/`
- `OUTPUT_DIRECTORY = ../../../target/doc/c-api`
- `GENERATE_LATEX = NO`
- `OPTIMIZE_OUTPUT_FOR_C = YES`
- `EXTRACT_ALL = NO` (only documented items)
- `WARN_IF_UNDOCUMENTED = YES`

The `just doc` recipe should work without Doxygen installed (skip C docs
with a warning if `doxygen` is not in PATH). Doxygen is NOT added to
`just setup` — it's an optional tool for doc generation.

**Files**:
- `packages/core/nros-c/Doxyfile`
- `justfile`

### 59.6 — Document Missing Public Items in nros-node

Add `///` doc comments to undocumented public items in nros-node.

**Missing type docs:**
- `SessionStore<S>` (executor/spin.rs:132) — public struct, no doc
- `TimerCallbackFn` (timer.rs:148) — type alias, no doc

**Missing method docs on `ActionServerHandle`** (executor/action.rs:341-410):
- `publish_feedback()` — publish feedback for an active goal
- `complete_goal()` — mark goal as succeeded/aborted/canceled
- `set_goal_status()` — update goal status
- `active_goal_count()` — number of active goals
- `for_each_active_goal()` — iterate over active goals

**Missing method docs on `Executor`:**
- `add_service_raw()` / `add_service_raw_sized()` (spin.rs:827-905)
- Setter methods with 1-line docs: `set_trigger`, `set_semantics`,
  `set_invocation` — expand to 2-3 sentences

**Files**:
- `packages/core/nros-node/src/executor/action.rs`
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/timer.rs`

### 59.7 — Document Missing Public Items in nros-core and nros-serdes

**nros-core** — add docs to:
- `ServiceResult` type alias (service.rs:89)
- `ServiceCallback` type alias (service.rs:92)

**nros-serdes** — add docs to:
- `error.rs` — add module-level `//!` doc
- `SerError::StringTooLong` / `SequenceTooLong` — state limits (u32::MAX)
- `DeserError::CapacityExceeded` — clarify heapless container overflow
- `CdrWriter::origin` field — explain alignment-relative-to-header purpose
- `CdrWriter::new_with_header()` — explain origin=4 alignment shift

**Files**:
- `packages/core/nros-core/src/service.rs`
- `packages/core/nros-serdes/src/error.rs`
- `packages/core/nros-serdes/src/cdr.rs`

### 59.8 — Expand Sparse Module-Level Docs

Several modules have 1-line `//!` docs. Expand to 3-5 lines explaining
purpose, key types, and relationship to other modules.

- `nros-core/src/types.rs` — "Core ROS type traits" → explain
  `RosMessage`, `RosService`, and generated type pattern
- `nros-core/src/time.rs` — "ROS time types" → explain `Time`,
  `Duration`, monotonic semantics, no_std compatibility
- `nros-rmw/src/traits.rs` — "Transport abstraction traits" → explain
  Session/Publisher/Subscriber/Service trait hierarchy
- `nros-serdes/src/primitives.rs` — "Primitive type serialization
  implementations" → list covered types (bool, integers, floats, strings,
  heapless containers, alloc types behind feature gate)

**Files**:
- `packages/core/nros-core/src/types.rs`
- `packages/core/nros-core/src/time.rs`
- `packages/core/nros-rmw/src/traits.rs`
- `packages/core/nros-serdes/src/primitives.rs`

### 59.9 — Add Crate-Level Examples to nros-rmw and nros-serdes

Both crates lack end-to-end examples in their `lib.rs` docs.

**nros-serdes** — add `# Examples` showing:
```rust
let mut buf = [0u8; 256];
let mut w = CdrWriter::new_with_header(&mut buf)?;
msg.serialize(&mut w)?;
let mut r = CdrReader::new(&buf[..w.position()])?;
let decoded = MyMsg::deserialize(&mut r)?;
```

**nros-rmw** — add `# Examples` showing the trait hierarchy and how
backends plug in (conceptual, using `ignore` block since concrete
sessions require a backend crate).

**Files**:
- `packages/core/nros-rmw/src/lib.rs`
- `packages/core/nros-serdes/src/lib.rs`

### 59.10 — Fix Broken Intra-Doc Links in nros-rmw Trait Docs

Several trait method docs reference types that don't resolve or use
unclear terminology:

- `process_raw_in_place()` — "buffer is locked during `f`" → clarify
  re-entrancy prevention for zero-copy access
- `drive_io()` — "Pull-based backends override this" → explain
  zenoh-pico/XRCE-DDS require network I/O polling
- `try_recv_raw_with_info()` — "RMW attachment" → explain publisher GID
  and timestamp metadata from ROS 2

Also fix any `[`Type`]` links that fail to resolve when building docs
for nros-rmw in isolation (run `RUSTDOCFLAGS="-W rustdoc::broken_intra_doc_links"
cargo doc -p nros-rmw --no-deps`).

**Files**:
- `packages/core/nros-rmw/src/traits.rs`

## Acceptance Criteria

- [x] `cargo build -p nros-c` produces `nros_generated.h` with Doxygen tags
      (`@param`, `@retval`, `@pre`) — no raw `# Parameters` headings remain
- [x] `grep -c 'usize::MAX\|Box<\|nros_node::' include/nros/nros_generated.h`
      returns 0 (no Rust-isms in generated header)
- [x] `grep '_origin\|_context\|_ready\|_count' include/nros/nros_generated.h`
      returns 0 for function parameter names (underscore prefixes removed)
- [x] `doxygen Doxyfile` completes with 0 warnings on documented items
- [x] `cargo doc --workspace --no-deps` completes with no broken intra-doc links
- [x] `just doc` generates both Rust and C API docs under `target/doc/`
- [x] `just quality` still passes (no regressions from doc comment changes)
- [x] `RUSTDOCFLAGS="-W missing_docs" cargo doc -p nros-node --no-deps`
      completes with 0 warnings
- [x] `RUSTDOCFLAGS="-W missing_docs" cargo doc -p nros-core --no-deps`
      completes with 0 warnings
- [x] All four focus crates have `//!` module docs of 3+ lines on every
      public module

## Notes

- The post-processor is intentionally simple (no regex, ~80 lines) and runs
  inside `build.rs` with zero extra dependencies. If patterns grow more complex,
  consider extracting to a `build/` helper file via `include!()`.
- Doxygen is treated as optional — CI does not require it. The primary
  verification is that the generated header contains correct Doxygen tags,
  which can be checked via grep without Doxygen installed.
- The `# Safety` → `@pre` mapping was chosen over `@warning` because safety
  preconditions are caller obligations (preconditions), not informational
  warnings. This matches Doxygen's `@pre` semantics exactly.
- cbindgen's `documentation_style` option controls comment syntax (`/* */` vs
  `///`) but does NOT transform content. The post-processor is necessary
  regardless of cbindgen settings.
