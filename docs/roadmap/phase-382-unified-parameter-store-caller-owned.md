# Phase 382 — one parameter store, caller-owned and alloc-free

**Status (2026-08-25). NOT STARTED — designed, EXPLORED, and re-planned.** The
first plan (W1–W6, below the line) would not have survived contact: three of its
load-bearing claims were wrong and one acceptance criterion was unsatisfiable
under its own mechanism. What follows is the corrected plan; the corrections are
kept rather than deleted because each names a trap an implementer would
otherwise re-discover.

Issue 0793 found two disjoint C parameter stores and asked which to delete. The
answer is neither: conciliate them into one API that keeps what each does well.

**Implements.** RFC-0045 (boot config), and amends RFC-0036's parameter row.
Closes the store half of issue 0793.

**Decided 2026-08-25: alloc-free is the target.** The store already is —
`cargo check -p nros-params --no-default-features` is clean, storage is
`[Option<ParameterEntry>; MAX_PARAMETERS]`, and names/strings/arrays are all
`heapless` and bounded at build time. The `alloc` lives entirely in the SERVICES
and is an accident of implementation: three `Box`es, every one about PLACEMENT
rather than dynamic sizing (a value too large for the stack, with the heap as the
nearest home). So an image with no heap will be able to answer `ros2 param`.

Note what this decision does NOT change: `impl ParameterVariant for
String`/`Vec<T>` stays `alloc`-gated, correctly — those are ergonomics for a
hosted caller, not capability.

The cost, stated plainly: W1' replaces generated `Serialize` calls with
hand-written field writes that can drift from the generated impls. The
round-trip oracle test is the mitigation, not a cure.

## The situation, measured

Two stores, and the consumer counts invert the obvious reading:

| | legacy `nros_param_server_t` | executor-owned (`nros_params::ParameterServer`) |
| --- | --- | --- |
| in-tree consumers | **3** — `nros::ParameterServer` (C++), `examples/native/c/parameters`, `custom-transport-loopback` | **0** |
| C entry points | all 10 `rcl_interfaces` types incl. arrays | 4 scalars |
| visible to `ros2 param` | **no** | yes |
| storage | **caller-owned** (`*mut nros_parameter_t` into a caller array) | `[Option<ParameterEntry>; MAX_PARAMETERS]`, build-time const |
| needs `alloc` | no | **yes** |

So the documented, exampled, C++-wrapped path is the invisible one, and the path
`ros2 param` can see has no users, no example, and a narrower C surface.

Two facts make conciliation cheap rather than a rewrite:

* **`nros_params::ParameterValue` covers all ten types**, arrays included — the
  4-scalar limit is in the C *entry points*, not the store. But see W0: for
  ARRAYS this is not a free adoption, and the first plan's claim that it "loses
  no capability" was wrong.
* **The store is already a fixed array**, not a heap structure. The `alloc::`
  uses in `nros-params` are edge conversions (`impl ParameterVariant for
  String`/`Vec<T>`), never storage.

## Why `param-services` requires `alloc` today — and why that is fixable

Not the store. Two separate reasons, both about placement rather than dynamic
sizing:

1. The executor holds `params: Option<alloc::boxed::Box<ParamState>>`.
2. The service handlers return `Box<Response>` because the response types are
   enormous — measured, not estimated: `GetParametersResponse` is **1,176,072**
   bytes, `DescribeParametersResponse` 55,304, `ListParametersResponse` 33,808.
   Two of the six box responses of **272 and 72 bytes**, which is pure waste.
3. **A third reason the first plan never listed:**
   `ParamState.services: Option<Box<dyn ParamServiceProcessor>>`, whose concrete
   `ParameterServiceServers` carries 6 × 2 × 4096 ≈ **49 KB** of buffers.

### And the request side is worse, and nothing addresses it

`ServiceTrait::handle_request_boxed` boxes only the REPLY:

```rust
let req = S::Request::deserialize(&mut reader)?;   // by value — a STACK local
let reply = handler(&req);                          // only this is boxed
```

`SetParametersRequest` is **1,192,968 bytes**. So every `ros2 param set` against
an nros node puts a 1.19 MB local on the calling task's stack today, on every
platform, unboxed — larger than the reply the boxing exists for, and four times
the whole 285 KB store issue 0756 was about. `param-services` is live on Zephyr.
Fixing only the reply leaves the worse half in place.

## The mechanism already exists

phase-271 moved six executor tables into caller-owned storage:
`ExecutorSizing { cbs, sc, arena }` names the capacities, `carve()` splits a
caller-supplied backing into typed sub-slices, `ExecutorSlices<'s>` is what the
executor borrows, and a `#[repr(C)] ExecutorStorage<CBS, SC, ARENA>` reference
layout is unit-tested against the carve. Issue 0563 added a seventh table
(remaps) the same way.

**The parameter table becomes the eighth carved region** — a fixed
`MAX_PARAMETERS` count, the shape issue 0563 used, NOT an `ExecutorSizing`
field. That yields alloc-freedom, caller placement of the backing, and
`ros2 param` visibility.

**There is no ninth table.** The first plan carved "the 32 KB response scratch";
exploration measured the responses and the number is wrong by ~37×
(`GetParametersResponse` is 1,176,072 bytes, not ~32 KB — and two handlers box
responses of 272 and 72 bytes). The right answer is not a bigger scratch, it is
not to build the value at all — see W1' below.

**Caller-chosen CAPACITY is deferred to its own phase.** `ExecutorSizing` is a
public `Copy` struct with public fields built by struct literals, and
`executor_storage_layout(cbs, sc, arena)` is `pub` with three positional args and
five in-tree callers. Worse, `ExecutorInlineStorage.backing` sizes the C/C++
`_opaque`: `NROS_EXECUTOR_SIZE` is 89,576 today, and a default 32-slot table
would add ~273 KB — a 4× `.bss` growth for every C/C++ image whether it uses
parameters or not, flowing straight through the sizes-header mirror (the
0088→0114→0122→0123→0245→0268 recurrence class). `cfg`-gating it on
`param-services` to dodge that is worse: it makes the executor's SIZE
feature-dependent, which is issue 0665's probe-vs-link trap at 273 KB instead of
16 bytes.


## Work items (re-planned after exploration)

### W0 — settle the ARRAY question. **DECIDED 2026-08-26: borrowed variant alongside copying.**

The unified store carries BOTH: the copying `heapless::Vec<T, MAX_ARRAY_LEN>` and
a BORROWED `{ data, len }` variant that records the caller's pointer verbatim.
W6' is therefore a MIGRATION, not a break.

Why borrowed survives rather than being folded away:

* It IS caller capacity and placement — the property this phase set out to keep.
  The copying variant forfeits both, and does so precisely on the values large
  enough for it to matter.
* The legacy store has NO array cap, because the memory is the caller's.
  Unifying on `heapless::Vec<T, 32>` would be a capability LOSS, not merely an
  ABI change: arrays over 32 elements stop being expressible at all.
* `parameter.h:261-287` documents pointer identity as load-bearing, and
  `nros/parameter.hpp` recovers each block's capacity from an out-of-band word
  in FRONT of the returned pointer. Breaking it fails every C++
  `set_parameter(Seq)` and the live `parameters_roundtrip` test.

**The cost, stated plainly: a borrowed array can dangle.** The store does not own
the buffer and cannot observe the caller freeing it, where the copying variant is
memory-safe by construction. This is the tradeoff the decision accepts, so the
implementation owes it a mitigation rather than a comment — the borrowed variant
must be reachable ONLY through the declare-time C entry points, whose contract
already says the caller's storage must outlive the node, and must never be
constructible from a wire `set` (see W5': a remote set may not choose placement).



`parameter.h:261-287` documents array pointer identity as **load-bearing**, not
incidental: `nros_param_declare_*_array` records the caller's `data` pointer
verbatim and never copies, `get` returns *that same pointer*, and
`nros/parameter.hpp` recovers each block's capacity by reading an out-of-band
word immediately in FRONT of the returned pointer. The header says in as many
words that changing this requires changing `parameter.hpp` in the same commit.

`nros_params::ParameterValue` **copies** into `heapless::Vec<T, MAX_ARRAY_LEN=32>`.
So naive unification:

1. caps arrays at 32 elements, where the legacy store has no cap at all because
   the memory is the caller's;
2. breaks pointer identity, so every C++ `set_parameter(Seq)` fails — loudly,
   thanks to the defensive pool check, but completely;
3. is pinned by a live test (`parameters_roundtrip` greps
   `mpc_weights[0]=4.000000 n=4`).

Decide: keep a BORROWED array variant (pointer + len) in the unified store
alongside the copying one, or accept a breaking C ABI change and do the
capacity-in-server-state fix `parameter.h` already names as "the proper fix".
This determines whether W6' is a migration or a break.

### W1' — the streaming service seam. **LANDED 2026-08-25.**

`ServiceTrait::handle_request_raw` (`nros-rmw/src/traits.rs`) plus the
`ServiceServerHandle` wrapper. All 11 handlers stream — 6 parameter, 5
lifecycle. No `alloc` on any streaming path.

**The defect was bigger than this doc first said.** `handle_request_boxed` boxes
the REPLY and leaves the REQUEST as a by-value stack local — and
`SetParametersRequest` (1,192,968 B) is LARGER than the reply the boxing exists
for. Every `ros2 param set` put 1.19 MB on the calling task's stack.

Dominant local per handler (`size_of`; the largest single local, NOT a frame
total):

| handler | request, was on stack | reply, was boxed | largest local now |
| --- | ---: | ---: | ---: |
| `get_parameters` | 16,904 | 1,176,072 | **0** |
| `set_parameters` | **1,192,968** | 17,416 | **8,464** |
| `set_parameters_atomically` | **1,192,968** | 272 | **8,464** |
| `list_parameters` | 16,912 | 33,808 | **2,064** |
| `describe_parameters` | 16,904 | 55,304 | **0** |
| `get_parameter_types` | 16,904 | 72 | **0** |
| `change_state` | 272 | 272 | **24** |
| `get_state` | 272 | 272 | **24** |
| `get_available_states` | 272 | 17,416 | **24** |
| `get_available_transitions` | 272 | 52,232 | **24** |
| `get_transition_graph` | 272 | 52,232 | **24** |

Headline: `set_parameters` goes 1,192,968 -> 8,464, **141x**.

**This does NOT yet make a heapless image build.** The three `compile_error!`s
in `nros-node/src/lib.rs` stay. W1' removed the per-REQUEST allocation; the
per-NODE ones (`Box<ParamState>`, `Box<dyn ParamServiceProcessor>`, the
lifecycle processor box) are W3'/W4'.

Two things to keep in mind when reading this code:

* **`SetParametersAtomically` needs the request twice** (validate all, then apply
  all) and `CdrReader` cannot seek. It takes the body as a borrowed slice and
  builds a fresh reader per pass — exact only while that slice begins at the
  reader's alignment ORIGIN. Break it and CDR padding shifts silently, with no
  other symptom. So it is ENFORCED, not documented: `CdrReader::is_at_origin()`
  plus a hard check in the handler, and a field read added above that line fails
  loudly. Inverting the check fails exactly one test, so it is on a live path.
* **Both halves are guarded by round-trip oracle tests.** The old by-value
  handlers survive as `#[cfg(test)]` and each `*_streams_like_the_oracle` asserts
  the streamed bytes are byte-identical to the generated `Serialize` AND
  deserialize back equal. Both were mutation-checked — swapped fields, altered
  reason strings, raw byte runs in place of `write_string` — and the mutations
  turn tests red, so the oracles constrain the writes rather than merely
  exercising them.

### W1' original plan (kept for the reasoning)

Add `ServiceTrait::handle_request_raw`, taking `&mut CdrReader` / `&mut CdrWriter`
instead of a by-value request and a boxed reply. It works because:

* **No dheaders anywhere in `rcl_interfaces`** — plain sequential CDR, so
  hand-written field writes are byte-identical to the generated impls;
* `EmbeddedServiceServer` owns `req_buffer` and `reply_buffer` as disjoint
  fields, so a handler can hold `&req` and `&mut reply` at once;
* `CdrReader::read_string()` already borrows from the receive buffer.

`GetParameters` then needs **no scratch at all**: read the count, per name borrow
the `&nros_params::ParameterValue` from the store and write its fields straight
out. No wire value is ever constructed. `SetParameters` applies one parameter at
a time; peak stack becomes one internal `ParameterValue` (8,464 B) — **141×
better than today's 1.19 MB**.

**Verified 2026-08-25, and it is broader than stated:** `grep -rn begin_dheader
packages/interfaces/` returns nothing across all 64 generated serializers, so NO
generated message in the tree uses a DHEADER, not merely `rcl_interfaces`. The
hand-written writes are therefore byte-identical to the generated impls today.

The tripwire for tomorrow is the round-trip test itself: if codegen ever emits
XCDR2 extensibility for a streamed message, its generated `Deserialize` starts
expecting a DHEADER the streaming handler does not write, and the round-trip
assert fails. That only holds while the test covers EVERY streamed message —
which is why the acceptance below is per-handler rather than a sample.

Guard the one real risk — hand-written writes drifting from the generated
`Serialize` — with a round-trip test that deserialises the streamed bytes back
into the generated type, keeping today's by-value handler as a test-only oracle.

`handle_request_boxed` is also used by **lifecycle_services** (5 sites, its own
`compile_error!`), so this seam fixes both. Scope it that way from the start.

### W2' — `ParameterServer` borrows its table

`[Option<ParameterEntry>; MAX_PARAMETERS]` → `&'s mut [Option<ParameterEntry>]`.
Known fallout, all of which the first plan missed:

* **`ParameterEntry` is private** and the carve lives in `nros-node`, which
  cannot name it. Decide up front: make it `pub`, move the carve into
  `nros-params`, or add a `ParameterTable<'s>` newtype. This is the first wall.
* `impl Default` must go; `pub const fn new()` becomes `new_in(&mut [...])` —
  20+ call sites including the lib.rs doctest, `ghost_checks`, and the
  **`#[cfg(kani)]` proofs**, so `just verify-kani` must be re-run.
* `ghost.max == 32` is asserted literally and becomes `s.entries.len()`.
* Six builder types gain a second lifetime (`LegacyParameterBuilder`,
  `ParameterBuilder`, `MandatoryParameter`, `OptionalParameter`,
  `ReadOnlyParameter`, `UndeclaredParameters`), rippling to
  `Executor::parameter`.
* `init_in_place` becomes dead — its whole reason (issue 0756's 285 KB stack
  temporary) dissolves. Move its rationale into the carve's comment rather than
  deleting the history.
* `Executor` is already `Executor<'s>`, so `params: Option<ParamState<'s>>` is
  expressible. That part works.

### W3' — carve it, as a FIXED region

`MAX_PARAMETERS` count, no `ExecutorSizing` change — the issue-0563 shape.
Report `NROS_EXECUTOR_SIZE` before and after in the commit, as 0563 did.

**Extend the layout test first.** `layout_matches_typed_repr_c` asserts only the
whole struct's `size` and `align` — a carve that permuted two same-size tables
would pass. The first plan called it "what keeps the carve and the `#[repr(C)]`
reference honest"; it does not. Add per-field `offset_of!` assertions (stable
since 1.77). Note `carve_yields_right_lengths_and_inits` is `#[cfg(feature =
"alloc")]`, so an alloc-free table's carve test needs a home outside that gate.

W3's original wording — "`ParamState` inline rather than `Box`ed" — would trip
`executor_stays_small_enough_to_construct_on_a_stack` (`size_of::<Executor>()
≤ 6 KiB`) by 285 KB. It must be **carved**, never inline.

### W4' — remove the remaining `alloc` reasons

De-`Box` `ParamState` and `ParameterServiceServers` (the ~49 KB of buffers needs
a home — carve it, or the `compile_error!` cannot go). Then delete
`nros-node/src/lib.rs:250`'s `compile_error!` and prove `param-services` builds
without `alloc`.

### W5' — one C surface, and the veto on the path a remote set takes

There are **three** C-visible families, not two: `nros_param_*`,
`nros_executor_*_param_*`, and `nros_cpp_register_parameter_services` /
`nros_cpp_declare_param` / `nros_cpp_get_param_*` in `params_shim.rs`.

`nros_params` has **no callback concept at all** — there is no slot to move the
veto into. Adding one brings three sub-problems the first plan did not see:

* `SetParameterResult` has no "rejected by callback" variant, and
  `impl From<SetParameterResult> for ParameterError` ends in
  `_ => panic!(...)` — so the first remote rejection would **panic the node**.
  Fix in the same commit.
* `handle_set_parameters_atomically` bypasses `set` in its pre-check, so a veto
  living only inside `set` would be skipped during validation and fire during
  apply, **breaking atomicity**. It needs a `would_accept(&self, name, &value)`
  the pre-check also calls.
* rclcpp's equivalent returns a *reason string* and supports *multiple*
  callbacks; ours is one `bool` slot. Decide here, because W7's rename rows
  depend on it.

### W6' — the consumers, including two that have no executor

* `nros::ParameterServer<Cap>` owns `nros_parameter_t storage_[Capacity]`
  inline, is non-movable, is constructed with **no executor handle**, and
  **discards the init return**. A borrow from executor storage has no handle to
  take and nowhere to report failure.
* `ComponentNode` embeds it at `Capacity = 256` — 256 × 8,536 B ≈ **2.2 MB** if
  it becomes the unified type. It also has **no setter of any kind**. And
  `adopt_launch_seed_` exists *only* because the two stores are separate
  (issue 0745); unification should delete it, or it double-writes.
* **`examples/native/c/parameters` has no executor, no node and no
  `nros_support_init`.** There is no migration that keeps its spelling — an
  executor-owned store cannot serve a program with no executor. Either the
  example gains a runtime (changing what it demonstrates, and its fixture) or
  the standalone store survives for exactly this shape.
* `custom-transport-loopback` is worse: the server is a member of a file-scope
  struct bulk-`memset` to zero and `init`ed *before* `nros_support_init`.

### W7' — the ledger

32 `gap` and 22 `rename` rows in `param.json`, several asserting the split this
phase removes. `just check-api-parity` stays green.

### Deferred to its own phase — caller-chosen CAPACITY

`ExecutorSizing` + `executor_storage_layout`'s positional args + `_opaque` +
the sizes-header mirror. It is a C-ABI change, not a table, and bundling it is
what made this phase look four times its size.

## Acceptance (corrected)

* A parameter declared through the one C API is visible to `ros2 param list` and
  settable by `ros2 param set`, in a live interop cell.
* The accept/reject callback fires for a remote set — and for the atomic path.
* `param-services` builds with no `alloc`.
* `SetParametersRequest` no longer appears as a stack local: measure the
  handler's peak stack before and after, and put both numbers in the commit.
* ~~"A caller can place the parameter table in a named linker section, and there
  is an example that does."~~ **Unsatisfiable as written and removed.** Under the
  eighth-table design the table lives in the *executor's* backing, so a caller
  places the whole backing or nothing. Either restate it as "the caller places
  the executor backing, parameters included" — which is true and already
  testable — or give the table its own buffer, at which point it is not the
  eighth table and W3' is a different design.

## What this deliberately does not do

Domain and locator in the baked boot config (issue 0794) are a different
producer question. The `EnvRung` asymmetry noted there belongs in RFC-0045.

`MAX_PARAMS_PER_REQUEST = 64` must equal a frozen codegen literal, and the
store-vs-wire mismatch (`MAX_ARRAY_LEN=32` against wire 64) is issue 0323's live
behaviour. Streaming makes it moot for the handlers; unification does **not**
remove it.
