# Phase 59 — API Documentation

**Goal**: Produce publication-quality API reference for both Rust and C users,
with no external toolchain dependencies beyond what `just setup` already provides.

**Status**: In Progress
**Priority**: Medium
**Depends on**: Phase 57 (Code Quality)

## Overview

nano-ros exposes two public APIs: a Rust API (`nros-node`, `nros-core`, etc.) and
a C API (`nros-c`).

- **Rust users** consume docs via `cargo doc` (rustdoc).
- **C users** consume docs via Doxygen on hand-written per-module C headers.

### Design Evolution

The original approach (59.1) used a `doxygen_postprocess()` state machine in
`build.rs` to transform Rust Markdown doc comments in the cbindgen-generated
`nros_generated.h` into Doxygen-tagged output. This worked but had drawbacks:

- All declarations appeared on a single Doxygen file page (no modular navigation)
- Doc comments had to be written in Rust Markdown and transformed mechanically
- C users couldn't include only the headers they needed

In 59.11, the approach was replaced with **hand-written per-module C headers**
containing native Doxygen tags (`@param`, `@retval`, `@pre`). The
`doxygen_postprocess()` function was removed from `build.rs`. `nros_generated.h`
remains as an internal cbindgen artifact but is no longer the documentation
source.

## Architecture

```
Rust source (/// Markdown)
        │
        ├──→ cargo doc ──→ Rust HTML (rustdoc)
        │
        └──→ cbindgen ──→ nros_generated.h (internal, not documented)

Hand-written C headers (per-module, with Doxygen tags)
        │
        └──→ doxygen ──→ C HTML (modular, per-module pages)
```

### C Header Structure

```
include/nros/
├── nros.h                 # Umbrella: includes all per-module headers
├── types.h                # Shared types: nros_ret_t, time, duration, QoS, constants
├── init.h                 # nros_support_t + initialisation functions
├── node.h                 # nros_node_t + node lifecycle functions
├── publisher.h            # nros_publisher_t + publish functions
├── subscription.h         # nros_subscription_t + callback registration
├── service.h              # nros_service_t + request/response functions
├── client.h               # nros_client_t + service client functions
├── executor.h             # nros_executor_t + spin/add/trigger functions
├── timer.h                # nros_timer_t + periodic timer functions
├── guard_condition.h      # nros_guard_condition_t + manual wake-up
├── lifecycle.h            # nros_lifecycle_state_machine_t + REP-2002
├── action.h               # Action server/client types + goal management
├── parameter.h            # nros_param_server_t + declare/get/set
├── cdr.h                  # CDR serialization read/write functions
├── clock.h                # nros_clock_t + time arithmetic
├── visibility.h           # NROS_PUBLIC etc. (unchanged)
├── platform.h             # Platform abstraction (unchanged)
└── nros_generated.h       # Internal: cbindgen output (not in Doxygen)
```

Each per-module header is the authoritative C API surface for its module.
`types.h` is the shared foundation included by all other headers.

### Doxygen Configuration

The `Doxyfile` at `packages/core/nros-c/Doxyfile` lists each per-module header
explicitly (no `RECURSIVE`). `nros_generated.h` is excluded. Output goes to
`target/doc/c-api/html/`.

### Justfile Recipes

```bash
just doc-rust     # cargo doc --workspace --no-deps
just doc-c        # doxygen packages/core/nros-c/Doxyfile
just doc-c-check  # cc -fsyntax-only on nros.h (verify headers compile)
just doc          # doc-rust + doc-c
```

## Work Items

- [x] 59.1 — build.rs Doxygen post-processor (superseded by 59.11)
- [x] 59.2 — Fix Rust-isms in source doc comments
- [x] 59.3 — Fix underscore-prefixed C parameter names
- [x] 59.4 — Audit doc coverage for undocumented public items
- [x] 59.5 — Add Doxyfile and justfile recipes
- [x] 59.6 — Document missing public items in nros-node
- [x] 59.7 — Document missing public items in nros-core and nros-serdes
- [x] 59.8 — Expand sparse module-level docs
- [x] 59.9 — Add crate-level examples to nros-rmw and nros-serdes
- [ ] 59.10 — Fix broken intra-doc links in nros-rmw trait docs
- [x] 59.11 — Hand-written per-module C headers with Doxygen docs
- [x] 59.12 — Fix C API Quick Start example in mainpage.md
- [x] 59.13 — Expand thin callback typedef docs in C headers
- [x] 59.14 — Normalise `is_valid()` return wording across C headers
- [ ] 59.15 — Add Executor const generic guidance to Rust crate docs
- [ ] 59.16 — Explain Session trait in nros crate-level docs
- [ ] 59.17 — Link to guides and examples from nros crate docs

### 59.1 — build.rs Doxygen Post-Processor (SUPERSEDED)

*Superseded by 59.11.* The `doxygen_postprocess()` state machine was removed
from `build.rs` when per-module headers replaced the single-header approach.
The function, `push_line()` helper, and `DocSection` enum (~150 lines) were
deleted.

### 59.2 — Fix Rust-isms in Source Doc Comments

Fix doc comments in Rust source that reference Rust-specific concepts
visible to C users:

- Replace `usize::MAX = not registered` with `SIZE_MAX = not registered`
- Replace `Box<CExecutor>` / `Box<ActionServerInternal>` with
  "opaque internal pointer"
- Remove Rust module paths (`nros_node::Executor`) from function docs

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
Renamed in Rust source to drop the underscore prefix where the parameter
is meaningful to C callers (`_origin` → `origin`, `_context` → `context`,
etc.).

**Files**:
- `packages/core/nros-c/src/cdr.rs`
- `packages/core/nros-c/src/executor.rs`

### 59.4 — Audit Doc Coverage for Undocumented Public Items

Run `cargo doc` with `-Wrustdoc::missing_docs` on nros-c and core crates
to identify undocumented public items. Fix gaps in `#[repr(C)]` struct
fields and `extern "C"` functions.

**Files**:
- `packages/core/nros-c/src/*.rs`
- `packages/core/nros-node/src/lib.rs`
- `packages/core/nros-core/src/lib.rs`

### 59.5 — Add Doxyfile and Justfile Recipes

Created `Doxyfile` and justfile recipes (`doc-rust`, `doc-c`, `doc-c-check`,
`doc`). Doxygen is optional — `just doc` skips C docs with a warning if
`doxygen` is not in PATH.

**Files**:
- `packages/core/nros-c/Doxyfile`
- `justfile`

### 59.6 — Document Missing Public Items in nros-node

Added `///` doc comments to undocumented public items in nros-node:
`SessionStore<S>`, `TimerCallbackFn`, `ActionServerHandle` methods,
`Executor` raw service methods.

**Files**:
- `packages/core/nros-node/src/executor/action.rs`
- `packages/core/nros-node/src/executor/spin.rs`
- `packages/core/nros-node/src/timer.rs`

### 59.7 — Document Missing Public Items in nros-core and nros-serdes

Added docs to `ServiceResult`, `ServiceCallback`, `SerError` variants,
`DeserError::CapacityExceeded`, `CdrWriter::origin`, `CdrWriter::new_with_header()`.

**Files**:
- `packages/core/nros-core/src/service.rs`
- `packages/core/nros-serdes/src/error.rs`
- `packages/core/nros-serdes/src/cdr.rs`

### 59.8 — Expand Sparse Module-Level Docs

Expanded 1-line `//!` docs to 3-5 lines for `types.rs`, `time.rs`,
`traits.rs`, `primitives.rs`.

**Files**:
- `packages/core/nros-core/src/types.rs`
- `packages/core/nros-core/src/time.rs`
- `packages/core/nros-rmw/src/traits.rs`
- `packages/core/nros-serdes/src/primitives.rs`

### 59.9 — Add Crate-Level Examples to nros-rmw and nros-serdes

Added `# Examples` to `lib.rs` docs for both crates.

**Files**:
- `packages/core/nros-rmw/src/lib.rs`
- `packages/core/nros-serdes/src/lib.rs`

### 59.10 — Fix Broken Intra-Doc Links in nros-rmw Trait Docs

Several trait method docs reference types that don't resolve or use
unclear terminology:

- `process_raw_in_place()` — clarify re-entrancy prevention
- `drive_io()` — explain zenoh-pico/XRCE-DDS polling
- `try_recv_raw_with_info()` — explain publisher GID and timestamp metadata

Also fix any `[Type]` links that fail to resolve when building docs
for nros-rmw in isolation.

**Files**:
- `packages/core/nros-rmw/src/traits.rs`

### 59.11 — Hand-Written Per-Module C Headers with Doxygen Docs

Replaced the post-processor approach with hand-written per-module C headers.
Each header is the authoritative C API surface for its module, with native
Doxygen tags (`@file`, `@brief`, `@param`, `@retval`, `@pre`).

Changes:

- Rewrote 15 per-module headers (types.h, init.h, node.h, publisher.h,
  subscription.h, service.h, client.h, executor.h, timer.h,
  guard_condition.h, lifecycle.h, action.h, parameter.h, cdr.h, clock.h)
- Created `nros.h` umbrella header
- Removed `doxygen_postprocess()`, `push_line()`, `DocSection` from build.rs
- Updated Doxyfile with explicit INPUT list (excludes nros_generated.h)
- Updated mainpage.md with new Header Organisation section
- Added `just doc-c-check` recipe for syntax verification

**Files**:
- `packages/core/nros-c/include/nros/*.h` (15 headers + nros.h)
- `packages/core/nros-c/build.rs`
- `packages/core/nros-c/Doxyfile`
- `packages/core/nros-c/docs/mainpage.md`
- `justfile`

### 59.12 — Fix C API Quick Start Example in mainpage.md

The Quick Start example in `mainpage.md` won't compile:

- `nros_node_init(&node, "my_node", "")` — missing `support` parameter;
  actual signature is `nros_node_init(node, support, name, namespace_)`
- `nros_publisher_init(&pub, &node, "chatter", serialize, deserialize)` —
  wrong parameter list; actual signature is
  `nros_publisher_init(publisher, node, type_info, topic_name)`
- Missing `nros_support_t` initialization step entirely

Rewrite the example to match actual function signatures and include the
full lifecycle: support init → node init → publisher init → publish →
publisher fini → node fini → support fini.

**Files**:
- `packages/core/nros-c/docs/mainpage.md`

### 59.13 — Expand Thin Callback Typedef Docs in C Headers

Several callback typedefs have only a one-line `/** Comment. */` with no
`@param`/`@return` docs, while others (timer, subscription, service) have
full parameter documentation. Expand the thin ones to match:

- `nros_guard_condition_callback_t` (guard_condition.h) — add `@param context`
- `nros_param_callback_t` (parameter.h) — add `@param name`, `@param value`,
  `@return` (accept/reject semantics)
- `nros_feedback_callback_t` (action.h) — add `@param goal_uuid`,
  `@param data`, `@param len`, `@param context`
- `nros_result_callback_t` (action.h) — add `@param goal_uuid`,
  `@param status`, `@param data`, `@param len`, `@param context`
- `nros_goal_callback_t` (action.h) — add `@param goal_uuid`, `@param data`,
  `@param len`, `@param context`, `@return`
- `nros_cancel_callback_t` (action.h) — add `@param goal`, `@param context`,
  `@return`
- `nros_accepted_callback_t` (action.h) — add `@param goal`, `@param context`

**Files**:
- `packages/core/nros-c/include/nros/guard_condition.h`
- `packages/core/nros-c/include/nros/parameter.h`
- `packages/core/nros-c/include/nros/action.h`

### 59.14 — Normalise `is_valid()` Return Wording Across C Headers

All `is_valid()` functions return `bool` but use inconsistent wording:

- clock.h: `@return @c true if valid, @c false otherwise.`
- publisher.h, node.h, etc.: `@return Non-zero if valid, 0 if invalid or NULL.`

Pick one style and apply it consistently. Since all functions return `bool`,
the `@c true`/`@c false` wording is more precise. Apply to all `is_valid()`
and `is_ready()` functions across all per-module headers.

**Files**:
- `packages/core/nros-c/include/nros/publisher.h`
- `packages/core/nros-c/include/nros/subscription.h`
- `packages/core/nros-c/include/nros/service.h`
- `packages/core/nros-c/include/nros/client.h`
- `packages/core/nros-c/include/nros/executor.h`
- `packages/core/nros-c/include/nros/timer.h`
- `packages/core/nros-c/include/nros/guard_condition.h`
- `packages/core/nros-c/include/nros/node.h`
- `packages/core/nros-c/include/nros/clock.h`

### 59.15 — Add Executor Const Generic Guidance to Rust Crate Docs

The Quick Start shows `Executor::<_, 4, 4096>` with no explanation of
what 4 and 4096 mean. Add a section to the `nros` crate-level docs
explaining:

- `MAX_CBS` — maximum number of registered callbacks (subscriptions +
  timers + services + guard conditions); size to total handle count
- `CB_ARENA` — byte budget for callback closures stored inline; 4096 is
  generous for most use cases, reduce for memory-constrained targets
- `DEFAULT_TX_BUF` (1024) — default publish buffer size; use `_sized`
  variants for larger messages

**Files**:
- `packages/core/nros/src/lib.rs`

### 59.16 — Explain Session Trait in nros Crate-Level Docs

Users see `<S: Session>` on `Executor`, `Node`, etc. but have no
explanation that `S` is auto-selected by the RMW feature flag. Add a
brief "Transport Backends" section to the crate docs explaining:

- `S` is the abstract transport session (zenoh or XRCE-DDS)
- Selected at compile time via `rmw-zenoh` or `rmw-xrce` feature
- Users never need to name `S` explicitly — it's inferred by the compiler
- Advanced users can access concrete types via `nros::internals::RmwSession`

**Files**:
- `packages/core/nros/src/lib.rs`

### 59.17 — Link to Guides and Examples from nros Crate Docs

The crate docs have no "next step" after the Quick Start. Add a
"Further Reading" section with links to:

- `docs/guides/getting-started.md` — full setup walkthrough
- `docs/guides/creating-examples.md` — how to create new examples
- `docs/guides/message-generation.md` — code generation workflow
- `examples/` directory — working examples by platform

Use relative `[text](url)` links that work in both rustdoc and GitHub.

**Files**:
- `packages/core/nros/src/lib.rs`

## Acceptance Criteria

- [x] `cargo build -p nros-c` produces `nros_generated.h` with Doxygen tags
      (`@param`, `@retval`, `@pre`) — no raw `# Parameters` headings remain
- [x] `grep -c 'usize::MAX\|Box<\|nros_node::' include/nros/nros_generated.h`
      returns 0 (no Rust-isms in generated header)
- [x] `grep '_origin\|_context\|_ready\|_count' include/nros/nros_generated.h`
      returns 0 for function parameter names (underscore prefixes removed)
- [x] `doxygen Doxyfile` completes with 0 warnings
- [x] `just doc-c-check` passes (all per-module headers compile)
- [x] `cargo doc --workspace --no-deps` completes with no broken intra-doc links
- [x] `just doc` generates both Rust and C API docs under `target/doc/`
- [x] `just quality` still passes (no regressions from doc comment changes)
- [x] `RUSTDOCFLAGS="-W missing_docs" cargo doc -p nros-node --no-deps`
      completes with 0 warnings
- [x] `RUSTDOCFLAGS="-W missing_docs" cargo doc -p nros-core --no-deps`
      completes with 0 warnings
- [x] All four focus crates have `//!` module docs of 3+ lines on every
      public module
- [x] C Quick Start example in mainpage.md compiles and shows full lifecycle
- [x] All callback typedefs have `@param`/`@return` docs
- [x] `is_valid()` / `is_ready()` return wording is consistent across all headers
- [ ] Rust crate docs explain Executor const generics and Session trait
- [ ] Rust crate docs link to guides and examples

## Notes

- Doxygen is treated as optional — CI does not require it. The primary
  verification is `just doc-c-check` (syntax) plus `just doc-c` (0 warnings).
- `nros_generated.h` is still produced by cbindgen in build.rs for potential
  future use (e.g., automated drift detection tooling) but is excluded from
  Doxygen documentation.
- The `# Safety` → `@pre` mapping was chosen over `@warning` because safety
  preconditions are caller obligations (preconditions), not informational
  warnings. This matches Doxygen's `@pre` semantics exactly.
- The original drift detection plan (`#ifdef NROS_DRIFT_CHECK` including
  `nros_generated.h` from per-module headers) was abandoned because C
  does not allow enum/struct re-definition. Signature drift between
  hand-written headers and Rust FFI functions is caught at link time
  by `just test-c`.
