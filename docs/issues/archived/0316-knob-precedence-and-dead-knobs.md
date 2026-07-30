---
id: 316
title: "Compile-time pool knobs silently do nothing: Kconfig overwrites the environment, and five XRCE knobs are exported under names nothing reads"
status: resolved
type: bug
area: build
related: [issue-0135, issue-0269, issue-0271]
---

## Finding (2026-07-28)

A "knob" here is a compile-time static pool size — a slot count, ring depth or
buffer width baked into `.bss` at build time. There are **61** of them read by
Rust build scripts across the tree.

Two separate defects make a knob you set silently do nothing. Both were found
while chasing the remaining 14,404 bytes of issue 0271, and both have the same
shape: a value is *accepted* by one mechanism and *discarded* by another, with
no diagnostic in between.

### Defect 1 — Kconfig silently overwrites the environment

`zephyr/cmake/nros_cargo_build.cmake` bridges Kconfig to the cargo environment
with unconditional writes:

```cmake
set(ENV{ZPICO_MAX_PENDING_GETS} "${CONFIG_NROS_MAX_PENDING_GETS}")
set(ENV{NROS_EXECUTOR_MAX_CBS} "${CONFIG_NROS_EXECUTOR_MAX_CBS}")
```

There is no `if(NOT DEFINED ENV{...})` guard. So on the Zephyr path, 20 knobs
have their environment value **overwritten by the Kconfig default**, while the
other 41 pass through untouched. Two classes with opposite precedence, spelled
identically at the call site, with nothing marking which class a knob is in.

The user-visible symptom: you export a knob, the build succeeds, the value is
ignored, and no output says so.

**This is live in `autoware_sentinel`.** Its SPE image is a Zephyr build
(`west build -b native_sim/native/64`) and its justfile sets 24 knobs. Six are
in the shadowed class with no matching `CONFIG_` in any `.conf`:

| knob | justfile asks | actually compiled |
| --- | --- | --- |
| `NROS_EXECUTOR_MAX_CBS` | 96 | Kconfig default |
| `ZPICO_MAX_PENDING_GETS` | 1 | Kconfig default |
| `ZPICO_MAX_PUBLISHERS` | 40 | Kconfig default |
| `ZPICO_MAX_QUERYABLES` | 2 | Kconfig default |
| `ZPICO_MAX_LIVELINESS` | 80 | Kconfig default |
| `ZPICO_MAX_SUBSCRIBERS` | 4 | **16 / 32** |

The corroborating detail: the four knobs that produced issue 0271's measured
91% reduction (`ZPICO_SUBSCRIBER_LARGE_SIZE`, `ZPICO_MAX_LARGE_SUBSCRIBERS`,
`ZPICO_SUBSCRIBER_RING_DEPTH`, `NROS_RMW_SUBSCRIBER_SLOTS`) are all in the
*pass-through* class. The ones that moved nothing are exactly the shadowed
ones. The justfile even carries a comment that `west build` does not inherit
`.env` — someone hit an adjacent symptom and worked around it without finding
this.

### Defect 2 — five XRCE knobs are exported under names nothing reads

`nros_cargo_build.cmake:124-128` exports the **unprefixed** spellings:

```cmake
set(ENV{XRCE_MAX_SUBSCRIBERS} "${CONFIG_NROS_XRCE_MAX_SUBSCRIBERS}")
set(ENV{XRCE_BUFFER_SIZE}     "${CONFIG_NROS_XRCE_BUFFER_SIZE}")
```

The only reader is `packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs:238,258-274`,
which reads the **`NROS_`-prefixed** names. `xrce-sys/build.rs` reads only
`XRCE_TRANSPORT_MTU` and `XRCE_MAX_SESSION_*`, and
`zephyr/cmake/nros_rmw_xrce.cmake` sets no defines at all.

So `CONFIG_NROS_XRCE_{MAX_SUBSCRIBERS, MAX_SERVICE_SERVERS, MAX_SERVICE_CLIENTS,
BUFFER_SIZE, STREAM_HISTORY}` are inert on Zephyr — five menuconfig options
that appear to work and do nothing. The C defaults in `internal.h:42-88` always
win.

Four of the five Kconfig defaults happen to equal the C defaults, so nothing
visibly broke. `STREAM_HISTORY` does not: Kconfig says 4, C says 16. Repairing
the name alone would therefore shrink every Zephyr XRCE stream buffer 4×
(`XRCE_STREAM_BUFFER_SIZE = MTU × STREAM_HISTORY`) as a side effect of a
rename. The Kconfig default must be aligned to the compiled value in the same
change.

### Defect 3 — configure-time `set(ENV{...})` never reaches cargo

Found while verifying the fix for defects 1 and 2, and it is the largest of the
three.

`set(ENV{X})` changes the environment of the **cmake configure process**. Cargo
runs at **build** time, under ninja. The only knob values that actually reach a
`build.rs` are the ones named explicitly in the `add_custom_target` command,
whose `$ENV{}` is expanded at configure time and baked into the build rule:

```cmake
COMMAND ${CMAKE_COMMAND} -E env
    ZPICO_MAX_PUBLISHERS=$ENV{ZPICO_MAX_PUBLISHERS}
    ...
```

That list was hand-maintained, and it is a *third* place each knob's spelling
appears. It had drifted in both directions:

- The five XRCE entries used the unprefixed spelling from defect 2 — so even
  after fixing the export, the value forwarded to cargo still carried a name no
  `build.rs` reads.
- **`NROS_EXECUTOR_MAX_CBS` and the RFC-0049 tx trio were absent entirely.**
  They were resolved into the configure environment and then dropped on the
  floor. `CONFIG_NROS_EXECUTOR_MAX_CBS` was therefore completely inert on the
  Zephyr C path: every image compiled `nros-node/build.rs`'s own default of 4,
  no matter what menuconfig said.

This also explains why a knob exported in the shell *does* work while the same
knob set in Kconfig does not: ninja inherits the invoking shell's environment,
so shell-set values reach cargo directly, bypassing the forwarding list. Two
mechanisms, and only one of them is the documented one.

### The aggravating factor: the C sources take a parallel path

`zephyr/cmake/nros_rmw_zenoh.cmake:153-163` feeds the *same nine values* to the
Zephyr-compiled C TUs as preprocessor defines, read straight from `CONFIG_*`:

```cmake
zephyr_compile_definitions(
    ZPICO_MAX_PUBLISHERS=${CONFIG_NROS_MAX_PUBLISHERS}
    ...)
```

This is why defect 1 cannot be fixed by simply guarding the `set(ENV{...})`
calls. If the environment wins for the cargo build but the C TUs keep reading
`CONFIG_*`, Rust and C disagree about a struct's size — which is issue 0135's
silent-ABI-break failure mode, reintroduced by the fix. The two consumers must
be fed from **one** resolution.

## Why it stayed hidden

Nothing enumerates the knobs. There is no command that answers "what pools does
this image have, how big are they, and where did each value come from". A knob
that is ignored is indistinguishable from a knob that is honored, because
neither prints anything.

The duplication behind that is the same class this repo has been closing all
week — two `system.toml` parsers (issue 0293), two entry emitters, two
`SchedApplyMode` definitions. Here: `env_usize(name, default)` is defined
**seven** times (`nros-build-helpers/src/shared.rs:551` `pub`, plus copies in
`nros-node/build.rs:129`, `nros-params/build.rs:43`, `xrce-sys/build.rs:7`,
`nros-rmw-zenoh/build.rs:89`, `nros-zpico-build/src/runner.rs:64`, and
`nros-smoltcp/build.rs:90` as `env_usize_compat`). Six of the seven ignore the
shared one.

## Scope: how far a single instrumentation point reaches

A full audit of every sizing path was run before designing a fix, because the
whole value of instrumenting the resolution point depends on it being the *only*
resolution point. **It is not.** There are five mechanisms:

| # | Mechanism | Reachable from a build-script hook? |
| --- | --- | --- |
| A | `env_usize` in `build.rs` → `OUT_DIR/*.rs` | yes (~40 knobs) |
| B | `build.rs` → `cargo:rustc-env` → `env!()` in source | yes |
| C | `option_env!` read directly in library source, **no build script** | **no** |
| D | CMake `zephyr_compile_definitions` → C preprocessor, bypassing cargo | **no** |
| E | Hand-written `#define` in C/C++ headers, no knob at all | **no** |

Plus a sixth injection route: `nros` writes `[env]` blocks into the workspace
`.cargo/config.toml` (`orchestration/model_ingest.rs:430-490` manages
`NROS_CYCLONEDDS_MAX_TYPES` from the SystemModel's distinct-type count), so a
pool can be sized by a generated file rather than by the invoking shell.

Concrete misses for mechanism C: `NROS_CYCLONEDDS_{MAX_TYPES, MAX_FIELDS,
MAX_KINDS, MAX_NESTED_DEPTH}` (`nros-rmw-cyclonedds/src/type_registry.rs:53`,
`dynamic_type.rs:60,66,71`) and `NROS_FREERTOS_APP_STACK_KB`
(`nros-board-freertos/src/config.rs:87`) — none of which get
`rerun-if-env-changed` either, so they do not even trigger a rebuild when
changed.

For mechanism E, the largest unknobbed pools found:

- `NROS_COMPONENT_MAX_PARAMS` = **256** (`nros-cpp/component_node.hpp:100`),
  instantiating `ParameterServer<256,8,…>` at `:543`. The Rust-side
  `NROS_MAX_PARAMETERS` defaults to 32 — the two disagree by 8×.
- `XRCE_SUBSCRIBER_RING_DEPTH` = 32 (`nros-rmw-xrce/src/internal.h:134`), whose
  own comment states the cost: 32 × `XRCE_BUFFER_SIZE` × 8 subscribers = 256 KB.
- `XRCE_SERVICE_REQUEST_RING_DEPTH` = 4 (`internal.h:65`) — no env var anywhere.
- `NROS_C_SERVICE_CLIENT_STORAGE_SIZE` = 4632 (`nros-c/component.h:245`).
- ~20 literal-sized Rust arrays (`nros-node/src/limits.rs:8-32`,
  `executor/monitor.rs:133`, `parameter_services.rs:54`, …).

Also found: `NROS_MAX_CONCURRENT_GOALS` is **documented** as a knob
(`nros-c/docs/configuration.md:26`, mirrored in the C++ and Rust guides) but no
code reads it — `MAX_CONCURRENT_GOALS` is a literal 4 at
`nros-node/src/limits.rs`. The same table documents `XRCE_BUFFER_SIZE` and
`XRCE_STREAM_HISTORY` under their unprefixed names, which is where defect 2's
spelling probably came from.

### Per-unit cost is not always a product of one knob

Any report that prints bytes needs the arithmetic declared, not inferred:

- Receive payload `.bss` is a **3-knob product across two crates**:
  `ZPICO_MAX_SUBSCRIBERS × RING_DEPTH × BUFFER_SIZE + MAX_LARGE_SUBSCRIBERS ×
  RING_DEPTH × LARGE_SIZE` (`nros-rmw-zenoh/src/shim/subscriber.rs:192-198`),
  where `ZPICO_MAX_SUBSCRIBERS` arrives from *zpico-sys*'s generated
  `shim_constants.rs` and the other two from *nros-rmw-zenoh*. 160 KiB at
  defaults.
- `MessageInfoSlot` gains three fields under
  `cfg(all(feature = "alloc", feature = "safety-e2e"))`
  (`nros-rmw-cffi/src/lib.rs:545-565`), so a **Cargo feature** — not an env var
  — changes the per-unit cost.
- `NROS_RMW_SUBSCRIBER_SLOTS` is a clean product, but the multiplicand
  (`SLOT_SIZE = 1024`, `rust_adapter.rs:75`) is an unknobbed literal.

## Fix

Staged, because the defects have very different risk.

**Stage 1 — precedence (this issue's bug fix). DONE.** `nros_resolve_knobs()`
resolves each knob exactly once, before any consumer, and both the cargo
environment and the zenoh C preprocessor defines read `NROS_RESOLVED_<KNOB>`
rather than `CONFIG_*`. Uniform rule: an explicit environment value wins over
Kconfig, and a disagreement prints. A `FATAL_ERROR` guards against a consumer
running before resolution.

**Stage 2 — defects 2 and 3. DONE.** The knobs are exported and forwarded under
the spellings the readers actually use, and the forwarding list is now
*generated* from the resolved set rather than hand-maintained, which makes
"resolved but not forwarded" unrepresentable.

Two Kconfig defaults were realigned so that repairing the plumbing does not
change what gets compiled — the discipline being that a knob starting to work
must not silently move bytes:

- `CONFIG_NROS_XRCE_STREAM_HISTORY` 4 → **16**, matching `internal.h`. Left at
  4 it would have shrunk every reliable stream buffer 4×.
- `CONFIG_NROS_EXECUTOR_MAX_CBS` 16 → **4**, matching `nros-node/build.rs`.
  Left at 16 it would have grown a native_sim listener's `.bss` by **219,464
  bytes (+35%)** the moment the option started working. The old default of 16
  had never been built, because the option never reached cargo.

### Receipts

- `zephyr build-one c/listener zenoh`: `text=730335 bss=622688`, **byte-identical**
  to the pre-fix build. Behavior-neutral, and it confirms the tx trio's Kconfig
  defaults already matched what the RFC-0049 ladder produced.
- Precedence works end to end: with `ZPICO_MAX_SUBSCRIBERS=4` against a Kconfig
  of 8, cmake prints `environment wins`, the C define becomes
  `ZPICO_MAX_SUBSCRIBERS=4`, **and** the generated `shim_constants.rs` carries
  `pub const ZPICO_MAX_SUBSCRIBERS: usize = 4` — Rust and C agreeing is the
  property issue 0135 is about.
- The XRCE knobs are live: `NROS_XRCE_MAX_SUBSCRIBERS=0` now fails the build at
  `nros-rmw-xrce-cffi/build.rs:281` (`too small (minimum 1)`). Before, an
  out-of-range value was accepted silently because the variable never arrived.
- `build.ninja` now forwards `NROS_XRCE_*` and `NROS_EXECUTOR_MAX_CBS`; the
  unread unprefixed `XRCE_*` pool names are gone.

Verification note worth keeping: three separate measurements during this work
were taken with the wrong instrument and had to be discarded — grepping cargo's
`output` file for a define that `cc` passes on the command line; grepping a
build tree for `XRCE_MAX_SUBSCRIBERS` and matching Zephyr's `autoconf.h`
instead; and treating `just zephyr build-c` as a build when it is a stub that
echoes success (see below). Each looked like a failed fix. The decisive tests
were the ones with an unambiguous signal: a build that *fails* on an
out-of-range value, and a byte-identical `size` comparison.

### Unrelated defect found on the way

`just zephyr build-c`, `build-cpp` and `build-xrce` are stubs: they check the
workspace exists, `cd` into it, and echo "built successfully" without building
anything. `build-examples` depends on all three, so it reports success while
building none of them. This is the CLAUDE.md "tests must fail on unmet
preconditions" rule in recipe form, and it is why the first green receipt in
this work was vacuous. Not fixed here — already repaired independently by
`01dba8791 fix(#314)`, which restored the six-role `just zephyr build-one`
loops in all three recipes.

**Stage 3 — enumeration (not in this issue).** Consolidate the seven
`env_usize` copies onto the shared helper, have it record what it resolved
(name, default, resolved value, winning source, per-unit cost), and add a
command that prints the table for the current build. The `source` column is the
load-bearing part: it is what makes a shadowed knob visible.

Note the scope limit honestly — per the audit above, stage 3 covers mechanisms
A and B only, roughly 40 of ~55 sizing knobs. Mechanisms C, D and E need the
declaration sites to opt in, and the ~20 unknobbed literals need a knob before
they can be reported at all. A report that silently covers 40/55 while looking
complete would be its own version of this bug, so whatever ships must state its
own coverage.

## Related

- Issue 0135 — Rust/C config disagreement as a silent ABI break; the reason
  stage 1 cannot just guard the `set(ENV{...})` calls.
- Issue 0269 — added `NROS_RMW_SUBSCRIBER_SLOTS`, one of the pass-through knobs.
- Issue 0271 — the footprint work that surfaced all of this.
