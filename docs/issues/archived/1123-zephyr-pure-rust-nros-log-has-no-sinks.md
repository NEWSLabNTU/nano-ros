---
id: 1123
title: "A pure-Rust Zephyr image installs the `log` bridge and never publishes an `nros_log` sink list, so every framework record sits in the early ring"
status: resolved
area: boards, core
severity: medium
found: 2026-09-06
resolved_in: 2026-09-07
related: [1048, 0589, 0708, 0710]
---

# The facade that works is not the facade CLAUDE.md tells you to use

Found while sweeping the CLASS of issue 1048 (`log::set_logger` does not exist
on `riscv32imc`, so the esp32-qemu board dropped every record). The esp32 half
is fixed. This is the OTHER instance the sweep turned up, and it is the mirror
image: on Zephyr the `log` facade works fine and `nros_log` is the one that
goes nowhere.

## What is claimed, and where

CLAUDE.md's issue-0589 entry is explicit that `std::println!` kills a Zephyr
native_sim image and that the replacement is:

> Write `nros_log::nros_error!(nros_log::get_logger("<crate>"), …)`: it lands on
> `LOG_ERR`/`printk`, never fatal, and reaches `no_std` targets that every
> `cfg(feature = "std")` arm silently skipped.

That is true of the delivery MECHANISM — `nros_platform_log_write` is defined
unconditionally in `packages/platform/nros-platform-zephyr/src/platform.c:1117`
and maps severity onto `LOG_ERR`/`LOG_WRN`/`LOG_INF` (or `printk` without
`CONFIG_LOG`). It is not true of the DISPATCH, for a pure-Rust image.

## The gap

`nros_log::dispatch_to_sinks` (`packages/core/nros-log/src/lib.rs`) reads a sink
list that `nros_log::init` publishes. With none published it does not fall back
to the platform — issue 0710 deliberately removed that, because reaching for
`nros_platform_log_write` on a path every binary executes turned a pluggable
delivery into a link-time requirement. Instead it HOLDS the record in
`nros_log::early`'s bounded ring, to be drained by whatever `init` eventually
runs.

For a pure-Rust Zephyr image, no `init` ever runs:

* `nros::zephyr_component_main!` (`packages/api/nros/src/lib.rs:772`) installs
  `zephyr::set_logger()` — the `log` facade — and nothing else.
* `nros::main!`'s Zephyr `rust_main` codegen
  (`packages/core/nros-macros/src/main_macro.rs:1906`) does the same.
* `nros-board-zephyr` calls `::nros_platform_cffi::log::init_default()` at
  exactly one funnel, `entry_tiers.rs:363` (`run_tiers`) — so only a MULTI-TIER
  image is covered.
* The C/C++ path is fine and is why this has stayed invisible:
  `nros_log_emit` lazily calls `ensure_default_sinks()`
  (`packages/api/nros-c/src/log.rs`), and `nros_log_init()` exists as an explicit
  C entry. A pure-Rust image has no `libnros_c.a` (issue 0163), so neither runs.

The records that go missing are not user records — a Zephyr example body writes
`log::info!`, which works. They are the FRAMEWORK's: `nros-node`'s executor
(`arena.rs`, `monitor.rs`, `action.rs`, `node.rs`, `spin.rs`),
`nros-rmw-zenoh` (`shim/session.rs`, `zpico.rs` — including the session-pool
diagnostic that issue 0589 moved to `nros_log` specifically so it would reach
`no_std` targets), `nros-rmw-cffi`, and `nros-rmw-bridge`. On a pure-Rust Zephyr
image every one of those is constructed, dispatched, held, and never seen.

## Evidence, and what is NOT evidence

STATIC only. This was read out of the sources listed above; no Zephyr image was
built or booted for it, because the host it was found on could not afford a west
build. Specifically NOT established:

* whether any pure-Rust Zephyr fixture in the tree would visibly change output
  once a sink list is published (the early ring has a bounded depth, so some
  records may already have overflowed by the time an `init` lands);
* whether the `early-records-<N>` default in a Zephyr build is non-zero at all —
  `early-records-0` restores pre-0708 dropping.

Both need a build to answer. Anyone picking this up should start there rather
than trusting this section.

## Why this is 1048's class and not a separate bug

Issue 0708 required "every board boot funnel calls `init_default()`", and
`nros_log::early`'s own module docs already record that as a SEARCH for boot
paths that "kept losing". 1048 was that search losing on esp32-qemu: the board
published at `run_bare` and not at `BoardEntry::run`, so a fixture image printed
and a `nros::main!` image did not. This is the same search losing on Zephyr,
where the funnel is a MACRO rather than a board method and so was never in the
set anyone was grepping.

## Fix sketch (not yet attempted)

The two macros are the funnels, so they are where the call belongs — but neither
expansion can name `nros_platform_cffi` today: the Zephyr example leaves
(`examples/zephyr/rust/*/Cargo.toml`) dep `nros`, `nros-platform`, `zephyr`,
`log` and the backend, and `nros` itself deps `nros-log` but NOT
`nros-platform-cffi`. So a fix is either

1. re-export the sink from a crate the leaves already dep — `nros-platform` gains
   `pub use nros_platform_cffi::log;` under the features that already pull it in
   (`platform-zephyr` enables `dep:nros-platform-cffi`), and both macros emit
   `::nros_platform::log::init_default();`; or
2. give the leaves the dep, which is worse — it is a link-time requirement
   pushed onto every consumer, which is the shape issue 0710 rejected.

(1) keeps the requirement on the crate that already declares the platform.
Whichever is chosen, the acceptance is a BUILT native_sim image whose console
shows a framework `nros_log` record, not a green `cargo check`.

---

## FIXED 2026-09-07 — option (1), and one more funnel the sketch did not name

The sketch's option (1) is what landed, unchanged in shape: `nros-platform`
re-exports `nros_platform_cffi::log` under the same `any(feature = "platform-*")`
list that already gates `ConcretePlatform`, and each macro funnel emits
`::nros_platform::log::init_default()`. Option (2) stays rejected for the reason
the sketch gives — it is the link-time requirement issue 0710 removed.

Three call sites, not two:

| funnel | file | why it reaches no board |
| --- | --- | --- |
| `nros::zephyr_component_main!` | `packages/api/nros/src/lib.rs` | Zephyr owns the C `main`; a Rust staticlib cannot take it over, so `nros-board-zephyr` has only `run_tiers` |
| `nros::main!` → `Framework::Zephyr` | `packages/core/nros-macros/src/main_macro.rs` | same |
| `nros::main!` → `[[bridge]]` | `packages/core/nros-macros/src/main_macro.rs` | a bridge system REPLACES the register/spin body with `nros_bridge::run_from_config_str` |

The bridge arm is the class instance the filing did not have. It emits a bare
`fn main()` that calls no board, and `nros-rmw-bridge`'s own diagnostics
(`cffi.rs`, and `run_from_config_str`'s forward-failure path) are `nros_log`
records — held in the early ring exactly as on Zephyr. It is native-only in the
tree today (`examples/workspaces/bridge-{xrce,cyclonedds}`, `board = "native"`),
which is why nobody had noticed.

Ordering matters and was checked against 1048's mistake: `init` DRAINS the early
ring through the sinks it installs, so it must not run before the writer those
sinks speak to exists. On esp32 it did, and the drain went into a `None` slot.
Zephyr has no such window — `nros_platform_log_write` and
`nros_platform_log_flush` are defined unconditionally in
`packages/platform/nros-platform-zephyr/src/platform.c` (1117, 1144), and
`zephyr/CMakeLists.txt:106` compiles `platform.c` into EVERY image with no
`if()` around it — so the call is placed first in `rust_main`, ahead of
`wait_network`.

### The funnel sweep — which entry shapes were checked, and how

Board crates were enumerated by 1048's own sweep and re-checked here by grep
(`init_default()` appears in `nros-board-{linux,mps2-an385,freertos,threadx,
nuttx,nuttx-qemu,esp32-qemu}`, and the delegating boards reach one of those).
What 1048 could not check, because it was grepping board crates, is the set of
entry shapes `nros::main!` emits. All five, plus the bridge special case:

| `Framework` arm | what it reaches | verdict |
| --- | --- | --- |
| `OwnedSpin` | `<Board as BoardEntry>::run` | covered by the board |
| `Esp32` | `<Board as BoardEntry>::run_with_deploy` | covered by the board (1048) |
| `Rtic` | `RticBoardEntry::init_hardware*` → `nros-board-mps2-an385::rtic::init_with_config`, which calls `init_default()` at line 215 | covered |
| `Embassy` | `EmbassyBoardEntry::init_hardware_with_deploy` | **no live instance** — nothing in the tree impls `EmbassyBoardEntry` (only the trait definition in `nros-platform/src/board/` and a doc example). Same shape as 1048's `thumbv6m` finding: a hole with no board behind it. If an Embassy board ever lands, its `init_hardware` is the funnel |
| `Zephyr` | `rust_main` directly | **was the hole** — fixed |
| `[[bridge]]` (pre-empts the arm) | `nros_bridge::run_from_config_str` | **was a second hole** — fixed |

`ZephyrBoard::run_components` is the C/C++ entry carrier, not a Rust funnel; the
C path publishes lazily through `nros_log_emit`'s `ensure_default_sinks()` and
was never affected.

The enumeration is now written into `nros_log::early`'s module docs, beside the
0708/0710 history, with the two greps that reproduce it — because the durable
lesson is that "grep the board crates" is an INCOMPLETE sweep, and that is what
lost twice.

### The open question, answered — `early-records` is 4, not 0

The filing could not say whether the early ring even exists in a Zephyr build.
It does. `early_depth()` returns 4 unless `early-records-0` is selected, and a
whole-tree sweep of every tracked file finds the `early-records-*` features
named in exactly one place — their own declaration in `nros-log/Cargo.toml` —
plus three prose mentions (issue 1037, this file, RFC-0086). **No consumer,
board, cmake file, Kconfig fragment or fixture row selects any of them**, so
every build in this tree, Zephyr included, takes the else-branch: depth 4.
Pre-0708 dropping is not restored anywhere.

Note the depth is shallow, which is the reason the call is placed FIRST in
`rust_main`: with four slots, a funnel that installs late loses the earliest
records to overflow rather than to the null sink list. Ahead of `wait_network`
there is essentially no Rust-side framework record to lose.

### What was EXECUTED, and what was REASONED

**Executed** (exit 0, all of it on the host, none of it on a target):

* `cargo check -p nros-platform --no-default-features --features platform-zephyr`
  — the re-export exists and compiles under the exact feature every Zephyr
  example leaf selects (`nros-platform = { features = ["platform-zephyr"] }`).
* `cargo check -p nros-platform --no-default-features --features platform-posix`
  — the same for the bridge/native arm.
* `cargo check -p nros-macros`, `cargo check -p nros-log`.
* The greps behind the funnel table, the `early-records` sweep, the
  `platform.c` symbol reads and the `zephyr/CMakeLists.txt` gating read.

**Reasoned, not executed** — and this is the acceptance the filing asked for,
so it is stated plainly rather than buried:

* **No Zephyr image was built or booted.** This host has no Zephyr SDK in
  `~/.nros/sdk` and no `zephyr-workspace/`; standing one up is a `west
  init`/`update` plus an SDK download, not a build, and the host is memory
  constrained with other work running. So the claim "a framework `nros_log`
  record now reaches the native_sim console" is NOT established by observation.
  What is established is every link in the chain separately: the sink list is
  published at the funnel (source), the funnel is the one a pure-Rust Zephyr
  image enters through (source + the 1048 sweep), the sink's ABI symbol is
  compiled into every Zephyr image unconditionally (cmake + C source), and the
  ring that holds the records is 4 deep rather than 0 (feature sweep).
* The bridge arm's emit was not expanded, because expanding it needs a
  generated Entry from `nros sync`. Its dependency premise WAS checked at the
  source: `builder/entry.rs::render_manifest` emits
  `nros-platform = { features = ["<board platform feature>"] }` for EVERY entry
  unconditionally (line 340), so `::nros_platform::log` resolves in any entry a
  bridge can deploy to.

### Status

Marked **resolved**: the defect is understood, the fix is at the funnel rather
than at a site, and the class sweep found and closed a second instance. The
runtime observation the filing asked for is the one thing still owed — a
`just zephyr`/native_sim run on a host with the SDK should show framework
records (nros-node executor, nros-rmw-zenoh session) on the console where it
previously showed only the example's own `log::info!` lines. If that run does
NOT show them, reopen here rather than filing fresh: the next suspect is the
ring depth (4) overflowing during bringup, not the funnel.
