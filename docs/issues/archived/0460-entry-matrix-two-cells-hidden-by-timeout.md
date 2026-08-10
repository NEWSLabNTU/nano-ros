---
id: 460
title: "entry_matrix: two RTOS cells fail — invisible until the nextest timeout stopped killing the run"
status: resolved
type: bug
severity: medium
area: testing, nuttx, zephyr
related: [issue-0422, issue-0445, issue-0406, issue-0307, issue-0135, phase-295, phase-276, phase-280, phase-331]
resolved_in: "issue-0460 (queryable table + Kconfig knob forwarding)"
---

## How these surfaced

`entry_e2e::entry_matrix` reported `TIMEOUT [60.003s]` with no output on every
run. It is not a hang: the matrix takes **228 s** because it boots up to 15 RTOS
images (QEMU nuttx/threadx/freertos plus zephyr native_sim) and aggregates its
verdict at the end, so nothing prints until it finishes.

The cause was a stale filter in `.config/nextest.toml`. phase-295 W3.b
consolidated 15 per-cell tests into one test named `entry_matrix`, but the
timeout override still read

```toml
filter = "binary(entry_e2e) and test(zephyr_rust_lifecycle)"
```

— a test name that no longer exists, so the override matched NOTHING and the
whole matrix ran under the default 30 s × 2 ceiling. Fixed to
`filter = "binary(entry_e2e)"` with a 120 s × 3 budget.

With the run allowed to finish, **13 of 15 cells pass** and two fail. Both were
being absorbed by the TIMEOUT verdict — issue 0445's shape, one level up: there
the verdict was staleness, here it is the harness clock, and in both cases a
terminal self-explaining verdict hid a real runtime result behind it.

## The two failing cells

**1. `nuttx-arm/rust/entry_pubsub`**

```
[nuttx-arm rust] native observer never received the entry image's /chatter
```

The test's own note points at phase-280 W3 (`703e840dd`): the Rust entry path's
`entry_net_init` must push the guest IP into `eth0` via `SIOCSIFADDR` before
`Executor::open`, or the image dies in `Transport(ConnectionFailed)`. Worth
checking whether that path still runs in the current image before assuming the
transport is at fault — the observer is native, so either side can be the
silent one.

**2. `zephyr/rust/params`**

```
[zephyr rust params] subscriber never saw the live-read baked param value (250)
```

Note references phase-276 W1 / #128 (`Framework::Zephyr` gained
`apply_param_services`, so launch-baked initials reach the store) and #147/#278
(the observer must be the TYPED int32-sink; the old String listener only matched
while its fixture was a stale pre-W4 Int32 build). Check the observer's type
first — that exact confusion has already produced one false diagnosis here.

## Why they are not in #0422

#0422 indexes the runtime E2E baseline; its `params` row is the interop
`params` binary, not this zephyr entry cell, and it carries no nuttx entry row.
These two were simply never observable while the timeout killed the run.

## Reproduce

```
cargo nextest run -p nros-tests --test entry_e2e      # ~228s, 2 of 15 cells fail
```

## Reporting fixed, both cells still open (2026-08-06)

The two delivery assertions timed out on the OBSERVER and then blamed the guest
— "the embedded LAUNCH-entry runtime delivery did not work" — without ever
showing the guest's output. Either side can be the silent one, and the message
picked one by assertion.

They now print the guest's own log and classify it with
`nros_tests::output::runtime_silence_note`: if the runtime never spoke, the
fault is before delivery and no amount of looking at the transport will find
it. The issue's own advice ("the observer is native, so either side can be the
silent one") is now enforced by the message rather than left to the reader.

**Neither cell is fixed and this issue stays open** — `nuttx-arm/rust/
entry_pubsub` and `zephyr/rust/params` still fail. The first checks are
unchanged: whether `entry_net_init` still pushes the guest IP into `eth0` before
`Executor::open`, and whether the params observer is the TYPED int32 sink.

## Re-measured on fresh fixtures (2026-08-07) — half fixed, and the shape changed

**`nuttx-arm/rust/entry_pubsub` PASSES.** Fixed by someone else's work between
the filing and now; not attributed.

**`zephyr/rust/params` still fails**, and it is not alone. With every fixture
rebuilt (linux-rust, nuttx, and zephyr — the last needed a `nros sync` in the
seven `examples/zephyr/rust/*` leaves first, because #0463 makes an unsynced
leaf unparseable):

```
entry_matrix: 12 ran, 0 skipped, 3 failed (of 15 cells)
  zephyr/rust/params:    subscriber never saw the live-read baked param value (250)
  zephyr/rust/lifecycle: `ros2 lifecycle nodes` listed no managed node
  zephyr/rust/qos:       observer never saw 3 `/qos_ok` republishes
```

Three cells, one platform+language, three different FEATURE entries. That is a
family, not three bugs: the common suspect is the zephyr-rust entry's feature
wiring (each cell asserts a different declared capability reaching the running
image), not three unrelated runtime paths. Whoever takes this should look there
first rather than at params, lifecycle and qos separately.

## What the measurement itself cost

The first re-run said "1 of 15 FAILED" and listed neither of this issue's cells,
which reads as "both fixed". They had been SKIPPED on stale fixtures — the
harness collected skips and only reported them when EVERY cell skipped, so ten
passes and four skips printed identically to fourteen passes. Fixed in the same
commit as this note: `entry_matrix` now always prints `N ran, M skipped, K
failed`, with the skip list.

The skip text was already carrying the issue-0445 ledger, which is what made the
staleness legible once the count was visible:

```
NOT RUN: 2th consecutive stale verdict for this fixture, first 8m ago.
This coordinate has produced no runtime result since then …
```

Staying open for the three zephyr/rust cells.

## Root cause, mostly (2026-08-07)

The three zephyr/rust cells were not hanging. **The entry was returning an error
and the generated `rust_main` was dropping it**:

```rust
pub extern "C" fn rust_main() {
    unsafe { let _ = ::zephyr::set_logger(); }
    let _ = __nros_zephyr_entry_run();   // <- Result dropped
}
```

Its own comment claimed errors "are logged and the `Result` is dropped"; only
the dropping was implemented. A successful entry never returns (it spins
forever), so ANY return is a failure — and returning quietly leaves Zephyr's
main thread terminated with only kernel threads alive. That is what a gdb dump
showed: conn_mgr, two workqueues, the shell, the sys workqueue, idle, and **no
application thread at all**. The image then idles to the test's timeout having
printed nothing after "Network ready", which is indistinguishable from a hang.

Fixed: the entry now logs and panics. The three cells immediately named
themselves:

```
<err> rust: rustapp: nros: zephyr entry FAILED: NodeRegister("lifecycle")
```

**Why lifecycle, in the params and qos entries.** The features workspace holds
ONE `system.toml` for every feature demo — deliberately, because rust cannot
hold two systems (phase-315 W1) — and it declares
`features = ["param_services", "lifecycle"]`. That list is a UNION over the
whole workspace, so EVERY entry emits BOTH capabilities regardless of which
launch file it selected. `register_lifecycle_services()` then fails.

`zephyr/rust/safety` passes throughout, and it is the control that makes this
readable: `safety` is a BACKEND feature that registers no services.

## What is fixed here, and what is not

Fixed:

* the silent drop (above) — three "hangs" are now named errors;
* capability services are counted in `executor_sizing`. `[lifecycle]` is five
  REP-2002 services and `[param_services]` six, none of which the model counts,
  against a `DEFAULT_MAX_CBS` of 4. Gated by `check-capability-slot-counts`,
  which ties the constants to the server structs' field counts (the constants
  must live in `executor_sizing`, which only the proc-macro can read, while the
  services live in `nros-node`, which does not depend on it — nothing in the
  type system connects them). Watched to fire.

NOT fixed, and the next thread: raising `CONFIG_NROS_EXECUTOR_MAX_CBS` to 16 in
the three entries' `prj.conf` does NOT clear it, even though the value is
confirmed in the build's generated `.config`. So either the failure is not
capacity, or the Kconfig is not reaching the RUST lane's crate build — the
Kconfig's own help text says it "was never forwarded to Cargo before issue 0316,
so every Zephyr C image compiled the crate default of 4", which names exactly
this forwarding path. Check that before assuming the capacity theory is wrong.

## The Kconfig never reached the Rust lane (2026-08-07) — fixed, but not the cause

The previous note left one thread: `CONFIG_NROS_EXECUTOR_MAX_CBS=16` was in the
build's generated `.config` and changed nothing. Measured, and it is worse than
"not applied":

```
$ grep -c NROS_EXECUTOR_MAX_CBS <build>/build.ninja
0
$ grep -o 'MAX_CBS: usize = [0-9]*' <build>/rust/target/.../nros-node-*/out/*.rs
MAX_CBS: usize = 4
```

Zero occurrences in the build graph, and the crate compiled the default of 4
while Kconfig said 16.

**Why.** `zephyr/cmake/nros_cargo_build.cmake` exports every resolved knob with
`set(ENV{...})`, which only touches the CONFIGURE-time cmake process. The C lane
survives that because `nros_cargo_build()` re-bakes the vars into its build
command (`cmake -E env …`). The RUST lane's command is built by
zephyr-lang-rust's `rust_cargo_application`, which passes its own fixed variable
list and inherits nothing — so **every Zephyr Rust image has been compiling
nros-node's crate defaults regardless of Kconfig**, for every knob, not just
this one. The Kconfig's own help text says the option "was never forwarded to
Cargo before issue 0316, so every Zephyr **C** image compiled the crate default
of 4" — the C wording turns out to be load-bearing.

**Fixed** in `nros-node/build.rs`: when a knob's env var is absent, read
`CONFIG_<NAME>` from the file named by `$DOTCONFIG`, which IS in that command's
environment. One place, covers every knob `build.rs` reads, and needs no change
to the vendored module. Explicit env still wins, so the C lane is untouched.
Verified: the crate now compiles `MAX_CBS: usize = 16`.

**It is not the cause of these three cells.** With 16 genuinely compiled in, all
three still fail identically on `NodeRegister("lifecycle")`. So capacity is
ruled OUT, and the `CONFIG_NROS_EXECUTOR_MAX_CBS=16` lines added to the three
`prj.conf` files are correct sizing hygiene (11 capability services against a
default of 4) rather than the fix.

## Where this now stands

The failure is inside `register_lifecycle_services()`, which can only fail two
ways: `NameTooLong` on the FQN heapless string, or
`ServiceServerCreationFailed` from `create_lc_srv`. The node FQN comes from the
executor's own namespace/node_name, so no node needs to be registered first, and
the names here are short — which points at queryable declaration.
`Z_FEATURE_QUERYABLE` is 1 in the image's generated zenoh config, so it is not a
disabled feature.

Naming the cause needs the `NodeError` to survive to a log line. It currently
cannot: `apply_lifecycle` maps it to `()`, the macro turns that into
`RuntimeError::NodeRegister("lifecycle")`, and `nros` has no `log` dependency to
report it from (attempted; it does not compile). The clean fix is to widen those
trait methods to return `NodeError` so the macro — whose emitted code DOES have
`log` — can print it. That is the next step, and it is the same
make-the-failure-name-itself move that got this far.

## Root cause: the queryable table, and why nobody could see it (2026-08-10)

The reason survived to a log line, and it named itself on the first boot:

```
<err> rust: rustapp: nros: zephyr entry FAILED: Capability {
    name: "lifecycle",
    reason: "Transport::ServiceServerCreationFailed (the RMW refused to declare the queryable)" }
```

**A service server IS a zenoh queryable, and the table holds 8.** The macro
emits `apply_param_services` before `apply_lifecycle`, so the six parameter
services take slots 0–5, lifecycle takes 6 and 7, and the third lifecycle
service is refused. Eleven capability services against an 8-slot table: the
arithmetic is the whole bug. Proven both ways — with the table at 16 the entry
prints `zephyr workspace entry up (1 nodes)` and spins.

Three things had to line up for this to be invisible for as long as it was:

1. **The overflow's only diagnostic was `cfg(feature = "std")`.** Issue 0406
   added a `log::error!` that names the knob — and gated it on `std`, i.e. off
   on every embedded image, which is the only place the 8-slot budget applies.
   The caller got a bare `ServiceServerCreationFailed`.
2. **`create_lc_srv` threw the reason away** — `.map_err(|_| Transport(
   ServiceServerCreationFailed))` at five sites in `spin.rs` and two in
   `node.rs`, so even a specific backend error arrived as the generic one.
3. **`CONFIG_NROS_MAX_QUERYABLES` never reached the Rust lane.** This is the
   `MAX_CBS` finding above, generalized: `nros_cargo_build.cmake` publishes
   every knob with `set(ENV{...})`, the C lane re-bakes them into its build
   command, and zephyr-lang-rust's `rust_cargo_application` builds its own
   cargo invocation that inherits nothing. Measured on this leaf: `.config`
   said 16, the cmake-compiled TU got `-DZPICO_MAX_QUERYABLES=16`, and the
   cargo-compiled crate const stayed 8. **Every Zephyr Rust image has been
   compiling crate defaults for every knob**, not just this one — and when the
   two halves disagree it is also an issue-0135 ABI split.

## The fix

* `nros_zephyr_build::knob_usize(env, kconfig_key, default)` / `dotconfig_usize`
  — ONE spelling of the env → `$DOTCONFIG` → default ladder, in the crate that
  already owns "read Kconfig from a build script". `nros-node`'s private copy
  (added earlier in this issue) now calls it, and the three other knob readers
  join: `nros-zpico-build` (11 knobs + the tx trio's string ladder),
  `nros-rmw-zenoh` (the two buffer sizes), `nros-rmw-xrce-cffi` (the six pool
  knobs, which had the identical silent-default bug on this lane).
* `check-kconfig-knob-forwarding` — every `_nros_resolve_knob()` in the cmake
  module must be read by a Rust build script. 21 knobs today. A knob added to
  one side and not the other is one more silently-defaulted image, and nothing
  else in the build would say so.
* The overflow now returns `TransportError::Backend("zenoh queryable table
  exhausted — raise CONFIG_NROS_MAX_QUERYABLES …")`. A `&'static str` crosses
  `no_std` with no logger and no allocator, and the capability seam prints it
  verbatim.
* The seven `map_err(|_| …ServiceServerCreationFailed)` sites became
  `map_err(NodeError::Transport)`, so a backend reason reaches the caller.
* `capability_reason` in `nros` maps every `NodeError` variant and every
  `TransportError` variant by name (exhaustive on `NodeError` — a wildcard
  there is what this issue is about).
* `CONFIG_NROS_MAX_QUERYABLES=16` in the three entries' `prj.conf`. The shim
  default stays 8: these tables are static arrays and every slot costs RAM on
  targets that are not native_sim.

## The third cell was a stale assertion, not a runtime fault

With the queryable table fixed, `lifecycle` and `qos` passed and `params` still
failed — but on a different thing entirely. The entry publishes **120**, and the
cell asserted 250.

250 is the launch file's inline `<param>`. 120 comes from a `params_files`
overlay `system.toml` declares on `rust_param_talker_pkg` (ported here by
phase-331 W3 from the retired `ws-params-rust`), and the model's ordered
`param_sources` fold puts a file AFTER an inline value — rlm's deliberate rule
(phase-54, issue 0307), so the file wins. Only the RUST params model carries an
overlay; the C and C++ params models have none and still resolve 250, which is
why this one cell moved and its siblings did not.

The overlay's YAML has two blocks — `param_talker: 120` and `/**: 999` — which
exists precisely so something observes 120 rather than 999. The cell is the only
observer, so 120 is the intended value and the 250 predates the port. Updated,
and the note now records what the value proves: file projection, within-file
specificity ranking, on-target seeding, and the live re-read, in one assertion.

## Verified (2026-08-10)

```
entry_matrix: 14 ran, 1 skipped, 0 failed (of 15 cells)
```

The one skip is `nuttx-arm/rust/entry_pubsub`, whose fixture the native lane
does not build — not a cell failure. Both cells this issue was filed for, plus
the two the measurement added, now pass.
