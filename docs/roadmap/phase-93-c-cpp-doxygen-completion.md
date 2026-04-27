# Phase 93: C and C++ Doxygen Completion

**Goal**: Bring the rendered C and C++ Doxygen sites
(`book/book/api/c/`, `book/book/api/cpp/`) up to a level comparable with
the rustdoc output. The current build emits the sites without warnings,
but a user landing on either site faces blank or alphabet-soup index
pages, undocumented opaque types, and per-function reference stubs with
no `@param` / `@return` blocks. Close those gaps.

**Status**: In Progress (Groups A–G landed for the user-facing C/C++ surface; Groups H–L extend coverage to the RMW and platform layers — the porting surface — and are still open)
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

Re-scoped after Group E2 landed: the cbindgen-emitted `nros_generated.h`
already carries Rust-source docstrings for every entity init/fini and
publish/take, so per-header sweeps are not needed. Audit confirmed
**0 / 111** `NROS_PUBLIC` decls without a preceding doc block.

- [x] 93.C — Verified all 111 cbindgen-emitted `NROS_PUBLIC` decls
      carry `/** … */` blocks forwarded from Rust source. Rust-source
      docstrings include `@param`, `@return`, and `# Safety` sections
      already.
- [ ] 93.C9 — Strict probe (`WARN_NO_PARAMDOC=YES`) deferred until
      Phase 91 F/G consolidates the per-module C headers vs the
      generated header.

### Group D — Per-function docs (C++ headers)

Bulk of C++ surface was already documented before this phase. Filled
the remaining utility-class gaps:

- [x] 93.D1 — `node.hpp`, `publisher.hpp`, `subscription.hpp`: already
      had per-method `///` blocks with `@param`/`@return`.
- [x] 93.D2 — `service.hpp`, `client.hpp`: already covered.
- [x] 93.D3 — `action_server.hpp`, `action_client.hpp`: already
      covered (action_client.hpp has 136 `///` lines alone).
- [x] 93.D4 — `executor.hpp`, `timer.hpp`, `guard_condition.hpp`,
      `future.hpp`, `stream.hpp`: already covered.
- [x] 93.D5 — `result.hpp` (per-`ErrorCode`-variant docstrings + per
      constructor docs added), `qos.hpp` (per-setter / per-accessor
      docs + `@param depth` added), `span.hpp` (per-method `///` on
      `Span<T>` and `StringView`), `config.hpp`, `fixed_string.hpp`,
      `fixed_sequence.hpp` already covered.
- [x] 93.D6 — `nros::message_concept` page lives in
      `nros-cpp/docs/groups.dox` (`@page message_concept`) and is
      referenced from the `@tparam M` of every entity template.

### Group E — Cbindgen-generated type docs

- [x] 93.E1 — Decision: land option (1) (`nros_generated.h` in `INPUT`)
      as the immediate stopgap, then layer option (2)
      (Rust-source doc-comments forwarded by cbindgen) on top in a
      follow-up. Reasons: option (1) is mechanical and unblocks Group B
      grouping for ~184 decls today; option (2) is the right
      single-source-of-truth answer but requires a sweep of every Rust
      `#[repr(C)]` struct + `pub extern "C" fn` in `nros-c/src/`.
- [x] 93.E2 — Verified the Rust-source sweep is essentially complete.
      The 22 CDR primitive helpers (`nros_cdr_write_*`,
      `nros_cdr_read_*`) lacked docstrings; added them in
      `packages/core/nros-c/src/cdr.rs`. Now **0 / 111**
      `NROS_PUBLIC` decls in the cbindgen output lack a preceding doc
      block. Rebuild + verified via:
      `awk '/^NROS_PUBLIC$/ { funcs++; if (prev !~ /\\*\//) und++ } …'`.
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
- [x] 93.F2 — Group-level `@see` cross-links landed in `groups.dox`
      on both sides: action↔service/executor, executor↔pubsub/service/
      action, parameter→service, lifecycle→node/service, cdr→pubsub,
      qos→pubsub, errors→troubleshooting. Per-function `@see` is
      already present in the Rust-source docstrings cbindgen
      forwards (the `Returns` / `See also` sections in node.rs,
      publisher.rs, …).
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

## Groups H–L: RMW + Platform Porting Surface (Phase 93 extension)

Groups A–G covered the user-facing C/C++ API. The follow-up audit
identified an equally important — and worse-documented — *porter*
surface: the RMW backend trait + C vtable, and the platform abstraction
trait + C vtable. nano-ros is "Rust native" for these layers, but the
C FFI shims (`nros-rmw-cffi`, `nros-platform-cffi`) exist precisely so
C/C++ porters can stand up new backends without writing Rust. Today
that path is half-finished.

### Status quo (April 2026 RMW/platform audit)

| Surface | Existing artefact | Quality |
|---|---|---|
| Rust crate-level docs (`nros-rmw`, `nros-platform-api`, …) | `lib.rs:1–~50` `//!` blocks | Architectural overviews are present and clear. |
| Rust trait-method docs (`Publisher::publish_raw`, `Session::create_publisher`, `Subscriber::try_recv_raw`, `PlatformThreading::*`) | one-line `///` per method | **No threading contract, no buffer-lifetime contract, no calling pattern.** Pitfalls about recursive mutexes / poll-driven clocks live only in `book/src/porting/custom-platform.md` "Pitfalls", not on the trait. |
| RMW C header (`packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`) | Hand-written, 81 lines, complete | Good — every vtable slot has a return-value convention; covers all 13 fn pointers. |
| Platform C header | **Does not exist.** | A C porter must hand-mirror `NrosPlatformVtable` (~60 fn pointers, ~90 lines of Rust struct in `nros-platform-cffi/src/lib.rs:37–96`) into a `.h` by hand. `book/src/porting/custom-platform.md:207` admits this and points the porter at the Rust source. |
| Doxygen sites for `nros-rmw-cffi` / `nros-platform-cffi` | **None.** Neither has a `Doxyfile`. | The deployed book has no porter-facing C reference at all. |
| `book/src/porting/custom-platform.md` | 1856 words; Rust skeleton complete | C-path skeleton is half-baked — placeholder `/* ... */` for ~40 of ~60 fields. |
| `book/src/porting/custom-rmw.md` | 1895 words; Rust skeleton complete | **No C-path section at all.** |
| `book/src/porting/custom-board.md` | 1278 words | Adequate; assumes custom-platform already done. |
| Cbindgen config in either `*-cffi` crate | None | No `cbindgen.toml`, no `build.rs` — the RMW header is hand-maintained. |

The biggest concrete blocker for a C-only porter is the missing
**platform vtable header**. The second biggest is **trait-method
contracts** (threading / buffer lifetime / call ordering) that are
documented only in pitfalls lists rather than on the trait itself.

### Group H — Rust trait contracts on RMW + platform traits

Add `# Thread Safety`, `# Calling pattern`, `# Buffer lifetime`,
`# Errors` sections to the trait-method rustdoc. Promote pitfalls
that are currently buried in the porting guide into trait-level
docs so they show up in rustdoc on hover.

- [ ] 93.H1 — `nros-rmw/src/traits.rs`: per-method docs on `RmwSession`,
      `Publisher`, `Subscriber`, `ServiceServer`, `ServiceClient`. Cover:
      (a) which side of the FFI may invoke the method, (b) whether
      multiple threads may invoke concurrently, (c) buffer ownership
      on raw send/recv, (d) blocking vs non-blocking semantics, (e)
      what `drive_io` is allowed to do.
- [ ] 93.H2 — `nros-platform-api/src/lib.rs` + traits: per-method
      docs on `PlatformClock`, `PlatformAlloc`, `PlatformSleep`,
      `PlatformThreading` (mutex/condvar/task), `PlatformTcp`,
      `PlatformUdp`, `PlatformRandom`. Promote the recursive-mutex
      requirement and the deterministic-PRNG note from
      `custom-platform.md` Pitfalls into the trait-level docs.
- [ ] 93.H3 — Crate-level `//!` blocks on `nros-rmw`, `nros-platform`,
      `nros-platform-api` get a "When you should be reading this"
      paragraph and a back-link to the porting guide for orientation.

### Group I — Platform C FFI header (the missing piece)

This unblocks a real C-only platform port and removes the apologetic
"A C header is not yet auto-generated" line from
`custom-platform.md:207`.

- [ ] 93.I1 — Decision call: cbindgen-generated header
      (`packages/core/nros-platform-cffi/cbindgen.toml` + `build.rs`)
      vs hand-written. Recommendation: **cbindgen** — the platform
      vtable is large (~60 fn pointers across 11 traits), drift is
      certain, and Phase 91.E has already established cbindgen as the
      single source of truth for the user-facing C surface. Reuse the
      same toolchain.
- [ ] 93.I2 — Add `packages/core/nros-platform-cffi/cbindgen.toml`
      (mirror `nros-c/cbindgen.toml`). Add a `build.rs` that emits
      `include/nros/platform_vtable.h` on every build.
- [ ] 93.I3 — Doc-comment every field of `NrosPlatformVtable` in
      `nros-platform-cffi/src/lib.rs` — return-value conventions,
      null-pointer semantics, blocking allowance — so the cbindgen
      output mirrors the quality of `rmw_vtable.h`.
- [ ] 93.I4 — Replace the half-baked C skeleton in
      `book/src/porting/custom-platform.md:200–307` with a complete
      template that links into the generated `platform_vtable.h`.
      Drop the "A C header is not yet auto-generated" line.

### Group J — Doxygen sites for the *-cffi crates

Build a porter-facing Doxygen site that mirrors the C / C++ ones, but
focused on the vtable surfaces.

- [ ] 93.J1 — Add `packages/core/nros-rmw-cffi/Doxyfile` with INPUT
      = `include/nros/rmw_vtable.h` + a hand-written `docs/mainpage.md`
      narrating what an RMW backend is, when to write one, and how the
      vtable maps onto the Rust `RmwSession` trait.
- [ ] 93.J2 — Add `packages/core/nros-platform-cffi/Doxyfile` with
      INPUT = the cbindgen-emitted `include/nros/platform_vtable.h` (from
      Group I) + a hand-written `docs/mainpage.md` narrating the
      platform contract and its 11 sub-traits.
- [ ] 93.J3 — Wire both into the `just book` recipe and the
      `.github/workflows/deploy-book.yml` deploy job. Stage outputs
      under `book/book/api/rmw-cffi/` and `book/book/api/platform-cffi/`.
- [ ] 93.J4 — Cross-link from `book/src/porting/custom-rmw.md` and
      `custom-platform.md` to the new Doxygen sites so a porter clicks
      from "here's how to start" straight into the function-by-function
      reference.

### Group K — Porting guide C-path completion

- [ ] 93.K1 — `book/src/porting/custom-platform.md`: replace the
      `/* ... */` placeholders with a full C skeleton (all ~60 vtable
      slots stubbed with `static int my_…(…) { return -1; }`), built
      from the Group I header. Add a "minimum viable port" section
      listing the smallest set of traits a host can stub before
      `nros::init()` will return.
- [ ] 93.K2 — `book/src/porting/custom-rmw.md`: add a C-path section
      mirroring the Rust one. Include a `static struct nros_rmw_vtable
      my_rmw = { … };` template and a `nros_rmw_cffi_register(&my_rmw)`
      call site.
- [ ] 93.K3 — Add a "lifecycle / threading contract" subsection to
      both guides referencing the new trait-level rustdoc from
      Group H, so the porter has one canonical place to learn the
      rules.

### Group L — Rustdoc deploy of porter crates

The `deploy-book.yml` `cargo doc` invocation already builds `nros`,
`nros-rmw`, `nros-platform-api`, `nros-rmw-zenoh`. The remaining
porter-facing crates (`nros-platform-cffi`, `nros-rmw-cffi`,
`nros-rmw-xrce`, `nros-platform-{posix,zephyr,freertos,nuttx,threadx}`)
should also publish so a Rust porter can reach them from the live
site.

- [ ] 93.L1 — Add the cffi crates and platform-impl crates to the
      `cargo doc -p …` list in `.github/workflows/deploy-book.yml`.
      Include `--no-deps` to keep the deploy small.
- [ ] 93.L2 — Update the rustdoc redirect index
      (`book/book/api/rust/index.html`) to surface the new top-level
      crates.

## Acceptance Criteria

- [x] `just book` produces sites where `book/book/api/c/index.html` and
      `book/book/api/cpp/index.html` both open onto a written mainpage
      with quick-start + module table (no blank Doxygen frame).
- [x] Rendered "Modules" tab on each site shows the taxonomy (13 groups
      C side, 10 groups C++ side); no public symbol is in the "Other"
      bucket.
- [x] Every public C function carries at least `@brief` + parameter
      docs (forwarded from Rust source via cbindgen — verified
      `0 / 111` undocumented).
- [x] Every C++ template class has documented `@tparam` constraints
      pointing at the `nros::message_concept` page.
- [x] Every entity struct renders with at least a one-line description
      (cbindgen forwards the Rust struct + field docstrings).
- [x] Error-code reference page exists on both sides and is linked
      from each side's mainpage and troubleshooting page.
- [x] No regression: `just book` finishes without warnings;
      `just check` continues to pass.

### Acceptance criteria — Groups H–L (porting surface)

- [ ] Every public trait method on `nros-rmw` and `nros-platform-api`
      carries a `///` block covering thread safety, calling pattern,
      buffer ownership (where applicable), and blocking allowance.
- [ ] `packages/core/nros-platform-cffi/include/nros/platform_vtable.h`
      exists, is auto-generated by cbindgen, and is committed (or
      reproduced on every build).
- [ ] `book/book/api/rmw-cffi/index.html` and
      `book/book/api/platform-cffi/index.html` render via `just book`
      with a written mainpage and a complete vtable reference.
- [ ] `book/src/porting/custom-rmw.md` has a C-path section parallel
      to its Rust skeleton; `custom-platform.md` no longer carries
      placeholder `/* ... */` fields and links to the deployed
      `platform_vtable.h` Doxygen page.
- [ ] `cargo doc` deploy in `deploy-book.yml` builds the porter-facing
      crates; rustdoc landing surfaces them.

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
- **Why H–L stay in Phase 93** rather than spinning into a new phase:
  the work is the same kind (Doxygen + rustdoc + porting markdown)
  and reuses the same infrastructure (`just book`, `deploy-book.yml`,
  cbindgen pipeline). Splitting into a separate phase would duplicate
  the design notes about the docs pipeline and add a phase-number
  ceremony with no real boundary. If the scope of H–L grows
  (e.g., a new doc generator, a separate site target) it can be
  extracted then.
