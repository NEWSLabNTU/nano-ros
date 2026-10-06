---
id: 1037
title: "`nros-log` carries FOUR pick-one Cargo-feature families in `packages/core/`,
  three of which encode \"off\" — RFC-0086 D5 forbids both and its audit missed them"
status: resolved
type: bug
area: config, api
related: [rfc-0086, rfc-0049, rfc-0102, phase-400, phase-417, phase-479, issue-0503, issue-0710, issue-1712]
---

## The rule, and the claim that is wrong

RFC-0086 D5:

> * Cargo features may **pull code in**. They may not express "off", and they may
>   not encode a pick-one family.
> * Any knob whose correct value is sometimes "off", and any exclusive choice,
>   belongs in the ladder with a lane front-end that can carry `0` as well as `1`.
> * **No migration is outstanding. The audit found no exclusive or negative
>   configuration expressed as a Cargo feature in core or api.**

`packages/core/nros-log/Cargo.toml` has four:

| family | members | encodes "off"? |
| --- | --- | --- |
| `max-level-*` | trace, debug, info, warn, error, **off** | yes — `max-level-off` |
| `early-records-<N>` | **0**, 8, 16 | yes — `early-records-0` |
| `dynamic-loggers-<N>` | **0**, 8, 32 | yes — `dynamic-loggers-0` |
| `buffer-size-<N>` | 128, 256, 512, 1024 | no, but pick-one |

Sixteen features across four exclusive families, in `core`, three of them with a
member whose whole meaning is "off". Every one is a knob a user picks, and every
one is invisible to `nros config explain`.

`platform-clock` is the fifth and a different shape: it legitimately *pulls code
in*, so D5's first clause allows it — but its ABSENCE silently changes observable
behaviour in all three languages (records carry `timestamp_ns: 0`, and
`nros_log_throttle_admit` has no time base so a 200 ms window admits every
record — measured 40 of 40 without, 5 of 40 with). That is the negative half of
D5 arriving through the back door.

## Why the audit missed it

D5's audit looked for configuration expressed as a Cargo feature. These read as
*sizing* — `buffer-size-256` looks like a build detail rather than a policy — and
`nros-log` was not one of the tenants phase-400 W6 migrated (`executor`,
`memory`, `params`, `rmw`, `transport`). Logging simply was not on the list, so
nothing pointed at it.

The general shape: **a knob looks like a Cargo feature exactly when the crate
that owns it is not yet a tenant.** The five migrated tenants have the same kind
of knob and none of them is a feature.

## How phase-417 made it worse, twice

1. W4.d added `dynamic-loggers-{0,8,32}` — a **new** pick-one family with an
   "off" member, in core, after D5 was written.
2. Integration wired `nros-log/platform-clock` through `nros-c`'s five
   `platform-*` features. That was a fix for a real bug (setting it on the
   *dependency* turned it on workspace-wide, and test binaries link no platform
   port, so `nros-log`'s own tests failed on `undefined reference to
   nros_platform_clock_ns`) — but the correct answer was never a Cargo feature.

## What it should be

`capabilities` is a `BTreeMap<String, bool>` — an **open vocabulary**
(`platform_config.rs:92`), so the clock needs no schema change:

```toml
# packages/platform/nros-platform-{posix,zephyr,freertos,nuttx,threadx}/nros-platform.toml
[capabilities]
clock = true          # nros_platform_clock_us is exported by this port
```

and the sizing knobs become a `logging` tenant beside the five phase-400 W6
migrated, resolved builtin < platform < board < env, reaching C, C++ and Rust
from one declaration and printed by `nros config explain`:

```toml
[knobs.logging]
max_level     = "info"
buffer_size   = 256
early_records = 4
dynamic_loggers = 16
```

The Cargo features do not all have to disappear. D5 permits one that only pulls
code in, and a build script may still *derive* a `cfg` from the resolved ladder —
that is internal logic reading a public declaration, which is the distinction
that matters. What must stop is a HUMAN writing `features = ["max-level-info"]`
in a manifest to choose product behaviour.

## The fix method

The pattern already exists and `nros-node` is the exact precedent — phase-400 W6
migrated the `executor` tenant from build-script env reads to the ladder, and
that crate is in the same position as this one: **it deliberately has no
`platform-*` cargo feature** (phase-248 C2), so its build script cannot learn
its platform from a `cfg` either.

### Step 1 — a `logging` tenant

`LoggingKnobs` beside `ExecutorKnobs`/`MemoryKnobs`/`ParamKnobs`/`RmwKnobs`/
`TransportKnobs` in `platform_config.rs`, plus a `LOGGING_KNOBS` table and a
`logging_env_key()`. The env front-end names are DERIVED from that one table
rather than retyped — `nros-node` does exactly this with `knob_for_env`, and
retyping them is the drift `check-knob-single-reader` exists to catch.

```toml
[knobs.logging]
max_level       = "info"   # compile-time CEILING
buffer_size     = 256
early_records   = 4
dynamic_loggers = 16
```

### Step 2 — `nros-log` gains a build script

```rust
let rungs = BuildRungs::from_build_env().map(|r| r.logging_rungs()).unwrap_or_default();
```

Sizes become generated consts in `OUT_DIR`; `max_level` becomes a
`cargo::rustc-cfg`, because it gates macro EXPANSION rather than a value.

**This step also fixes the bug that started this issue, and that is the
strongest argument for the migration.** `BuildRungs::from_build_env()` returns
`None` when no lane exported a pointer, and every rung is then `None`. So:

* a bare `cargo test -p nros-log` — no lane, no clock rung, no
  `has_platform_clock` cfg, and the test binary links, because it never
  references `nros_platform_clock_ns`;
* a real image — the lane resolved a platform that declares the capability, the
  cfg is on, the symbol exists.

A Cargo feature cannot express that distinction *at all*: features unify across
the workspace, so "on for `nros-c`, off for `nros-log`'s own tests" is
unsayable. That is not a tidiness argument — it is the exact failure the tier
caught, and the ladder is the mechanism that makes it expressible.

### Step 3 — the clock is a capability, not a knob

`capabilities` is an open `BTreeMap<String, bool>` (`platform_config.rs:92`), so:

```toml
# the five ports that export nros_platform_clock_us
[capabilities]
clock = true
```

A capability is a software-stack FACT, which is what this is — the same shape as
`ip_stack` and `serial`. `capability_check` already errors when something
requires a capability the platform does not declare, so a board asking for
timestamps on a clockless port gets a configure-time error instead of records
stamped `0`.

### Step 4 — retire the sixteen features

As ONE batch with a changelog entry, per the retirement discipline: it is the
irreversible step for an out-of-tree consumer naming `features =
["max-level-info"]`. `platform-clock` may survive as an internal cfg the build
script sets; what retires is the human-written spelling.

### The open decision, resolved

I filed this saying a compile-time ceiling contradicting the runtime
`nros_logger_set_level` would be two sources of truth. **On checking upstream,
that was wrong and they compose.** rcutils has both: `RCUTILS_LOG_MIN_SEVERITY_*`
(`logging_macros.h:40-43`) is a compile-time floor that eliminates code, and
`rcutils_logging_set_logger_level` (`logging.h:403`) is the runtime threshold.

They are a BOUND and a VALUE, not two answers to one question: the ceiling says
what can possibly be emitted, the runtime level says what is emitted now, and a
runtime level below the ceiling is simply unreachable. So `max_level` is
per-image and `set_level` stays per-logger, which is also what ROS 2 does — the
principle the campaign already follows.

## Why this is filed rather than fixed

It reaches the config ladder, five platform manifests, `nros-log`'s build, the
Kconfig `imply` path and every consumer that currently names a feature. It also
needs one decision that is not obvious: whether a `logging` tenant's knobs are
per-image (like `memory`) or per-logger, since a named logger's threshold is
already a runtime value (`nros_logger_set_level`) and a compile-time ceiling
that contradicts it would be two sources of truth for one number.

Meanwhile the shipped state works and is not a regression — `platform-clock`
reaches every real image through the platform features, which is why the C
throttle functions. It is debt with a stated shape, not a broken build.

**D5's third bullet should be corrected**: a migration IS outstanding, and the
"enforced at review" claim did not hold — a new violating family landed after
the rule was written.

## Resolution (2026-10-06)

All five families are knobs, the clock is a platform fact, and RFC-0086 D5's
third bullet now says the migration landed. `dynamic-loggers-<N>` went first
(phase-479 W5, PR #1704); this change moved the rest onto the SAME tenant and
the SAME deprecation rule.

### What changed

- **One tenant, `[knobs.log]`** (`LogKnobs` in `platform_config.rs`, which W5
  had created for `dynamic_loggers`): `max_level`, `buffer_size`,
  `early_records`, `rosout_records` join it, with `LOG_KNOBS` /
  `log_env_key` / `resolve_log` extended and `nros config explain` printing
  all five (`max_level` by name).
- **One reader, `nros-log/build.rs`**, each knob read through
  `nros_zephyr_build::knob` — env / `[image.<id>] env` > Kconfig
  `CONFIG_NROS_LOG_*` > board / platform rung > builtin — into
  `$OUT_DIR/nros_log_config.rs`. Range-checked (buffer 128–4096 — the C++
  runtime-refusal budget needs 128; early 0–256; rosout 1–1024, since
  `KEEP_LAST(0)` is not a QoS). `max_level` accepts a name or `0`–`6` through
  ONE parser (`parse_log_level`), because Kconfig states it as an int, as
  Zephyr's own `LOG_MAX_LEVEL` does.
- **Zephyr**: four Kconfig symbols + `_nros_resolve_knob` rows. Cargo and cmake
  need no new carrier — the same `nros-cargo.toml` `[env]` (cargo road) and
  board facts (cmake road) that carry W5's knob; see Measured for the cmake
  road's missing image-env rung (issue 1712).
- **The clock**: `[capabilities] clock = true` in the posix, zephyr, freertos,
  nuttx, threadx and bare-metal descriptors. `build.rs` sets
  `cfg(nros_log_clock)` when the lane's platform (board rung included, through
  a new `BuildRungs::capability`) declares it, OR when the `platform-clock`
  feature is on; a feature on a platform that declares `clock = false` is a
  build error.
- **The fifth family.** The sweep found `rosout-records-<N>` (phase-467),
  added after this issue was filed and absent from its table — the same
  pick-one shape, same file. It moved too (`NROS_LOG_ROSOUT_RECORDS`).
- **Deprecation, one batch** (`changelog.d/1037.breaking.md`): each family is
  honoured with a `cargo:warning` when no knob is stated, reported redundant
  when it agrees with one, and a BUILD ERROR when it disagrees with a stated
  knob or with a sibling. They left `default` — a default member would have
  disagreed with every stated knob.

### Where this deviates from the fix method above

- **`max_level` is a generated `const`, not a `cargo::rustc-cfg`.** The method
  said cfg "because it gates macro EXPANSION". The code shows it does not: the
  `nros_*!` macros expand to `if severity_enabled_at_compile_time(..)`, a
  `const fn` defined in `nros-log`, so a const folds the branch exactly as the
  feature did. The one place that genuinely needs a cfg is the clock — it
  selects between two `macro_rules!` definitions of `__nros_throttle_now`
  (one a `compile_error!`) — and that is the cfg this change emits.
- **The ceiling also reaches C and C++, at the facade.** `Logger::is_enabled`
  now refuses below the ceiling. Before, the `max-level-*` features filtered
  Rust call sites only — `<nros/log.hpp>` claimed "compile-time filtering is
  via `max-level-*` (compiled into the nros-c staticlib)", which was false:
  `nros_log_emit_at` never consulted it.
- **`platform-clock` is NOT retired as a feature.** It only pulls code in (D5's
  allowed clause), and it is the one carrier on a road that names no platform
  to the build script: a plain-cmake C build, or the Zephyr Rust lane (whose
  `rust_cargo_application` passes no `NROS_PLATFORM_NAME`). `nros-c`'s
  `platform-*` arms keep setting it. What changed is that its ABSENCE no
  longer silently costs a lane build its timestamps.
- **`buffer_size` does not resize the C printf frame.** `<nros/log.h>`'s
  `nros_log_emit_fmt_at` is `static inline`, compiled in the CALLER's
  translation unit from a header that has no per-build value to read (the
  per-build config header is not included by `log.h`, and `c-stubs/log_fmt.c`
  includes it alone). So a C record is bounded by `min(255,
  NROS_LOG_BUFFER_SIZE)`; documented in the header, the README and the book.

### Measured

Every rung, from BUILT binaries (a scratch crate linking `nros-log[rosout]` +
the POSIX port, one target dir per row, reading the values back at run time):

| build | max_level | buffer | early | rosout | dynamic | clock |
| --- | --- | --- | --- | --- | --- | --- |
| no lane (builtin) | 0 (trace) | 256 | 4 | 16 | 16 | 0 |
| `NROS_PLATFORM_NAME=posix` only | 0 | 256 | 4 | 16 | 16 | **1** (capability) |
| + board `[board.knobs.log]` warn/512/8/32/20 | 3 | 512 | 8 | 32 | 20 | 1 |
| + `DOTCONFIG` (Kconfig) 1/640/10/40/28 | 1 | 640 | 10 | 40 | 28 | 1 |
| + env error/768/12/48/36 | 4 | 768 | 12 | 48 | 36 | 1 |

With the clock on, `__timestamp_ns()` read non-zero at run time; with no lane
it is 0 and the binary links with no clock symbol referenced.

- **The APP rung on the cargo road**: `bins/log-arena-probe`'s `[image.native]
  env` now states all five; `tests/log_arena_knob.rs`
  (`every_log_knob_is_read_back_from_the_image_env`) asserts `max_level=2
  buffer=384 early=6 rosout=12` and `clock=1` from the built fixture — the
  clock with NO `platform-clock` feature anywhere in that leaf's graph.
- **The cmake road** (`examples/native/c/talker` copied out, fixture-lane
  configure arguments): the board rung arrives — `nros-log` resolved the
  `posix` platform (its descriptors are watched) and the `native` board's
  `dynamic_loggers = 32`, and the clock cfg is on — and a `NROS_LOG_MAX_LEVEL=warn`
  exported in the shell running `cmake --build` lands (`MAX_LEVEL = 3`). The
  leaf's `[image.native] env` does NOT: both `nros-log` builds kept the
  builtins. That is a pre-existing gap of the cmake road, not of this knob —
  W5's `dynamic_loggers` has it too — filed as issue 1712.
- **C**: `libnros_c.a` (`std,rmw-cffi,platform-posix,ros-humble`) linked into a
  C program with a capture sink. Builtin: `nros_logger_is_enabled`
  debug/info/warn = 1/1/1, and all three `NROS_LOG_*` records delivered.
  Built with `NROS_LOG_MAX_LEVEL=warn`: 0/0/1, and only the WARN record
  delivered — with the logger's runtime level at DEBUG.
- **Deprecation**: `features = ["nros-log/max-level-warn"]` alone → value 3
  and the warning; with `NROS_LOG_MAX_LEVEL=warn` → redundant warning; with
  `NROS_LOG_MAX_LEVEL=info` → build error naming both. `NROS_LOG_MAX_LEVEL=loud`
  and `NROS_LOG_BUFFER_SIZE=64` → build errors.
- **The original bug**: `cargo test -p nros-log` with no lane passes (all
  targets), the clock off. The same command with `NROS_PLATFORM_NAME=posix`
  exported turns the clock on and its integration-test binaries, which link no
  port, fail on `undefined symbol: nros_platform_clock_ns` — the expected
  trade, and the reason the capability is read only from a lane's own
  pointer, which no test recipe exports.

### Sweep

```sh
git grep -n -E 'max-level-|early-records-|buffer-size-[0-9]|rosout-records-|dynamic-loggers-|platform-clock' \
  -- ':!docs/issues/archived' ':!docs/roadmap/archived' ':!*.lock'
```

Every remaining hit is the deprecated feature list and its rule in
`nros-log`'s manifest/`build.rs`, the docs describing the deprecation, the
legitimate `platform-clock` pull-in (`nros-c` arms, `rosout-talker`,
`check-baremetal-platform-arms`), `nros-core`'s unrelated `platform-clock`
feature, and historical prose in other issues/ledger rows.
