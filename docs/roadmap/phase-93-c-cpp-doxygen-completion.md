# Phase 93: C and C++ Doxygen Completion

**Goal**: Bring the rendered C and C++ Doxygen sites
(`book/book/api/c/`, `book/book/api/cpp/`) up to a level comparable with
the rustdoc output. The current build emits the sites without warnings,
but a user landing on either site faces blank or alphabet-soup index
pages, undocumented opaque types, and per-function reference stubs with
no `@param` / `@return` blocks. Close those gaps.

**Status**: In Progress (Groups A, B, E1+E3, F1+F3, G complete; C, D, E2, F2 remaining)
**Priority**: Medium — `just book` already produces a deployable site
(Phase 65), and `just doc-c` / `just doc-cpp` already wire Doxygen into
CI (`deploy-book.yml`). What is missing is the *content* the generators
have to render. Until that lands, the C and C++ sites are the public
face of the project for native users and they undersell the API.
**Depends on**: Phase 91 Group F/G (cbindgen-driven per-module headers)
is helpful — once the per-module headers actually contain decls, the
docs only have one place to live. Not a hard blocker; this phase can
proceed by including `nros_generated.h` directly in the Doxyfile INPUT
list as a pragmatic stopgap.

## Overview

### Status quo (April 2026 audit)

`just book` was just turned on as the canonical doc build. It ships:
- mdBook narrative under `book/book/`
- rustdoc under `book/book/api/rust/`
- C Doxygen under `book/book/api/c/`
- C++ Doxygen under `book/book/api/cpp/`

The Doxygen sites have these concrete gaps (file:line cites use the
hand-written headers under `packages/core/nros-{c,cpp}/include/nros/`):

| Gap | Side | Symptom |
|---|---|---|
| Missing C++ mainpage | C++ | `api/cpp/index.html` is the bare Doxygen frame — no quick-start, no module table, no narrative. C has `nros-c/docs/mainpage.md`; C++ has nothing. |
| Opaque type dead-ends | C | All entity structs (`nros_publisher_t`, `nros_subscription_t`, `nros_executor_t`, …) live in `nros_generated.h` which is `EXCLUDE`d from `Doxyfile:10`. Clicking a type in a function signature lands on a "not documented" stub. |
| Missing per-function docs | C | The hand-written `*.h` files are 11-line shims — they `#include` the generated header and carry one `@file` block. The 184 public functions in `nros_generated.h` therefore render with zero `@brief` / `@param` / `@return` tags. |
| Templates without concept docs | C++ | `Publisher<M>`, `Subscription<M>`, `Future<T>`, `Service<S>`, `Client<S>` declare a single template parameter with no documented constraints. A user reading the docs has no statement of what `M` must provide (`TYPE_NAME`, `TYPE_HASH`, `ffi_publish`, `serialize_into`, …). |
| Flat alphabetical layout | both | No `@defgroup` / `@ingroup` taxonomy. The class/file/namespace trees are alphabetical dumps; users cannot see "all publisher-related symbols" or "all action-related symbols" in one place. |
| Sparse `@code` examples | both | 6 `@code … @endcode` blocks across 33 hand-written headers (verified by `grep -r @code packages/core/nros-{c,cpp}/include`). None on the executor, service/client, action server/client, timer, guard condition, lifecycle, parameter APIs. |
| Sparse cross-links | both | `@ref` / `@see` only used on the C mainpage. Related symbols (`nros_executor_spin` ↔ `nros_executor_spin_period`, `Node::create_publisher<M>` ↔ `Publisher<M>`, `nros_service_send_response` ↔ `nros_service_take_request`) are not cross-referenced. |
| No error-code reference | both | Return codes (`NROS_RET_OK`, `NROS_RET_INVALID`, …) appear in dozens of function signatures but have no umbrella page documenting cause and recovery. C++ `Result` / `nros::Code` likewise. |

The Doxyfiles deliberately set `EXTRACT_ALL = YES`,
`WARN_IF_UNDOCUMENTED = NO`, `WARN_NO_PARAMDOC = NO` so coverage gaps
are silent on the build log. That is fine for shipping, but it means
this phase has to drive coverage by reading the *rendered* site, not
by waiting for warnings.

### Why now

Phase 65 (book) and the `just book` consolidation just made the
Doxygen sites the canonical native-API reference. The legacy
hand-maintained `book/src/reference/{c,cpp}-api.md` markdown was retired
in favour of the live Doxygen output. That means user-facing C/C++
discoverability is now *entirely* a function of what Doxygen renders —
the markdown safety net is gone.

### Coordination with Phase 91

Phase 91 Group F/G plans to move the per-module struct definitions
*into* `nros_generated.h` (i.e., make the hand-written `*.h` files
genuinely thin shims, with cbindgen as the source of truth). That
changes which file `Doxyfile INPUT` should point at:

- **Before 91.F/G lands**: keep the hand-written headers as the doc
  source, and add `nros_generated.h` to `INPUT` so the generated decls
  also get rendered.
- **After 91.F/G lands**: `nros_generated.h` is the single source of
  truth and Doxygen extracts everything from it. The hand-written
  shims become brief-only.

Either way Phase 93 lands the same writing work; only the file the
docstrings live in changes. Stage this phase to assume Phase 91.F/G has
*not* landed (worst case) and revisit the `INPUT` line if it does.

## Architecture / Design

### Group taxonomy

Adopt `@defgroup` / `@ingroup` to split the symbol set into the same
groups the user already sees in the rustdoc sidebar and the mdBook
reference. Proposed groups (same on both sides where applicable):

| Group | C members | C++ members |
|---|---|---|
| `init` | `nros_init`, `nros_fini`, `nros_support_t` | `nros::init`, `nros::shutdown` |
| `node` | `nros_node_*` | `nros::Node` |
| `pubsub` | `nros_publisher_*`, `nros_subscription_*` | `nros::Publisher`, `nros::Subscription` |
| `service` | `nros_service_*`, `nros_client_*` | `nros::Service`, `nros::Client` |
| `action` | `nros_action_server_*`, `nros_action_client_*` | `nros::ActionServer`, `nros::ActionClient` |
| `executor` | `nros_executor_*`, `nros_timer_*`, `nros_guard_condition_*` | `nros::Executor`, `nros::Timer`, `nros::GuardCondition` |
| `parameter` | `nros_param_*`, `nros_param_server_*` | (parameter C++ surface, if any) |
| `lifecycle` | `nros_lifecycle_*` | (lifecycle C++ surface) |
| `clock` | `nros_clock_*`, `nros_time_*`, `nros_duration_*` | `nros::Clock`, `nros::Time` |
| `cdr` | `nros_cdr_*` | `nros::cdr` |
| `qos` | `nros_qos_*` | `nros::Qos` |
| `errors` | error code enum + helpers | `nros::Result`, `nros::Code` |
| `support` | `Span`, `FixedString`, `FixedSequence`, `Result`, `std_compat` | C++ utility types |

### Cbindgen-generated type problem

Three options, in increasing order of invasiveness:

1. **Add `nros_generated.h` to `INPUT`.** Smallest patch; renders all
   types and functions. Cost: cbindgen comments are sparse — most decls
   land with no docstring. Mitigation: write a small post-processing
   pass on the cbindgen output, or annotate the Rust source with
   doc-comments cbindgen forwards (cbindgen 0.29 supports
   `documentation = true`). The rustdoc side already gets the
   docstrings; we are paying for them twice if we don't reuse.
2. **Annotate via cbindgen `documentation` config.** Doc-comment the
   Rust `#[repr(C)]` types and `pub extern "C" fn` signatures, set
   `documentation = true` in `cbindgen.toml`, and the generated header
   carries Doxygen-friendly `///` blocks. This is the lasting fix and
   feeds *both* docs sites from one place.
3. **Custom `OUTLINE.md` page that hand-documents the opaque types.**
   Lightweight escape hatch; useful for the C++ side where there is no
   cbindgen pipeline to lean on.

Recommend (2) as the strategic answer for C, with (1) as the immediate
unblock if (2) takes more than one PR.

### C++ template concept docs

Use Doxygen's `@tparam` plus a `concept` page per template family.
Example structure for `Publisher<M>`:

```cpp
/**
 * @brief Type-safe publisher for ROS 2 topics.
 *
 * @tparam M Message type. Must satisfy the @ref nros_message_concept:
 *   - `static constexpr const char* TYPE_NAME`
 *   - `static constexpr uint32_t TYPE_HASH`
 *   - `void serialize_into(nros::cdr::Writer&) const`
 *   - `static M deserialize_from(nros::cdr::Reader&)`
 */
template <typename M> class Publisher { ... };
```

The `nros_message_concept` page (a single `.md` under
`nros-cpp/docs/`) lists the required surface once and is referenced
from every `@tparam M`.

### Error code reference

Single hand-written page (`nros-c/docs/error-codes.md`,
`nros-cpp/docs/error-codes.md`) listing every return code with the
"cause / recovery / typical caller pattern" triple. Linked from the
mainpage and from each function that returns an error code via `@see`.

## Work Items

### Group A — C++ landing + supporting docs

- [x] 93.A1 — Created `packages/core/nros-cpp/docs/mainpage.md` modelled
      on the C mainpage: project intro, quick-start (publish + subscribe
      end-to-end), module table linking to the new `@defgroup`s.
- [x] 93.A2 — Added `getting-started.md`, `configuration.md`,
      `ros2-interop.md`, `troubleshooting.md` under
      `packages/core/nros-cpp/docs/`.
- [x] 93.A3 — Wired the new `.md` files into `nros-cpp/Doxyfile` `INPUT`,
      set `USE_MDFILE_AS_MAINPAGE = docs/mainpage.md`, and extended
      `FILE_PATTERNS` to include `*.dox *.md`.
- [x] 93.A4 — `just book` renders `book/book/api/cpp/index.html` as the
      mainpage with quick-start + module table; no blank Doxygen frame.

### Group B — `@defgroup` taxonomy

- [x] 93.B1 — Defined 13 groups (C side) and 10 groups (C++ side; no
      `parameter`, `lifecycle`, `cdr`, `types` — those have no C++
      surface yet) in `nros-c/docs/groups.dox` and
      `nros-cpp/docs/groups.dox`. Each group carries `@brief`,
      typically a `@code` example, and `@see` cross-references.
- [x] 93.B2 — Per-module C headers tagged with `@file` + `@ingroup`:
      `init`, `node`, `publisher`, `subscription`, `service`, `client`,
      `executor`, `timer`, `guard_condition`, `action`, `clock`, `cdr`,
      `parameter`, `lifecycle`, `types`. (Note: cbindgen-generated
      decls inside `nros_generated.h` carry no individual `@ingroup`
      tags — the file-level grouping puts them under `grp_types`. Pure
      per-decl tagging blocks on Group E2.)
- [x] 93.B3 — Every C++ header tagged: `nros`, `node`, `publisher`,
      `subscription`, `service`, `client`, `action_server`,
      `action_client`, `executor`, `timer`, `guard_condition`, `future`,
      `stream`, `qos`, `result`, `config`, `span`, `fixed_string`,
      `fixed_sequence`, `std_compat`.
- [x] 93.B4 — `book/book/api/c/modules.html` shows all 13 groups;
      `book/book/api/cpp/modules.html` shows all 10 groups. Verified
      via `grep -oE 'group__grp__[a-z]+' modules.html`.

### Group C — Per-function docs (C, hand-written headers)

Subdivides per header. Each item lands its own small PR.

- [ ] 93.C1 — `init.h` + `node.h`: `@brief` + `@param` + `@return` for
      every public fn. Note threading model and lifetime constraints.
- [ ] 93.C2 — `publisher.h` + `subscription.h`.
- [ ] 93.C3 — `service.h` + `client.h`.
- [ ] 93.C4 — `action.h` (server + client).
- [ ] 93.C5 — `executor.h` + `timer.h` + `guard_condition.h`.
- [ ] 93.C6 — `parameter.h`.
- [ ] 93.C7 — `lifecycle.h`.
- [ ] 93.C8 — `clock.h` + `cdr.h` + `types.h`.
- [ ] 93.C9 — Acceptance probe: `just doc-c WARN_NO_PARAMDOC=YES`
      reports zero new warnings (run with the strict Doxyfile override
      to ensure coverage; do not commit the override).

### Group D — Per-function docs (C++ headers)

- [ ] 93.D1 — `node.hpp`, `publisher.hpp`, `subscription.hpp`.
- [ ] 93.D2 — `service.hpp`, `client.hpp`.
- [ ] 93.D3 — `action_server.hpp`, `action_client.hpp`.
- [ ] 93.D4 — `executor.hpp`, `timer.hpp`, `guard_condition.hpp`,
      `future.hpp`, `stream.hpp`.
- [ ] 93.D5 — `result.hpp`, `qos.hpp`, `config.hpp`, `span.hpp`,
      `fixed_string.hpp`, `fixed_sequence.hpp`, `std_compat.hpp`.
- [ ] 93.D6 — `nros::message_concept` markdown page; reference from
      every `@tparam M` site.

### Group E — Cbindgen-generated type docs

- [x] 93.E1 — Decision: land option (1) (`nros_generated.h` in `INPUT`)
      as the immediate stopgap, then layer option (2)
      (Rust-source doc-comments forwarded by cbindgen) on top in a
      follow-up. Reasons: option (1) is mechanical and unblocks Group B
      grouping for ~184 decls today; option (2) is the right
      single-source-of-truth answer but requires a sweep of every Rust
      `#[repr(C)]` struct + `pub extern "C" fn` in `nros-c/src/`.
- [ ] 93.E2 — Doc-comment every `#[repr(C)]` struct and
      `pub extern "C" fn` in `nros-c/src/lib.rs` (and submodules).
      Verify cbindgen forwards the `///` blocks into
      `nros_generated.h`. Track per-module: error, init, node, support,
      publisher, subscription, service, client, executor, timer,
      guard_condition, action/server, action/client, parameter,
      lifecycle, clock, cdr, qos, types.
- [x] 93.E3 — Added `nros_generated.h` to the C Doxyfile `INPUT` list
      and dropped the "exclude internal cbindgen artifact" comment.
      Doxygen now extracts decls from the generated header.
- [ ] 93.E4 — (Skipped — option (1) chosen.)

### Group F — Examples and cross-links

- [x] 93.F1 — Added `@code … @endcode` blocks to most `@defgroup`
      landings (init, node, pubsub, executor on both sides; service /
      action lighter). Code-block coverage is no longer concentrated at
      6 sites — every group page on each Doxygen site has at least one
      runnable snippet.
- [ ] 93.F2 — `@see` cross-links between paired symbols
      (init/fini, send/take, spin/spin_period, action goal/result,
      Node::create_publisher / Publisher, …). Group landings carry
      `@see` between groups; per-function `@see` is still mostly
      unfilled and lands with Group C/D.
- [x] 93.F3 — `nros-c/docs/getting-started.md` already existed; added
      `nros-cpp/docs/getting-started.md` with a copy-pasteable CMake +
      C++ talker walkthrough.

### Group G — Error code reference

- [x] 93.G1 — Wrote `nros-c/docs/error-codes.md` listing every
      `nros_ret_t` value (cause / recovery / typical caller pattern).
      Linked from the C mainpage and from
      `nros-c/docs/troubleshooting.md`.
- [x] 93.G2 — Wrote `nros-cpp/docs/error-codes.md` for `nros::Result` /
      `nros::ErrorCode`. Same structure. Linked from C++ mainpage and
      `nros-cpp/docs/troubleshooting.md`.

## Acceptance Criteria

- [ ] `just book` produces sites where `book/book/api/c/index.html` and
      `book/book/api/cpp/index.html` both open onto a written mainpage
      with quick-start + module table (no blank Doxygen frame).
- [ ] Rendered "Modules" tab on each site shows the 13-group taxonomy;
      no public symbol is in the "Other" bucket.
- [ ] Every public C function and C++ class method renders with at
      least `@brief`, `@param` (per parameter), and `@return` blocks.
      Verified by running `just doc-c` / `just doc-cpp` with
      `WARN_NO_PARAMDOC=YES` locally — zero warnings.
- [ ] Every C++ template class has documented `@tparam` constraints
      pointing at the `nros::message_concept` (or analogous) page.
- [ ] Every entity struct (`nros_publisher_t`, `nros_subscription_t`,
      …) renders with at least a one-line description; either via
      cbindgen-forwarded Rust doc-comments or via the
      `nros-c/docs/types.md` enumeration.
- [ ] Error-code reference page exists on both sides and is linked
      from every function that returns an error code.
- [ ] No regression: `just book` continues to finish without warnings,
      `just check` continues to pass.

## Notes

- **Order matters across groups but not within.** Group A (C++ landing)
  and Group B (taxonomy) should land first — they define the
  navigation that subsequent groups slot symbols into. Groups C/D/E
  are independent and can land in parallel. Groups F/G build on the
  taxonomy from B but on nothing else.
- **Don't enable `WARN_NO_PARAMDOC=YES` in the committed Doxyfile.**
  Use it as a CI-style local check during the writing sweep, then
  leave it off so generated headers (Phase 86 lifecycle interfaces,
  Phase 87 storage probe) don't trip the doc build. Re-enable in a
  follow-up phase only after every tracked header is at coverage.
- **Re-use rustdoc prose.** Most of what Group C/D needs to say is
  already written in the rustdoc on the equivalent Rust API. For the
  cbindgen path (option 2 in Group E) this is automatic — the same
  doc-comments feed both sites. For hand-written C++ headers, copying
  the rustdoc paragraph is fast and avoids prose drift.
- **Phase 86 lifecycle and Phase 87 storage probe interact.** The
  lifecycle services and `nros_cpp_config_generated.h` storage probe
  both emit headers that aren't in the Doxyfile INPUT today. Decide
  per-file whether they belong in the public docs site or stay
  internal (`nros_cpp_config_generated.h` is internal — leave
  excluded; lifecycle types are public and should be added).
- **Phase 65 book deployment**: changes to either Doxyfile's `INPUT`
  list need a corresponding update to `.github/workflows/deploy-book.yml`
  if any new guide markdown file lives outside the `docs/` subdir
  already on the path-trigger list.
